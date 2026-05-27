//! Response body wrapping an execution trace.
//!
//! [`TraceResponse`] is returned by both `GET /sessions/{id}/traces/latest` and
//! `GET /sessions/{id}/traces/{trace_id}`, wrapping the [`Trace`] under a
//! `trace` key.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::TraceResponse;
//! use smista_core::intent::TaskIntent;
//! use smista_core::model::Provider;
//! use smista_core::trace::Trace;
//! use uuid::Uuid;
//!
//! let response = TraceResponse {
//!     trace: Trace {
//!         id: "trace:xyz".to_string(),
//!         session_id: Uuid::nil(),
//!         task_type: TaskIntent::Edit,
//!         provider: Provider::Anthropic,
//!         model: "claude-sonnet".to_string(),
//!         matched_rule: None,
//!         events: Vec::new(),
//!     },
//! };
//! assert_eq!(response.trace.id, "trace:xyz");
//! ```

use serde::{Deserialize, Serialize};

use crate::trace::Trace;

/// Envelope wrapping a [`Trace`] under a `trace` key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceResponse {
    /// The execution trace.
    pub trace: Trace,
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::intent::TaskIntent;
    use crate::model::Provider;

    #[test]
    fn should_roundtrip_trace_response() {
        let response = TraceResponse {
            trace: Trace {
                id: "trace:xyz".to_string(),
                session_id: Uuid::nil(),
                task_type: TaskIntent::Edit,
                provider: Provider::Anthropic,
                model: "claude-sonnet".to_string(),
                matched_rule: Some("rule".to_string()),
                events: Vec::new(),
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<TraceResponse>(&json).unwrap(),
            response
        );
    }

    #[test]
    fn should_nest_trace_under_trace_key() {
        let response = TraceResponse {
            trace: Trace {
                id: "trace:xyz".to_string(),
                session_id: Uuid::nil(),
                task_type: TaskIntent::Edit,
                provider: Provider::Anthropic,
                model: "claude-sonnet".to_string(),
                matched_rule: None,
                events: Vec::new(),
            },
        };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["trace"]["id"], "trace:xyz");
        assert_eq!(value["trace"]["task_type"], "edit");
    }
}
