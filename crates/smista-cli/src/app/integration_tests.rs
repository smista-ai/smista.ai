//! App-level integration tests for interactive CLI flows.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use smista_mock_web_server::{
    Endpoint, MockRouter, Request, ResponseGate, ResponseTemplate, defaults, sse,
};
use smista_sdk::client::{Client as _, ReqwestClient, RouterClientConfig};
use smista_sdk::core::api::{
    ApprovalDecision, ContentRef, ContinueRequest, CreateSessionRequest, CreateSessionResponse,
    ExecuteRequest, GetSessionResponse, ListSessionsResponse, MessageContent, SessionMessageDetail,
    ToolApproval, ToolRequest, TurnEvent, TurnOutcome, TurnResponse,
};
use smista_sdk::core::intent::TaskIntent;
use smista_sdk::core::message::MessageRole;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::input_listener::InputEvent;
use super::tui::{ActiveComponentState, ExecutionTurn, HistoryEntry, RouterState, State};
use super::{App, AppContext, AppLogSink};
use crate::config::Config;
use crate::credentials::{CredentialBackend, CredentialsStorage, E2eeKeysCredentials};
use crate::skills::SkillStore;

const FIRST_ANSWER: &str = "Use a userspace WireGuard backend for portability.";
const FOLLOW_UP_ANSWER: &str = "WGApi<Userspace> is portable; WGApi<Kernel> uses the kernel.";
const FOLLOW_UP_PROMPT: &str = "What is the difference between WGApi<Userspace> and WGApi<Kernel>?";
const SETUP_PROMPT: &str = "Set up WireGuard for this project.";

#[tokio::test]
async fn submits_prompt_and_renders_the_completed_exchange() {
    let router = MockRouter::builder()
        .respond(Endpoint::Execute, completed_stream(FIRST_ANSWER, "trace-1"))
        .start()
        .await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit(SETUP_PROMPT).await;
    let state = app
        .wait_for(|state| has_assistant_message(state, FIRST_ANSWER))
        .await;

    assert_eq!(
        state.history,
        vec![
            HistoryEntry::UserMessage(SETUP_PROMPT.to_owned()),
            HistoryEntry::AssistantMessage(FIRST_ANSWER.to_owned()),
        ]
    );
    assert_eq!(state.router, RouterState::Idle);

    let requests = router.received_requests().await;
    assert_eq!(create_session_requests(&requests).len(), 1);
    let execute = execute_requests(&requests);
    assert_eq!(execute.len(), 1);
    assert_eq!(execute[0].input.text, SETUP_PROMPT);

    app.shutdown().await;
}

#[tokio::test]
async fn accepts_follow_up_prompt_after_a_completed_turn() {
    let router = MockRouter::builder()
        .respond_sequence(
            Endpoint::Execute,
            [
                completed_stream(FIRST_ANSWER, "trace-1"),
                completed_stream(FOLLOW_UP_ANSWER, "trace-2"),
            ],
        )
        .start()
        .await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit(SETUP_PROMPT).await;
    app.wait_for(|state| has_assistant_message(state, FIRST_ANSWER))
        .await;
    app.submit(FOLLOW_UP_PROMPT).await;
    let state = app
        .wait_for(|state| has_assistant_message(state, FOLLOW_UP_ANSWER))
        .await;

    assert_eq!(
        state.history,
        vec![
            HistoryEntry::UserMessage(SETUP_PROMPT.to_owned()),
            HistoryEntry::AssistantMessage(FIRST_ANSWER.to_owned()),
            HistoryEntry::UserMessage(FOLLOW_UP_PROMPT.to_owned()),
            HistoryEntry::AssistantMessage(FOLLOW_UP_ANSWER.to_owned()),
        ]
    );
    assert_eq!(state.router, RouterState::Idle);

    let requests = router.received_requests().await;
    assert_eq!(create_session_requests(&requests).len(), 1);
    let prompts = execute_requests(&requests)
        .into_iter()
        .map(|request| request.input.text)
        .collect::<Vec<_>>();
    assert_eq!(prompts, [SETUP_PROMPT, FOLLOW_UP_PROMPT]);

    app.shutdown().await;
}

#[tokio::test]
async fn completes_an_encrypted_decrypt_and_seal_round_trip_with_file_credentials() {
    const ANSWER: &str = "The encrypted continuation completed.";
    const HISTORY: &str = "Earlier encrypted context.";
    const PROMPT: &str = "Continue the encrypted session.";
    const RUN_INPUT: &str = "Encrypted run-input bundle.";
    const STORED_ANSWER: &str = "Router-authored encrypted answer.";

    let cwd = temp_cwd();
    let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets"))
        .expect("test credentials storage builds");
    assert_eq!(credentials.backend(), CredentialBackend::File);
    let credentials = Arc::new(credentials);
    let e2ee_keys = Arc::new(E2eeKeysCredentials::new(credentials, &cwd));
    let key_id = e2ee_keys.create_key().expect("E2EE key is created");
    let history_payload = e2ee_keys
        .encrypt_payload(&key_id, HISTORY)
        .expect("history fixture encrypts");
    let session_id = Uuid::from_u128(3);
    let history_ref = ContentRef::Message("history-1".to_owned());
    let run_input_ref = ContentRef::RunInput(session_id.to_string());
    let answer_ref = ContentRef::Message("answer-1".to_owned());

    let router = MockRouter::builder()
        .respond(
            Endpoint::CreateSession,
            encrypted_create_session_response(session_id, "Encrypted session", &key_id),
        )
        .respond(
            Endpoint::Execute,
            sse(&[TurnEvent::TurnEnd(Box::new(TurnResponse {
                outcome: TurnOutcome::AwaitingDecrypt {
                    to_decrypt: [(history_ref.clone(), history_payload)]
                        .into_iter()
                        .collect(),
                    to_encrypt: [(run_input_ref.clone(), RUN_INPUT.to_owned())]
                        .into_iter()
                        .collect(),
                    trace_id: "trace-decrypt".to_owned(),
                },
                allowed_continuations: Vec::new(),
            }))]),
        )
        .respond_sequence(
            Endpoint::ContinueRun,
            [
                completed_stream_with_seals(
                    ANSWER,
                    "trace-completed",
                    [(answer_ref.clone(), STORED_ANSWER.to_owned())]
                        .into_iter()
                        .collect(),
                ),
                idle_stream("trace-sealed"),
            ],
        )
        .start()
        .await;
    let mut app = App::mock(app_context_with_e2ee(&router, &cwd, e2ee_keys.clone()).await);

    app.submit(PROMPT).await;
    let state = app
        .wait_for(|state| has_assistant_message(state, ANSWER) && state.router == RouterState::Idle)
        .await;
    assert!(has_assistant_message(&state, ANSWER));

    let requests = router.received_requests().await;
    let create_request: CreateSessionRequest = create_session_requests(&requests)[0]
        .body_json()
        .expect("create-session request body decodes");
    assert!(create_request.key_id.is_some());

    let continuations = continue_requests(&requests);
    assert_eq!(continuations.len(), 2);
    let ContinueRequest::Decrypted {
        plaintext,
        encrypted,
    } = &continuations[0]
    else {
        panic!("decrypted continuation expected");
    };
    assert_eq!(plaintext.get(&history_ref), Some(&HISTORY.to_owned()));
    assert_eq!(plaintext.get(&run_input_ref), Some(&RUN_INPUT.to_owned()));
    assert_eq!(
        e2ee_keys
            .decrypt_payload(encrypted.get(&run_input_ref).expect("run input is sealed"))
            .expect("run input decrypts"),
        RUN_INPUT
    );

    let ContinueRequest::Sealed { encrypted } = &continuations[1] else {
        panic!("sealed continuation expected");
    };
    assert_eq!(
        e2ee_keys
            .decrypt_payload(encrypted.get(&answer_ref).expect("answer is sealed"))
            .expect("answer decrypts"),
        STORED_ANSWER
    );
    assert_eq!(
        run_request_paths(&requests),
        ["/execute", "/continue", "/continue"]
    );

    app.shutdown().await;
}

#[tokio::test]
async fn completes_multiple_automatic_tool_continuations() {
    const FIRST_CONTENT: &str = "pub struct Userspace;\n";
    const MULTI_ANSWER: &str = "Both WireGuard backend files are consistent.";
    const MULTI_PROMPT: &str = "Review both WireGuard backend implementations.";
    const SECOND_CONTENT: &str = "pub struct Kernel;\n";

    let router = MockRouter::builder()
        .respond(
            Endpoint::Execute,
            awaiting_tool_stream("read-userspace", "userspace.rs", ToolApproval::Allow),
        )
        .respond_sequence(
            Endpoint::ContinueRun,
            [
                awaiting_tool_stream("read-kernel", "kernel.rs", ToolApproval::Allow),
                completed_stream(MULTI_ANSWER, "trace-multi"),
            ],
        )
        .start()
        .await;
    let cwd = temp_cwd();
    std::fs::write(cwd.join("userspace.rs"), FIRST_CONTENT).expect("userspace fixture is written");
    std::fs::write(cwd.join("kernel.rs"), SECOND_CONTENT).expect("kernel fixture is written");
    let mut app = start_app(&router, &cwd).await;

    app.submit(MULTI_PROMPT).await;
    let state = app
        .wait_for(|state| {
            has_assistant_message(state, MULTI_ANSWER) && state.router == RouterState::Idle
        })
        .await;

    assert!(state.history.iter().any(|entry| {
        matches!(
            entry,
            HistoryEntry::ToolCall { input, .. } if input == "read-userspace"
        )
    }));
    assert!(state.history.iter().any(|entry| {
        matches!(
            entry,
            HistoryEntry::ToolCall { input, .. } if input == "read-kernel"
        )
    }));

    let requests = router.received_requests().await;
    let continuations = continue_requests(&requests);
    assert_eq!(continuations.len(), 2);
    let contents = continuations
        .into_iter()
        .map(|request| {
            let ContinueRequest::ToolResults { results, .. } = request else {
                panic!("tool results continuation expected");
            };
            assert_eq!(results.len(), 1);
            (results[0].call_id.clone(), results[0].content.clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        contents,
        [
            ("read-userspace".to_owned(), FIRST_CONTENT.to_owned()),
            ("read-kernel".to_owned(), SECOND_CONTENT.to_owned()),
        ]
    );
    assert_eq!(
        run_request_paths(&requests),
        ["/execute", "/continue", "/continue"]
    );

    app.shutdown().await;
}

#[tokio::test]
async fn interrupts_an_execute_request_while_the_model_is_processing() {
    const INTERRUPTED_ANSWER: &str = "This answer must never reach the console.";
    const INTERRUPTED_PROMPT: &str = "Perform a long WireGuard analysis.";
    const RECOVERY_ANSWER: &str = "The new prompt ran after the interruption.";
    const RECOVERY_PROMPT: &str = "Give me the short version instead.";

    let execute_gate = ResponseGate::new();
    let router = MockRouter::builder()
        .respond_sequence(
            Endpoint::Execute,
            [
                completed_stream(INTERRUPTED_ANSWER, "trace-interrupted")
                    .set_gate(execute_gate.clone()),
                completed_stream(RECOVERY_ANSWER, "trace-recovery"),
            ],
        )
        .respond(Endpoint::ContinueRun, idle_stream("trace-break"))
        .start()
        .await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit(INTERRUPTED_PROMPT).await;
    tokio::time::timeout(Duration::from_secs(5), execute_gate.wait_until_blocked())
        .await
        .expect("execute request reaches the response gate");
    app.wait_for(|state| matches!(state.router, RouterState::Thinking(_)))
        .await;

    app.send(InputEvent::Escape).await;
    app.wait_for(|state| state.router == RouterState::Idle)
        .await;
    app.submit(RECOVERY_PROMPT).await;
    let state = app
        .wait_for(|state| {
            has_assistant_message(state, RECOVERY_ANSWER) && state.router == RouterState::Idle
        })
        .await;
    execute_gate.open();

    assert!(!has_assistant_message(&state, INTERRUPTED_ANSWER));
    assert!(has_assistant_message(&state, RECOVERY_ANSWER));

    let requests = router.received_requests().await;
    assert_eq!(
        run_request_paths(&requests),
        ["/execute", "/continue", "/execute"]
    );
    assert_eq!(continue_requests(&requests), [ContinueRequest::Break]);
    assert_eq!(create_session_requests(&requests).len(), 1);

    app.shutdown().await;
}

#[tokio::test]
async fn reviews_attached_file_and_continues_with_a_local_tool_result() {
    const MAIN_CONTENT: &str = "mod vpn;\nfn main() { vpn::start(); }\n";
    const VPN_CONTENT: &str = "pub fn start() { /* userspace tunnel */ }\n";
    const REVIEW_ANSWER: &str = "The WireGuard startup path is consistent.";
    const REVIEW_PROMPT: &str = "Review @main.rs";

    let router = MockRouter::builder()
        .respond(
            Endpoint::Execute,
            awaiting_tool_stream("read-vpn", "vpn.rs", ToolApproval::Allow),
        )
        .respond(
            Endpoint::ContinueRun,
            completed_stream(REVIEW_ANSWER, "trace-review"),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    std::fs::write(cwd.join("main.rs"), MAIN_CONTENT).expect("main fixture is written");
    std::fs::write(cwd.join("vpn.rs"), VPN_CONTENT).expect("vpn fixture is written");
    let mut app = start_app(&router, &cwd).await;

    app.submit(REVIEW_PROMPT).await;
    let state = app
        .wait_for(|state| has_assistant_message(state, REVIEW_ANSWER))
        .await;

    assert!(state.history.iter().any(|entry| {
        matches!(
            entry,
            HistoryEntry::ToolCall { name, input }
                if name == "read_file" && input == "read-vpn"
        )
    }));
    assert!(has_assistant_message(&state, REVIEW_ANSWER));

    let requests = router.received_requests().await;
    let execute = execute_requests(&requests)
        .into_iter()
        .next()
        .expect("execute request is sent");
    assert_eq!(execute.input.text, "Review main.rs");
    let main_path = cwd.join("main.rs");
    assert_eq!(
        execute.workspace.referenced_paths.as_slice(),
        std::slice::from_ref(&main_path)
    );
    let main = execute
        .attachments
        .files
        .iter()
        .find(|file| file.path == main_path)
        .expect("main.rs is attached");
    assert_eq!(main.content, MAIN_CONTENT);

    let continuation = continue_requests(&requests)
        .into_iter()
        .next()
        .expect("tool continuation is sent");
    let ContinueRequest::ToolResults { results, encrypted } = continuation else {
        panic!("tool results continuation expected");
    };
    assert!(encrypted.is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, "read-vpn");
    assert_eq!(results[0].content, VPN_CONTENT);
    assert!(!results[0].is_error);
    assert_eq!(results[0].decision, None);

    app.shutdown().await;
}

#[tokio::test]
async fn plan_mode_approves_a_tool_and_completes_its_continuation() {
    const FILE_CONTENT: &str = "pub struct Tunnel;\n";
    const PLAN_ANSWER: &str = "Plan: preserve the tunnel abstraction.";
    const PLAN_PROMPT: &str = "Plan the tunnel refactor.";

    let router = MockRouter::builder()
        .respond(
            Endpoint::Execute,
            awaiting_tool_stream("plan-read", "vpn.rs", ToolApproval::Ask),
        )
        .respond(
            Endpoint::ContinueRun,
            completed_stream(PLAN_ANSWER, "trace-plan"),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    std::fs::write(cwd.join("vpn.rs"), FILE_CONTENT).expect("vpn fixture is written");
    let mut app = start_app(&router, &cwd).await;

    app.submit("/plan").await;
    app.wait_for(|state| state.plan).await;
    app.submit(PLAN_PROMPT).await;
    let approval_state = app
        .wait_for(|state| {
            matches!(
                state.execution_turn,
                Some(ExecutionTurn::Approval(ref prompt)) if prompt.id == "plan-read"
            )
        })
        .await;
    assert!(
        matches!(
            approval_state.execution_turn,
            Some(ExecutionTurn::Approval(ref prompt)) if prompt.id == "plan-read"
        ),
        "approval remains visible as the live execution turn"
    );

    app.send(InputEvent::Char('y')).await;
    let final_state = app
        .wait_for(|state| {
            has_assistant_message(state, PLAN_ANSWER) && state.router == RouterState::Idle
        })
        .await;
    assert!(final_state.plan);

    let requests = router.received_requests().await;
    let execute = execute_requests(&requests)
        .into_iter()
        .next()
        .expect("execute request is sent");
    assert_eq!(execute.input.command, Some(TaskIntent::Plan));
    let continuation = continue_requests(&requests)
        .into_iter()
        .next()
        .expect("approved tool continuation is sent");
    let ContinueRequest::ToolResults { results, .. } = continuation else {
        panic!("tool results continuation expected");
    };
    assert_eq!(results[0].content, FILE_CONTENT);
    assert_eq!(results[0].decision, Some(ApprovalDecision::Approved));
    assert!(!results[0].is_error);

    app.shutdown().await;
}

#[tokio::test]
async fn rejected_plan_tool_returns_an_error_and_accepts_the_next_prompt() {
    const AFTER_REJECTION: &str = "I will plan without reading that file.";
    const NEXT_ANSWER: &str = "The ordinary follow-up is complete.";
    const NEXT_PROMPT: &str = "Summarize the approach.";

    let router = MockRouter::builder()
        .respond_sequence(
            Endpoint::Execute,
            [
                awaiting_tool_stream("reject-read", "vpn.rs", ToolApproval::Ask),
                completed_stream(NEXT_ANSWER, "trace-next"),
            ],
        )
        .respond(
            Endpoint::ContinueRun,
            completed_stream(AFTER_REJECTION, "trace-rejected"),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    std::fs::write(cwd.join("vpn.rs"), "private details\n").expect("vpn fixture is written");
    let mut app = start_app(&router, &cwd).await;

    app.submit("/plan").await;
    app.wait_for(|state| state.plan).await;
    app.submit("Plan without changing files.").await;
    app.wait_for(|state| {
        matches!(
            state.execution_turn,
            Some(ExecutionTurn::Approval(ref prompt)) if prompt.id == "reject-read"
        )
    })
    .await;
    app.send(InputEvent::Char('n')).await;
    app.wait_for(|state| {
        has_assistant_message(state, AFTER_REJECTION) && state.router == RouterState::Idle
    })
    .await;

    app.submit("/chat").await;
    app.wait_for(|state| !state.plan).await;
    app.submit(NEXT_PROMPT).await;
    let final_state = app
        .wait_for(|state| {
            has_assistant_message(state, NEXT_ANSWER) && state.router == RouterState::Idle
        })
        .await;
    assert!(has_assistant_message(&final_state, AFTER_REJECTION));
    assert!(has_assistant_message(&final_state, NEXT_ANSWER));
    assert!(
        final_state
            .history
            .contains(&HistoryEntry::UserMessage(NEXT_PROMPT.to_owned()))
    );

    let requests = router.received_requests().await;
    let continuation = continue_requests(&requests)
        .into_iter()
        .next()
        .expect("rejected tool continuation is sent");
    let ContinueRequest::ToolResults { results, .. } = continuation else {
        panic!("tool results continuation expected");
    };
    assert_eq!(results[0].decision, Some(ApprovalDecision::Rejected));
    assert!(results[0].is_error);
    assert!(results[0].content.contains("rejected"));

    let prompts = execute_requests(&requests)
        .into_iter()
        .map(|request| request.input.text)
        .collect::<Vec<_>>();
    assert_eq!(prompts, ["Plan without changing files.", NEXT_PROMPT]);

    app.shutdown().await;
}

#[tokio::test]
async fn clear_fetches_usage_and_ends_the_local_session_without_deleting_it() {
    let router = MockRouter::builder()
        .respond(Endpoint::Execute, completed_stream(FIRST_ANSWER, "trace-1"))
        .start()
        .await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit(SETUP_PROMPT).await;
    app.wait_for(|state| has_assistant_message(state, FIRST_ANSWER))
        .await;
    app.submit("/clear").await;
    let state = app
        .wait_for(|state| {
            state.history.iter().any(|entry| {
                matches!(
                    entry,
                    HistoryEntry::Notice(notice) if notice.contains("To continue this session")
                )
            })
        })
        .await;

    assert!(!has_assistant_message(&state, FIRST_ANSWER));
    assert!(
        !state
            .history
            .contains(&HistoryEntry::UserMessage(SETUP_PROMPT.to_owned()))
    );
    assert!(state.history.iter().any(|entry| {
        matches!(
            entry,
            HistoryEntry::Notice(notice)
                if notice.contains("total=1700")
                    && notice.contains("cost=USD0.042")
        )
    }));
    assert_eq!(state.router, RouterState::Idle);

    let requests = router.received_requests().await;
    assert_eq!(endpoint_requests(&requests, "/usage").len(), 1);
    assert!(
        requests
            .iter()
            .all(|request| request.method.as_str() != "DELETE")
    );

    app.shutdown().await;
}

#[tokio::test]
async fn resume_picker_restores_the_previous_session_over_the_current_history() {
    const FIRST_SESSION_ANSWER: &str = "First session answer.";
    const SECOND_SESSION_ANSWER: &str = "Second session answer.";
    const RESTORED_USER: &str = "Original restored question.";
    const RESTORED_ASSISTANT: &str = "Original restored answer.";

    let first_id = Uuid::from_u128(1);
    let second_id = Uuid::from_u128(2);
    let router = MockRouter::builder()
        .respond_sequence(
            Endpoint::CreateSession,
            [
                create_session_response(first_id, "First session"),
                create_session_response(second_id, "Second session"),
            ],
        )
        .respond_sequence(
            Endpoint::Execute,
            [
                completed_stream(FIRST_SESSION_ANSWER, "trace-first"),
                completed_stream(SECOND_SESSION_ANSWER, "trace-second"),
            ],
        )
        .respond(
            Endpoint::ListSessions,
            list_sessions_response(first_id, "First session"),
        )
        .respond(
            Endpoint::GetSession,
            get_session_response(
                first_id,
                "First session",
                [
                    (MessageRole::User, RESTORED_USER),
                    (MessageRole::Assistant, RESTORED_ASSISTANT),
                ],
            ),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit("First question.").await;
    app.wait_for(|state| has_assistant_message(state, FIRST_SESSION_ANSWER))
        .await;
    app.submit("/clear").await;
    app.wait_for(|state| {
        state.history.iter().any(|entry| {
            matches!(
                entry,
                HistoryEntry::Notice(notice) if notice.contains(&first_id.to_string())
            )
        })
    })
    .await;
    app.submit("Second question.").await;
    app.wait_for(|state| has_assistant_message(state, SECOND_SESSION_ANSWER))
        .await;

    app.submit("/resume").await;
    app.wait_for(|state| {
        matches!(
            state.active_component,
            ActiveComponentState::SessionsList(ref sessions)
                if sessions.selected().is_some_and(|session| session.id == first_id)
        )
    })
    .await;
    app.send(InputEvent::Enter).await;
    let state = app
        .wait_for(|state| has_assistant_message(state, RESTORED_ASSISTANT))
        .await;

    assert!(!has_assistant_message(&state, SECOND_SESSION_ANSWER));
    assert!(
        !state
            .history
            .contains(&HistoryEntry::UserMessage("Second question.".to_owned()))
    );
    assert!(has_assistant_message(&state, RESTORED_USER));
    assert!(has_assistant_message(&state, RESTORED_ASSISTANT));
    assert!(matches!(
        state.active_component,
        ActiveComponentState::Console(_)
    ));

    let requests = router.received_requests().await;
    assert_eq!(create_session_requests(&requests).len(), 2);
    assert_eq!(endpoint_requests(&requests, "/api/v1/sessions").len(), 3);
    assert!(requests.iter().any(|request| {
        request.method.as_str() == "GET"
            && request.url.path() == format!("/api/v1/sessions/{first_id}")
    }));

    app.shutdown().await;
}

#[tokio::test]
async fn usage_command_displays_positive_tokens_cost_and_currency() {
    let router = MockRouter::builder()
        .respond(Endpoint::Execute, completed_stream(FIRST_ANSWER, "trace-1"))
        .start()
        .await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit(SETUP_PROMPT).await;
    app.wait_for(|state| has_assistant_message(state, FIRST_ANSWER))
        .await;
    app.submit("/usage").await;
    let state = app
        .wait_for(|state| matches!(state.active_component, ActiveComponentState::Usage(_)))
        .await;

    let ActiveComponentState::Usage(usage) = state.active_component else {
        panic!("usage view expected");
    };
    assert_eq!(usage.usage().total.input_tokens, Some(1_200));
    assert_eq!(usage.usage().total.output_tokens, Some(500));
    assert_eq!(usage.usage().total.total_tokens, Some(1_700));
    assert_eq!(
        usage
            .usage()
            .total
            .estimated_cost
            .expect("estimated cost is present")
            .to_string(),
        "0.042"
    );
    assert_eq!(usage.usage().total.currency.as_deref(), Some("USD"));

    let requests = router.received_requests().await;
    assert_eq!(endpoint_requests(&requests, "/usage").len(), 1);

    app.shutdown().await;
}

#[tokio::test]
async fn skills_command_lists_workspace_skills_without_router_requests() {
    let router = MockRouter::start().await;
    let cwd = temp_cwd();
    let skill_dir = cwd.join(".agents").join("skills").join("wireguard-review");
    std::fs::create_dir_all(&skill_dir).expect("skill directory is created");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: wireguard-review\ndescription: Review WireGuard integrations.\n---\nReview carefully.\n",
    )
    .expect("skill fixture is written");
    let baseline_request_count = router.received_requests().await.len();
    let mut app = start_app(&router, &cwd).await;
    let signed_in_request_count = router.received_requests().await.len();
    assert_eq!(signed_in_request_count, baseline_request_count + 1);

    app.submit("/skills").await;
    let state = app
        .wait_for(|state| matches!(state.active_component, ActiveComponentState::SkillList(_)))
        .await;

    let ActiveComponentState::SkillList(skills) = state.active_component else {
        panic!("skill list expected");
    };
    let (_, skill) = skills
        .entries()
        .iter()
        .find(|(name, _)| name == "wireguard-review")
        .expect("workspace skill is listed");
    assert_eq!(skill.description(), "Review WireGuard integrations.");
    assert_eq!(
        router.received_requests().await.len(),
        signed_in_request_count
    );

    app.shutdown().await;
}

#[tokio::test]
async fn preview_renders_a_report_without_calling_model_execute() {
    let router = MockRouter::start().await;
    let cwd = temp_cwd();
    let mut app = start_app(&router, &cwd).await;

    app.submit("/preview Review the WireGuard setup.").await;
    let state = app
        .wait_for(|state| {
            state
                .history
                .iter()
                .any(|entry| matches!(entry, HistoryEntry::Preview(_)))
        })
        .await;

    assert!(state.history.iter().any(|entry| {
        matches!(
            entry,
            HistoryEntry::Preview(preview)
                if preview.routing.intent == TaskIntent::Review
                    && preview.estimated_cost_currency == "USD"
        )
    }));
    let requests = router.received_requests().await;
    assert_eq!(endpoint_requests(&requests, "/preview").len(), 1);
    assert!(endpoint_requests(&requests, "/execute").is_empty());

    app.shutdown().await;
}

async fn start_app(router: &MockRouter, cwd: &Path) -> super::AppTestDriver {
    App::mock(app_context(router, cwd).await)
}

async fn app_context(router: &MockRouter, cwd: &Path) -> AppContext {
    let credentials = CredentialsStorage::new_file_for_tests(cwd.join("global-secrets"))
        .expect("test credentials storage builds");
    assert_eq!(credentials.backend(), CredentialBackend::File);
    let credentials = Arc::new(credentials);
    let e2ee_keys = Arc::new(E2eeKeysCredentials::new(credentials, cwd));
    app_context_with_e2ee(router, cwd, e2ee_keys).await
}

async fn app_context_with_e2ee(
    router: &MockRouter,
    cwd: &Path,
    e2ee_keys: Arc<E2eeKeysCredentials>,
) -> AppContext {
    let router_client = ReqwestClient::new(RouterClientConfig::new(router.base_url()))
        .expect("test router client builds");
    router_client
        .sign_in()
        .await
        .expect("mock router signs in the test client");

    AppContext {
        config: Arc::new(Config::default()),
        cwd: cwd.to_path_buf(),
        e2ee_keys,
        exit: CancellationToken::new(),
        logs: AppLogSink::new(),
        router_client: Arc::new(router_client),
        skills_store: Arc::new(SkillStore::discover(cwd)),
    }
}

fn temp_cwd() -> PathBuf {
    tempfile::tempdir()
        .expect("temporary directory is created")
        .keep()
}

fn completed_stream(message: &str, trace_id: &str) -> ResponseTemplate {
    let mut response = defaults::turn();
    let TurnOutcome::Completed(turn) = &mut response.outcome else {
        panic!("default completed turn expected");
    };
    turn.message.content = message.to_owned();
    turn.trace_id = trace_id.to_owned();
    sse(&[TurnEvent::TurnEnd(Box::new(response))])
}

fn completed_stream_with_seals(
    message: &str,
    trace_id: &str,
    to_encrypt: std::collections::BTreeMap<ContentRef, String>,
) -> ResponseTemplate {
    let mut response = defaults::turn();
    let TurnOutcome::Completed(turn) = &mut response.outcome else {
        panic!("default completed turn expected");
    };
    turn.message.content = message.to_owned();
    turn.trace_id = trace_id.to_owned();
    turn.to_encrypt = to_encrypt;
    sse(&[TurnEvent::TurnEnd(Box::new(response))])
}

fn awaiting_tool_stream(
    call_id: &str,
    path: &str,
    requires_approval: ToolApproval,
) -> ResponseTemplate {
    let request = ToolRequest {
        call_id: call_id.to_owned(),
        name: "read_file".to_owned(),
        arguments: serde_json::json!({ "path": path }),
        requires_approval,
    };
    sse(&[
        TurnEvent::ToolCallStarted {
            call_id: call_id.to_owned(),
            name: "read_file".to_owned(),
        },
        TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingTool {
                tool_requests: vec![request],
                to_encrypt: Default::default(),
                trace_id: format!("trace-{call_id}"),
            },
            allowed_continuations: Vec::new(),
        })),
    ])
}

fn idle_stream(trace_id: &str) -> ResponseTemplate {
    sse(&[TurnEvent::TurnEnd(Box::new(TurnResponse {
        outcome: TurnOutcome::Idle {
            trace_id: trace_id.to_owned(),
        },
        allowed_continuations: Vec::new(),
    }))])
}

fn create_session_response(id: Uuid, title: &str) -> ResponseTemplate {
    let mut response: CreateSessionResponse = defaults::create_session();
    response.session.id = id;
    response.session.title = Some(title.to_owned());
    ResponseTemplate::new(201).set_body_json(response)
}

fn encrypted_create_session_response(id: Uuid, title: &str, key_id: &str) -> ResponseTemplate {
    let mut response: CreateSessionResponse = defaults::create_session();
    response.session.id = id;
    response.session.title = Some(title.to_owned());
    response.session.encrypted = true;
    response.session.key_id = Some(key_id.to_owned());
    ResponseTemplate::new(201).set_body_json(response)
}

fn list_sessions_response(id: Uuid, title: &str) -> ResponseTemplate {
    let mut response: ListSessionsResponse = defaults::list_sessions();
    response.sessions[0].id = id;
    response.sessions[0].title = Some(title.to_owned());
    ResponseTemplate::new(200).set_body_json(response)
}

fn get_session_response<const N: usize>(
    id: Uuid,
    title: &str,
    messages: [(MessageRole, &str); N],
) -> ResponseTemplate {
    let mut response: GetSessionResponse = defaults::get_session();
    response.session.id = id;
    response.session.title = title.to_owned();
    response.session.messages = messages
        .into_iter()
        .map(|(role, content)| SessionMessageDetail {
            role,
            content: MessageContent::Plaintext(content.to_owned()),
            provider: None,
            model: None,
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(response)
}

fn has_assistant_message(state: &State, message: &str) -> bool {
    state
        .history
        .contains(&HistoryEntry::AssistantMessage(message.to_owned()))
}

fn create_session_requests(requests: &[Request]) -> Vec<&Request> {
    requests
        .iter()
        .filter(|request| {
            request.method.as_str() == "POST" && request.url.path() == "/api/v1/sessions"
        })
        .collect()
}

fn execute_requests(requests: &[Request]) -> Vec<ExecuteRequest> {
    endpoint_requests(requests, "/execute")
        .into_iter()
        .map(|request| request.body_json().expect("execute request body decodes"))
        .collect()
}

fn continue_requests(requests: &[Request]) -> Vec<ContinueRequest> {
    endpoint_requests(requests, "/continue")
        .into_iter()
        .map(|request| request.body_json().expect("continue request body decodes"))
        .collect()
}

fn endpoint_requests<'a>(requests: &'a [Request], suffix: &str) -> Vec<&'a Request> {
    requests
        .iter()
        .filter(|request| request.url.path().ends_with(suffix))
        .collect()
}

fn run_request_paths(requests: &[Request]) -> Vec<&str> {
    requests
        .iter()
        .filter_map(|request| {
            let path = request.url.path();
            (path.ends_with("/execute") || path.ends_with("/continue")).then_some(
                if path.ends_with("/execute") {
                    "/execute"
                } else {
                    "/continue"
                },
            )
        })
        .collect()
}
