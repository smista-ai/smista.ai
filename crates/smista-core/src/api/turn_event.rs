//! Streaming events for a turn delivered over Server-Sent Events.
//!
//! `/stream`, and `/continue` when the client asks for streaming, deliver a turn
//! as a sequence of [`TurnEvent`]s: incremental text and reasoning, tool-call
//! activity, usage, and a terminal [`TurnEnd`](TurnEvent::TurnEnd) carrying the
//! turn's [`TurnResponse`]. The terminal event names the outcome, so the client
//! never has to infer whether a turn finished or paused for a continuation.
//!
//! This is the web wire vocabulary, distinct from the provider-internal
//! [`StreamEvent`](crate::stream::StreamEvent): the router maps the provider's
//! stream onto these events, enriching a tool call with the policy's
//! `requires_approval` and appending the terminal `turn_end`.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::TurnEvent;
//!
//! let event = TurnEvent::TextDelta { delta: "Hello".to_string() };
//! let json = serde_json::to_string(&event).unwrap();
//! assert_eq!(json, r#"{"type":"text_delta","delta":"Hello"}"#);
//! ```

use serde::{Deserialize, Serialize};

use super::{ToolApproval, TurnResponse};
use crate::usage::Usage;

/// A single event in a streamed turn.
///
/// Serialized with an internally tagged `type` discriminator in snake_case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum TurnEvent {
    /// An incremental chunk of generated text.
    TextDelta {
        /// The text appended by this event.
        delta: String,
    },
    /// An incremental chunk of reasoning text, for models that stream it.
    ReasoningDelta {
        /// The reasoning text appended by this event.
        delta: String,
    },
    /// A tool call is forming: its name is known, its arguments still streaming.
    ToolCallStarted {
        /// Identifier correlating this event with its later
        /// [`ToolCallRequested`](Self::ToolCallRequested).
        call_id: String,
        /// Name of the tool being called.
        name: String,
    },
    /// The model requested a tool call the client must run.
    ToolCallRequested {
        /// Identifier correlating this call with its later result.
        call_id: String,
        /// Name of the tool to invoke.
        name: String,
        /// Arguments for the tool, as a provider-agnostic JSON value.
        arguments: serde_json::Value,
        /// Whether the user must approve the call before it runs.
        requires_approval: ToolApproval,
    },
    /// Token usage and cost totals for the turn.
    Usage(Usage),
    /// The terminal event: the turn's outcome, carrying the same
    /// [`TurnResponse`] the buffered call returns (its `status` included).
    TurnEnd(Box<TurnResponse>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_tag_text_delta() {
        let event = TurnEvent::TextDelta {
            delta: "Hello".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "text_delta", "delta": "Hello" })
        );
    }

    #[test]
    fn should_carry_requires_approval_on_tool_call() {
        let event = TurnEvent::ToolCallRequested {
            call_id: "c1".to_string(),
            name: "shell".to_string(),
            arguments: serde_json::json!({ "command": "cargo test" }),
            requires_approval: ToolApproval::Ask,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "tool_call_requested");
        assert_eq!(value["requires_approval"], "ask");
    }

    #[test]
    fn should_flatten_turn_response_under_turn_end() {
        let event = TurnEvent::TurnEnd(Box::new(TurnResponse::AwaitingTool {
            tool_requests: Vec::new(),
            trace_id: "trace:xyz".to_string(),
        }));
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "turn_end");
        assert_eq!(value["status"], "awaiting_tool");
        assert_eq!(serde_json::from_value::<TurnEvent>(value).unwrap(), event);
    }
}
