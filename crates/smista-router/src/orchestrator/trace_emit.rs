//! Per-turn trace emission helpers over the [`Tracer`].
//!
//! Each helper builds the matching [`SerializedPayload`], wraps it as plaintext,
//! and records it through the [`Tracer`], returning the authored trace row's
//! [`Uuid`] so an encrypted session can fold the payload into `to_encrypt`. The
//! orchestrator builds the typed payloads from the resolved turn and the usage
//! totals; this module only serializes and records them.
#![allow(
    dead_code,
    reason = "the encrypted-session variants are wired in a later orchestrator task"
)]

use smista_core::policy::Classification;
use smista_core::trace::{
    ApprovalPayload, ContextSelectionPayload, CostPayload, MessagePayload, RoutingDecisionPayload,
    ToolCallPayload,
};
use smista_storage::types::SecretContent;
use uuid::Uuid;

use crate::trace::{SerializedPayload, TraceContext, Tracer, TracerResult};

/// Records a classification trace event in a plaintext session.
pub(crate) async fn trace_classification(
    tracer: &Tracer,
    context: TraceContext,
    classification: Classification,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::classification(classification)?;
    tracer
        .record_classification(context, SecretContent::plaintext(payload.into_string()))
        .await
}

/// Records a routing-decision trace event in a plaintext session.
pub(crate) async fn trace_routing(
    tracer: &Tracer,
    context: TraceContext,
    payload: RoutingDecisionPayload,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::routing_decision(payload)?;
    tracer
        .record_routing_decision(context, SecretContent::plaintext(payload.into_string()))
        .await
}

/// Records a context-selection trace event in a plaintext session.
pub(crate) async fn trace_context(
    tracer: &Tracer,
    context: TraceContext,
    payload: ContextSelectionPayload,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::context_selection(payload)?;
    tracer
        .record_context_selection(context, SecretContent::plaintext(payload.into_string()))
        .await
}

/// Records a message trace event in a plaintext session.
pub(crate) async fn trace_message(
    tracer: &Tracer,
    context: TraceContext,
    payload: MessagePayload,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::message(payload)?;
    tracer
        .record_message(context, SecretContent::plaintext(payload.into_string()))
        .await
}

/// Records a tool-call trace event in a plaintext session.
pub(crate) async fn trace_tool_call(
    tracer: &Tracer,
    context: TraceContext,
    payload: ToolCallPayload,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::tool_call(payload)?;
    tracer
        .record_tool_call(context, SecretContent::plaintext(payload.into_string()))
        .await
}

/// Records an approval trace event in a plaintext session.
pub(crate) async fn trace_approval(
    tracer: &Tracer,
    context: TraceContext,
    payload: ApprovalPayload,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::approval(payload)?;
    tracer
        .record_approval(context, SecretContent::plaintext(payload.into_string()))
        .await
}

/// Records a cost trace event in a plaintext session.
pub(crate) async fn trace_cost(
    tracer: &Tracer,
    context: TraceContext,
    payload: CostPayload,
) -> TracerResult<Uuid> {
    let payload = SerializedPayload::cost(payload)?;
    tracer
        .record_cost(context, SecretContent::plaintext(payload.into_string()))
        .await
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use smista_core::intent::TaskIntent;
    use smista_core::model::Provider;
    use smista_core::policy::{Classification, Confidence, IntentSource};
    use smista_storage::api::Pagination;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use smista_storage::entity::{Session, Table, TraceEventType, User};
    use smista_storage::surrealdb::RecordId;

    use super::*;

    async fn trace_fixture() -> (Tracer, Uuid, Uuid) {
        let db = SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database");
        let user_id = Uuid::now_v7();
        db.create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("failed to create user");
        let session_id = Uuid::now_v7();
        db.create_session(Session {
            id: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            encrypted: false,
            key_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        })
        .await
        .expect("failed to create session");

        let tracer = Tracer::new(db, session_id, user_id);
        (tracer, session_id, user_id)
    }

    fn sample_context() -> TraceContext {
        TraceContext {
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            matched_rule: None,
        }
    }

    fn sample_classification() -> Classification {
        Classification {
            intent: TaskIntent::Edit,
            source: IntentSource::Inferred,
            reason: "keyword matched".to_string(),
            matched_rule: None,
            confidence: Some(Confidence::High),
        }
    }

    #[tokio::test]
    async fn should_record_classification_trace_in_plaintext_session() {
        let (tracer, _session_id, _user_id) = trace_fixture().await;
        trace_classification(&tracer, sample_context(), sample_classification())
            .await
            .expect("failed to record classification trace");

        let trace = tracer
            .traces(Pagination::default())
            .await
            .expect("failed to read traces")
            .expect("trace missing");
        assert!(
            trace
                .events
                .iter()
                .any(|event| event.event_type == TraceEventType::Classification)
        );
    }
}
