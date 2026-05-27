//! Execution traces recording how a task was routed and run.
//!
//! A [`Trace`] is the deterministic record of a single execution: which intent
//! was detected, which provider and model served it, the routing rule that
//! matched, and the ordered events that followed. The router produces it, the
//! HTTP API ([`crate::api`]) returns it for the `/trace` and `/why` views, and
//! smista-trace persists it.
//!
//! Event payloads are kept as opaque [`serde_json::Value`]s for now: the trace
//! event vocabulary is still being designed, and storing them untyped lets the
//! recorder evolve without churning this wire type.
//!
//! # Examples
//!
//! ```
//! use smista_core::intent::TaskIntent;
//! use smista_core::model::Provider;
//! use smista_core::trace::Trace;
//! use uuid::Uuid;
//!
//! let trace = Trace {
//!     id: "trace:xyz".to_string(),
//!     session_id: Uuid::nil(),
//!     task_type: TaskIntent::Edit,
//!     provider: Provider::Anthropic,
//!     model: "claude-sonnet".to_string(),
//!     matched_rule: Some("edits under src/auth/** use claude-sonnet".to_string()),
//!     events: Vec::new(),
//! };
//! assert_eq!(trace.task_type, TaskIntent::Edit);
//! ```

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::intent::TaskIntent;
use crate::model::Provider;

/// The recorded outcome of routing and running a single task.
///
/// `matched_rule` is the human-readable description of the routing rule that
/// selected the model, or `None` when the default route was used. `events`
/// holds the ordered execution events as opaque JSON values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    /// Stable identifier of the trace, for example `trace:xyz`.
    pub id: String,
    /// Session the traced task belongs to.
    pub session_id: Uuid,
    /// Detected intent that drove routing.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Description of the routing rule that matched, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<String>,
    /// Ordered execution events, as opaque JSON values.
    pub events: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Trace {
        Trace {
            id: "trace:xyz".to_string(),
            session_id: Uuid::nil(),
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
            matched_rule: Some("rule".to_string()),
            events: Vec::new(),
        }
    }

    #[test]
    fn should_serialize_with_snake_case_fields() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["task_type"], "edit");
        assert_eq!(value["session_id"], Uuid::nil().to_string());
        assert_eq!(value["events"], serde_json::json!([]));
    }

    #[test]
    fn should_omit_absent_matched_rule() {
        let trace = Trace {
            matched_rule: None,
            ..sample()
        };
        let value = serde_json::to_value(trace).unwrap();
        assert!(value.get("matched_rule").is_none());
    }

    #[test]
    fn should_roundtrip_serde() {
        let trace = sample();
        let json = serde_json::to_string(&trace).unwrap();
        assert_eq!(serde_json::from_str::<Trace>(&json).unwrap(), trace);
    }
}
