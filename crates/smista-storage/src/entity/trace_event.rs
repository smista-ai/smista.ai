//! Trace event tables.
//!
//! A [`TraceEvent`] is a structured, append-only event recorded during task
//! execution — the detailed history surfaced by `/trace`. Its free-form payload
//! lives in the paired [`TraceEventContent`] table, which shares the same record
//! id (`trace_event:⟨uuid⟩` ↔ `trace_event_content:⟨uuid⟩`). The split keeps
//! metadata queryable while the payload may later be encrypted before
//! persistence.
//!
//! The assembled read view is `smista_core::trace::Trace`; this table is the
//! per-event source it is built from.

use chrono::{DateTime, Utc};
use smista_core::intent::TaskIntent;
use smista_core::model::Provider;
#[doc(inline)]
pub use smista_core::trace::TraceEventType;
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;
use crate::types::SecretContent;

/// A structured, append-only event recorded during task execution.
///
/// The free-form payload lives in [`TraceEventContent`]; only queryable
/// metadata stays here. `user` is stored redundantly so ownership checks never
/// need a join. The routing fields (`task_type`, `provider`, `model`,
/// `matched_rule`) carry the routing context of the task that emitted the
/// event, so the assembled [`smista_core::trace::Trace`] read view can be built
/// from trace events alone.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct TraceEvent {
    /// Unique identifier for the event.
    pub id: RecordId,
    /// Session the event belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Kind of trace event.
    pub event_type: TraceEventType,
    /// Task that the emitting routing context served.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Routing rule that matched, if any.
    pub matched_rule: Option<String>,
    /// When the event occurred.
    pub created_at: DateTime<Utc>,
}

impl Table for TraceEvent {
    fn name() -> &'static str {
        "trace_event"
    }
}

/// The payload of a [`TraceEvent`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct TraceEventContent {
    /// Record id, identical to the owning [`TraceEvent`].
    pub id: RecordId,
    /// Structured event payload, in clear or sealed for an encrypted session.
    pub payload: SecretContent,
}

impl Table for TraceEventContent {
    fn name() -> &'static str {
        "trace_event_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_trace_event() {
        let id = RecordId::new(TraceEvent::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let event = TraceEvent {
            id,
            session: session.clone(),
            user: user.clone(),
            event_type: TraceEventType::RoutingDecision,
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            matched_rule: Some("edit -> claude".to_string()),
            created_at: Utc::now(),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), event).await;
    }

    #[tokio::test]
    async fn should_store_and_read_trace_event_content() {
        let id = RecordId::new(TraceEventContent::name(), uuid::Uuid::now_v7().to_string());
        let content = TraceEventContent {
            id,
            payload: SecretContent::plaintext("{\"model\":\"claude\"}"),
        };

        crate::tests::roundtrip(content).await;
    }
}
