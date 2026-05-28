//! Events emitted while a model produces a response.
//!
//! A [`StreamEvent`] is one item in the stream of server-sent events a model
//! invocation produces: incremental text, tool-call activity, usage totals,
//! errors and a terminal marker. The trait that *produces* these lives in
//! smista-providers; only the value types belong here, shared with trace
//! and the web API.
//!
//! Events are internally tagged with a `type` field, matching the SSE wire
//! shape — for example `{"type":"text_delta","delta":"He"}`.
//!
//! # Examples
//!
//! ```
//! use smista_core::stream::StreamEvent;
//!
//! let event = StreamEvent::TextDelta {
//!     delta: "Hello".to_string(),
//! };
//! let json = serde_json::to_string(&event).unwrap();
//! assert_eq!(json, r#"{"type":"text_delta","delta":"Hello"}"#);
//! ```

use serde::{Deserialize, Serialize};

use crate::usage::Usage;

/// A single event in a model's response stream.
///
/// Serialized with an internally tagged `type` discriminator in snake_case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum StreamEvent {
    /// An incremental chunk of generated text.
    TextDelta {
        /// The text appended by this event.
        delta: String,
    },
    /// The model requested a tool call.
    ToolCallRequested {
        /// Identifier correlating this call with its later [`Self::ToolResult`].
        call_id: String,
        /// Name of the tool to invoke.
        name: String,
        /// Arguments for the tool, as a provider-agnostic JSON value.
        arguments: serde_json::Value,
    },
    /// A tool call needs user approval before it can run.
    ApprovalRequired {
        /// Identifier the approval decision refers to.
        approval_id: String,
    },
    /// The result of a tool call, correlated by `call_id`.
    ToolResult {
        /// Identifier matching the originating [`Self::ToolCallRequested`].
        call_id: String,
        /// The tool's output.
        content: String,
        /// Whether the tool call failed.
        #[serde(default)]
        is_error: bool,
    },
    /// Token usage and cost totals for the invocation.
    Usage(Usage),
    /// A terminal error ended the stream.
    Error {
        /// Machine-readable error code.
        code: String,
        /// Human-readable error message.
        message: String,
    },
    /// The stream completed successfully.
    Done,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_tag_text_delta() {
        let event = StreamEvent::TextDelta {
            delta: "Hello".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "text_delta", "delta": "Hello" })
        );
    }

    #[test]
    fn should_tag_done_as_unit_variant() {
        assert_eq!(
            serde_json::to_value(StreamEvent::Done).unwrap(),
            serde_json::json!({ "type": "done" })
        );
    }

    #[test]
    fn should_flatten_usage_under_tag() {
        let event = StreamEvent::Usage(Usage {
            input_tokens: Some(10),
            ..Default::default()
        });
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({ "type": "usage", "input_tokens": 10 })
        );
    }

    #[test]
    fn should_default_tool_result_error_flag() {
        let json = r#"{"type":"tool_result","call_id":"c1","content":"ok"}"#;
        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert_eq!(
            event,
            StreamEvent::ToolResult {
                call_id: "c1".to_string(),
                content: "ok".to_string(),
                is_error: false,
            }
        );
    }

    #[test]
    fn should_roundtrip_tool_call_requested() {
        let event = StreamEvent::ToolCallRequested {
            call_id: "c1".to_string(),
            name: "search".to_string(),
            arguments: serde_json::json!({ "query": "rust" }),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<StreamEvent>(&json).unwrap(), event);
    }

    #[test]
    fn should_roundtrip_error() {
        let event = StreamEvent::Error {
            code: "rate_limited".to_string(),
            message: "slow down".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<StreamEvent>(&json).unwrap(), event);
    }
}
