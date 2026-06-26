//! Durable persistence of the work a turn produces.
//!
//! Thin helpers over [`UserSession`] and the database that turn the
//! orchestrator's in-memory results — the run-input snapshot, messages, the
//! routing decision and context references — into rows. Each builds the storage
//! entity, keying record ids off the session and a fresh message id, and
//! appends it. Plaintext content is wrapped in [`SecretContent::plaintext`];
//! sealing for encrypted sessions rides a separate fold.
#![allow(
    dead_code,
    reason = "wired into the orchestrator run loop across later tasks"
)]

use chrono::Utc;
use smista_core::api::{ApprovalDecision, ToolApproval};
use smista_core::intent::TaskIntent;
use smista_core::message::MessageRole;
use smista_core::model::Provider;
use smista_storage::StorageError;
use smista_storage::database::Database as _;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::{
    PlanStatus, Session, SessionApproval, SessionContextReference, SessionMessage,
    SessionMessageContent, SessionPlan, SessionPlanContent, SessionRoutingDecision,
    SessionRunInput, SessionRunInputContent, SessionToolCall, SessionToolCallContent, Table,
    ToolCallStatus, User,
};
use smista_storage::surrealdb::RecordId;
use smista_storage::types::SecretContent;
use uuid::Uuid;

use crate::orchestrator::mediation::ClientCall;
use crate::orchestrator::run_input::{RunInputBundle, RunInputMeta};
use crate::router::resolver::RoutingDecision;
use crate::router::resolver::context::{CandidateKind, ContextReference};
use crate::session::{SessionError, SessionResult, UserSession};

/// Persists the run-input snapshot: clear meta plus the sealable bundle.
///
/// The metadata halves (policy, preferences, workspace paths) are serialized as
/// JSON and stored in clear. For a plaintext session the bundle (input,
/// attachments, git diff) is stored as plaintext content; for an encrypted one
/// the router holds no key, so it writes a placeholder and the turn folds the
/// bundle into the seal, which overwrites the placeholder with ciphertext once
/// the client returns it. The row is keyed by session id, so a re-run overwrites
/// it in place.
pub(crate) async fn persist_run_input(
    database: &SurrealDatabase,
    user_id: Uuid,
    session_id: Uuid,
    run_id: Uuid,
    meta: &RunInputMeta,
    bundle: &RunInputBundle,
    encrypted: bool,
) -> SessionResult<()> {
    let policy = serde_json::to_string(&meta.policy).map_err(StorageError::from)?;
    let local_preferences =
        serde_json::to_string(&meta.local_preferences).map_err(StorageError::from)?;
    let workspace = serde_json::to_string(&meta.workspace).map_err(StorageError::from)?;

    // A run that starts from a plan command stays in plan mode across turns; the
    // flag is persisted so a continuation need not re-send the command.
    let plan_active = bundle.input.command == Some(TaskIntent::Plan);
    let input = SessionRunInput::new(
        session_id,
        user_id,
        run_id,
        policy,
        local_preferences,
        workspace,
        plan_active,
    );
    // The placeholder for an encrypted run carries no secret; the sealed bundle
    // is written over it on the answering continuation.
    let bundle_content = if encrypted {
        SecretContent::plaintext(String::new())
    } else {
        SecretContent::plaintext(serde_json::to_string(bundle).map_err(StorageError::from)?)
    };
    let content = SessionRunInputContent {
        id: RecordId::new(SessionRunInputContent::name(), session_id.to_string()),
        content: bundle_content,
    };
    database
        .set_run_input(user_id, input, content)
        .await
        .map_err(SessionError::Database)?;
    Ok(())
}

/// Persists one message and its plaintext content, returning the message id.
pub(crate) async fn persist_plaintext_message(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    role: MessageRole,
    provider: Provider,
    model: &str,
    text: &str,
) -> SessionResult<Uuid> {
    let message_id = Uuid::now_v7();
    let message = SessionMessage {
        id: RecordId::new(SessionMessage::name(), message_id.to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        role,
        provider,
        model: model.to_string(),
        created_at: Utc::now(),
    };
    let content = SessionMessageContent {
        id: RecordId::new(SessionMessageContent::name(), message_id.to_string()),
        content: SecretContent::plaintext(text),
    };
    session.append_message(message, content).await?;
    Ok(message_id)
}

/// A tool call persisted for a pause, with the identity the client echoes back.
pub(crate) struct PersistedCall {
    /// The router-assigned id correlating the request, the row and the result.
    pub(crate) call_id: String,
    /// Name of the tool to invoke.
    pub(crate) name: String,
    /// Arguments for the call, as a provider-agnostic JSON value.
    pub(crate) arguments: serde_json::Value,
    /// Whether the user must approve the call before it runs.
    pub(crate) requires_approval: ToolApproval,
}

/// The rows a tool-request pause authored, with the calls handed to the client.
pub(crate) struct PersistedToolRequest {
    /// References to the router-authored rows (assistant message, tool calls),
    /// in the order they must be sealed for an encrypted session.
    pub(crate) authored: Vec<RecordId>,
    /// The client-bound calls, each with its router-assigned identity.
    pub(crate) calls: Vec<PersistedCall>,
}

/// Persists the assistant tool-request message and one row per client call.
///
/// The assistant message records the model's text alongside the request; each
/// client call becomes a [`SessionToolCall`] row (status [`ToolCallStatus::Pending`])
/// keyed by a fresh router id, with its arguments stored as the paired content.
/// That router id is the [`call_id`](PersistedCall::call_id) the client echoes
/// back with the result, so the orchestrator never depends on the provider's own
/// identifiers surviving across turns.
pub(crate) async fn persist_tool_request(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    provider: Provider,
    model: &str,
    assistant_text: &str,
    client: &[ClientCall],
) -> SessionResult<PersistedToolRequest> {
    let message_id = persist_plaintext_message(
        session,
        user_id,
        session_id,
        MessageRole::Assistant,
        provider,
        model,
        assistant_text,
    )
    .await?;
    let mut authored = vec![RecordId::new(
        SessionMessage::name(),
        message_id.to_string(),
    )];
    let mut calls = Vec::with_capacity(client.len());
    for client_call in client {
        let call_uuid = Uuid::now_v7();
        let arguments =
            serde_json::to_string(&client_call.call.arguments).map_err(StorageError::from)?;
        let tool_call = SessionToolCall {
            id: RecordId::new(SessionToolCall::name(), call_uuid.to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            tool_name: client_call.call.name.clone(),
            status: ToolCallStatus::Pending,
            created_at: Utc::now(),
            completed_at: None,
        };
        let content = SessionToolCallContent {
            id: RecordId::new(SessionToolCallContent::name(), call_uuid.to_string()),
            arguments: SecretContent::plaintext(arguments),
            result: None,
            error: None,
        };
        session.append_tool_call(tool_call, content).await?;
        authored.push(RecordId::new(
            SessionToolCall::name(),
            call_uuid.to_string(),
        ));
        calls.push(PersistedCall {
            call_id: call_uuid.to_string(),
            name: client_call.call.name.clone(),
            arguments: client_call.call.arguments.clone(),
            requires_approval: client_call.requires_approval,
        });
    }
    Ok(PersistedToolRequest { authored, calls })
}

/// Snapshots a draft plan and its body, returning the new plan's id.
///
/// A planning turn ends by recording the model's output as a [`SessionPlan`] at
/// [`PlanStatus::Draft`] with its snapshot stored as plaintext content; the
/// orchestrator then raises a plan approval against the returned id.
pub(crate) async fn persist_plan_draft(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    body: &str,
) -> SessionResult<Uuid> {
    let plan_id = Uuid::now_v7();
    let plan = SessionPlan {
        id: RecordId::new(SessionPlan::name(), plan_id.to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        path: "PLAN.md".to_string(),
        status: PlanStatus::Draft,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        approved_at: None,
        content_hash: None,
    };
    let content = SessionPlanContent {
        id: RecordId::new(SessionPlanContent::name(), plan_id.to_string()),
        content: Some(SecretContent::plaintext(body)),
    };
    session.append_plan(plan, content).await?;
    Ok(plan_id)
}

/// Writes a message's metadata and its already-sealed content in one paired write.
///
/// Used when finalizing an encrypted run: the router stored nothing for the
/// message up front, so its metadata (recalled from the run's [`PendingWrite`])
/// and the ciphertext the client returned are written together here.
#[allow(clippy::too_many_arguments, reason = "a message row's fields are wide")]
pub(crate) async fn write_sealed_message(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    id: Uuid,
    role: MessageRole,
    provider: Provider,
    model: &str,
    content: SecretContent,
) -> SessionResult<()> {
    let message = SessionMessage {
        id: RecordId::new(SessionMessage::name(), id.to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        role,
        provider,
        model: model.to_string(),
        created_at: Utc::now(),
    };
    let content = SessionMessageContent {
        id: RecordId::new(SessionMessageContent::name(), id.to_string()),
        content,
    };
    session.append_message(message, content).await?;
    Ok(())
}

/// Writes a draft plan's metadata and its already-sealed snapshot together.
pub(crate) async fn write_sealed_plan(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    id: Uuid,
    content: SecretContent,
) -> SessionResult<()> {
    let plan = SessionPlan {
        id: RecordId::new(SessionPlan::name(), id.to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        path: "PLAN.md".to_string(),
        status: PlanStatus::Draft,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        approved_at: None,
        content_hash: None,
    };
    let content = SessionPlanContent {
        id: RecordId::new(SessionPlanContent::name(), id.to_string()),
        content: Some(content),
    };
    session.append_plan(plan, content).await?;
    Ok(())
}

/// Writes a tool call's metadata and its already-sealed result together.
///
/// The arguments are not sealed under the tool call's own reference — the durable
/// secret a tool call carries is its `result`, sealed by the client — so the
/// stored arguments are left empty for an encrypted run; the model's request
/// rides the sealed assistant message instead.
#[allow(
    clippy::too_many_arguments,
    reason = "a tool-call row's fields are wide"
)]
pub(crate) async fn write_sealed_tool_call(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    id: Uuid,
    tool_name: &str,
    status: ToolCallStatus,
    result: Option<SecretContent>,
    error: Option<SecretContent>,
) -> SessionResult<()> {
    let completed_at =
        matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed).then(Utc::now);
    let tool_call = SessionToolCall {
        id: RecordId::new(SessionToolCall::name(), id.to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        tool_name: tool_name.to_string(),
        status,
        created_at: Utc::now(),
        completed_at,
    };
    let content = SessionToolCallContent {
        id: RecordId::new(SessionToolCallContent::name(), id.to_string()),
        arguments: SecretContent::plaintext(""),
        result,
        error,
    };
    session.append_tool_call(tool_call, content).await?;
    Ok(())
}

/// Persists a user approval decision for an operation, returning nothing.
///
/// `target_type` names what was decided (for example `tool_call` or `plan`) and
/// `target_id` references the row it applies to; the decision and its optional
/// reason are recorded as the audit trail the run state machine folds approvals
/// into.
pub(crate) async fn persist_approval(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    target_type: &str,
    target_id: &str,
    decision: ApprovalDecision,
    reason: Option<String>,
) -> SessionResult<()> {
    let row = SessionApproval {
        id: RecordId::new(SessionApproval::name(), Uuid::now_v7().to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        decision,
        reason,
        created_at: Utc::now(),
    };
    session.append_approval(row).await?;
    Ok(())
}

/// Persists the routing decision chosen for a turn.
pub(crate) async fn persist_routing_decision(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    decision: &RoutingDecision,
) -> SessionResult<()> {
    let row = SessionRoutingDecision {
        id: RecordId::new(SessionRoutingDecision::name(), Uuid::now_v7().to_string()),
        session: RecordId::new(Session::name(), session_id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        task_type: decision.intent,
        provider: decision.provider.clone(),
        model: decision.model.clone(),
        matched_rule: decision.matched_rule.clone(),
        fallback_used: Some(decision.fallback_used),
        override_used: Some(decision.override_used),
        reason: decision.reason.clone(),
        created_at: Utc::now(),
    };
    session.append_routing_decision(row).await?;
    Ok(())
}

/// Persists the references-only record of what context was selected or excluded.
pub(crate) async fn persist_context_references(
    session: &UserSession,
    user_id: Uuid,
    session_id: Uuid,
    references: &[ContextReference],
) -> SessionResult<()> {
    for reference in references {
        let row = SessionContextReference {
            id: RecordId::new(SessionContextReference::name(), Uuid::now_v7().to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            path: reference
                .path
                .as_ref()
                .map(|path| path.display().to_string()),
            kind: candidate_kind_str(&reference.kind).to_string(),
            included: reference.included,
            reason: reference.reason.clone(),
            created_at: Utc::now(),
        };
        session.append_context_reference(row).await?;
    }
    Ok(())
}

/// The stable wire string recorded for a context candidate's kind.
fn candidate_kind_str(kind: &CandidateKind) -> &'static str {
    match kind {
        CandidateKind::Message => "message",
        CandidateKind::File => "file",
        CandidateKind::Instruction => "instruction",
        CandidateKind::Skill => "skill",
        CandidateKind::Memory => "memory",
        CandidateKind::Diff => "diff",
    }
}
