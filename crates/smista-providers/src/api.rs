//! Request and response vocabulary for the [`Model`] interface.
//!
//! These types are the provider-agnostic shapes that routing builds to invoke a
//! model and reads back from it. They deliberately do **not** reuse the lean
//! core `Message`: a request turn must be able to carry assistant tool-call
//! requests and tool results correlated by `call_id`, which core `Message`
//! omits. Where a value already has a canonical home in `smista-core` —
//! [`ModelParameters`], [`Usage`], [`StreamEvent`], [`ProviderError`] — it is
//! reused rather than redefined here.
//!
//! [`Model`]: crate::model::Model

use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use smista_core::error::ProviderError;
use smista_core::model::ModelParameters;
use smista_core::stream::StreamEvent;
use smista_core::usage::Usage;

/// A single request sent to a model.
///
/// Carries the conversation so far, the generation parameters to apply, and the
/// tools the model is allowed to call. An empty [`Self::tools`] means the model
/// is offered no tools regardless of [`Self::tool_choice`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompletionRequest {
    /// The conversation turns, oldest first.
    pub messages: Vec<RequestMessage>,
    /// Sampling and generation parameters applied to this invocation.
    pub parameters: ModelParameters,
    /// Tool definitions offered to the model; empty means no tools.
    pub tools: Vec<ToolDefinition>,
    /// How the model is permitted to use the offered tools.
    pub tool_choice: ToolChoice,
}

/// A single conversation turn on the request side.
///
/// Richer than core `Message`: assistant turns can carry the tool calls the
/// model previously requested, and a dedicated [`Self::ToolResult`] variant
/// feeds a tool's output back, correlated to its call by `call_id`.
#[derive(Debug, Clone, PartialEq)]
pub enum RequestMessage {
    /// A system instruction that frames the conversation.
    System {
        /// The instruction text.
        content: String,
    },
    /// A turn authored by the end user.
    User {
        /// The user's message text.
        content: String,
    },
    /// A turn authored by the model, optionally requesting tool calls.
    Assistant {
        /// The assistant's message text; may be empty when only tools are
        /// requested.
        content: String,
        /// Tool calls the assistant requested on this turn.
        tool_calls: Vec<ToolCall>,
    },
    /// The result of a previously requested tool call.
    ToolResult {
        /// Identifier matching the originating [`ToolCall::call_id`].
        call_id: String,
        /// The tool's output.
        content: String,
        /// Whether the tool call failed.
        is_error: bool,
    },
}

/// A completion returned by a model.
///
/// Pairs the model's final text with any tool calls it issued for the router to
/// mediate, plus the usage totals and the reason generation stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionResponse {
    /// The model's final generated text.
    pub content: String,
    /// Tool calls the model requested for the router to mediate.
    pub tool_calls: Vec<ToolCall>,
    /// Token usage and cost totals for the invocation.
    pub usage: Usage,
    /// Why the model stopped generating.
    pub finish_reason: FinishReason,
}

/// A tool offered to a model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    /// The tool's unique name, as referenced by a [`ToolCall::name`].
    pub name: String,
    /// A human-readable description the model uses to decide when to call it.
    pub description: String,
    /// JSON Schema describing the tool's arguments.
    pub parameters: serde_json::Value,
}

/// A model's request to invoke a tool.
///
/// Issued by the model on the response side and echoed back on an
/// [`RequestMessage::Assistant`] turn; its [`Self::call_id`] correlates it with
/// the [`RequestMessage::ToolResult`] that carries the outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// Identifier correlating this call with its later tool result.
    pub call_id: String,
    /// Name of the tool to invoke; matches a [`ToolDefinition::name`].
    pub name: String,
    /// Arguments for the call, as a provider-agnostic JSON value.
    pub arguments: serde_json::Value,
}

/// How a model is permitted to use the tools offered to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolChoice {
    /// The model decides whether and which tools to call.
    #[default]
    Auto,
    /// The model must call at least one tool.
    Required,
    /// The model must not call any tool.
    None,
}

/// Why a model stopped generating a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// The model reached a natural stopping point.
    Stop,
    /// Generation hit the output token limit.
    Length,
    /// The model stopped to request one or more tool calls.
    ToolCalls,
    /// Output was withheld by a content filter.
    ContentFilter,
    /// A provider-specific reason that maps to none of the above.
    Other(String),
}

/// A stream of response events from a model invocation.
///
/// A newtype over a boxed [`futures::Stream`] so the underlying transport (an
/// in-process channel, an HTTP response body, etc) is not pinned into the public
/// API. Each item is either a [`StreamEvent`] or a transport/terminal
/// [`ProviderError`]; core's [`StreamEvent::Error`] variant is reserved for the
/// web SSE wire shape and is not produced here.
pub struct ResponseStream(Pin<Box<dyn Stream<Item = Result<StreamEvent, ProviderError>> + Send>>);

impl ResponseStream {
    /// Wraps a stream of response events into a [`ResponseStream`].
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<StreamEvent, ProviderError>> + Send + 'static,
    {
        Self(Box::pin(stream))
    }
}

impl std::fmt::Debug for ResponseStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ResponseStream").finish_non_exhaustive()
    }
}

impl Stream for ResponseStream {
    type Item = Result<StreamEvent, ProviderError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.as_mut().poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use smista_core::error::ProviderErrorCategory;
    use smista_core::model::Provider;

    use super::*;

    #[test]
    fn should_default_tool_choice_to_auto() {
        assert_eq!(ToolChoice::default(), ToolChoice::Auto);
    }

    #[test]
    fn should_default_request_to_empty() {
        let request = CompletionRequest::default();
        assert!(request.messages.is_empty());
        assert!(request.tools.is_empty());
        assert_eq!(request.tool_choice, ToolChoice::Auto);
        assert_eq!(request.parameters, ModelParameters::default());
    }

    #[tokio::test]
    async fn should_yield_wrapped_events_in_order() {
        let events = vec![
            Ok(StreamEvent::TextDelta {
                delta: "Hello".to_string(),
            }),
            Err(ProviderError {
                category: ProviderErrorCategory::Timeout,
                message: "timed out".to_string(),
                provider: Provider::Ollama,
                model: None,
            }),
        ];
        let mut stream = ResponseStream::new(futures::stream::iter(events.clone()));

        let collected: Vec<_> = (&mut stream).collect().await;
        assert_eq!(collected, events);
    }

    #[test]
    fn should_render_debug_without_inner_stream() {
        let stream = ResponseStream::new(futures::stream::empty());
        assert_eq!(format!("{stream:?}"), "ResponseStream(..)");
    }
}
