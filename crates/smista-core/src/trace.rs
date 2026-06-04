//! Execution traces recording how a session's tasks were routed and run.
//!
//! A [`Trace`] is the deterministic record of a session's execution: the ordered
//! [`TraceEvent`]s that were emitted, each carrying the routing context of the
//! task it belongs to and a free-form payload. The router produces it, the HTTP
//! API ([`crate::api`]) returns it for the `/trace` and `/why` views, and
//! smista-storage persists the events and assembles this view.
//!
//! Event payloads are kept as opaque [`serde_json::Value`]s for now: the trace
//! event vocabulary is still being designed, and storing them untyped lets the
//! recorder evolve without churning this wire type.
//!
//! # Examples
//!
//! ```
//! use smista_core::trace::Trace;
//! use uuid::Uuid;
//!
//! let trace = Trace {
//!     session_id: Uuid::nil(),
//!     events: Vec::new(),
//! };
//! assert!(trace.events.is_empty());
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "surrealdb")]
use surrealdb_types::SurrealValue;
use uuid::Uuid;

use crate::intent::TaskIntent;
use crate::model::Provider;

/// The kind of a trace event.
///
/// **Provisional**: these variants are a placeholder until the private spec
/// pins the trace taxonomy. Serialized as `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "surrealdb", derive(SurrealValue))]
#[cfg_attr(feature = "surrealdb", surreal(crate = "::surrealdb_types"))]
#[cfg_attr(feature = "surrealdb", surreal(rename_all = "snake_case", untagged))]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TraceEventType {
    /// A message was recorded.
    Message,
    /// The router selected a provider/model for a task.
    RoutingDecision,
    /// Context was selected or excluded for a task.
    ContextSelection,
    /// A tool was requested or executed.
    ToolCall,
    /// A confirmation decision was recorded.
    Approval,
    /// Token usage or cost was recorded.
    Cost,
}

/// A single recorded execution event within a [`Trace`].
///
/// Each event carries the routing context of the task that emitted it
/// (`task_type`, `provider`, `model`, `matched_rule`) and a free-form `payload`
/// whose shape depends on `event_type`. The payload is opaque here while the
/// trace vocabulary is still being designed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct TraceEvent {
    /// Kind of trace event.
    pub event_type: TraceEventType,
    /// Detected intent that drove routing for this event's task.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Description of the routing rule that matched, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub matched_rule: Option<String>,
    /// When the event occurred.
    pub created_at: DateTime<Utc>,
    /// Free-form event payload, as an opaque JSON value.
    pub payload: serde_json::Value,
}

/// The recorded outcome of routing and running a session's tasks.
///
/// `events` holds the session's ordered execution events, oldest first. Each
/// [`TraceEvent`] is self-describing — it carries its own routing context — so
/// the trace needs no top-level routing fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Trace {
    /// Session the traced tasks belong to.
    pub session_id: Uuid,
    /// Ordered execution events, oldest first.
    pub events: Vec<TraceEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> TraceEvent {
        TraceEvent {
            event_type: TraceEventType::RoutingDecision,
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
            matched_rule: Some("rule".to_string()),
            created_at: Utc::now(),
            payload: serde_json::json!({ "step": 1 }),
        }
    }

    fn sample() -> Trace {
        Trace {
            session_id: Uuid::nil(),
            events: vec![event()],
        }
    }

    #[test]
    fn should_serialize_with_snake_case_fields() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["session_id"], Uuid::nil().to_string());
        assert_eq!(value["events"][0]["event_type"], "routing_decision");
        assert_eq!(value["events"][0]["task_type"], "edit");
        assert_eq!(value["events"][0]["payload"]["step"], 1);
    }

    #[test]
    fn should_omit_absent_matched_rule() {
        let event = TraceEvent {
            matched_rule: None,
            ..event()
        };
        let value = serde_json::to_value(event).unwrap();
        assert!(value.get("matched_rule").is_none());
    }

    #[test]
    fn should_roundtrip_serde() {
        let trace = sample();
        let json = serde_json::to_string(&trace).unwrap();
        assert_eq!(serde_json::from_str::<Trace>(&json).unwrap(), trace);
    }
}
