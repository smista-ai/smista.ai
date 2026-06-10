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
    AssistantContent, Message as RigMessage, Text, ToolCall as RigToolCall,
    ToolChoice as RigToolChoice, ToolFunction, ToolResult as RigToolResult, ToolResultContent,
    UserContent,
};
use rig_core::streaming::StreamedAssistantContent;
use serde::Serialize;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::{ModelParameters, Provider};
use smista_core::stream::StreamEvent;
use smista_core::usage::Usage;

use crate::ProviderResult;
use crate::api::{
    CompletionRequest, CompletionResponse, FinishReason, RequestMessage, ResponseStream, ToolCall,
    ToolChoice, ToolDefinition,
};
use crate::memory::{MemoryRecord, MemoryStorage, MemoryTool};

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
    /// The name of the model to use for the agent's reasoning and tool execution.
    pub model: String,
    /// A preamble to inject to give instructions to the agent about what it should do.
    pub preamble: String,
    /// The provider configuration for the agent.
    pub provider: Provider,
    /// The memory storage to use for the agent's memory operations.
    pub storage: Arc<S>,
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
    /// Names of the tools the agent executes itself (the memory tool); calls to
    /// any other tool are returned to the caller for router mediation.
    internal_tools: BTreeSet<String>,
    model: String,
    provider: Provider,
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
            model,
            preamble,
            provider,
            storage,
        }: AgentArgs<C, S>,
    ) -> ProviderResult<Self>
    where
        S: MemoryStorage + 'static,
    {
        // load preamble from memory storage
        tracing::debug!(
            "loading preamble from memory storage for {model} with provider {provider}"
        );
        let memory_preamble = load_memories_preamble(storage.as_ref(), provider, &model).await?;

        // load memory tool
        let memory_tool = MemoryTool::new(storage.clone());

        // build agent
        tracing::debug!("Creating agent with provider: {provider}, model: {model}",);
        let builder = completion_model
            .agent(model.clone())
            .preamble(&preamble)
            .tool(memory_tool);
        // `preamble` replaces the system prompt, so the memory preamble must be
        // appended rather than set or it would wipe the base preamble.
        let builder = match memory_preamble {
            Some(memories) => builder.append_preamble(&memories),
            None => builder,
        };
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
                    provider,
                    Some(model.clone()),
                    format!("failed to enumerate agent tools: {error}"),
                )
            })?
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        tracing::debug!("Agent created successfully");

        Ok(Self {
            agent,
            internal_tools,
            model,
            provider,
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

            let all_internal = !tool_calls.is_empty()
                && tool_calls
                    .iter()
                    .all(|call| self.internal_tools.contains(&call.function.name));
            if !all_internal {
                return Ok(CompletionResponse {
                    finish_reason: finish_reason(&response.raw_response, !tool_calls.is_empty()),
                    content,
                    tool_calls: tool_calls.into_iter().map(api_tool_call).collect(),
                    usage: api_usage(usage),
                });
            }

            // every call in this turn is agent-internal: execute them and feed
            // the results back to the model as the next turn
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
    pub async fn stream(&self, request: CompletionRequest) -> ProviderResult<ResponseStream>
    where
        C::CompletionModel: 'static,
    {
        let CompletionRequest {
            messages,
            parameters,
            tools,
            tool_choice,
        } = request;

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

        let provider = self.provider;
        let model = self.model.clone();
        let events = stream
            .filter_map(move |item| futures::future::ready(map_stream_item(item, provider, &model)))
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
            });

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

        tracing::debug!(
            "executing internal tool call `{name}`",
            name = call.function.name
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

    /// Builds a [`ProviderError`] carrying this agent's provider and model context.
    fn error(&self, category: ProviderErrorCategory, message: impl Into<String>) -> ProviderError {
        crate::error::provider_error(category, self.provider, Some(self.model.clone()), message)
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
/// zeroes map to `None`. Costs are not computed here (#14).
fn api_usage(usage: RigUsage) -> Usage {
    let reported = |value: u64| (value != 0).then_some(value);
    Usage {
        input_tokens: reported(usage.input_tokens),
        output_tokens: reported(usage.output_tokens),
        cached_tokens: reported(usage.cached_input_tokens),
        reasoning_tokens: reported(usage.reasoning_tokens),
        total_tokens: reported(usage.total_tokens),
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
/// Text chunks become [`StreamEvent::TextDelta`], complete tool calls become
/// [`StreamEvent::ToolCallRequested`], and the provider's final payload yields
/// [`StreamEvent::Usage`] when it reports token usage. Partial tool-call and
/// reasoning fragments are dropped: tool-call arguments are only valid once
/// complete, and reasoning deltas have no core event yet (#14).
fn map_stream_item<R>(
    item: Result<StreamedAssistantContent<R>, CompletionError>,
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
        Ok(StreamedAssistantContent::ToolCall { tool_call, .. }) => {
            let call = api_tool_call(tool_call);
            Some(Ok(StreamEvent::ToolCallRequested {
                call_id: call.call_id,
                name: call.name,
                arguments: call.arguments,
            }))
        }
        Ok(StreamedAssistantContent::Final(raw)) => raw
            .token_usage()
            .map(|usage| Ok(StreamEvent::Usage(api_usage(usage)))),
        Ok(
            StreamedAssistantContent::ToolCallDelta { .. }
            | StreamedAssistantContent::Reasoning(_)
            | StreamedAssistantContent::ReasoningDelta { .. },
        ) => None,
        Err(error) => Some(Err(crate::error::provider_error(
            crate::error::category_from_completion(&error),
            provider,
            Some(model.to_string()),
            error.to_string(),
        ))),
    }
}

/// Loads the preamble for the agent by fetching user and session memories from the storage, normalizing the results, and building the preamble string.
async fn load_memories_preamble<S>(
    storage: &S,
    provider: Provider,
    model: &str,
) -> ProviderResult<Option<String>>
where
    S: MemoryStorage,
{
    let user_memories = normalize_memories_result::<S>(
        storage.get_user_memories(Some(MEMORIES_MAX_RECORDS)).await,
        provider,
        model,
    )
    .await?;
    let session_memories = normalize_memories_result::<S>(
        storage
            .get_session_memories(Some(MEMORIES_MAX_RECORDS))
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
            tracing::debug!("loaded {} memory records from storage", memories.len());
            Ok(memories)
        }
        Err(err) => {
            tracing::error!("failed to load memory records from storage: {err}");
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
        assert_eq!(api_usage(RigUsage::new()), Usage::default());

        let usage = api_usage(RigUsage {
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            cached_input_tokens: 3,
            reasoning_tokens: 2,
            ..RigUsage::new()
        });
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(15));
        assert_eq!(usage.cached_tokens, Some(3));
        assert_eq!(usage.reasoning_tokens, Some(2));
        assert_eq!(usage.actual_cost, None);
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
        fn token_usage(&self) -> Option<RigUsage> {
            Some(RigUsage {
                input_tokens: 5,
                output_tokens: 7,
                total_tokens: 12,
                ..RigUsage::new()
            })
        }
    }

    #[test]
    fn should_map_stream_text_to_text_delta() {
        let item = map_stream_item::<RawWithUsage>(
            Ok(StreamedAssistantContent::text("Hello")),
            Provider::Anthropic,
            "model",
        );
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
        let item = map_stream_item::<RawWithUsage>(
            Ok(StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id: "internal".to_string(),
            }),
            Provider::Anthropic,
            "model",
        );
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
    fn should_map_stream_final_to_usage_event() {
        let item = map_stream_item(
            Ok(StreamedAssistantContent::Final(RawWithUsage)),
            Provider::Anthropic,
            "model",
        );
        let Some(Ok(StreamEvent::Usage(usage))) = item else {
            panic!("expected a usage event");
        };
        assert_eq!(usage.input_tokens, Some(5));
        assert_eq!(usage.output_tokens, Some(7));
        assert_eq!(usage.total_tokens, Some(12));
    }

    #[test]
    fn should_map_stream_error_to_provider_error() {
        let item = map_stream_item::<RawWithUsage>(
            Err(CompletionError::ProviderError(
                "rate limit reached".to_string(),
            )),
            Provider::Anthropic,
            "model",
        );
        let Some(Err(error)) = item else {
            panic!("expected an error item");
        };
        assert_eq!(error.category, ProviderErrorCategory::RateLimit);
        assert_eq!(error.provider, Provider::Anthropic);
        assert_eq!(error.model.as_deref(), Some("model"));
    }

    #[test]
    fn should_drop_stream_fragments_without_core_events() {
        let item = map_stream_item::<RawWithUsage>(
            Ok(StreamedAssistantContent::ReasoningDelta {
                id: None,
                reasoning: "thinking".to_string(),
            }),
            Provider::Anthropic,
            "model",
        );
        assert_eq!(item, None);
    }
}
