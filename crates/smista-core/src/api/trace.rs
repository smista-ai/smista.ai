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
//! use smista_core::trace::Trace;
//! use uuid::Uuid;
//!
//! let response = TraceResponse {
//!     trace: Trace {
//!         session_id: Uuid::nil(),
//!         events: Vec::new(),
//!     },
//! };
//! assert!(response.trace.events.is_empty());
//! ```

use serde::{Deserialize, Serialize};

use crate::trace::Trace;

/// Envelope wrapping a [`Trace`] under a `trace` key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct TraceResponse {
    /// The execution trace.
    pub trace: Trace,
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::intent::TaskIntent;
    use crate::model::Provider;
    use crate::trace::{TraceEvent, TraceEventType};

    fn trace() -> Trace {
        Trace {
            session_id: Uuid::nil(),
            events: vec![TraceEvent {
                event_type: TraceEventType::RoutingDecision,
                task_type: TaskIntent::Edit,
                provider: Provider::Anthropic,
                model: "claude-sonnet".to_string(),
                matched_rule: Some("rule".to_string()),
                created_at: Utc::now(),
                payload: serde_json::json!({ "step": 1 }),
            }],
        }
    }

    #[test]
    fn should_roundtrip_trace_response() {
        let response = TraceResponse { trace: trace() };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<TraceResponse>(&json).unwrap(),
            response
        );
    }

    #[test]
    fn should_nest_trace_under_trace_key() {
        let response = TraceResponse { trace: trace() };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["trace"]["session_id"], Uuid::nil().to_string());
        assert_eq!(value["trace"]["events"][0]["task_type"], "edit");
    }
}
