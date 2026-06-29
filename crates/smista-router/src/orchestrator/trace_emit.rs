//! Per-turn trace emission helpers over the [`Tracer`].
//!
//! Each helper builds the matching [`SerializedPayload`], wraps it as plaintext,
//! and records it through the [`Tracer`], returning the authored trace row's
//! [`Uuid`] so an encrypted session can fold the payload into `to_encrypt`. The
//! orchestrator builds the typed payloads from the resolved turn and the usage
//! totals; this module only serializes and records them.
use smista_core::api::ApprovalDecision;
use smista_core::message::MessageRole;
use smista_core::model::Provider;
use smista_core::policy::Classification;
use smista_core::tool::ToolCallStatus;
use smista_core::trace::{
    ApprovalPayload, ContextSelectionPayload, CostPayload, MessagePayload, RoutingDecisionPayload,
    ToolCallPayload,
};
use smista_core::usage::Usage;
use smista_storage::types::SecretContent;
use uuid::Uuid;

use crate::router::resolver::ResolvedTurn;
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

/// Builds the per-turn trace context from the resolved turn.
pub(crate) fn context_of(resolved: &ResolvedTurn) -> TraceContext {
    TraceContext {
        task_type: resolved.classification.intent,
        provider: resolved.routing.provider.clone(),
        model: resolved.routing.model.clone(),
        matched_rule: resolved.routing.matched_rule.clone(),
    }
}

/// Records the classification, routing and context-selection traces a resolved
/// turn produces.
///
/// Best-effort: a trace write failure is logged and swallowed, never fatal —
/// the deterministic trace is observability and must not abort the user's turn.
pub(crate) async fn record_resolution(tracer: &Tracer, resolved: &ResolvedTurn) {
    let ctx = context_of(resolved);

    if let Err(error) =
        trace_classification(tracer, ctx.clone(), resolved.classification.clone()).await
    {
        tracing::warn!(%error, "failed to record classification trace");
    }

    let routing = RoutingDecisionPayload {
        provider: resolved.routing.provider.clone(),
        model: resolved.routing.model.clone(),
        matched_rule: resolved.routing.matched_rule.clone(),
        fallback_used: resolved.routing.fallback_used,
        override_used: resolved.routing.override_used,
        reason: resolved.routing.reason.clone(),
    };
    if let Err(error) = trace_routing(tracer, ctx.clone(), routing).await {
        tracing::warn!(%error, "failed to record routing-decision trace");
    }

    for reference in &resolved.context.references {
        let payload = ContextSelectionPayload {
            path: reference
                .path
                .as_ref()
                .map(|path| path.display().to_string()),
            kind: format!("{:?}", reference.kind),
            included: reference.included,
            reason: reference.reason.clone(),
        };
        if let Err(error) = trace_context(tracer, ctx.clone(), payload).await {
            tracing::warn!(%error, "failed to record context-selection trace");
        }
    }
}

/// Records a message trace for a persisted user or assistant message.
pub(crate) async fn record_message_event(
    tracer: &Tracer,
    ctx: TraceContext,
    role: MessageRole,
    provider: Provider,
    model: String,
) {
    let payload = MessagePayload {
        role,
        provider,
        model,
    };
    if let Err(error) = trace_message(tracer, ctx, payload).await {
        tracing::warn!(%error, "failed to record message trace");
    }
}

/// Records the cost trace for an invocation's usage.
pub(crate) async fn record_cost_event(
    tracer: &Tracer,
    ctx: TraceContext,
    usage: &Usage,
    provider: Provider,
    model: String,
) {
    let payload = CostPayload {
        provider,
        model,
        input_tokens: usage.input_tokens.unwrap_or(0),
        output_tokens: usage.output_tokens.unwrap_or(0),
        cost: usage.actual_cost.map(|cost| cost.to_string()),
    };
    if let Err(error) = trace_cost(tracer, ctx, payload).await {
        tracing::warn!(%error, "failed to record cost trace");
    }
}

/// Records the trace for a tool call the model requested.
pub(crate) async fn record_tool_request(
    tracer: &Tracer,
    ctx: TraceContext,
    tool_name: String,
    arguments: Option<String>,
) {
    let payload = ToolCallPayload {
        tool_name,
        status: ToolCallStatus::Pending,
        arguments,
        result: None,
        error: None,
    };
    if let Err(error) = trace_tool_call(tracer, ctx, payload).await {
        tracing::warn!(%error, "failed to record tool-call request trace");
    }
}

/// Records the trace for a tool call's result once the client returns it.
pub(crate) async fn record_tool_result(
    tracer: &Tracer,
    ctx: TraceContext,
    tool_name: String,
    is_error: bool,
) {
    let status = if is_error {
        ToolCallStatus::Failed
    } else {
        ToolCallStatus::Completed
    };
    let payload = ToolCallPayload {
        tool_name,
        status,
        arguments: None,
        result: None,
        error: None,
    };
    if let Err(error) = trace_tool_call(tracer, ctx, payload).await {
        tracing::warn!(%error, "failed to record tool-call result trace");
    }
}

/// Records the trace for an approval decision, folded or standalone.
pub(crate) async fn record_approval_event(
    tracer: &Tracer,
    ctx: TraceContext,
    target_type: String,
    target_id: String,
    decision: ApprovalDecision,
    reason: Option<String>,
) {
    let payload = ApprovalPayload {
        target_type,
        target_id,
        decision,
        reason,
    };
    if let Err(error) = trace_approval(tracer, ctx, payload).await {
        tracing::warn!(%error, "failed to record approval trace");
    }
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
