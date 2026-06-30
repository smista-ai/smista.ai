use chrono::Utc;
use smista_core::api::{
    Attachments, ExecutePolicy, LocalPreferences, TaskInput, ToolApproval, Workspace,
};
use smista_core::message::MessageRole;
use smista_core::policy::{
    ClassificationConfig, DefaultRoute, PrivacyPolicy, RoutingPolicy, ToolsConfig,
};
use smista_core::usage::Usage;
use smista_providers::api::{CompletionResponse, FinishReason, ToolCall};
use smista_storage::database::Database as _;
use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
use smista_storage::entity::{Session, Table, User};
use smista_storage::surrealdb::RecordId;

use super::*;

/// An orchestrator over an in-memory database with one plaintext session,
/// routed through the given `router`.
async fn orchestrator_with_router(router: Router) -> (Orchestrator, Uuid, Uuid) {
    let database = SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Memory,
    })
    .await
    .expect("in-memory database");

    let user_id = Uuid::now_v7();
    database
        .create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("create user");

    let session_id = Uuid::now_v7();
    database
        .create_session(Session {
            id: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            scope: None,
            encrypted: false,
            key_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        })
        .await
        .expect("create session");

    let orchestrator = Orchestrator::new(database, Arc::new(router), Arc::new(Resolver::default()));
    (orchestrator, user_id, session_id)
}

/// An orchestrator routed through the default completing mock.
async fn orchestrator_with_session() -> (Orchestrator, Uuid, Uuid) {
    orchestrator_with_router(Router::mock()).await
}

/// Wraps a plaintext string as a deterministic, reversible test envelope.
///
/// No real cryptography: the ciphertext is the plaintext, so a test can seal
/// and open round-trip and assert what was stored. Only the `Encrypted`
/// variant matters for the at-rest assertions.
fn test_seal(plaintext: &str) -> EncryptedPayload {
    EncryptedPayload {
        version: 1,
        algorithm: "test".to_string(),
        key_id: "kf_test".to_string(),
        nonce: "nonce".to_string(),
        ciphertext: plaintext.to_string(),
    }
}

/// An orchestrator over an encrypted session seeded with one sealed history
/// message, routed through the default completing mock.
async fn encrypted_session_with_sealed_history() -> (Orchestrator, Uuid, Uuid) {
    use smista_storage::entity::{SessionMessage, SessionMessageContent};
    use smista_storage::types::{ContentEnvelope, SecretContent};

    let database = SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Memory,
    })
    .await
    .expect("in-memory database");

    let user_id = Uuid::now_v7();
    database
        .create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("create user");

    let session_id = Uuid::now_v7();
    database
        .create_session(Session {
            id: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            scope: None,
            encrypted: true,
            key_id: Some("kf_test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        })
        .await
        .expect("create session");

    let message_id = Uuid::now_v7();
    database
        .append_message(
            user_id,
            SessionMessage {
                id: RecordId::new(SessionMessage::name(), message_id.to_string()),
                session: RecordId::new(Session::name(), session_id.to_string()),
                user: RecordId::new(User::name(), user_id.to_string()),
                role: MessageRole::User,
                provider: Provider::Anthropic,
                model: "claude".to_string(),
                created_at: Utc::now(),
            },
            SessionMessageContent {
                id: RecordId::new(SessionMessageContent::name(), message_id.to_string()),
                content: SecretContent::Encrypted(ContentEnvelope {
                    version: 1,
                    algorithm: "test".to_string(),
                    key_id: "kf_test".to_string(),
                    nonce: "nonce".to_string(),
                    ciphertext: "earlier".to_string(),
                }),
            },
        )
        .await
        .expect("seed sealed history");

    let orchestrator = Orchestrator::new(
        database,
        Arc::new(Router::mock()),
        Arc::new(Resolver::default()),
    );
    (orchestrator, user_id, session_id)
}

/// An orchestrator over an empty encrypted session, completing mock.
async fn encrypted_session() -> (Orchestrator, Uuid, Uuid) {
    let database = SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Memory,
    })
    .await
    .expect("in-memory database");

    let user_id = Uuid::now_v7();
    database
        .create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("create user");

    let session_id = Uuid::now_v7();
    database
        .create_session(Session {
            id: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            scope: None,
            encrypted: true,
            key_id: Some("kf_test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        })
        .await
        .expect("create session");

    let orchestrator = Orchestrator::new(
        database,
        Arc::new(Router::mock()),
        Arc::new(Resolver::default()),
    );
    (orchestrator, user_id, session_id)
}

/// A completion that requests one tool with empty arguments.
fn tool_call_response(name: &str) -> CompletionResponse {
    CompletionResponse {
        content: String::new(),
        tool_calls: vec![ToolCall {
            call_id: format!("{name}-1"),
            name: name.to_string(),
            arguments: serde_json::json!({}),
        }],
        usage: Usage::default(),
        finish_reason: FinishReason::ToolCalls,
    }
}

/// A completion that requests the same tool twice, so the pause waits on two calls.
fn two_tool_call_response(name: &str) -> CompletionResponse {
    CompletionResponse {
        content: String::new(),
        tool_calls: vec![
            ToolCall {
                call_id: format!("{name}-1"),
                name: name.to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                call_id: format!("{name}-2"),
                name: name.to_string(),
                arguments: serde_json::json!({}),
            },
        ],
        usage: Usage::default(),
        finish_reason: FinishReason::ToolCalls,
    }
}

/// A completion that ends the turn with a final answer.
fn completed_response() -> CompletionResponse {
    CompletionResponse {
        content: "done".to_string(),
        tool_calls: Vec::new(),
        usage: Usage::default(),
        finish_reason: FinishReason::Stop,
    }
}

/// A request that grants `read_file` without confirmation.
fn allow_read_file_request() -> ExecuteRequest {
    let mut request = sample_execute_request();
    request
        .policy
        .tools
        .set("read_file", smista_core::policy::PermissionMode::Allow);
    request
}

/// A completion that requests `edit_file` with a concrete path and edit.
fn edit_file_call_response() -> CompletionResponse {
    CompletionResponse {
        content: String::new(),
        tool_calls: vec![ToolCall {
            call_id: "edit_file-1".to_string(),
            name: "edit_file".to_string(),
            arguments: serde_json::json!({
                "path": "src/lib.rs",
                "old": "old line",
                "new": "new line",
            }),
        }],
        usage: Usage::default(),
        finish_reason: FinishReason::ToolCalls,
    }
}

/// A request that grants `edit_file` without confirmation.
fn allow_edit_file_request() -> ExecuteRequest {
    let mut request = sample_execute_request();
    request
        .policy
        .tools
        .set("edit_file", smista_core::policy::PermissionMode::Allow);
    request
}

/// A request that forbids `shell`.
fn deny_shell_request() -> ExecuteRequest {
    let mut request = sample_execute_request();
    request
        .policy
        .tools
        .set("shell", smista_core::policy::PermissionMode::Deny);
    request
}

/// An execute request that routes to the local mock model by default.
fn sample_execute_request() -> ExecuteRequest {
    ExecuteRequest {
        input: TaskInput {
            text: "hello".to_string(),
            command: None,
            explicit_model: None,
        },
        workspace: Workspace {
            root: std::path::PathBuf::from("/repo"),
            git_branch: None,
            git_diff: None,
            referenced_paths: Vec::new(),
            active_file: None,
        },
        policy: ExecutePolicy {
            version: 1,
            source: "merged".to_string(),
            classification: ClassificationConfig::default(),
            routing: RoutingPolicy {
                rules: Vec::new(),
                default: Some(DefaultRoute {
                    model: "ollama/mock-local".parse().expect("valid reference"),
                    fallbacks: Vec::new(),
                }),
            },
            tools: ToolsConfig::default(),
            privacy: PrivacyPolicy::default(),
        },
        local_preferences: LocalPreferences {
            auto_apply: false,
            stream: false,
            local_only: false,
            no_network: false,
        },
        attachments: Attachments {
            files: Vec::new(),
            instructions: Vec::new(),
            invoked_skills: Vec::new(),
            available_skills: Vec::new(),
        },
    }
}

#[tokio::test]
async fn should_complete_a_plaintext_single_turn() {
    let (orchestrator, user_id, session_id) = orchestrator_with_session().await;

    let response = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("turn completes");

    match response.outcome {
        TurnOutcome::Completed(turn) => {
            assert_eq!(turn.message.role, MessageRole::Assistant);
            assert!(!turn.message.content.is_empty());
        }
        other => panic!("expected completed, got {other:?}"),
    }

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");

    let messages = session.messages().await.expect("messages");
    assert_eq!(messages.len(), 2);

    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert_eq!(run_state.phase, RunPhase::Idle);
    assert_eq!(run_state.active, None);
}

#[tokio::test]
async fn should_reject_a_second_turn_while_one_is_in_flight() {
    let (orchestrator, user_id, session_id) = orchestrator_with_session().await;

    // Seed an active lock so admission must reject the request.
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let mut state = RunState::new(session_id, user_id, Uuid::now_v7(), RunPhase::Idle);
    state.active = Some(ActiveTurn {
        started_at: Utc::now(),
        lease: "held".to_string(),
    });
    session.set_run_state(state).await.expect("seed lock");

    let error = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect_err("rejected while in flight");
    assert!(matches!(error, OrchestratorError::Busy));
}

#[tokio::test]
async fn should_pause_for_allow_tool_call() {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![tool_call_response("read_file")]))
            .await;

    let response = orchestrator
        .execute(
            user_id,
            session_id,
            allow_read_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for a tool");

    match response.outcome {
        TurnOutcome::AwaitingTool { tool_requests, .. } => {
            assert_eq!(tool_requests.len(), 1);
            assert_eq!(tool_requests[0].name, "read_file");
            assert_eq!(tool_requests[0].requires_approval, ToolApproval::Allow);
        }
        other => panic!("expected awaiting_tool, got {other:?}"),
    }
    assert!(
        response
            .allowed_continuations
            .contains(&ContinueKind::ToolResults)
    );

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert!(matches!(run_state.phase, RunPhase::AwaitingTool { .. }));
    assert_eq!(run_state.active, None);
}

#[tokio::test]
async fn should_feed_denied_tool_back_without_pausing() {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![
            tool_call_response("shell"),
            completed_response(),
        ]))
        .await;

    let response = orchestrator
        .execute(user_id, session_id, deny_shell_request(), HashMap::new())
        .await
        .expect("turn completes after the denial is fed back");

    assert!(matches!(response.outcome, TurnOutcome::Completed(_)));
}

/// Drives a fresh run to a pause on an `allow`-mode `read_file` tool.
async fn paused_on_allow_tool() -> (Orchestrator, Uuid, Uuid) {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![tool_call_response("read_file")]))
            .await;
    let response = orchestrator
        .execute(
            user_id,
            session_id,
            allow_read_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for a tool");
    assert!(matches!(response.outcome, TurnOutcome::AwaitingTool { .. }));
    (orchestrator, user_id, session_id)
}

/// Drives a fresh run to a pause on an `ask`-mode `read_file` tool.
async fn paused_on_ask_tool() -> (Orchestrator, Uuid, Uuid) {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![tool_call_response("read_file")]))
            .await;
    let response = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for a tool");
    match response.outcome {
        TurnOutcome::AwaitingTool { tool_requests, .. } => {
            assert_eq!(tool_requests[0].requires_approval, ToolApproval::Ask);
        }
        other => panic!("expected awaiting_tool, got {other:?}"),
    }
    (orchestrator, user_id, session_id)
}

/// The single tool-call wait recorded in the paused run's phase.
async fn pending_call_id(orchestrator: &Orchestrator, user_id: Uuid, session_id: Uuid) -> String {
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    match run_state.phase {
        RunPhase::AwaitingTool { calls, .. } => calls[0].call_id.clone(),
        other => panic!("expected awaiting_tool, got {other:?}"),
    }
}

#[tokio::test]
async fn should_resume_after_tool_results_and_complete() {
    let (orchestrator, user_id, session_id) = paused_on_allow_tool().await;
    let call_id = pending_call_id(&orchestrator, user_id, session_id).await;

    let continuation = ContinueRequest::ToolResults {
        results: vec![ToolResult {
            call_id,
            content: "file body".to_string(),
            is_error: false,
            decision: None,
        }],
        encrypted: Default::default(),
    };
    let response = orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect("run resumes and completes");
    assert!(matches!(response.outcome, TurnOutcome::Completed(_)));

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    assert_eq!(state.tool_calls.len(), 1);
    assert_eq!(state.tool_calls[0].status, ToolCallStatus::Completed);

    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert_eq!(run_state.phase, RunPhase::Idle);
    assert_eq!(run_state.active, None);
}

#[tokio::test]
async fn should_record_trace_events_for_a_completed_turn() {
    use smista_storage::api::Pagination;
    use smista_storage::entity::TraceEventType;

    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![
            completed_response(),
            completed_response(),
        ]))
        .await;
    // Two turns: the second recalls the first turn's messages as context, so a
    // context-selection trace is produced alongside the always-present events.
    for _ in 0..2 {
        orchestrator
            .execute(
                user_id,
                session_id,
                sample_execute_request(),
                HashMap::new(),
            )
            .await
            .expect("turn completes");
    }

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let trace = session
        .traces(Pagination::default())
        .await
        .expect("traces read")
        .expect("trace present");
    let kinds: Vec<TraceEventType> = trace.events.iter().map(|event| event.event_type).collect();
    for expected in [
        TraceEventType::Classification,
        TraceEventType::RoutingDecision,
        TraceEventType::ContextSelection,
        TraceEventType::Message,
        TraceEventType::Cost,
    ] {
        assert!(
            kinds.contains(&expected),
            "missing trace event {expected:?}"
        );
    }
}

#[tokio::test]
async fn should_seal_trace_events_after_an_encrypted_run() {
    let (orchestrator, user_id, session_id) = encrypted_session().await;

    let r1 = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("turn completes");
    let to_encrypt = match r1.outcome {
        TurnOutcome::Completed(turn) => turn.to_encrypt,
        other => panic!("expected completed, got {other:?}"),
    };
    assert!(
        to_encrypt
            .keys()
            .any(|reference| matches!(reference, ContentRef::Trace(_))),
        "the deterministic trace is folded into the seal, never left in clear"
    );

    let encrypted: BTreeMap<ContentRef, EncryptedPayload> = to_encrypt
        .iter()
        .map(|(reference, plaintext)| (reference.clone(), test_seal(plaintext)))
        .collect();
    orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::Sealed { encrypted },
            HashMap::new(),
        )
        .await
        .expect("seal completes");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let rows = session
        .list_trace_content()
        .await
        .expect("trace content read");
    assert!(!rows.is_empty(), "the run recorded trace events");
    assert!(
        rows.iter()
            .all(|(_, content)| content.content.is_encrypted()),
        "every trace row is sealed at rest after an encrypted run"
    );
}

#[tokio::test]
async fn should_record_tool_and_approval_traces_across_a_tool_turn() {
    use smista_storage::api::Pagination;
    use smista_storage::entity::TraceEventType;

    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![
            tool_call_response("read_file"),
            completed_response(),
        ]))
        .await;
    orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for an ask tool");
    let call_id = pending_call_id(&orchestrator, user_id, session_id).await;

    let continuation = ContinueRequest::ToolResults {
        results: vec![ToolResult {
            call_id,
            content: "ok".to_string(),
            is_error: false,
            decision: Some(smista_core::api::ApprovalDecision::Approved),
        }],
        encrypted: Default::default(),
    };
    orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect("run resumes after the tool result");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let trace = session
        .traces(Pagination::default())
        .await
        .expect("traces read")
        .expect("trace present");
    let kinds: Vec<TraceEventType> = trace.events.iter().map(|event| event.event_type).collect();
    assert!(
        kinds.contains(&TraceEventType::ToolCall),
        "a tool turn records tool-call traces"
    );
    assert!(
        kinds.contains(&TraceEventType::Approval),
        "a folded tool approval is traced"
    );
}

#[tokio::test]
async fn should_record_a_proposed_diff_when_a_file_editing_tool_is_requested() {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![edit_file_call_response()])).await;

    orchestrator
        .execute(
            user_id,
            session_id,
            allow_edit_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for the edit tool");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");

    assert_eq!(
        state.diffs.len(),
        1,
        "a file-editing tool request records one diff"
    );
    assert_eq!(state.diffs[0].path, "src/lib.rs");
    assert_eq!(
        state.diffs[0].status,
        smista_storage::entity::DiffStatus::Proposed
    );
}

#[tokio::test]
async fn should_mark_the_diff_applied_when_the_edit_succeeds() {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![
            edit_file_call_response(),
            completed_response(),
        ]))
        .await;

    orchestrator
        .execute(
            user_id,
            session_id,
            allow_edit_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for the edit tool");
    let call_id = pending_call_id(&orchestrator, user_id, session_id).await;

    let continuation = ContinueRequest::ToolResults {
        results: vec![ToolResult {
            call_id,
            content: "edit applied".to_string(),
            is_error: false,
            decision: None,
        }],
        encrypted: Default::default(),
    };
    orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect("run resumes and completes");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");

    assert_eq!(state.diffs.len(), 1, "the edit is recorded as one diff");
    assert_eq!(
        state.diffs[0].status,
        smista_storage::entity::DiffStatus::Applied
    );
    assert!(
        state.diffs[0].applied_at.is_some(),
        "an applied diff carries an applied_at timestamp"
    );
}

#[tokio::test]
async fn should_seal_and_apply_a_diff_in_an_encrypted_run() {
    let (orchestrator, user_id, session_id) =
        encrypted_orchestrator_with_router(Router::mock_scripted(vec![
            edit_file_call_response(),
            completed_response(),
        ]))
        .await;

    let r1 = orchestrator
        .execute(
            user_id,
            session_id,
            allow_edit_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for the edit tool");
    let (tool_requests, to_encrypt) = match r1.outcome {
        TurnOutcome::AwaitingTool {
            tool_requests,
            to_encrypt,
            ..
        } => (tool_requests, to_encrypt),
        other => panic!("expected awaiting_tool, got {other:?}"),
    };
    assert!(
        to_encrypt
            .keys()
            .any(|reference| matches!(reference, ContentRef::Diff(_))),
        "the proposed diff body is folded into the seal, never stored in clear"
    );

    // The client seals every folded row (the diff included) and answers the tool.
    let mut encrypted = seal_map(&to_encrypt);
    let mut results = Vec::with_capacity(tool_requests.len());
    for request in &tool_requests {
        encrypted.insert(
            ContentRef::ToolCall(request.call_id.clone()),
            test_seal("ok"),
        );
        results.push(ToolResult {
            call_id: request.call_id.clone(),
            content: "ok".to_string(),
            is_error: false,
            decision: None,
        });
    }
    orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::ToolResults { results, encrypted },
            HashMap::new(),
        )
        .await
        .expect("run resumes after the sealed tool results");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    assert_eq!(
        state.diffs.len(),
        1,
        "the encrypted edit is recorded as one diff"
    );
    assert_eq!(state.diffs[0].path, "src/lib.rs");
    assert_eq!(
        state.diffs[0].status,
        smista_storage::entity::DiffStatus::Applied
    );

    let diff_id = Uuid::parse_str(&tool_requests[0].call_id).expect("diff id is the call id");
    let diff_content = session
        .get_content("session_diff", diff_id)
        .await
        .expect("diff content read")
        .expect("diff content present");
    assert!(
        diff_content.is_encrypted(),
        "the diff body is sealed at rest"
    );
}

#[tokio::test]
async fn should_record_folded_approval_on_ask_tool() {
    let (orchestrator, user_id, session_id) = paused_on_ask_tool().await;
    let call_id = pending_call_id(&orchestrator, user_id, session_id).await;

    let continuation = ContinueRequest::ToolResults {
        results: vec![ToolResult {
            call_id,
            content: "ok".to_string(),
            is_error: false,
            decision: Some(smista_core::api::ApprovalDecision::Approved),
        }],
        encrypted: Default::default(),
    };
    orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect("run resumes");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    assert_eq!(state.approvals.len(), 1, "the folded approval was recorded");
}

/// The tool-call waits recorded in the paused run's `AwaitingTool` phase.
async fn pending_call_ids(
    orchestrator: &Orchestrator,
    user_id: Uuid,
    session_id: Uuid,
) -> Vec<String> {
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    match run_state.phase {
        RunPhase::AwaitingTool { calls, .. } => {
            calls.into_iter().map(|wait| wait.call_id).collect()
        }
        other => panic!("expected awaiting_tool, got {other:?}"),
    }
}

#[tokio::test]
async fn should_preserve_checkpoint_on_invalid_continuation() {
    let (orchestrator, user_id, session_id) = paused_on_allow_tool().await;

    // A tool result for a call the pause is not waiting on must be rejected
    // without destroying the `AwaitingTool` checkpoint.
    let continuation = ContinueRequest::ToolResults {
        results: vec![ToolResult {
            call_id: "not-a-pending-call".to_string(),
            content: "body".to_string(),
            is_error: false,
            decision: None,
        }],
        encrypted: Default::default(),
    };
    let error = orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect_err("an unknown tool result is rejected");
    assert!(matches!(error, OrchestratorError::UnexpectedContinuation));

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert!(
        matches!(run_state.phase, RunPhase::AwaitingTool { .. }),
        "the checkpoint survives a rejected continuation"
    );
    assert_eq!(run_state.active, None, "the lock is released");
}

#[tokio::test]
async fn should_reject_partial_tool_results() {
    let (orchestrator, user_id, session_id) = orchestrator_with_router(Router::mock_scripted(
        vec![two_tool_call_response("read_file")],
    ))
    .await;
    orchestrator
        .execute(
            user_id,
            session_id,
            allow_read_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for two tools");
    let calls = pending_call_ids(&orchestrator, user_id, session_id).await;
    assert_eq!(calls.len(), 2, "two tool calls are outstanding");

    // Answering only one of the two pending calls must be rejected; the run may
    // not advance with a call still unanswered.
    let continuation = ContinueRequest::ToolResults {
        results: vec![ToolResult {
            call_id: calls[0].clone(),
            content: "body".to_string(),
            is_error: false,
            decision: None,
        }],
        encrypted: Default::default(),
    };
    let error = orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect_err("a partial tool answer is rejected");
    assert!(matches!(error, OrchestratorError::UnexpectedContinuation));

    let run_state = pending_call_ids(&orchestrator, user_id, session_id).await;
    assert_eq!(run_state.len(), 2, "the pause still waits on both calls");
}

#[tokio::test]
async fn should_reject_duplicate_tool_results() {
    let (orchestrator, user_id, session_id) = orchestrator_with_router(Router::mock_scripted(
        vec![two_tool_call_response("read_file")],
    ))
    .await;
    orchestrator
        .execute(
            user_id,
            session_id,
            allow_read_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for two tools");
    let calls = pending_call_ids(&orchestrator, user_id, session_id).await;

    // The first call answered twice, the second not at all: a duplicate that
    // also leaves a call unanswered must be rejected.
    let continuation = ContinueRequest::ToolResults {
        results: vec![
            ToolResult {
                call_id: calls[0].clone(),
                content: "body".to_string(),
                is_error: false,
                decision: None,
            },
            ToolResult {
                call_id: calls[0].clone(),
                content: "body again".to_string(),
                is_error: false,
                decision: None,
            },
        ],
        encrypted: Default::default(),
    };
    let error = orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect_err("a duplicate tool answer is rejected");
    assert!(matches!(error, OrchestratorError::UnexpectedContinuation));
}

/// A plan-mode execute request that routes to the local mock model.
fn plan_request() -> ExecuteRequest {
    let mut request = sample_execute_request();
    request.input.command = Some(smista_core::intent::TaskIntent::Plan);
    request
}

/// Drives a planning run to its plan-approval pause, returning the id.
async fn paused_on_plan_approval() -> (Orchestrator, Uuid, Uuid, String) {
    let (orchestrator, user_id, session_id) =
        orchestrator_with_router(Router::mock_scripted(vec![tool_call_response("edit_file")]))
            .await;
    let response = orchestrator
        .execute(user_id, session_id, plan_request(), HashMap::new())
        .await
        .expect("planning turn pauses for approval");
    let approval_id = match response.outcome {
        TurnOutcome::AwaitingApproval { approval, .. } => {
            assert_eq!(approval.kind, ApprovalKind::Plan);
            approval.approval_id
        }
        other => panic!("expected awaiting_approval, got {other:?}"),
    };
    (orchestrator, user_id, session_id, approval_id)
}

#[tokio::test]
async fn should_deny_edits_and_snapshot_plan_while_planning() {
    let (orchestrator, user_id, session_id, _approval_id) = paused_on_plan_approval().await;

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    assert_eq!(state.plans.len(), 1, "a draft plan was snapshotted");
    assert_eq!(
        state.plans[0].status,
        smista_storage::entity::PlanStatus::Draft
    );

    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert!(matches!(run_state.phase, RunPhase::AwaitingApproval { .. }));
}

#[tokio::test]
async fn should_leave_plan_mode_on_accept() {
    let (orchestrator, user_id, session_id, approval_id) = paused_on_plan_approval().await;

    let continuation = ContinueRequest::ApprovalDecisions {
        decisions: vec![smista_core::api::ApprovalDecisionEntry {
            approval_id,
            decision: smista_core::api::ApprovalDecision::Approved,
            reason: None,
        }],
        encrypted: Default::default(),
    };
    orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect("plan accepted");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let (input, _) = session
        .run_input()
        .await
        .expect("run input read")
        .expect("run input present");
    assert!(!input.plan_active, "accepting the plan leaves plan mode");

    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    assert_eq!(
        state.plans[0].status,
        smista_storage::entity::PlanStatus::Approved
    );
}

#[tokio::test]
async fn should_break_to_idle_and_persist_partial() {
    let (orchestrator, user_id, session_id) = paused_on_allow_tool().await;

    let response = orchestrator
        .advance(user_id, session_id, ContinueRequest::Break, HashMap::new())
        .await
        .expect("break returns the run to idle");
    assert!(matches!(response.outcome, TurnOutcome::Idle { .. }));

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert_eq!(run_state.phase, RunPhase::Idle);
    assert_eq!(run_state.active, None);

    // The outstanding tool call was cancelled, not left pending.
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    assert_eq!(state.tool_calls.len(), 1);
    assert_eq!(state.tool_calls[0].status, ToolCallStatus::Failed);
}

#[tokio::test]
async fn should_inject_and_run_next_turn() {
    let (orchestrator, user_id, session_id) = paused_on_allow_tool().await;

    let continuation = ContinueRequest::Inject {
        messages: vec![UserMessage {
            text: "do Y instead".to_string(),
            ciphertext: None,
        }],
    };
    let response = orchestrator
        .advance(user_id, session_id, continuation, HashMap::new())
        .await
        .expect("inject supersedes and runs the next turn");
    assert!(matches!(
        response.outcome,
        TurnOutcome::Completed(_) | TurnOutcome::AwaitingTool { .. }
    ));

    // The injected message was recorded after the prior turn's output.
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let messages = session.messages().await.expect("messages");
    assert!(
        messages
            .iter()
            .any(|(_, content)| content.content.as_plaintext() == Some("do Y instead")),
        "the injected user message is in history"
    );
}

#[tokio::test]
async fn should_reject_continuation_with_no_active_run() {
    let (orchestrator, user_id, session_id) = orchestrator_with_session().await;
    let error = orchestrator
        .advance(user_id, session_id, ContinueRequest::Break, HashMap::new())
        .await
        .expect_err("no run to advance");
    assert!(matches!(error, OrchestratorError::NoActiveRun));
}

#[tokio::test]
async fn should_accept_execute_when_idle_and_lockfree() {
    let (orchestrator, user_id, session_id) = orchestrator_with_session().await;

    let result = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn should_request_decrypt_then_seal_on_encrypted_run() {
    let (orchestrator, user_id, session_id) = encrypted_session_with_sealed_history().await;

    // The run needs the sealed history opened before it can build a prompt, and
    // the authoring turn seals its run-input bundle in the same round.
    let r1 = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("decrypt requested");
    let (to_decrypt, to_seal) = match r1.outcome {
        TurnOutcome::AwaitingDecrypt {
            to_decrypt,
            to_encrypt,
            ..
        } => {
            assert!(
                to_encrypt
                    .keys()
                    .any(|reference| matches!(reference, ContentRef::RunInput(_))),
                "the run-input bundle is sealed at the first decrypt"
            );
            (to_decrypt, to_encrypt)
        }
        other => panic!("expected awaiting_decrypt, got {other:?}"),
    };
    assert!(!to_decrypt.is_empty());
    assert!(r1.allowed_continuations.contains(&ContinueKind::Decrypted));

    // Open the history, and return the run-input bundle both opened (so the turn
    // can use it) and sealed (so it is written at rest); the turn then completes
    // and asks to seal the user and assistant messages it authored.
    let mut plaintext: BTreeMap<ContentRef, String> = to_decrypt
        .iter()
        .map(|(reference, payload)| (reference.clone(), payload.ciphertext.clone()))
        .collect();
    for (reference, opened) in &to_seal {
        plaintext.insert(reference.clone(), opened.clone());
    }
    let sealed_bundle: BTreeMap<ContentRef, EncryptedPayload> = to_seal
        .iter()
        .map(|(reference, opened)| (reference.clone(), test_seal(opened)))
        .collect();
    let r2 = orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::Decrypted {
                plaintext,
                encrypted: sealed_bundle,
            },
            HashMap::new(),
        )
        .await
        .expect("turn completes");
    let to_encrypt = match r2.outcome {
        TurnOutcome::Completed(turn) => {
            let message_count = turn
                .to_encrypt
                .keys()
                .filter(|reference| matches!(reference, ContentRef::Message(_)))
                .count();
            assert_eq!(
                message_count, 2,
                "the user and assistant messages are sealed"
            );
            assert!(
                turn.to_encrypt
                    .keys()
                    .any(|reference| matches!(reference, ContentRef::Trace(_))),
                "the run's trace events are sealed in the same fold"
            );
            turn.to_encrypt
        }
        other => panic!("expected completed, got {other:?}"),
    };
    assert!(r2.allowed_continuations.contains(&ContinueKind::Sealed));

    // Return the ciphertext; the run writes the sealed rows and idles.
    let encrypted: BTreeMap<ContentRef, EncryptedPayload> = to_encrypt
        .iter()
        .map(|(reference, plaintext)| (reference.clone(), test_seal(plaintext)))
        .collect();
    let r3 = orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::Sealed { encrypted },
            HashMap::new(),
        )
        .await
        .expect("seal completes");
    assert!(matches!(r3.outcome, TurnOutcome::Idle { .. }));

    // No plaintext is left at rest: every message content is sealed.
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let messages = session.messages().await.expect("messages");
    assert_eq!(messages.len(), 3, "seeded history plus user and assistant");
    assert!(
        messages
            .iter()
            .all(|(_, content)| content.content.is_encrypted()),
        "no plaintext message content at rest"
    );

    // The run-input bundle is sealed at rest too, not left as its placeholder.
    let (_, run_input_content) = session
        .run_input()
        .await
        .expect("run input read")
        .expect("run input present");
    assert!(
        run_input_content.content.is_encrypted(),
        "run-input bundle is sealed at rest"
    );

    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert_eq!(run_state.phase, RunPhase::Idle);
    assert_eq!(run_state.active, None);
}

#[tokio::test]
async fn should_seal_clear_session_memory_after_an_encrypted_run() {
    use smista_storage::entity::{ContextMemory, ContextMemoryContent};
    use smista_storage::types::SecretContent;

    let (orchestrator, user_id, session_id) = encrypted_session().await;

    // The memory tool writes session memory in clear during a run; seed one
    // such row to stand in for that write.
    let memory_id = Uuid::now_v7();
    orchestrator
        .database
        .record_context_memory(
            user_id,
            ContextMemory::new(memory_id, session_id, user_id, Some("topic".to_string())),
            ContextMemoryContent::new(memory_id, SecretContent::plaintext("remember me")),
        )
        .await
        .expect("seed clear memory");

    // A completed encrypted turn folds the clear memory into its seal.
    let r1 = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("turn completes");
    let to_encrypt = match r1.outcome {
        TurnOutcome::Completed(turn) => turn.to_encrypt,
        other => panic!("expected completed, got {other:?}"),
    };
    assert!(
        to_encrypt
            .keys()
            .any(|reference| matches!(reference, ContentRef::Memory(_))),
        "clear session memory folded into the seal"
    );

    let encrypted: BTreeMap<ContentRef, EncryptedPayload> = to_encrypt
        .iter()
        .map(|(reference, plaintext)| (reference.clone(), test_seal(plaintext)))
        .collect();
    let r2 = orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::Sealed { encrypted },
            HashMap::new(),
        )
        .await
        .expect("seal completes");
    assert!(matches!(r2.outcome, TurnOutcome::Idle { .. }));

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let memories = session
        .list_context_memory_with_content()
        .await
        .expect("memories");
    assert!(
        memories
            .iter()
            .all(|(_, content)| content.content.is_encrypted()),
        "session memory sealed after the run"
    );
}

#[tokio::test]
async fn should_seal_run_input_after_an_encrypted_run() {
    let (orchestrator, user_id, session_id) = encrypted_session().await;

    // A completed encrypted turn must fold the run-input bundle into its seal,
    // and the answering `sealed` continuation must store the ciphertext, so the
    // prompt, attachments and diff are never left readable at rest.
    let r1 = orchestrator
        .execute(
            user_id,
            session_id,
            sample_execute_request(),
            HashMap::new(),
        )
        .await
        .expect("turn completes");
    let to_encrypt = match r1.outcome {
        TurnOutcome::Completed(turn) => turn.to_encrypt,
        other => panic!("expected completed, got {other:?}"),
    };
    assert!(
        to_encrypt
            .keys()
            .any(|reference| matches!(reference, ContentRef::RunInput(_))),
        "the run-input bundle is folded into the seal"
    );

    let encrypted: BTreeMap<ContentRef, EncryptedPayload> = to_encrypt
        .iter()
        .map(|(reference, plaintext)| (reference.clone(), test_seal(plaintext)))
        .collect();
    orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::Sealed { encrypted },
            HashMap::new(),
        )
        .await
        .expect("seal completes");

    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let (_, content) = session
        .run_input()
        .await
        .expect("run input read")
        .expect("run input present");
    assert!(
        content.content.is_encrypted(),
        "run-input bundle is sealed at rest"
    );
}

/// An orchestrator over an empty encrypted session, routed through `router`.
async fn encrypted_orchestrator_with_router(router: Router) -> (Orchestrator, Uuid, Uuid) {
    let database = SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Memory,
    })
    .await
    .expect("in-memory database");

    let user_id = Uuid::now_v7();
    database
        .create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("create user");

    let session_id = Uuid::now_v7();
    database
        .create_session(Session {
            id: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            scope: None,
            encrypted: true,
            key_id: Some("kf_test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        })
        .await
        .expect("create session");

    let orchestrator = Orchestrator::new(database, Arc::new(router), Arc::new(Resolver::default()));
    (orchestrator, user_id, session_id)
}

#[tokio::test]
async fn should_reject_a_tool_continuation_that_omits_the_run_input_seal() {
    let (orchestrator, user_id, session_id) = encrypted_orchestrator_with_router(
        Router::mock_scripted(vec![tool_call_response("read_file")]),
    )
    .await;

    // The authoring turn defers its assistant message and the run-input bundle,
    // pausing for the tool with both folded into `to_encrypt`.
    let r1 = orchestrator
        .execute(
            user_id,
            session_id,
            allow_read_file_request(),
            HashMap::new(),
        )
        .await
        .expect("turn pauses for a tool");
    let (tool_requests, to_encrypt) = match r1.outcome {
        TurnOutcome::AwaitingTool {
            tool_requests,
            to_encrypt,
            ..
        } => (tool_requests, to_encrypt),
        other => panic!("expected awaiting_tool, got {other:?}"),
    };
    assert!(
        to_encrypt
            .keys()
            .any(|reference| matches!(reference, ContentRef::RunInput(_))),
        "the run-input bundle is folded into the tool pause seal"
    );

    // The client answers the tool but drops the run-input ciphertext from its
    // sealed map: a malformed continuation that would otherwise leave the bundle
    // an unsealed placeholder while the deferred rows are written.
    let mut encrypted = seal_map(&to_encrypt);
    encrypted.retain(|reference, _| !matches!(reference, ContentRef::RunInput(_)));
    let mut results = Vec::with_capacity(tool_requests.len());
    for request in &tool_requests {
        encrypted.insert(
            ContentRef::ToolCall(request.call_id.clone()),
            test_seal("ok"),
        );
        results.push(ToolResult {
            call_id: request.call_id.clone(),
            content: "ok".to_string(),
            is_error: false,
            decision: None,
        });
    }

    let error = orchestrator
        .advance(
            user_id,
            session_id,
            ContinueRequest::ToolResults { results, encrypted },
            HashMap::new(),
        )
        .await
        .expect_err("a continuation missing the run-input seal is rejected");
    assert!(
        matches!(error, OrchestratorError::UnexpectedContinuation),
        "the missing seal is an unexpected continuation, got {error:?}"
    );

    // The checkpoint survives: no row was written, so the run still awaits the
    // tool and the run-input bundle is still its unsealed placeholder.
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    let run_state = session
        .run_state()
        .await
        .expect("run state read")
        .expect("run state present");
    assert!(
        matches!(run_state.phase, RunPhase::AwaitingTool { .. }),
        "the tool checkpoint is preserved"
    );
    let (_, content) = session
        .run_input()
        .await
        .expect("run input read")
        .expect("run input present");
    assert_eq!(
        content.content.as_plaintext(),
        Some(""),
        "the run-input bundle is still its unsealed placeholder"
    );
}

// ----- Transition matrix harness -----------------------------------------
//
// `drive` plays the client side of the protocol to a terminal outcome, the
// same way for a plaintext and an encrypted session. Every matrix test below
// reuses it under both encryption modes, so one harness covers the whole
// state machine. The test crypto is the identity round-trip from `test_seal`.

use smista_core::api::{ApprovalDecision, ApprovalDecisionEntry};

/// A tag recorded for each pause the run passed through.
#[derive(Debug, PartialEq, Eq)]
enum Tag {
    Tool,
    Approval,
    Decrypt,
    Completed,
    Idle,
}

/// Seals every plaintext entry with the identity test crypto.
fn seal_map(to_encrypt: &BTreeMap<ContentRef, String>) -> BTreeMap<ContentRef, EncryptedPayload> {
    to_encrypt
        .iter()
        .map(|(reference, plaintext)| (reference.clone(), test_seal(plaintext)))
        .collect()
}

/// Drives a run from `execute` to a terminal outcome, folding `tool_decision`
/// into every tool result and answering plan approvals with `plan_decision`,
/// recording the pause trail.
async fn drive(
    orchestrator: &Orchestrator,
    user_id: Uuid,
    session_id: Uuid,
    request: ExecuteRequest,
    tool_decision: Option<ApprovalDecision>,
    plan_decision: ApprovalDecision,
) -> (TurnOutcome, Vec<Tag>) {
    let mut trail = Vec::new();
    let mut response = orchestrator
        .execute(user_id, session_id, request, HashMap::new())
        .await
        .expect("execute");
    loop {
        let continuation = match response.outcome {
            TurnOutcome::Completed(turn) => {
                trail.push(Tag::Completed);
                if turn.to_encrypt.is_empty() {
                    return (TurnOutcome::Completed(turn), trail);
                }
                ContinueRequest::Sealed {
                    encrypted: seal_map(&turn.to_encrypt),
                }
            }
            TurnOutcome::AwaitingTool {
                tool_requests,
                to_encrypt,
                ..
            } => {
                trail.push(Tag::Tool);
                let mut encrypted = seal_map(&to_encrypt);
                let mut results = Vec::with_capacity(tool_requests.len());
                for request in &tool_requests {
                    encrypted.insert(
                        ContentRef::ToolCall(request.call_id.clone()),
                        test_seal("ok"),
                    );
                    results.push(ToolResult {
                        call_id: request.call_id.clone(),
                        content: "ok".to_string(),
                        is_error: false,
                        decision: tool_decision,
                    });
                }
                ContinueRequest::ToolResults { results, encrypted }
            }
            TurnOutcome::AwaitingApproval {
                approval,
                to_encrypt,
                ..
            } => {
                trail.push(Tag::Approval);
                ContinueRequest::ApprovalDecisions {
                    decisions: vec![ApprovalDecisionEntry {
                        approval_id: approval.approval_id,
                        decision: plan_decision,
                        reason: None,
                    }],
                    encrypted: seal_map(&to_encrypt),
                }
            }
            TurnOutcome::AwaitingDecrypt {
                to_decrypt,
                to_encrypt,
                ..
            } => {
                trail.push(Tag::Decrypt);
                // The client opens the requested rows, and also returns the
                // plaintext of anything it is asked to seal in the same round
                // (the run-input bundle), since it holds that plaintext itself.
                let mut plaintext: BTreeMap<ContentRef, String> = to_decrypt
                    .iter()
                    .map(|(reference, payload)| (reference.clone(), payload.ciphertext.clone()))
                    .collect();
                for (reference, opened) in &to_encrypt {
                    plaintext.insert(reference.clone(), opened.clone());
                }
                ContinueRequest::Decrypted {
                    plaintext,
                    encrypted: seal_map(&to_encrypt),
                }
            }
            TurnOutcome::AwaitingEncrypt { to_encrypt, .. } => ContinueRequest::Sealed {
                encrypted: seal_map(&to_encrypt),
            },
            TurnOutcome::Idle { .. } => {
                trail.push(Tag::Idle);
                return (response.outcome, trail);
            }
            TurnOutcome::Error { .. } => return (response.outcome, trail),
        };
        response = orchestrator
            .advance(user_id, session_id, continuation, HashMap::new())
            .await
            .expect("advance");
    }
}

/// Builds an orchestrator over a fresh session under `encrypted`, routed
/// through the scripted completions.
async fn scripted(encrypted: bool, turns: Vec<CompletionResponse>) -> (Orchestrator, Uuid, Uuid) {
    let database = SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Memory,
    })
    .await
    .expect("in-memory database");
    let user_id = Uuid::now_v7();
    database
        .create_user(User {
            id: RecordId::new(User::name(), user_id.to_string()),
            api_key_hash: format!("hash-{user_id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("create user");
    let session_id = Uuid::now_v7();
    database
        .create_session(Session {
            id: RecordId::new(Session::name(), session_id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            scope: None,
            encrypted,
            key_id: encrypted.then(|| "kf_test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        })
        .await
        .expect("create session");
    let orchestrator = Orchestrator::new(
        database,
        Arc::new(Router::mock_scripted(turns)),
        Arc::new(Resolver::default()),
    );
    (orchestrator, user_id, session_id)
}

/// Asserts every stored content row of the session is sealed at rest.
async fn assert_all_sealed(orchestrator: &Orchestrator, user_id: Uuid, session_id: Uuid) {
    let sessions = Sessions::new(orchestrator.database.clone(), user_id);
    let session = sessions.open(session_id).await.expect("open session");
    for (_, content) in session.messages().await.expect("messages") {
        assert!(
            content.content.is_encrypted(),
            "a message is in clear at rest"
        );
    }
    let state = session
        .state()
        .await
        .expect("state read")
        .expect("state present");
    for tool_call in &state.tool_calls {
        let content = session
            .get_content("session_tool_call", tool_call.uuid())
            .await
            .expect("tool content")
            .expect("tool content present");
        assert!(content.is_encrypted(), "a tool result is in clear at rest");
    }
    let (_, run_input) = session
        .run_input()
        .await
        .expect("run input read")
        .expect("run input present");
    assert!(
        run_input.content.is_encrypted(),
        "the run-input bundle is in clear at rest"
    );
}

#[tokio::test]
async fn matrix_no_tools_completes_under_both_modes() {
    for encrypted in [false, true] {
        let (orchestrator, user_id, session_id) =
            scripted(encrypted, vec![completed_response()]).await;
        let (outcome, trail) = drive(
            &orchestrator,
            user_id,
            session_id,
            sample_execute_request(),
            None,
            ApprovalDecision::Approved,
        )
        .await;
        if encrypted {
            assert_eq!(trail, vec![Tag::Completed, Tag::Idle]);
            assert!(matches!(outcome, TurnOutcome::Idle { .. }));
            assert_all_sealed(&orchestrator, user_id, session_id).await;
        } else {
            assert_eq!(trail, vec![Tag::Completed]);
            assert!(matches!(outcome, TurnOutcome::Completed(_)));
        }
    }
}

#[tokio::test]
async fn matrix_allow_tool_completes_under_both_modes() {
    for encrypted in [false, true] {
        let (orchestrator, user_id, session_id) = scripted(
            encrypted,
            vec![tool_call_response("read_file"), completed_response()],
        )
        .await;
        let mut request = sample_execute_request();
        request
            .policy
            .tools
            .set("read_file", smista_core::policy::PermissionMode::Allow);

        let (outcome, trail) = drive(
            &orchestrator,
            user_id,
            session_id,
            request,
            None,
            ApprovalDecision::Approved,
        )
        .await;

        // Plaintext: tool then completed. Encrypted: the tool round seals its
        // rows, so the next turn opens them before completing.
        if encrypted {
            assert_eq!(
                trail,
                vec![Tag::Tool, Tag::Decrypt, Tag::Completed, Tag::Idle]
            );
            assert!(matches!(outcome, TurnOutcome::Idle { .. }));
            assert_all_sealed(&orchestrator, user_id, session_id).await;
        } else {
            assert_eq!(trail, vec![Tag::Tool, Tag::Completed]);
            assert!(matches!(outcome, TurnOutcome::Completed(_)));
        }

        let sessions = Sessions::new(orchestrator.database.clone(), user_id);
        let session = sessions.open(session_id).await.expect("open session");
        let state = session.state().await.expect("state").expect("present");
        assert_eq!(state.tool_calls.len(), 1);
        assert_eq!(state.tool_calls[0].status, ToolCallStatus::Completed);
    }
}

#[tokio::test]
async fn matrix_folded_ask_approval_is_recorded_under_both_modes() {
    for encrypted in [false, true] {
        let (orchestrator, user_id, session_id) = scripted(
            encrypted,
            vec![tool_call_response("read_file"), completed_response()],
        )
        .await;
        // `read_file` is unset, so it defaults to `ask`; the client folds an
        // approval into the result.
        drive(
            &orchestrator,
            user_id,
            session_id,
            sample_execute_request(),
            Some(ApprovalDecision::Approved),
            ApprovalDecision::Approved,
        )
        .await;

        let sessions = Sessions::new(orchestrator.database.clone(), user_id);
        let session = sessions.open(session_id).await.expect("open session");
        let state = session.state().await.expect("state").expect("present");
        assert_eq!(state.approvals.len(), 1, "the folded approval was recorded");
    }
}

#[tokio::test]
async fn matrix_plan_accept_then_complete_under_both_modes() {
    for encrypted in [false, true] {
        let (orchestrator, user_id, session_id) =
            scripted(encrypted, vec![completed_response(), completed_response()]).await;
        let mut request = sample_execute_request();
        request.input.command = Some(smista_core::intent::TaskIntent::Plan);

        let (_outcome, trail) = drive(
            &orchestrator,
            user_id,
            session_id,
            request,
            None,
            ApprovalDecision::Approved,
        )
        .await;
        assert_eq!(trail.first(), Some(&Tag::Approval), "planning pauses first");

        let sessions = Sessions::new(orchestrator.database.clone(), user_id);
        let session = sessions.open(session_id).await.expect("open session");
        let state = session.state().await.expect("state").expect("present");
        assert_eq!(state.plans.len(), 1);
        assert_eq!(
            state.plans[0].status,
            smista_storage::entity::PlanStatus::Approved
        );
        let (input, _) = session
            .run_input()
            .await
            .expect("run input")
            .expect("present");
        assert!(!input.plan_active, "accepting the plan leaves plan mode");
    }
}

#[tokio::test]
async fn matrix_plan_reject_idles_under_both_modes() {
    for encrypted in [false, true] {
        let (orchestrator, user_id, session_id) =
            scripted(encrypted, vec![completed_response()]).await;
        let mut request = sample_execute_request();
        request.input.command = Some(smista_core::intent::TaskIntent::Plan);

        let (outcome, _trail) = drive(
            &orchestrator,
            user_id,
            session_id,
            request,
            None,
            ApprovalDecision::Rejected,
        )
        .await;
        assert!(matches!(outcome, TurnOutcome::Idle { .. }));

        let sessions = Sessions::new(orchestrator.database.clone(), user_id);
        let session = sessions.open(session_id).await.expect("open session");
        let state = session.state().await.expect("state").expect("present");
        assert_eq!(
            state.plans[0].status,
            smista_storage::entity::PlanStatus::Rejected
        );
    }
}
