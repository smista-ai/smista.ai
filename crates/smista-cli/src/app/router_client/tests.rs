use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use smista_mock_web_server::{Endpoint, EndpointStatus, MockRouter, ResponseTemplate};
use smista_sdk::client::{Client as _, ReqwestClient, RouterClientConfig};
use smista_sdk::core::api::{
    EncryptedPayload, GetSessionResponse, MessageContent, SessionDetail, SessionMessageDetail,
    TraceResponse,
};
use smista_sdk::core::intent::TaskIntent;
use smista_sdk::core::message::MessageRole;
use smista_sdk::core::model::Provider as CoreProvider;
use smista_sdk::core::trace::{
    Payload, RoutingDecisionPayload, Trace, TraceEvent as CoreTraceEvent, TraceEventPayload,
    TraceEventType,
};
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::*;
use crate::config::Config;
use crate::credentials::{
    ApiKeyStorage, CredentialBackend, CredentialsStorage, E2eeKeysCredentials, ProvidersCredentials,
};
use crate::skills::SkillStore;

#[tokio::test]
async fn should_ignore_scaffold_commands_until_cancelled() {
    let exit = CancellationToken::new();
    let context = app_context(exit.clone());
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
    let (msg_tx, _msg_rx) = tokio::sync::mpsc::channel(1);
    let worker = RouterClient::new(cmd_rx, msg_tx, context).run();

    cmd_tx
        .send(Cmd::Execute {
            prompt: "hello".to_owned(),
            files: HashMap::default(),
            plan: false,
            explicit_model: None,
        })
        .await
        .expect("router worker receives scaffold commands");
    tokio::time::sleep(Duration::from_millis(10)).await;

    exit.cancel();
    tokio::time::timeout(Duration::from_secs(1), worker)
        .await
        .expect("router worker stops after cancellation")
        .expect("router worker does not panic on scaffold commands");
}

#[tokio::test]
async fn idle_rejects_continue() {
    let mut router_client = router_client_with_state(State::Idle);

    let handled = router_client
        .handle_cmd(Cmd::Continue(cmd::ContinueExecution::Break))
        .await;

    assert!(!handled);
}

#[tokio::test]
async fn non_idle_rejects_execute() {
    for state in non_idle_states() {
        let mut router_client = router_client_with_state(state);

        let handled = router_client
            .handle_cmd(Cmd::Execute {
                prompt: "hello".to_owned(),
                files: HashMap::default(),
                plan: false,
                explicit_model: None,
            })
            .await;

        assert!(!handled);
    }
}

#[tokio::test]
async fn execute_and_preview_commands_accept_explicit_model_overrides() {
    let mut execute_client = router_client_with_state(State::Idle);
    let handled_execute = execute_client
        .handle_cmd(Cmd::Execute {
            prompt: "hello".to_owned(),
            files: HashMap::default(),
            plan: false,
            explicit_model: Some(
                "anthropic/claude-sonnet"
                    .parse()
                    .expect("model reference parses"),
            ),
        })
        .await;

    let mut preview_client = router_client_with_state(State::Streaming);
    let handled_preview = preview_client
        .handle_cmd(Cmd::Preview {
            prompt: "hello".to_owned(),
            files: HashMap::default(),
            plan: true,
            explicit_model: Some(
                "ollama/qwen2.5-coder"
                    .parse()
                    .expect("model reference parses"),
            ),
        })
        .await;

    assert!(handled_execute);
    assert!(handled_preview);
}

#[tokio::test]
async fn break_and_inject_are_valid_from_every_non_idle_state() {
    for state in non_idle_states() {
        let mut break_client = router_client_with_state(state.clone());
        let break_handled = break_client
            .continue_execution(cmd::ContinueExecution::Break)
            .await;

        let mut inject_client = router_client_with_state(state);
        let inject_handled = inject_client
            .continue_execution(cmd::ContinueExecution::Inject {
                messages: Vec::new(),
            })
            .await;

        assert!(break_handled);
        assert!(inject_handled);
    }
}

#[tokio::test]
async fn tool_results_only_match_awaiting_tool() {
    for state in all_states() {
        let mut router_client = router_client_with_state(state.clone());

        let handled = router_client
            .continue_execution(cmd::ContinueExecution::ToolResults {
                results: Vec::new(),
            })
            .await;

        assert_eq!(handled, state == State::AwaitingTool);
    }
}

#[tokio::test]
async fn approval_decisions_only_match_awaiting_approval() {
    for state in all_states() {
        let mut router_client = router_client_with_state(state.clone());

        let handled = router_client
            .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
                decisions: Vec::new(),
            })
            .await;

        assert_eq!(handled, state == State::AwaitingApproval);
    }
}

#[tokio::test]
async fn handle_cmd_dispatches_catalog_and_status_commands() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;

    assert!(router_client.handle_cmd(Cmd::GetRouterStatus).await);
    assert!(router_client.handle_cmd(Cmd::ListModels).await);
    assert!(router_client.handle_cmd(Cmd::ListProviders).await);

    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::RouterStatus(msg::RouterStatus {
            status: "ok".to_owned(),
            version: "0.0.0".to_owned(),
        })
    );
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::ModelsList(models) if models.len() == 1
    ));
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::ProvidersList(providers) if providers.len() == 1
    ));
}

#[tokio::test]
async fn handle_cmd_dispatches_session_usage_trace_and_clear_commands() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::GetTraces,
            ResponseTemplate::new(200).set_body_json(trace_response(TraceEventPayload::Plaintext(
                routing_payload(),
            ))),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;

    assert!(router_client.handle_cmd(Cmd::ListSessions).await);
    assert!(
        router_client
            .handle_cmd(Cmd::ResumeSession(Uuid::nil()))
            .await
    );
    assert!(router_client.handle_cmd(Cmd::GetUsage).await);
    assert!(router_client.handle_cmd(Cmd::GetTrace).await);
    assert!(router_client.handle_cmd(Cmd::Clear).await);

    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::SessionsList(sessions) if sessions.len() == 1
    ));
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::ResumedSession(session) if session.id == Uuid::nil()
    ));
    assert!(matches!(recv_msg(&mut msg_rx).await, Msg::Usage(_)));
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::Trace(trace) if trace.events.len() == 1
    ));
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session_id, None);
}

#[tokio::test]
async fn status_handler_emits_router_status() {
    let router = MockRouter::start().await;
    let (router_client, mut msg_rx) = router_client_for_mock(&router).await;

    router_client.get_router_status().await;

    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::RouterStatus(msg::RouterStatus {
            status: "ok".to_string(),
            version: "0.0.0".to_string(),
        })
    );
}

#[tokio::test]
async fn catalog_handlers_emit_models_and_providers() {
    let router = MockRouter::start().await;
    let (router_client, mut msg_rx) = router_client_for_mock(&router).await;

    router_client.list_models().await;
    router_client.list_providers().await;

    let Msg::ModelsList(models) = recv_msg(&mut msg_rx).await else {
        panic!("models list message expected");
    };
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].provider, "ollama");
    assert_eq!(models[0].id, "qwen2.5-coder");
    assert_eq!(models[0].display_name, "qwen2.5-coder");

    let Msg::ProvidersList(providers) = recv_msg(&mut msg_rx).await else {
        panic!("providers list message expected");
    };
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].name, "OpenAI");
    assert!(!providers[0].local);
}

#[tokio::test]
async fn session_list_uses_workspace_scope() {
    let router = MockRouter::start().await;
    let (router_client, mut msg_rx) = router_client_for_mock(&router).await;

    router_client.list_sessions().await;

    let Msg::SessionsList(sessions) = recv_msg(&mut msg_rx).await else {
        panic!("sessions list message expected");
    };
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].title.as_deref(),
        Some("Refactor auth middleware")
    );

    let requests = router.received_requests().await;
    let request = requests
        .iter()
        .find(|request| request.url.path() == "/api/v1/sessions")
        .expect("list sessions request was sent");
    assert_eq!(
        request.url.query_pairs().find(|(name, _)| name == "scope"),
        Some(("scope".into(), router_client.context.cwd.to_string_lossy()))
    );
}

#[tokio::test]
async fn resume_session_maps_messages_and_resets_state() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::AwaitingTool;
    router_client.session_id = Some(Uuid::nil());
    router_client
        .approvals
        .approve("git commit -m test")
        .expect("approval records");

    router_client.resume_session(Uuid::nil()).await;

    let Msg::ResumedSession(session) = recv_msg(&mut msg_rx).await else {
        panic!("resumed session message expected");
    };
    assert_eq!(session.id, Uuid::nil());
    assert_eq!(session.title, "Refactor auth middleware");
    assert_eq!(session.messages.len(), 2);
    assert_eq!(session.messages[0].role, "user");
    assert_eq!(session.messages[0].content, "Refactor the auth middleware.");
    assert_eq!(router_client.session_id, Some(Uuid::nil()));
    assert_eq!(router_client.state, State::Idle);
    assert!(
        !router_client
            .approvals
            .approved("git commit --amend")
            .expect("approval lookup succeeds")
    );

    let requests = router.received_requests().await;
    assert!(
        requests
            .iter()
            .any(|request| request.url.path().ends_with("/continue"))
    );
    assert!(
        requests
            .iter()
            .any(|request| request.url.path() == format!("/api/v1/sessions/{}", Uuid::nil()))
    );
}

#[tokio::test]
async fn usage_handler_emits_current_session_usage() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.session_id = Some(Uuid::nil());

    router_client.get_usage().await;

    let Msg::Usage(usage) = recv_msg(&mut msg_rx).await else {
        panic!("usage message expected");
    };
    assert_eq!(usage.total.input_tokens, Some(1_200));
    assert_eq!(usage.total.output_tokens, Some(500));
}

#[tokio::test]
async fn trace_handler_emits_plaintext_events() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::GetTraces,
            ResponseTemplate::new(200).set_body_json(trace_response(TraceEventPayload::Plaintext(
                routing_payload(),
            ))),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.session_id = Some(Uuid::nil());

    router_client.get_traces().await;

    let Msg::Trace(trace) = recv_msg(&mut msg_rx).await else {
        panic!("trace message expected");
    };
    assert_eq!(trace.events.len(), 1);
    assert_eq!(trace.events[0].event_type, "routing_decision");
    assert_eq!(trace.events[0].task_type, "edit");
    assert_eq!(trace.events[0].provider, "anthropic");
    assert!(
        trace.events[0]
            .payload
            .contains("\"type\":\"routing_decision\"")
    );
}

#[tokio::test]
async fn trace_handler_deserializes_decrypted_payload_json() {
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let e2ee_keys = E2eeKeysCredentials::new(credentials.clone(), &cwd);
    let key_id = e2ee_keys.create_key().expect("E2EE key is created");
    let plaintext = serde_json::to_string(&routing_payload()).expect("payload serializes");
    let encrypted = e2ee_keys
        .encrypt_payload(&key_id, &plaintext)
        .expect("payload encrypts");
    let router = MockRouter::builder()
        .respond(
            Endpoint::GetTraces,
            ResponseTemplate::new(200)
                .set_body_json(trace_response(TraceEventPayload::Encrypted(encrypted))),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    router_client.session_id = Some(Uuid::nil());

    router_client.get_traces().await;

    let Msg::Trace(trace) = recv_msg(&mut msg_rx).await else {
        panic!("trace message expected");
    };
    assert_eq!(trace.events.len(), 1);
    assert!(
        trace.events[0]
            .payload
            .contains("\"reason\":\"best for edit\"")
    );
}

#[tokio::test]
async fn trace_handler_rejects_missing_active_session_before_requesting_router() {
    let router = MockRouter::start().await;
    let (router_client, mut msg_rx) = router_client_for_mock(&router).await;

    router_client.get_traces().await;

    assert_error_contains(&mut msg_rx, "No active session").await;
    let requests = router.received_requests().await;
    assert!(
        requests
            .iter()
            .all(|request| !request.url.path().ends_with("/traces"))
    );
}

#[tokio::test]
async fn trace_handler_emits_empty_trace_for_empty_response() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.session_id = Some(Uuid::nil());

    router_client.get_traces().await;

    let Msg::Trace(trace) = recv_msg(&mut msg_rx).await else {
        panic!("trace message expected");
    };
    assert!(trace.events.is_empty());
}

#[tokio::test]
async fn trace_handler_reports_decryption_errors_and_skips_payload() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::GetTraces,
            ResponseTemplate::new(200).set_body_json(trace_response(TraceEventPayload::Encrypted(
                invalid_encrypted_payload(),
            ))),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.session_id = Some(Uuid::nil());

    router_client.get_traces().await;

    assert_error_contains(&mut msg_rx, "Failed to decrypt trace event payload").await;
    let Msg::Trace(trace) = recv_msg(&mut msg_rx).await else {
        panic!("trace message expected");
    };
    assert!(trace.events.is_empty());
}

#[tokio::test]
async fn trace_handler_reports_invalid_decrypted_payload_json() {
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let e2ee_keys = E2eeKeysCredentials::new(credentials.clone(), &cwd);
    let key_id = e2ee_keys.create_key().expect("E2EE key is created");
    let encrypted = e2ee_keys
        .encrypt_payload(&key_id, "not json")
        .expect("payload encrypts");
    let router = MockRouter::builder()
        .respond(
            Endpoint::GetTraces,
            ResponseTemplate::new(200)
                .set_body_json(trace_response(TraceEventPayload::Encrypted(encrypted))),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    router_client.session_id = Some(Uuid::nil());

    router_client.get_traces().await;

    assert_error_contains(&mut msg_rx, "Failed to parse trace event payload").await;
    let Msg::Trace(trace) = recv_msg(&mut msg_rx).await else {
        panic!("trace message expected");
    };
    assert!(trace.events.is_empty());
}

#[tokio::test]
async fn clear_interrupts_active_run_resets_state_and_emits_idle() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session_id = Some(Uuid::nil());
    router_client
        .approvals
        .approve("git commit -m test")
        .expect("approval records");

    router_client.clear_session().await;

    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session_id, None);
    assert!(
        !router_client
            .approvals
            .approved("git commit --amend")
            .expect("approval lookup succeeds")
    );

    let requests = router.received_requests().await;
    assert!(
        requests
            .iter()
            .any(|request| request.url.path().ends_with("/continue"))
    );
}

#[tokio::test]
async fn clear_reports_interrupt_errors_then_resets_state_and_emits_idle() {
    let router = MockRouter::builder()
        .endpoint_status(Endpoint::ContinueRun, EndpointStatus::NotFound)
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session_id = Some(Uuid::nil());

    router_client.clear_session().await;

    assert_error_contains(&mut msg_rx, "Failed to terminate active run").await;
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session_id, None);
}

#[tokio::test]
async fn resume_session_reports_interrupt_errors_then_loads_session() {
    let router = MockRouter::builder()
        .endpoint_status(Endpoint::ContinueRun, EndpointStatus::NotFound)
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session_id = Some(Uuid::nil());

    router_client.resume_session(Uuid::nil()).await;

    assert_error_contains(&mut msg_rx, "Failed to terminate active run").await;
    let Msg::ResumedSession(session) = recv_msg(&mut msg_rx).await else {
        panic!("resumed session message expected");
    };
    assert_eq!(session.id, Uuid::nil());
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session_id, Some(Uuid::nil()));
}

#[tokio::test]
async fn resume_session_reports_decryption_errors_and_skips_messages() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::GetSession,
            ResponseTemplate::new(200).set_body_json(session_response_with_messages(vec![
                SessionMessageDetail {
                    role: MessageRole::User,
                    content: MessageContent::Encrypted(invalid_encrypted_payload()),
                    provider: None,
                    model: None,
                },
                SessionMessageDetail {
                    role: MessageRole::Assistant,
                    content: MessageContent::Plaintext("visible reply".to_owned()),
                    provider: Some(CoreProvider::Anthropic),
                    model: Some("claude-sonnet".to_owned()),
                },
            ])),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;

    router_client.resume_session(Uuid::nil()).await;

    assert_error_contains(&mut msg_rx, "Failed to decrypt message content").await;
    let Msg::ResumedSession(session) = recv_msg(&mut msg_rx).await else {
        panic!("resumed session message expected");
    };
    assert_eq!(session.messages.len(), 1);
    assert_eq!(session.messages[0].role, "assistant");
    assert_eq!(session.messages[0].content, "visible reply");
}

#[tokio::test]
async fn handlers_emit_errors_for_router_failures_and_missing_sessions() {
    let router = MockRouter::builder()
        .endpoint_status(Endpoint::Status, EndpointStatus::ServerError)
        .endpoint_status(Endpoint::ListModels, EndpointStatus::ServerError)
        .endpoint_status(Endpoint::ListProviders, EndpointStatus::ServerError)
        .endpoint_status(Endpoint::ListSessions, EndpointStatus::ServerError)
        .endpoint_status(Endpoint::GetSession, EndpointStatus::NotFound)
        .endpoint_status(Endpoint::GetTraces, EndpointStatus::NotFound)
        .endpoint_status(Endpoint::SessionUsage, EndpointStatus::NotFound)
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;

    router_client.get_router_status().await;
    assert_error_contains(&mut msg_rx, "Failed to get router health status").await;

    router_client.list_models().await;
    assert_error_contains(&mut msg_rx, "Failed to list models").await;

    router_client.list_providers().await;
    assert_error_contains(&mut msg_rx, "Failed to list providers").await;

    router_client.list_sessions().await;
    assert_error_contains(&mut msg_rx, "Failed to list sessions").await;

    router_client.get_usage().await;
    assert_error_contains(&mut msg_rx, "No active session").await;

    router_client.session_id = Some(Uuid::nil());
    router_client.get_usage().await;
    assert_error_contains(&mut msg_rx, "Failed to get usage statistics").await;
    router_client.session_id = None;

    router_client.resume_session(Uuid::nil()).await;
    assert_error_contains(&mut msg_rx, "Failed to get session").await;

    router_client.session_id = Some(Uuid::nil());
    router_client.get_traces().await;
    assert_error_contains(&mut msg_rx, "Failed to get execution trace").await;
}

#[tokio::test]
async fn send_msg_cancels_exit_when_receiver_is_gone() {
    let exit = CancellationToken::new();
    let context = app_context(exit.clone());
    let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(1);
    drop(msg_rx);
    let router_client = RouterClient::new(cmd_rx, msg_tx, context);

    router_client.send_msg(Msg::Idle).await;

    assert!(exit.is_cancelled());
}

fn all_states() -> Vec<State> {
    vec![
        State::Idle,
        State::AwaitingTool,
        State::AwaitingApproval,
        State::Streaming,
    ]
}

fn non_idle_states() -> Vec<State> {
    all_states()
        .into_iter()
        .filter(|state| *state != State::Idle)
        .collect()
}

fn router_client_with_state(state: State) -> RouterClient {
    let exit = CancellationToken::new();
    let context = app_context(exit);
    let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
    let (msg_tx, _msg_rx) = tokio::sync::mpsc::channel(1);
    let mut router_client = RouterClient::new(cmd_rx, msg_tx, context);
    router_client.state = state;
    router_client
}

async fn router_client_for_mock(router: &MockRouter) -> (RouterClient, Receiver<Msg>) {
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    router_client_for_mock_with_credentials(router, cwd, credentials).await
}

async fn router_client_for_mock_with_credentials(
    router: &MockRouter,
    cwd: PathBuf,
    credentials: Arc<CredentialsStorage>,
) -> (RouterClient, Receiver<Msg>) {
    let exit = CancellationToken::new();
    let context = app_context_for_router(router, exit, cwd, credentials).await;
    let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(8);
    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(8);

    (RouterClient::new(cmd_rx, msg_tx, context), msg_rx)
}

async fn app_context_for_router(
    router: &MockRouter,
    exit: CancellationToken,
    cwd: PathBuf,
    credentials: Arc<CredentialsStorage>,
) -> AppContext {
    let router_client = ReqwestClient::new(RouterClientConfig::new(router.base_url()))
        .expect("test router client builds");
    router_client
        .sign_in()
        .await
        .expect("mock router signs in the test client");

    AppContext {
        api_key: Arc::new(ApiKeyStorage::new(credentials.clone(), &cwd)),
        config: Arc::new(Config::default()),
        cwd: cwd.clone(),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        exit,
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        router_client: Arc::new(router_client),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
    }
}

fn temp_cwd() -> PathBuf {
    tempfile::tempdir()
        .expect("temporary directory is created")
        .keep()
}

fn file_credentials(cwd: &Path) -> Arc<CredentialsStorage> {
    let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets"))
        .expect("test credentials storage builds");
    assert_eq!(credentials.backend(), CredentialBackend::File);
    Arc::new(credentials)
}

async fn recv_msg(msg_rx: &mut Receiver<Msg>) -> Msg {
    msg_rx.recv().await.expect("router client emits a message")
}

async fn assert_error_contains(msg_rx: &mut Receiver<Msg>, expected: &str) {
    let Msg::Error(error) = recv_msg(msg_rx).await else {
        panic!("error message expected");
    };
    assert!(
        error.contains(expected),
        "expected error to contain {expected:?}, got {error:?}"
    );
}

fn routing_payload() -> Payload {
    Payload::RoutingDecision(RoutingDecisionPayload {
        provider: CoreProvider::Anthropic,
        model: "claude-sonnet".to_string(),
        matched_rule: Some("rule".to_string()),
        fallback_used: false,
        override_used: false,
        reason: "best for edit".to_string(),
    })
}

fn invalid_encrypted_payload() -> EncryptedPayload {
    EncryptedPayload {
        version: 1,
        algorithm: "xchacha20poly1305".to_owned(),
        key_id: "missing-key".to_owned(),
        nonce: "invalid-nonce".to_owned(),
        ciphertext: "invalid-ciphertext".to_owned(),
    }
}

fn session_response_with_messages(messages: Vec<SessionMessageDetail>) -> GetSessionResponse {
    GetSessionResponse {
        session: SessionDetail {
            id: Uuid::nil(),
            title: "Encrypted transcript".to_owned(),
            scope: None,
            encrypted: true,
            created_at: "2026-05-25T09:00:00Z".parse().expect("timestamp parses"),
            updated_at: "2026-05-25T09:00:00Z".parse().expect("timestamp parses"),
            messages,
            metadata: serde_json::json!({}),
        },
    }
}

fn trace_response(payload: TraceEventPayload) -> TraceResponse {
    TraceResponse {
        trace: Trace {
            session_id: Uuid::nil(),
            events: vec![CoreTraceEvent {
                event_type: TraceEventType::RoutingDecision,
                task_type: TaskIntent::Edit,
                provider: CoreProvider::Anthropic,
                model: "claude-sonnet".to_string(),
                matched_rule: Some("rule".to_string()),
                created_at: "2026-05-25T09:00:00Z".parse().expect("timestamp parses"),
                payload,
            }],
        },
    }
}

fn app_context(exit: CancellationToken) -> AppContext {
    let cwd = tempfile::tempdir()
        .expect("temporary directory is created")
        .keep();
    let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets"))
        .expect("test credentials storage builds");
    assert_eq!(credentials.backend(), CredentialBackend::File);
    let credentials = Arc::new(credentials);
    let router_client = ReqwestClient::new(RouterClientConfig::new(
        Url::parse("http://127.0.0.1:9").expect("test URL parses"),
    ))
    .expect("test router client builds");

    AppContext {
        api_key: Arc::new(ApiKeyStorage::new(credentials.clone(), &cwd)),
        config: Arc::new(Config::default()),
        cwd: cwd.clone(),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        exit,
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        router_client: Arc::new(router_client),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
    }
}
