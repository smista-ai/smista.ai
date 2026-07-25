//! Shared `rig`-backed agent used by every remote model adapter.
//!
//! [`Agent`] wraps a [`RigAgent`] built from the provider configuration and the
//! memory storage, and is the single place where the internal request/response
//! vocabulary in [`crate::api`] is mapped onto `rig` types. Nothing outside the
//! adapter layer sees `rig`.

use std::collections::BTreeSet;
use std::sync::Arc;

use futures::StreamExt;
use rig_core::OneOrMany;
use rig_core::agent::Agent as RigAgent;
use rig_core::client::CompletionClient;
use rig_core::completion::request::{
    Completion, CompletionError, CompletionRequestBuilder, GetTokenUsage,
    ToolDefinition as RigToolDefinition, Usage as RigUsage,
};
use rig_core::message::{
    AssistantContent, Message as RigMessage, Reasoning, ReasoningContent, Text,
    ToolCall as RigToolCall, ToolChoice as RigToolChoice, ToolFunction,
    ToolResult as RigToolResult, ToolResultContent, UserContent,
};
use rig_core::streaming::{StreamedAssistantContent, ToolCallDeltaContent};
use rust_decimal::Decimal;
use serde::Serialize;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::{ModelDescriptor, ModelParameters, Provider};
use smista_core::stream::StreamEvent;
use smista_core::usage::Usage;

use crate::ProviderResult;
use crate::api::{
    CompletionRequest, CompletionResponse, FinishReason, RequestMessage, ResponseStream, ToolCall,
    ToolChoice, ToolDefinition,
};
use crate::memory::{MemoryRecord, MemoryScope, MemoryStorage, MemoryTool};
use crate::provider::tool_call::{TextToolCallStream, decoder_for_provider};

/// Maximum number of memory records to load from each memory type (user and session) when building the agent preamble.
const MEMORIES_MAX_RECORDS: usize = 40;

/// Maximum number of consecutive turns [`Agent::complete`] spends executing
/// agent-internal tool calls (the memory tool) before giving up.
///
/// Memory operations complete in one or two turns; the bound only exists to
/// stop a model that keeps requesting internal tool calls from looping
/// forever. Raising it increases worst-case latency and cost per completion.
const MAX_INTERNAL_TOOL_TURNS: usize = 8;

/// Arguments for creating a new [`Agent`].
pub struct AgentArgs<C, S>
where
    C: CompletionClient,
    S: MemoryStorage,
{
    /// The completion client to use for the agent's reasoning and tool execution.
    pub completion_model: C,
    /// The descriptor of the model the agent drives: the single source of the
    /// model name, provider, pricing and streaming capability.
    pub descriptor: ModelDescriptor,
    /// A preamble to inject to give instructions to the agent about what it should do.
    pub preamble: String,
    /// Extra preamble segments appended after the base preamble and the memory,
    /// in order, each as its own block.
    ///
    /// The segments are opaque text: the agent never inspects them, so it stays
    /// agnostic to whatever a caller renders into them (skills, instruction
    /// documents, …). Empty when a turn adds nothing beyond base and memory.
    pub preamble_segments: Vec<String>,
    /// The memory storage to use for the agent's memory operations.
    pub storage: Arc<S>,
    /// The user and session the agent's memory operations are scoped to.
    pub scope: MemoryScope,
}

/// Internal structure which wraps an [`RigAgent`], and is built from the provider configuration and memory storage.
///
/// The agent is responsible for maintaining the conversation state, and for executing the tool calls to collect memory records.
///
/// It is used to communicate with the external agent, and to inject preamble into the conversation, and to inject the tool for
/// collecting memory records.
pub struct Agent<C>
where
    C: CompletionClient,
{
    agent: RigAgent<C::CompletionModel>,
    /// The driven model's descriptor: source of the model name, provider,
    /// pricing and streaming capability.
    descriptor: ModelDescriptor,
    /// Names of the tools the agent executes itself (the memory tool); calls to
    /// any other tool are returned to the caller for router mediation.
    internal_tools: BTreeSet<String>,
}

impl<C> Agent<C>
where
    C: CompletionClient,
{
    /// Creates a new [`Agent`] by loading the preamble from the [`MemoryStorage`], and building a [`RigAgent`] with the provided configuration.
    ///
    /// Fails if there is an error loading the preamble from the storage, or if there is an error building the agent.
    pub async fn new<S>(
        AgentArgs {
            completion_model,
            descriptor,
            preamble,
            preamble_segments,
            storage,
            scope,
        }: AgentArgs<C, S>,
    ) -> ProviderResult<Self>
    where
        S: MemoryStorage + 'static,
    {
        let model = descriptor.model.clone();
        let provider = descriptor.provider.clone();

        // load preamble from memory storage
        tracing::debug!(
            model.provider = %provider,
            model.name = %model,
            "loading preamble from memory storage"
        );
        let memory_preamble =
            load_memories_preamble(storage.as_ref(), scope, provider.clone(), &model).await?;

        // load memory tool
        let memory_tool = MemoryTool::new(storage.clone(), scope);

        // build agent
        tracing::debug!(
            model.provider = %provider,
            model.name = %model,
            "creating agent"
        );
        let mut builder = completion_model
            .agent(model.clone())
            .preamble(&preamble)
            .tool(memory_tool);
        // `preamble` replaces the system prompt, so the memory preamble must be
        // appended rather than set or it would wipe the base preamble.
        if let Some(memories) = memory_preamble {
            builder = builder.append_preamble(&memories);
        }
        // Extra segments follow the memory, each appended as its own block so
        // the final preamble order is base, then memory, then the segments.
        for segment in &preamble_segments {
            builder = builder.append_preamble(segment);
        }
        let agent = builder.build();

        // snapshot the names of the tools the agent executes itself, so
        // completions can tell internal tool calls apart from router-mediated ones
        let internal_tools = agent
            .tool_server_handle
            .get_tool_defs(None)
            .await
            .map_err(|error| {
                crate::error::provider_error(
                    ProviderErrorCategory::Unknown,
                    provider.clone(),
                    Some(model.clone()),
                    format!("failed to enumerate agent tools: {error}"),
                )
            })?
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        tracing::debug!(
            model.provider = %provider,
            model.name = %model,
            "agent created successfully"
        );

        Ok(Self {
            agent,
            descriptor,
            internal_tools,
        })
    }

    /// Sends a request and awaits the full completion.
    ///
    /// Returns the model's final content, any tool calls it issued for the
    /// router to mediate, usage totals and the reason generation stopped.
    ///
    /// Turns in which the model only calls agent-internal tools (the memory
    /// tool) are executed here and fed back to the model, up to
    /// [`MAX_INTERNAL_TOOL_TURNS`]; usage is accumulated across those turns.
    /// Any turn containing a tool call the agent cannot execute itself is
    /// returned as-is for the router to mediate.
    pub async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        let CompletionRequest {
            messages,
            parameters,
            tools,
            tool_choice,
        } = request;
        tracing::debug!(
            model.provider = %self.descriptor.provider,
            model.name = %self.descriptor.model,
            request.messages = messages.len(),
            request.tools = tools.len(),
            "starting completion"
        );
        let text_tool_call_decoder = decoder_for_provider(&self.descriptor.provider);

        let mut history: Vec<RigMessage> = messages.into_iter().map(into_rig_message).collect();
        let Some(mut prompt) = history.pop() else {
            return Err(self.error(
                ProviderErrorCategory::InvalidRequest,
                "completion request has no messages",
            ));
        };

        let mut usage = RigUsage::new();
        for _ in 0..MAX_INTERNAL_TOOL_TURNS {
            let response = self
                .request_builder(
                    prompt.clone(),
                    &history,
                    &parameters,
                    tools.clone(),
                    tool_choice,
                )
                .await?
                .send()
                .await
                .map_err(|error| {
                    self.error(
                        crate::error::category_from_completion(&error),
                        error.to_string(),
                    )
                })?;
            usage += response.usage;

            let mut content = String::new();
            let mut tool_calls: Vec<RigToolCall> = Vec::new();
            for item in response.choice.iter() {
                match item {
                    AssistantContent::Text(text) => content.push_str(&text.text),
                    AssistantContent::ToolCall(call) => tool_calls.push(call.clone()),
                    // Reasoning and image content have no place in the
                    // response vocabulary; they are preserved only when a turn
                    // is echoed back to the model below.
                    AssistantContent::Reasoning(_) | AssistantContent::Image(_) => {}
                }
            }
            let recovered_text_tool_calls = if tool_calls.is_empty() {
                text_tool_call_decoder.and_then(|decoder| {
                    decoder.decode(&content).map(|calls| {
                        tracing::debug!(
                            tool.calls = calls.len(),
                            "promoting textual model output to structured tool calls"
                        );
                        content.clear();
                        tool_calls.extend(calls.into_iter().enumerate().map(|(index, call)| {
                            RigToolCall::new(
                                format!("text-tool-call-{index}"),
                                ToolFunction::new(call.name, call.arguments),
                            )
                        }));
                    })
                })
            } else {
                None
            }
            .is_some();

            let all_internal = !tool_calls.is_empty()
                && tool_calls
                    .iter()
                    .all(|call| self.internal_tools.contains(&call.function.name));
            if !all_internal {
                tracing::debug!(
                    model.provider = %self.descriptor.provider,
                    model.name = %self.descriptor.model,
                    response.tool_calls = tool_calls.len(),
                    usage.input_tokens = usage.input_tokens,
                    usage.output_tokens = usage.output_tokens,
                    "completion finished with tool calls to mediate"
                );
                return Ok(CompletionResponse {
                    finish_reason: if recovered_text_tool_calls {
                        FinishReason::ToolCalls
                    } else {
                        finish_reason(&response.raw_response, !tool_calls.is_empty())
                    },
                    content,
                    tool_calls: tool_calls.into_iter().map(api_tool_call).collect(),
                    usage: api_usage(usage, self.pricing()),
                });
            }

            // every call in this turn is agent-internal: execute them and feed
            // the results back to the model as the next turn
            tracing::debug!(
                model.provider = %self.descriptor.provider,
                model.name = %self.descriptor.model,
                turn.tool_calls = tool_calls.len(),
                "executing agent-internal tool calls"
            );
            let mut results = Vec::with_capacity(tool_calls.len());
            for call in &tool_calls {
                results.push(self.execute_internal_tool(call).await?);
            }
            history.push(prompt);
            history.push(RigMessage::Assistant {
                id: response.message_id.clone(),
                content: response.choice.clone(),
            });
            prompt = RigMessage::User {
                content: OneOrMany::many(results)
                    .expect("tool results are non-empty because tool calls were non-empty"),
            };
        }

        Err(self.error(
            ProviderErrorCategory::Unknown,
            format!("model exceeded the internal tool turn limit ({MAX_INTERNAL_TOOL_TURNS})"),
        ))
    }

    /// Sends a request and returns a stream of response events.
    ///
    /// The streaming counterpart to [`Self::complete`]: events carry partial or
    /// final content, tool-call activity, usage updates and a terminal marker as
    /// the model produces them.
    ///
    /// A successful stream ends with [`StreamEvent::Done`]; a failure surfaces
    /// as a single `Err` item and ends the stream. Unlike [`Self::complete`],
    /// no tool call is executed here — every tool call, internal or not, is
    /// emitted as [`StreamEvent::ToolCallRequested`].
    ///
    /// Models without the `streaming` capability fall back to
    /// [`Self::complete`], whose response is replayed as a short stream:
    /// the full text, any tool calls, the usage totals and the terminal
    /// marker.
    pub async fn stream(&self, request: CompletionRequest) -> ProviderResult<ResponseStream>
    where
        C::CompletionModel: 'static,
    {
        if !self.descriptor.capabilities.streaming {
            tracing::debug!(
                model.provider = %self.descriptor.provider,
                model.name = %self.descriptor.model,
                "model lacks the streaming capability; falling back to a buffered completion"
            );
            return self.complete(request).await.map(completion_events);
        }

        let CompletionRequest {
            messages,
            parameters,
            tools,
            tool_choice,
        } = request;
        tracing::debug!(
            model.provider = %self.descriptor.provider,
            model.name = %self.descriptor.model,
            request.messages = messages.len(),
            request.tools = tools.len(),
            "starting streaming completion"
        );
        let text_tool_call_decoder = decoder_for_provider(&self.descriptor.provider);

        let mut history: Vec<RigMessage> = messages.into_iter().map(into_rig_message).collect();
        let Some(prompt) = history.pop() else {
            return Err(self.error(
                ProviderErrorCategory::InvalidRequest,
                "completion request has no messages",
            ));
        };

        let stream = self
            .request_builder(prompt, &history, &parameters, tools, tool_choice)
            .await?
            .stream()
            .await
            .map_err(|error| {
                self.error(
                    crate::error::category_from_completion(&error),
                    error.to_string(),
                )
            })?;

        let provider = self.descriptor.provider.clone();
        let model = self.descriptor.model.clone();
        let pricing = self.pricing();
        // tool calls whose start has been signalled (or that arrived already
        // complete), keyed by rig's internal call id
        let mut started_calls = BTreeSet::new();
        let events = stream
            .filter_map(move |item| {
                futures::future::ready(map_stream_item(
                    item,
                    &mut started_calls,
                    pricing,
                    provider.clone(),
                    &model,
                ))
            })
            .chain(futures::stream::once(futures::future::ready(Ok(
                StreamEvent::Done,
            ))))
            // a failure is terminal: drop everything after the first `Err`,
            // including the trailing `Done`
            .scan(false, |errored, item| {
                let next = if *errored {
                    None
                } else {
                    *errored = item.is_err();
                    Some(item)
                };
                futures::future::ready(next)
            })
            .scan(
                TextToolCallStream::new(text_tool_call_decoder),
                |normalizer, item| futures::future::ready(Some(normalizer.push(item))),
            )
            .flat_map(futures::stream::iter);

        Ok(ResponseStream::new(events))
    }

    /// Builds the `rig` completion request for one turn, layering the
    /// per-request tools, tool choice and generation parameters on top of the
    /// agent's own configuration (preamble and memory tool).
    async fn request_builder(
        &self,
        prompt: RigMessage,
        history: &[RigMessage],
        parameters: &ModelParameters,
        tools: Vec<ToolDefinition>,
        tool_choice: ToolChoice,
    ) -> ProviderResult<CompletionRequestBuilder<C::CompletionModel>> {
        let builder = self
            .agent
            .completion(prompt, history.iter().cloned())
            .await
            .map_err(|error| {
                self.error(
                    crate::error::category_from_completion(&error),
                    error.to_string(),
                )
            })?
            .tools(tools.into_iter().map(into_rig_tool).collect())
            .temperature_opt(parameters.temperature.map(f64::from))
            .max_tokens_opt(parameters.max_tokens.map(u64::from))
            .additional_params_opt(additional_params(parameters));

        Ok(match into_rig_tool_choice(tool_choice) {
            Some(choice) => builder.tool_choice(choice),
            None => builder,
        })
    }

    /// Executes one agent-internal tool call and wraps its output as the tool
    /// result content to feed back to the model.
    async fn execute_internal_tool(&self, call: &RigToolCall) -> ProviderResult<UserContent> {
        let arguments = serde_json::to_string(&call.function.arguments).map_err(|error| {
            self.error(crate::error::category_from_serde(&error), error.to_string())
        })?;

        // log only the tool name: arguments and output can carry user data
        tracing::debug!(
            model.provider = %self.descriptor.provider,
            model.name = %self.descriptor.model,
            tool.name = %call.function.name,
            "executing internal tool call"
        );
        let output = self
            .agent
            .tool_server_handle
            .call_tool(&call.function.name, &arguments)
            .await
            .map_err(|error| {
                self.error(
                    ProviderErrorCategory::Unknown,
                    format!("internal tool `{}` failed: {error}", call.function.name),
                )
            })?;

        Ok(UserContent::ToolResult(RigToolResult {
            id: call.id.clone(),
            call_id: call.call_id.clone(),
            content: OneOrMany::one(ToolResultContent::Text(Text::new(output))),
        }))
    }

    /// Returns the per-million-token prices from the model descriptor.
    fn pricing(&self) -> Pricing {
        Pricing {
            input_cost_per_million_tokens: self.descriptor.input_cost_per_million_tokens,
            output_cost_per_million_tokens: self.descriptor.output_cost_per_million_tokens,
        }
    }

    /// Builds a [`ProviderError`] carrying this agent's provider and model context.
    fn error(&self, category: ProviderErrorCategory, message: impl Into<String>) -> ProviderError {
        crate::error::provider_error(
            category,
            self.descriptor.provider.clone(),
            Some(self.descriptor.model.clone()),
            message,
        )
    }
}

/// Per-million-token prices read from the model descriptor, used to turn
/// reported token usage into an actual cost.
///
/// A small `Copy` snapshot so the stream mapping can carry it without
/// borrowing the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pricing {
    /// Input price per million tokens, when the descriptor declares one.
    input_cost_per_million_tokens: Option<Decimal>,
    /// Output price per million tokens, when the descriptor declares one.
    output_cost_per_million_tokens: Option<Decimal>,
}

impl Pricing {
    /// Computes the actual cost of an invocation from reported token counts.
    ///
    /// Each side contributes `tokens × price ÷ 1_000_000` when both its price
    /// and its token count are known. Returns `None` when nothing can be
    /// priced — no prices declared or no token counts reported — so an
    /// unknown cost is never conflated with a zero cost (a local model with
    /// zero prices reports `Some(0)`).
    fn actual_cost(self, input_tokens: Option<u64>, output_tokens: Option<u64>) -> Option<Decimal> {
        let tokens_per_price_unit = Decimal::from(1_000_000u32);
        let side = |tokens: Option<u64>, price: Option<Decimal>| {
            Some(Decimal::from(tokens?) * price? / tokens_per_price_unit)
        };
        let input = side(input_tokens, self.input_cost_per_million_tokens);
        let output = side(output_tokens, self.output_cost_per_million_tokens);
        match (input, output) {
            (None, None) => None,
            (input, output) => Some(input.unwrap_or_default() + output.unwrap_or_default()),
        }
    }
}

/// Converts an internal [`RequestMessage`] into the `rig` message vocabulary.
///
/// Tool results become user-side tool-result content correlated by `call_id`;
/// the `is_error` flag has no `rig` equivalent, so a failed call must convey
/// the failure through its result text.
fn into_rig_message(message: RequestMessage) -> RigMessage {
    match message {
        RequestMessage::System { content } => RigMessage::system(content),
        RequestMessage::User { content } => RigMessage::user(content),
        RequestMessage::Assistant {
            content,
            tool_calls,
        } => {
            let text = (!content.is_empty()).then(|| AssistantContent::Text(Text::new(content)));
            let calls = tool_calls.into_iter().map(|call| {
                AssistantContent::ToolCall(RigToolCall::new(
                    call.call_id,
                    ToolFunction::new(call.name, call.arguments),
                ))
            });
            RigMessage::Assistant {
                id: None,
                content: OneOrMany::many(text.into_iter().chain(calls))
                    // providers reject empty assistant turns, so an assistant
                    // message with no text and no calls degrades to empty text
                    .unwrap_or_else(|_| OneOrMany::one(AssistantContent::text(""))),
            }
        }
        RequestMessage::ToolResult {
            call_id,
            content,
            is_error: _,
        } => RigMessage::User {
            content: OneOrMany::one(UserContent::ToolResult(RigToolResult {
                id: call_id,
                call_id: None,
                content: OneOrMany::one(ToolResultContent::Text(Text::new(content))),
            })),
        },
    }
}

/// Converts an internal [`ToolDefinition`] into the `rig` tool definition.
fn into_rig_tool(tool: ToolDefinition) -> RigToolDefinition {
    RigToolDefinition {
        name: tool.name,
        description: tool.description,
        parameters: tool.parameters,
    }
}

/// Converts an internal [`ToolChoice`] into the `rig` tool choice.
///
/// [`ToolChoice::Auto`] maps to `None` so the field is omitted from the
/// provider request, which is every provider's default behaviour.
fn into_rig_tool_choice(tool_choice: ToolChoice) -> Option<RigToolChoice> {
    match tool_choice {
        ToolChoice::Auto => None,
        ToolChoice::Required => Some(RigToolChoice::Required),
        ToolChoice::None => Some(RigToolChoice::None),
    }
}

/// Converts a `rig` tool call into the internal [`ToolCall`].
///
/// The provider-specific `call_id` takes precedence over the generic `id` when
/// present, matching how providers correlate tool results.
fn api_tool_call(call: RigToolCall) -> ToolCall {
    ToolCall {
        call_id: call.call_id.unwrap_or(call.id),
        name: call.function.name,
        arguments: call.function.arguments,
    }
}

/// Converts `rig` usage totals into the internal [`Usage`].
///
/// `rig` reports `0` when a provider supplied no figure for a counter, so
/// zeroes map to `None`. The actual cost is derived from the reported input
/// and output tokens and the descriptor's `pricing`; the estimated cost is
/// not set here — up-front estimation belongs to the router, which owns the
/// token estimator.
fn api_usage(usage: RigUsage, pricing: Pricing) -> Usage {
    let reported = |value: u64| (value != 0).then_some(value);
    let input_tokens = reported(usage.input_tokens);
    let output_tokens = reported(usage.output_tokens);
    Usage {
        input_tokens,
        output_tokens,
        cached_tokens: reported(usage.cached_input_tokens),
        reasoning_tokens: reported(usage.reasoning_tokens),
        total_tokens: reported(usage.total_tokens),
        actual_cost: pricing.actual_cost(input_tokens, output_tokens),
        ..Default::default()
    }
}

/// Derives the [`FinishReason`] from a provider's raw response.
///
/// `rig` does not surface the stop reason in its generic response, but every
/// provider serializes it in the raw payload; this probes the field names used
/// by Anthropic (`stop_reason`), OpenAI (`choices[0].finish_reason` or a
/// top-level `finish_reason`) and Ollama (`done_reason`). When no reason is
/// found, the presence of tool calls decides between [`FinishReason::ToolCalls`]
/// and [`FinishReason::Stop`].
fn finish_reason<T>(raw_response: &T, has_tool_calls: bool) -> FinishReason
where
    T: Serialize,
{
    let fallback = if has_tool_calls {
        FinishReason::ToolCalls
    } else {
        FinishReason::Stop
    };
    let Ok(value) = serde_json::to_value(raw_response) else {
        return fallback;
    };
    let reason = value
        .get("stop_reason")
        .or_else(|| value.get("finish_reason"))
        .or_else(|| value.get("done_reason"))
        .or_else(|| value.pointer("/choices/0/finish_reason"))
        .and_then(serde_json::Value::as_str);

    match reason {
        None => fallback,
        Some("end_turn" | "stop" | "stop_sequence") => FinishReason::Stop,
        Some("max_tokens" | "length") => FinishReason::Length,
        Some("tool_use" | "tool_calls" | "function_call") => FinishReason::ToolCalls,
        Some("content_filter" | "refusal") => FinishReason::ContentFilter,
        Some(other) => FinishReason::Other(other.to_string()),
    }
}

/// Builds the `rig` additional parameters from the request's [`ModelParameters`].
///
/// `rig`'s builder has first-class temperature and max-tokens knobs; `top_p`
/// and the provider-specific extras travel as additional parameters merged
/// into the provider request body.
fn additional_params(parameters: &ModelParameters) -> Option<serde_json::Value> {
    let mut extra = parameters.extra.clone();
    if let Some(top_p) = parameters.top_p {
        extra.insert("top_p".to_string(), serde_json::json!(top_p));
    }
    (!extra.is_empty()).then_some(serde_json::Value::Object(extra))
}

/// Maps one `rig` stream item onto the internal [`StreamEvent`] vocabulary.
///
/// Text and reasoning chunks become [`StreamEvent::TextDelta`] and
/// [`StreamEvent::ReasoningDelta`]; a complete reasoning block is replayed as
/// a single reasoning delta. A tool call surfaces twice: a
/// [`StreamEvent::ToolCallStarted`] as soon as `rig` reports its name, and a
/// [`StreamEvent::ToolCallRequested`] once `rig` has buffered the complete
/// call — argument fragments are dropped because partial JSON is unusable
/// until complete. `started_calls` tracks which calls (by `rig`'s internal
/// id) already signalled their start, so each call starts at most once. The
/// provider's final payload yields [`StreamEvent::Usage`], priced via
/// `pricing`, when it reports token usage.
fn map_stream_item<R>(
    item: Result<StreamedAssistantContent<R>, CompletionError>,
    started_calls: &mut BTreeSet<String>,
    pricing: Pricing,
    provider: Provider,
    model: &str,
) -> Option<Result<StreamEvent, ProviderError>>
where
    R: Clone + Unpin + GetTokenUsage,
{
    match item {
        Ok(StreamedAssistantContent::Text(text)) => {
            Some(Ok(StreamEvent::TextDelta { delta: text.text }))
        }
        Ok(StreamedAssistantContent::ToolCall {
            tool_call,
            internal_call_id,
        }) => {
            // a call that arrives complete must not signal a (re)start later
            started_calls.insert(internal_call_id);
            let call = api_tool_call(tool_call);
            tracing::debug!(
                model.provider = %provider,
                model.name = %model,
                tool.name = %call.name,
                "model requested tool call"
            );
            Some(Ok(StreamEvent::ToolCallRequested {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            }))
        }
        Ok(StreamedAssistantContent::ToolCallDelta {
            id,
            internal_call_id,
            content: ToolCallDeltaContent::Name(name),
        }) => started_calls
            .insert(internal_call_id)
            .then_some(Ok(StreamEvent::ToolCallStarted { call_id: id, name })),
        // argument fragments are buffered by `rig` and surface as the
        // complete `ToolCall` above; partial JSON is unusable on its own
        Ok(StreamedAssistantContent::ToolCallDelta {
            content: ToolCallDeltaContent::Delta(_),
            ..
        }) => None,
        Ok(StreamedAssistantContent::Reasoning(reasoning)) => {
            let text = reasoning_text(&reasoning);
            (!text.is_empty()).then_some(Ok(StreamEvent::ReasoningDelta { delta: text }))
        }
        Ok(StreamedAssistantContent::ReasoningDelta { reasoning, .. }) => {
            Some(Ok(StreamEvent::ReasoningDelta { delta: reasoning }))
        }
        Ok(StreamedAssistantContent::Final(raw)) => Some(Ok(StreamEvent::Usage(api_usage(
            raw.token_usage(),
            pricing,
        )))),
        Err(error) => Some(Err(crate::error::provider_error(
            crate::error::category_from_completion(&error),
            provider,
            Some(model.to_string()),
            error.to_string(),
        ))),
    }
}

/// Joins the readable parts of a complete reasoning block into one string.
///
/// Encrypted and redacted parts are skipped: they are provider-opaque
/// payloads, not text a consumer can render.
fn reasoning_text(reasoning: &Reasoning) -> String {
    reasoning
        .content
        .iter()
        .filter_map(|part| match part {
            ReasoningContent::Text { text, .. } => Some(text.as_str()),
            ReasoningContent::Summary(text) => Some(text.as_str()),
            // encrypted/redacted payloads are provider-opaque, and the enum is
            // non-exhaustive: any future kind is unreadable until mapped here
            _ => None,
        })
        .collect()
}

/// Replays a finished completion as a [`ResponseStream`].
///
/// The fallback for models without the `streaming` capability: the full text
/// (when non-empty), each tool call, the usage totals (when reported) and the
/// terminal [`StreamEvent::Done`], in that order.
fn completion_events(response: CompletionResponse) -> ResponseStream {
    let CompletionResponse {
        content,
        tool_calls,
        usage,
        finish_reason: _,
    } = response;

    let text = (!content.is_empty()).then_some(StreamEvent::TextDelta { delta: content });
    let calls = tool_calls
        .into_iter()
        .map(|call| StreamEvent::ToolCallRequested {
            call_id: call.call_id,
            name: call.name,
            arguments: call.arguments,
        });
    let usage = (usage != Usage::default()).then_some(StreamEvent::Usage(usage));

    let events = text
        .into_iter()
        .chain(calls)
        .chain(usage)
        .chain(std::iter::once(StreamEvent::Done))
        .map(Ok);
    ResponseStream::new(futures::stream::iter(events))
}

/// Loads the preamble for the agent by fetching user and session memories from the storage, normalizing the results, and building the preamble string.
async fn load_memories_preamble<S>(
    storage: &S,
    scope: MemoryScope,
    provider: Provider,
    model: &str,
) -> ProviderResult<Option<String>>
where
    S: MemoryStorage,
{
    let user_memories = normalize_memories_result::<S>(
        storage
            .get_user_memories(scope, Some(MEMORIES_MAX_RECORDS))
            .await,
        provider.clone(),
        model,
    )
    .await?;
    let session_memories = normalize_memories_result::<S>(
        storage
            .get_session_memories(scope, Some(MEMORIES_MAX_RECORDS))
            .await,
        provider,
        model,
    )
    .await?;

    Ok(crate::memory::build_preamble(
        &user_memories,
        &session_memories,
    ))
}

/// Normalizes the result of loading memories from the storage,
/// converting any storage errors into a [`ProviderError`] with the appropriate category and context.
async fn normalize_memories_result<S>(
    result: Result<Vec<MemoryRecord>, S::Error>,
    provider: Provider,
    model: &str,
) -> ProviderResult<Vec<MemoryRecord>>
where
    S: MemoryStorage,
{
    match result {
        Ok(memories) => {
            tracing::debug!(
                model.provider = %provider,
                model.name = %model,
                memory.records = memories.len(),
                "loaded memory records from storage"
            );
            Ok(memories)
        }
        Err(err) => {
            tracing::error!(
                model.provider = %provider,
                model.name = %model,
                "failed to load memory records from storage: {err}"
            );
            Err(ProviderError {
                category: ProviderErrorCategory::Storage,
                message: err.to_string(),
                provider,
                model: Some(model.to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pricing with no declared prices, for tests not exercising costs.
    fn no_pricing() -> Pricing {
        Pricing {
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        }
    }

    /// Pricing at $3/Mtok input and $15/Mtok output.
    fn dollar_pricing() -> Pricing {
        Pricing {
            input_cost_per_million_tokens: Some(Decimal::new(3, 0)),
            output_cost_per_million_tokens: Some(Decimal::new(15, 0)),
        }
    }

    #[test]
    fn should_map_system_and_user_messages() {
        assert_eq!(
            into_rig_message(RequestMessage::System {
                content: "be brief".to_string(),
            }),
            RigMessage::system("be brief")
        );
        assert_eq!(
            into_rig_message(RequestMessage::User {
                content: "hello".to_string(),
            }),
            RigMessage::user("hello")
        );
    }

    #[test]
    fn should_map_assistant_message_with_tool_calls() {
        let message = into_rig_message(RequestMessage::Assistant {
            content: "checking".to_string(),
            tool_calls: vec![ToolCall {
                call_id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({ "query": "rust" }),
            }],
        });

        let RigMessage::Assistant { id, content } = message else {
            panic!("expected an assistant message");
        };
        assert_eq!(id, None);
        let items: Vec<_> = content.into_iter().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], AssistantContent::text("checking"));
        let AssistantContent::ToolCall(call) = &items[1] else {
            panic!("expected a tool call");
        };
        assert_eq!(call.id, "call-1");
        assert_eq!(call.function.name, "search");
        assert_eq!(
            call.function.arguments,
            serde_json::json!({"query": "rust"})
        );
    }

    #[test]
    fn should_map_empty_assistant_message_to_empty_text() {
        let message = into_rig_message(RequestMessage::Assistant {
            content: String::new(),
            tool_calls: Vec::new(),
        });

        let RigMessage::Assistant { content, .. } = message else {
            panic!("expected an assistant message");
        };
        assert_eq!(
            content.into_iter().collect::<Vec<_>>(),
            vec![AssistantContent::text("")]
        );
    }

    #[test]
    fn should_map_tool_result_to_user_tool_result_content() {
        let message = into_rig_message(RequestMessage::ToolResult {
            call_id: "call-1".to_string(),
            content: "42".to_string(),
            is_error: false,
        });

        let RigMessage::User { content } = message else {
            panic!("expected a user message");
        };
        let UserContent::ToolResult(result) = content.first() else {
            panic!("expected tool result content");
        };
        assert_eq!(result.id, "call-1");
        assert_eq!(
            result.content.first(),
            ToolResultContent::Text(Text::new("42"))
        );
    }

    #[test]
    fn should_prefer_provider_call_id_when_mapping_tool_calls() {
        let with_call_id = RigToolCall::new(
            "id-1".to_string(),
            ToolFunction::new("search".to_string(), serde_json::json!({})),
        )
        .with_call_id("call-7".to_string());
        assert_eq!(api_tool_call(with_call_id).call_id, "call-7");

        let without_call_id = RigToolCall::new(
            "id-1".to_string(),
            ToolFunction::new("search".to_string(), serde_json::json!({})),
        );
        assert_eq!(api_tool_call(without_call_id).call_id, "id-1");
    }

    #[test]
    fn should_omit_rig_tool_choice_for_auto() {
        assert_eq!(into_rig_tool_choice(ToolChoice::Auto), None);
        assert_eq!(
            into_rig_tool_choice(ToolChoice::Required),
            Some(RigToolChoice::Required)
        );
        assert_eq!(
            into_rig_tool_choice(ToolChoice::None),
            Some(RigToolChoice::None)
        );
    }

    #[test]
    fn should_map_zero_usage_counters_to_none() {
        assert_eq!(api_usage(RigUsage::new(), no_pricing()), Usage::default());

        let usage = api_usage(
            RigUsage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                cached_input_tokens: 3,
                reasoning_tokens: 2,
                ..RigUsage::new()
            },
            no_pricing(),
        );
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(15));
        assert_eq!(usage.cached_tokens, Some(3));
        assert_eq!(usage.reasoning_tokens, Some(2));
        assert_eq!(usage.actual_cost, None);
    }

    #[test]
    fn should_price_usage_from_token_counts() {
        let usage = api_usage(
            RigUsage {
                input_tokens: 1_000_000,
                output_tokens: 200_000,
                ..RigUsage::new()
            },
            dollar_pricing(),
        );
        // 1M input × $3/Mtok + 0.2M output × $15/Mtok = $6
        assert_eq!(usage.actual_cost, Some(Decimal::new(6, 0)));
        assert_eq!(usage.estimated_cost, None);
    }

    #[test]
    fn should_not_price_usage_without_token_counts() {
        assert_eq!(dollar_pricing().actual_cost(None, None), None);
    }

    #[test]
    fn should_not_price_usage_without_prices() {
        assert_eq!(no_pricing().actual_cost(Some(10), Some(5)), None);
    }

    #[test]
    fn should_price_partially_reported_usage() {
        // only the side with both a price and a token count contributes
        assert_eq!(
            dollar_pricing().actual_cost(None, Some(1_000_000)),
            Some(Decimal::new(15, 0))
        );
        let input_only = Pricing {
            output_cost_per_million_tokens: None,
            ..dollar_pricing()
        };
        assert_eq!(
            input_only.actual_cost(Some(2_000_000), Some(1_000_000)),
            Some(Decimal::new(6, 0))
        );
    }

    #[test]
    fn should_price_local_models_at_zero() {
        let free = Pricing {
            input_cost_per_million_tokens: Some(Decimal::ZERO),
            output_cost_per_million_tokens: Some(Decimal::ZERO),
        };
        assert_eq!(free.actual_cost(Some(10), Some(5)), Some(Decimal::ZERO));
    }

    #[test]
    fn should_derive_finish_reason_from_anthropic_stop_reason() {
        let raw = serde_json::json!({ "stop_reason": "max_tokens" });
        assert_eq!(finish_reason(&raw, false), FinishReason::Length);

        let raw = serde_json::json!({ "stop_reason": "tool_use" });
        assert_eq!(finish_reason(&raw, true), FinishReason::ToolCalls);
    }

    #[test]
    fn should_derive_finish_reason_from_openai_choices() {
        let raw = serde_json::json!({ "choices": [{ "finish_reason": "stop" }] });
        assert_eq!(finish_reason(&raw, false), FinishReason::Stop);
    }

    #[test]
    fn should_preserve_unknown_finish_reason() {
        let raw = serde_json::json!({ "stop_reason": "pause_turn" });
        assert_eq!(
            finish_reason(&raw, false),
            FinishReason::Other("pause_turn".to_string())
        );
    }

    #[test]
    fn should_fall_back_to_tool_calls_when_no_reason_is_reported() {
        let raw = serde_json::json!({});
        assert_eq!(finish_reason(&raw, true), FinishReason::ToolCalls);
        assert_eq!(finish_reason(&raw, false), FinishReason::Stop);
    }

    #[test]
    fn should_merge_top_p_into_additional_params() {
        let parameters = ModelParameters {
            top_p: Some(0.9),
            ..Default::default()
        };
        let params = additional_params(&parameters).expect("top_p produces params");
        assert!((params["top_p"].as_f64().unwrap() - 0.9).abs() < 1e-6);

        assert_eq!(additional_params(&ModelParameters::default()), None);
    }

    /// Raw streaming payload reporting fixed token usage for the tests below.
    #[derive(Clone)]
    struct RawWithUsage;

    impl GetTokenUsage for RawWithUsage {
        fn token_usage(&self) -> RigUsage {
            RigUsage {
                input_tokens: 5,
                output_tokens: 7,
                total_tokens: 12,
                ..RigUsage::new()
            }
        }
    }

    /// Maps one item with fresh tool-call state and no pricing.
    fn map_item(
        item: Result<StreamedAssistantContent<RawWithUsage>, CompletionError>,
    ) -> Option<Result<StreamEvent, ProviderError>> {
        map_stream_item(
            item,
            &mut BTreeSet::new(),
            no_pricing(),
            Provider::Anthropic,
            "model",
        )
    }

    #[test]
    fn should_map_stream_text_to_text_delta() {
        let item = map_item(Ok(StreamedAssistantContent::text("Hello")));
        assert_eq!(
            item,
            Some(Ok(StreamEvent::TextDelta {
                delta: "Hello".to_string(),
            }))
        );
    }

    #[test]
    fn should_map_stream_tool_call_to_tool_call_requested() {
        let tool_call = RigToolCall::new(
            "call-1".to_string(),
            ToolFunction::new("search".to_string(), serde_json::json!({ "query": "rust" })),
        );
        let item = map_item(Ok(StreamedAssistantContent::ToolCall {
            tool_call,
            internal_call_id: "internal".to_string(),
        }));
        assert_eq!(
            item,
            Some(Ok(StreamEvent::ToolCallRequested {
                call_id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({ "query": "rust" }),
            }))
        );
    }

    #[test]
    fn should_emit_tool_call_started_once_per_call() {
        let mut started_calls = BTreeSet::new();
        let name_delta = || {
            Ok(StreamedAssistantContent::<RawWithUsage>::ToolCallDelta {
                id: "call-1".to_string(),
                internal_call_id: "internal".to_string(),
                content: ToolCallDeltaContent::Name("search".to_string()),
            })
        };

        let first = map_stream_item(
            name_delta(),
            &mut started_calls,
            no_pricing(),
            Provider::Anthropic,
            "model",
        );
        assert_eq!(
            first,
            Some(Ok(StreamEvent::ToolCallStarted {
                call_id: "call-1".to_string(),
                name: "search".to_string(),
            }))
        );

        let repeated = map_stream_item(
            name_delta(),
            &mut started_calls,
            no_pricing(),
            Provider::Anthropic,
            "model",
        );
        assert_eq!(repeated, None);
    }

    #[test]
    fn should_not_start_a_tool_call_that_arrived_complete() {
        let mut started_calls = BTreeSet::new();
        let tool_call = RigToolCall::new(
            "call-1".to_string(),
            ToolFunction::new("search".to_string(), serde_json::json!({})),
        );
        let complete = map_stream_item::<RawWithUsage>(
            Ok(StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id: "internal".to_string(),
            }),
            &mut started_calls,
            no_pricing(),
            Provider::Anthropic,
            "model",
        );
        assert!(matches!(
            complete,
            Some(Ok(StreamEvent::ToolCallRequested { .. }))
        ));

        // a stray name fragment for the same call must not signal a start
        let late_name = map_stream_item::<RawWithUsage>(
            Ok(StreamedAssistantContent::ToolCallDelta {
                id: "call-1".to_string(),
                internal_call_id: "internal".to_string(),
                content: ToolCallDeltaContent::Name("search".to_string()),
            }),
            &mut started_calls,
            no_pricing(),
            Provider::Anthropic,
            "model",
        );
        assert_eq!(late_name, None);
    }

    #[test]
    fn should_drop_tool_call_argument_fragments() {
        let item = map_item(Ok(StreamedAssistantContent::ToolCallDelta {
            id: "call-1".to_string(),
            internal_call_id: "internal".to_string(),
            content: ToolCallDeltaContent::Delta(r#"{"que"#.to_string()),
        }));
        assert_eq!(item, None);
    }

    #[test]
    fn should_map_stream_reasoning_delta() {
        let item = map_item(Ok(StreamedAssistantContent::ReasoningDelta {
            id: None,
            reasoning: "thinking".to_string(),
        }));
        assert_eq!(
            item,
            Some(Ok(StreamEvent::ReasoningDelta {
                delta: "thinking".to_string(),
            }))
        );
    }

    #[test]
    fn should_map_reasoning_block_to_single_delta_of_readable_parts() {
        // `Reasoning` is non-exhaustive: build via `new` and replace the parts
        let mut reasoning = Reasoning::new("");
        reasoning.content = vec![
            ReasoningContent::Text {
                text: "step one; ".to_string(),
                signature: None,
            },
            ReasoningContent::Encrypted("opaque".to_string()),
            ReasoningContent::Summary("step two".to_string()),
        ];
        let item = map_item(Ok(StreamedAssistantContent::Reasoning(reasoning)));
        assert_eq!(
            item,
            Some(Ok(StreamEvent::ReasoningDelta {
                delta: "step one; step two".to_string(),
            }))
        );
    }

    #[test]
    fn should_drop_reasoning_block_without_readable_parts() {
        let mut reasoning = Reasoning::new("");
        reasoning.content = vec![ReasoningContent::Redacted {
            data: "opaque".to_string(),
        }];
        let item = map_item(Ok(StreamedAssistantContent::Reasoning(reasoning)));
        assert_eq!(item, None);
    }

    #[test]
    fn should_map_stream_final_to_usage_event() {
        let item = map_item(Ok(StreamedAssistantContent::Final(RawWithUsage)));
        let Some(Ok(StreamEvent::Usage(usage))) = item else {
            panic!("expected a usage event");
        };
        assert_eq!(usage.input_tokens, Some(5));
        assert_eq!(usage.output_tokens, Some(7));
        assert_eq!(usage.total_tokens, Some(12));
        assert_eq!(usage.actual_cost, None);
    }

    #[test]
    fn should_price_stream_usage_event() {
        let item = map_stream_item(
            Ok(StreamedAssistantContent::Final(RawWithUsage)),
            &mut BTreeSet::new(),
            dollar_pricing(),
            Provider::Anthropic,
            "model",
        );
        let Some(Ok(StreamEvent::Usage(usage))) = item else {
            panic!("expected a usage event");
        };
        // 5 input × $3/Mtok + 7 output × $15/Mtok = $0.00012
        assert_eq!(usage.actual_cost, Some(Decimal::new(12, 5)));
    }

    #[test]
    fn should_map_stream_error_to_provider_error() {
        let item = map_item(Err(CompletionError::ProviderError(
            "rate limit reached".to_string(),
        )));
        let Some(Err(error)) = item else {
            panic!("expected an error item");
        };
        assert_eq!(error.category, ProviderErrorCategory::RateLimit);
        assert_eq!(error.provider, Provider::Anthropic);
        assert_eq!(error.model.as_deref(), Some("model"));
    }

    #[tokio::test]
    async fn should_replay_completion_as_stream() {
        use futures::StreamExt as _;

        let response = CompletionResponse {
            content: "Hello".to_string(),
            tool_calls: vec![ToolCall {
                call_id: "call-1".to_string(),
                name: "search".to_string(),
                arguments: serde_json::json!({ "query": "rust" }),
            }],
            usage: Usage {
                input_tokens: Some(5),
                ..Default::default()
            },
            finish_reason: FinishReason::Stop,
        };

        let events: Vec<_> = completion_events(response).collect().await;
        assert_eq!(
            events,
            vec![
                Ok(StreamEvent::TextDelta {
                    delta: "Hello".to_string(),
                }),
                Ok(StreamEvent::ToolCallRequested {
                    call_id: "call-1".to_string(),
                    name: "search".to_string(),
                    arguments: serde_json::json!({ "query": "rust" }),
                }),
                Ok(StreamEvent::Usage(Usage {
                    input_tokens: Some(5),
                    ..Default::default()
                })),
                Ok(StreamEvent::Done),
            ]
        );
    }

    /// Memory backend returning a fixed set of user memories and no session
    /// memories, for asserting how the preamble is assembled.
    struct StubStorage {
        user: Vec<MemoryRecord>,
    }

    impl MemoryStorage for StubStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(self.user.clone())
        }

        async fn get_user_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    /// A descriptor for an OpenAI model with system-prompt support, enough to
    /// build an agent offline.
    fn descriptor() -> ModelDescriptor {
        use smista_core::model::{ModelAuthRequirement, ModelCapabilities};

        ModelDescriptor {
            provider: Provider::OpenAI,
            model: "gpt-test".to_string(),
            display_name: None,
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                json_output: true,
                system_prompt: true,
                images: false,
                reasoning: false,
                memory: true,
            },
            max_context_tokens: 8_192,
            max_output_tokens: Some(4_096),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    /// Builds an agent over an offline OpenAI client with the given base
    /// preamble, stored user memories and extra segments, and returns the
    /// resulting system prompt.
    async fn assembled_preamble(
        preamble: &str,
        user: Vec<MemoryRecord>,
        preamble_segments: Vec<String>,
    ) -> String {
        use rig_core::providers::openai::client::Client as OpenAIClient;
        use uuid::Uuid;

        let client = OpenAIClient::new("test-key").expect("client builds offline");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor(),
            preamble: preamble.to_string(),
            preamble_segments,
            storage: Arc::new(StubStorage { user }),
            scope: MemoryScope {
                user_id: Uuid::now_v7(),
                session_id: Uuid::now_v7(),
            },
        })
        .await
        .expect("agent builds");

        agent.agent.preamble.expect("a preamble was set")
    }

    #[tokio::test]
    async fn should_leave_preamble_unchanged_without_segments() {
        let preamble = assembled_preamble("base", Vec::new(), Vec::new()).await;
        assert_eq!(preamble, "base");
    }

    #[tokio::test]
    async fn should_append_segments_in_order_each_as_its_own_block() {
        let preamble = assembled_preamble(
            "base",
            Vec::new(),
            vec!["first".to_string(), "second".to_string()],
        )
        .await;
        assert_eq!(preamble, "base\nfirst\nsecond");
    }

    #[tokio::test]
    async fn should_append_segments_after_the_memory() {
        let memory = vec![MemoryRecord {
            handle: "h1".to_string(),
            key: None,
            content: "MEMORY_SENTINEL".to_string(),
        }];
        let preamble = assembled_preamble(
            "BASE_SENTINEL",
            memory,
            vec!["SEGMENT_SENTINEL".to_string()],
        )
        .await;

        let base = preamble.find("BASE_SENTINEL").expect("base present");
        let mem = preamble.find("MEMORY_SENTINEL").expect("memory present");
        let segment = preamble.find("SEGMENT_SENTINEL").expect("segment present");
        assert!(base < mem, "base must precede the memory");
        assert!(mem < segment, "the memory must precede the segment");
    }

    #[tokio::test]
    async fn should_omit_empty_text_and_usage_when_replaying_completion() {
        use futures::StreamExt as _;

        let response = CompletionResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            usage: Usage::default(),
            finish_reason: FinishReason::Stop,
        };

        let events: Vec<_> = completion_events(response).collect().await;
        assert_eq!(events, vec![Ok(StreamEvent::Done)]);
    }
}
