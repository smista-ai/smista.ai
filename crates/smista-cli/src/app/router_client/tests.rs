use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt as _;
use smista_mock_web_server::{
    Endpoint, EndpointStatus, MockRouter, ResponseTemplate, defaults, sse,
};
use smista_sdk::client::{Client as _, ReqwestClient, RouterClientConfig};
use smista_sdk::core::api::{
    ApiErrorBody, ApprovalDecision as ApiApprovalDecision, ApprovalKind, CompletedTurn, ContentRef,
    ContextOutcome, ContinueRequest, CreateSessionRequest, CreateSessionResponse, EncryptedPayload,
    GetSessionResponse, MessageContent, PendingApproval, SessionDetail, SessionMessageDetail,
    SessionSummary, ToolApproval, ToolRequest, TraceResponse, TurnEvent, TurnOutcome, TurnResponse,
};
use smista_sdk::core::intent::TaskIntent;
use smista_sdk::core::message::MessageRole;
use smista_sdk::core::model::Provider as CoreProvider;
use smista_sdk::core::policy::PermissionMode;
use smista_sdk::core::trace::{
    Payload, RoutingDecisionPayload, Trace, TraceEvent as CoreTraceEvent, TraceEventPayload,
    TraceEventType,
};
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::*;
use crate::config::Config;
use crate::credentials::{CredentialBackend, CredentialsStorage, E2eeKeysCredentials};
use crate::skills::SkillStore;
use crate::tools::{ToolCall, ToolExecutor};

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
            files: HashSet::default(),
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
                files: HashSet::default(),
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
            files: HashSet::default(),
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
            files: HashSet::default(),
            plan: true,
            explicit_model: Some(
                "ollama/qwen2.5-coder:7b"
                    .parse()
                    .expect("model reference parses"),
            ),
        })
        .await;

    assert!(handled_execute);
    assert!(handled_preview);
}

#[tokio::test]
async fn build_execute_request_uses_context_config_and_sorted_paths() {
    let mut router_client = router_client_with_state(State::Idle);
    let mut config = Config::default();
    config.local.auto_apply = Some(true);
    config.local.local_only = Some(true);
    config.local.no_network = Some(true);
    config.tools.set("shell", PermissionMode::Deny);
    router_client.context.config = Arc::new(config.clone());

    let request = router_client
        .build_execute_request(
            "review this".to_owned(),
            [PathBuf::from("b.rs"), PathBuf::from("a.rs")]
                .into_iter()
                .collect(),
            true,
            Some(
                "anthropic/claude-sonnet"
                    .parse()
                    .expect("model reference parses"),
            ),
        )
        .await;

    assert_eq!(request.input.text, "review this");
    assert_eq!(request.input.command, Some(TaskIntent::Plan));
    assert_eq!(
        request
            .input
            .explicit_model
            .as_ref()
            .map(ToString::to_string),
        Some("anthropic/claude-sonnet".to_owned())
    );
    assert_eq!(request.workspace.root, router_client.context.cwd);
    assert_eq!(
        request.workspace.referenced_paths,
        vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")]
    );
    assert_eq!(request.workspace.git_branch, None);
    assert_eq!(request.workspace.git_diff, None);
    assert_eq!(request.policy.version, 1);
    assert_eq!(request.policy.source, "merged");
    assert_eq!(request.policy.classification, config.classification);
    assert_eq!(request.policy.routing, config.routing);
    assert_eq!(request.policy.tools, config.tools);
    assert_eq!(request.policy.privacy, config.privacy);
    assert!(request.local_preferences.auto_apply);
    assert!(request.local_preferences.local_only);
    assert!(request.local_preferences.no_network);
}

#[tokio::test]
async fn build_execute_request_attaches_files_instructions_and_available_skills() {
    let mut router_client = router_client_with_state(State::Idle);
    let cwd = router_client.context.cwd.clone();
    let source = cwd.join("src");
    std::fs::create_dir_all(&source).expect("source directory is created");
    std::fs::write(source.join("lib.rs"), "pub fn answer() -> u8 { 42 }\n")
        .expect("context file is written");
    std::fs::write(cwd.join("AGENTS.md"), "Use repository instructions.\n")
        .expect("instructions are written");
    let skill_dir = cwd.join(".agents").join("skills").join("reviewer");
    std::fs::create_dir_all(&skill_dir).expect("skill directory is created");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: reviewer\ndescription: Review code.\n---\nReview carefully.\n",
    )
    .expect("skill is written");
    router_client.context.skills_store = Arc::new(SkillStore::discover(&cwd));

    let request = router_client
        .build_execute_request(
            "explain this file".to_owned(),
            [PathBuf::from("src/lib.rs")].into_iter().collect(),
            false,
            None,
        )
        .await;

    assert_eq!(request.input.command, None);
    assert_eq!(request.attachments.files.len(), 1);
    let file = &request.attachments.files[0];
    assert_eq!(file.path, PathBuf::from("src/lib.rs"));
    assert_eq!(file.content, "pub fn answer() -> u8 { 42 }\n");
    assert!(file.content_hash.starts_with("sha256:"));
    assert_eq!(file.content_hash.len(), "sha256:".len() + 64);
    assert!(file.required);
    assert_eq!(request.attachments.instructions.len(), 1);
    assert_eq!(request.attachments.instructions[0].source, "AGENTS.md");
    assert_eq!(
        request.attachments.instructions[0].content,
        "Use repository instructions.\n"
    );
    assert!(request.attachments.invoked_skills.is_empty());
    let skill = request
        .attachments
        .available_skills
        .iter()
        .find(|skill| skill.name == "reviewer")
        .expect("project skill is available");
    assert_eq!(skill.content, "Review carefully.");
}

#[tokio::test]
async fn build_execute_request_reads_unborn_git_branch_without_shelling_out() {
    let router_client = router_client_with_state(State::Idle);
    let cwd = router_client.context.cwd.clone();
    gix::init(&cwd).expect("git repository is initialized");
    std::fs::write(
        cwd.join(".git").join("HEAD"),
        "ref: refs/heads/smista-test\n",
    )
    .expect("symbolic HEAD is written");

    let request = router_client
        .build_execute_request("plan this".to_owned(), HashSet::new(), false, None)
        .await;

    assert_eq!(request.workspace.git_branch.as_deref(), Some("smista-test"));
}

#[tokio::test]
async fn break_and_inject_are_valid_from_every_non_idle_state() {
    for state in non_idle_states() {
        let mut break_client = router_client_with_session_state(state.clone());
        let break_handled = break_client
            .continue_execution(cmd::ContinueExecution::Break)
            .await;

        let mut inject_client = router_client_with_session_state(state);
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
async fn break_emits_interrupted_before_contacting_router() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::Break)
        .await;

    assert!(handled);
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Interrupted);
}

#[tokio::test]
async fn tool_results_only_match_awaiting_tool() {
    for state in all_states() {
        let mut router_client = router_client_with_session_state(state.clone());

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
        let mut router_client = router_client_with_session_state(state.clone());

        let handled = router_client
            .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
                decisions: Vec::new(),
            })
            .await;

        assert_eq!(
            handled,
            matches!(state, State::AwaitingTool | State::AwaitingApproval)
        );
    }
}

#[tokio::test]
async fn continue_without_session_is_not_handled() {
    let mut router_client = router_client_with_state(State::Streaming);

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::Break)
        .await;

    assert!(!handled);
}

#[tokio::test]
async fn tool_results_continuation_reports_seal_errors() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::AwaitingTool);
    router_client.session = Some(session_info(Uuid::nil(), "Plain session", None));
    router_client.pending_seals.insert(
        ContentRef::Message("message-1".to_owned()),
        "pending content".to_owned(),
    );

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::ToolResults {
            results: vec![cmd::ToolResult {
                call_id: "call-1".to_owned(),
                content: "tool output".to_owned(),
                is_error: false,
            }],
        })
        .await;

    assert!(handled);
    assert_error_contains(&mut msg_rx, "failed to build continuation request").await;
}

#[tokio::test]
async fn tool_approval_decision_reports_missing_request() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::AwaitingTool);
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
            decisions: vec![cmd::ApprovalDecision {
                id: "call-missing".to_owned(),
                outcome: cmd::ApprovalOutcome::Approved,
                scope: cmd::ApprovalScope::Once,
                reason: None,
            }],
        })
        .await;

    assert!(handled);
    assert_error_contains(&mut msg_rx, "no pending tool request matched decision").await;
}

#[tokio::test]
async fn rejected_tool_approval_returns_rejection_result() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let (mut router_client, _msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::AwaitingTool;
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));
    router_client.pending_tool_requests.insert(
        "call-1".to_owned(),
        ToolRequest {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
            arguments: serde_json::json!({ "command": "printf should-not-run" }),
            requires_approval: ToolApproval::Ask,
        },
    );
    router_client
        .pending_tool_prompts
        .insert("call-1".to_owned());

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
            decisions: vec![cmd::ApprovalDecision {
                id: "call-1".to_owned(),
                outcome: cmd::ApprovalOutcome::Rejected,
                scope: cmd::ApprovalScope::Once,
                reason: Some("not safe".to_owned()),
            }],
        })
        .await;

    assert!(handled);
    let ContinueRequest::ToolResults { results, encrypted } = continue_request_body(&router).await
    else {
        panic!("tool results continuation expected");
    };
    assert!(encrypted.is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, "call-1");
    assert_eq!(results[0].content, "not safe");
    assert!(results[0].is_error);
    assert_eq!(results[0].decision, Some(ApiApprovalDecision::Rejected));
}

#[tokio::test]
async fn approval_decision_continuation_seals_pending_content() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let (mut router_client, _msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    let key_id = router_client
        .context
        .e2ee_keys
        .create_key()
        .expect("E2EE key is created");
    router_client.state = State::AwaitingApproval;
    router_client.session = Some(session_info(
        Uuid::nil(),
        "Encrypted session",
        Some(key_id.clone()),
    ));
    router_client.pending_seals.insert(
        ContentRef::Message("message-1".to_owned()),
        "approval transcript".to_owned(),
    );

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
            decisions: vec![cmd::ApprovalDecision {
                id: "approval-1".to_owned(),
                outcome: cmd::ApprovalOutcome::Approved,
                scope: cmd::ApprovalScope::Once,
                reason: Some("ok".to_owned()),
            }],
        })
        .await;

    assert!(handled);
    assert!(router_client.pending_seals.is_empty());
    let ContinueRequest::ApprovalDecisions {
        decisions,
        encrypted,
    } = continue_request_body(&router).await
    else {
        panic!("approval decisions continuation expected");
    };
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].decision, ApiApprovalDecision::Approved);
    let payload = encrypted
        .get(&ContentRef::Message("message-1".to_owned()))
        .expect("pending approval content is sealed");
    assert_eq!(
        router_client
            .context
            .e2ee_keys
            .decrypt_payload(payload)
            .expect("pending content decrypts"),
        "approval transcript"
    );
}

#[tokio::test]
async fn inject_continuation_encrypts_messages_when_session_has_key() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let (mut router_client, _msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    let key_id = router_client
        .context
        .e2ee_keys
        .create_key()
        .expect("E2EE key is created");
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(
        Uuid::nil(),
        "Encrypted session",
        Some(key_id.clone()),
    ));

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::Inject {
            messages: vec![cmd::UserMessage {
                text: "extra context".to_owned(),
            }],
        })
        .await;

    assert!(handled);
    let ContinueRequest::Inject { messages } = continue_request_body(&router).await else {
        panic!("inject continuation expected");
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "extra context");
    let ciphertext = messages[0]
        .ciphertext
        .as_ref()
        .expect("injected message is encrypted");
    assert_eq!(
        router_client
            .context
            .e2ee_keys
            .decrypt_payload(ciphertext)
            .expect("injected message decrypts"),
        "extra context"
    );
}

#[tokio::test]
async fn tool_call_started_emits_progress_message() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallStarted {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
        }))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::ToolCallStarted(msg::ToolCallStarted {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
        })
    );
}

#[tokio::test]
async fn tool_call_requested_prompts_when_shell_approval_is_missing() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallRequested {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
            arguments: serde_json::json!({ "command": "cargo test -p smista-cli" }),
            requires_approval: ToolApproval::Ask,
        }))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::AwaitingTool);
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::ApprovalPrompt(msg::ApprovalPrompt {
            id: "call-1".to_owned(),
            title: "Approve shell".to_owned(),
            detail: "cargo test -p smista-cli".to_owned(),
            tool_name: Some("shell".to_owned()),
            wildcard_alias: Some("cargo test *".to_owned()),
        })
    );
}

#[tokio::test]
async fn tool_call_requested_executes_shell_when_session_alias_is_approved() {
    let (mut router_client, _msg_rx) = router_client_with_receiver(State::Streaming);
    router_client
        .approvals
        .approve("printf approved")
        .expect("approval alias is stored");

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallRequested {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
            arguments: serde_json::json!({ "command": "printf approved" }),
            requires_approval: ToolApproval::Ask,
        }))
        .await;

    assert!(next_stream.is_none());
    let result = router_client
        .pending_tool_results
        .get("call-1")
        .expect("approved shell call is executed");
    assert_eq!(result.content, "approved");
    assert!(!result.is_error);
    assert_eq!(result.decision, Some(ApiApprovalDecision::Approved));
}

#[tokio::test]
async fn tool_approval_decision_executes_request_and_folds_decision_into_result() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallRequested {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
            arguments: serde_json::json!({ "command": "printf approved" }),
            requires_approval: ToolApproval::Ask,
        }))
        .await;
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::ApprovalPrompt(_)
    ));

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
            decisions: vec![cmd::ApprovalDecision {
                id: "call-1".to_owned(),
                outcome: cmd::ApprovalOutcome::Approved,
                scope: cmd::ApprovalScope::AlwaysForSession,
                reason: None,
            }],
        })
        .await;

    assert!(handled);
    let request = continue_request_body(&router).await;
    let ContinueRequest::ToolResults { results, encrypted } = request else {
        panic!("tool results continuation expected");
    };
    assert!(encrypted.is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, "call-1");
    assert_eq!(results[0].content, "approved");
    assert!(!results[0].is_error);
    assert_eq!(results[0].decision, Some(ApiApprovalDecision::Approved));
    assert!(
        router_client
            .approvals
            .approved("printf approved")
            .expect("approval lookup succeeds")
    );
}

#[tokio::test]
async fn edit_approval_can_accept_later_edits_for_session() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));
    let path = router_client.context.cwd.join("src.txt");
    std::fs::write(&path, "before\n").expect("fixture file is written");

    router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallRequested {
            call_id: "edit-1".to_owned(),
            name: "edit_file".to_owned(),
            arguments: serde_json::json!({
                "path": "src.txt",
                "old": "before",
                "new": "after",
            }),
            requires_approval: ToolApproval::Ask,
        }))
        .await;
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::ApprovalPrompt(_)
    ));

    let handled = router_client
        .continue_execution(cmd::ContinueExecution::ApprovalDecisions {
            decisions: vec![cmd::ApprovalDecision {
                id: "edit-1".to_owned(),
                outcome: cmd::ApprovalOutcome::Approved,
                scope: cmd::ApprovalScope::AlwaysForSession,
                reason: None,
            }],
        })
        .await;

    assert!(handled);
    assert!(router_client.accept_edits);
    assert_eq!(
        std::fs::read_to_string(path).expect("fixture file is readable"),
        "after\n"
    );
}

#[tokio::test]
async fn accepted_edit_session_runs_later_edit_without_prompt() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client.accept_edits = true;
    let path = router_client.context.cwd.join("src.txt");
    std::fs::write(&path, "before\n").expect("fixture file is written");

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallRequested {
            call_id: "edit-1".to_owned(),
            name: "edit_file".to_owned(),
            arguments: serde_json::json!({
                "path": "src.txt",
                "old": "before",
                "new": "after",
            }),
            requires_approval: ToolApproval::Ask,
        }))
        .await;

    assert!(next_stream.is_none());
    assert!(msg_rx.try_recv().is_err());
    assert_eq!(
        std::fs::read_to_string(path).expect("fixture file is readable"),
        "after\n"
    );
    let result = router_client
        .pending_tool_results
        .get("edit-1")
        .expect("edit result is stored");
    assert_eq!(result.decision, Some(ApiApprovalDecision::Approved));
}

#[tokio::test]
async fn turn_end_completed_sends_assistant_turn_and_returns_idle() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(completed_turn_response(
            "done", "trace-1",
        )))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::AssistantTurn(msg::AssistantTurn {
            message: "done".to_owned(),
            trace_id: Some("trace-1".to_owned()),
        })
    );
}

#[tokio::test]
async fn text_and_reasoning_events_emit_stream_chunks() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    assert!(
        router_client
            .on_exec_stream_event(Ok(TurnEvent::TextDelta {
                delta: "hello".to_owned(),
            }))
            .await
            .is_none()
    );
    assert!(
        router_client
            .on_exec_stream_event(Ok(TurnEvent::ReasoningDelta {
                delta: "because".to_owned(),
            }))
            .await
            .is_none()
    );

    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::StreamedContentChunk("hello".to_owned())
    );
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::StreamedReasoningChunk("because".to_owned())
    );
}

#[tokio::test]
async fn stream_errors_emit_error_and_return_idle() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Err(smista_sdk::client::RouterClientError::Decode(
            "bad event".to_owned(),
        )))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Execution stream error").await;
}

#[tokio::test]
async fn handle_turn_stream_returns_when_stream_is_empty() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    router_client
        .handle_turn_stream(futures::stream::empty().boxed())
        .await;

    assert!(msg_rx.try_recv().is_err());
}

#[tokio::test]
async fn handle_turn_stream_returns_when_cancelled() {
    let exit = CancellationToken::new();
    let context = app_context(exit.clone());
    let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel(1);
    let mut router_client = RouterClient::new(cmd_rx, msg_tx, context);
    exit.cancel();

    router_client
        .handle_turn_stream(futures::stream::pending().boxed())
        .await;

    assert!(msg_rx.try_recv().is_err());
}

#[tokio::test]
async fn turn_end_idle_clears_pending_tool_state() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client
        .pending_tool_prompts
        .insert("call-1".to_owned());
    router_client.pending_tool_results.insert(
        "call-2".to_owned(),
        smista_sdk::core::api::ToolResult {
            call_id: "call-2".to_owned(),
            content: "ok".to_owned(),
            is_error: false,
            decision: None,
        },
    );

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::Idle {
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert!(router_client.pending_tool_prompts.is_empty());
    assert!(router_client.pending_tool_results.is_empty());
    assert!(msg_rx.try_recv().is_err());
}

#[tokio::test]
async fn turn_end_error_clears_pending_state_and_emits_error() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client
        .pending_tool_prompts
        .insert("call-1".to_owned());

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::Error {
                error: ApiErrorBody {
                    code: "no_route".to_owned(),
                    message: "No route matched".to_owned(),
                    details: None,
                },
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert!(router_client.pending_tool_prompts.is_empty());
    assert_error_contains(&mut msg_rx, "No route matched").await;
}

#[tokio::test]
async fn completed_turn_with_content_to_seal_reports_missing_session() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    let mut response = completed_turn_response("done", "trace-1");
    let TurnOutcome::Completed(turn) = &mut response.outcome else {
        panic!("completed fixture expected");
    };
    turn.to_encrypt.insert(
        ContentRef::Message("message-1".to_owned()),
        "seal me".to_owned(),
    );

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(response))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::AssistantTurn(msg::AssistantTurn {
            message: "done".to_owned(),
            trace_id: Some("trace-1".to_owned()),
        })
    );
    assert_error_contains(&mut msg_rx, "Cannot submit sealed content").await;
    assert_eq!(router_client.state, State::Idle);
}

#[tokio::test]
async fn turn_end_awaiting_approval_emits_approval_prompt() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingApproval {
                approval: PendingApproval {
                    approval_id: "approval-1".to_owned(),
                    kind: ApprovalKind::RemoteDisclosure,
                    detail: serde_json::json!({ "provider": "anthropic" }),
                },
                to_encrypt: Default::default(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::AwaitingApproval);
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::ApprovalPrompt(msg::ApprovalPrompt {
            id: "approval-1".to_owned(),
            title: "Approve RemoteDisclosure".to_owned(),
            detail: "{\n  \"provider\": \"anthropic\"\n}".to_owned(),
            tool_name: None,
            wildcard_alias: None,
        })
    );
}

#[tokio::test]
async fn awaiting_decrypt_without_session_reports_error() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingDecrypt {
                to_decrypt: Default::default(),
                to_encrypt: Default::default(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Cannot submit decrypted content").await;
}

#[tokio::test]
async fn awaiting_decrypt_reports_decrypt_errors() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client.session = Some(session_info(Uuid::nil(), "Encrypted session", None));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingDecrypt {
                to_decrypt: [(
                    ContentRef::Message("history-1".to_owned()),
                    invalid_encrypted_payload(),
                )]
                .into_iter()
                .collect(),
                to_encrypt: Default::default(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Failed to decrypt continuation content").await;
}

#[tokio::test]
async fn awaiting_decrypt_reports_seal_errors() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client.session = Some(session_info(Uuid::nil(), "Plain session", None));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingDecrypt {
                to_decrypt: Default::default(),
                to_encrypt: [(
                    ContentRef::RunInput(Uuid::nil().to_string()),
                    "run input".to_owned(),
                )]
                .into_iter()
                .collect(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Failed to seal continuation content").await;
}

#[tokio::test]
async fn awaiting_encrypt_submits_sealed_content() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let (mut router_client, _msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    let key_id = router_client
        .context
        .e2ee_keys
        .create_key()
        .expect("E2EE key is created");
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(
        Uuid::nil(),
        "Encrypted session",
        Some(key_id.clone()),
    ));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingEncrypt {
                to_encrypt: [(
                    ContentRef::Message("message-1".to_owned()),
                    "sealed text".to_owned(),
                )]
                .into_iter()
                .collect(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_some());
    let request = continue_request_body(&router).await;
    let ContinueRequest::Sealed { encrypted } = request else {
        panic!("sealed continuation expected");
    };
    let payload = encrypted
        .get(&ContentRef::Message("message-1".to_owned()))
        .expect("message content is sealed");
    assert_eq!(
        router_client
            .context
            .e2ee_keys
            .decrypt_payload(payload)
            .expect("sealed content decrypts"),
        "sealed text"
    );
}

#[tokio::test]
async fn awaiting_encrypt_without_session_reports_error() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingEncrypt {
                to_encrypt: [(
                    ContentRef::Message("message-1".to_owned()),
                    "seal me".to_owned(),
                )]
                .into_iter()
                .collect(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Cannot submit sealed content").await;
}

#[tokio::test]
async fn awaiting_encrypt_reports_seal_errors() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    router_client.session = Some(session_info(Uuid::nil(), "Plain session", None));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingEncrypt {
                to_encrypt: [(
                    ContentRef::Message("message-1".to_owned()),
                    "seal me".to_owned(),
                )]
                .into_iter()
                .collect(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Failed to seal continuation content").await;
}

#[tokio::test]
async fn continuation_stream_open_failure_reports_error() {
    let router = MockRouter::builder()
        .endpoint_status(Endpoint::ContinueRun, EndpointStatus::ServerError)
        .start()
        .await;
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let (mut router_client, mut msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    let key_id = router_client
        .context
        .e2ee_keys
        .create_key()
        .expect("E2EE key is created");
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(Uuid::nil(), "Encrypted session", Some(key_id)));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingEncrypt {
                to_encrypt: Default::default(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::Idle);
    assert_error_contains(&mut msg_rx, "Failed to submit sealed content").await;
}

#[tokio::test]
async fn awaiting_decrypt_submits_plaintext_and_sealed_content() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    let credentials = file_credentials(&cwd);
    let (mut router_client, _msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    let key_id = router_client
        .context
        .e2ee_keys
        .create_key()
        .expect("E2EE key is created");
    let history_payload = router_client
        .context
        .e2ee_keys
        .encrypt_payload(&key_id, "history text")
        .expect("history encrypts");
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(
        Uuid::nil(),
        "Encrypted session",
        Some(key_id.clone()),
    ));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingDecrypt {
                to_decrypt: [(ContentRef::Message("history-1".to_owned()), history_payload)]
                    .into_iter()
                    .collect(),
                to_encrypt: [(
                    ContentRef::RunInput(Uuid::nil().to_string()),
                    "run input".to_owned(),
                )]
                .into_iter()
                .collect(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_some());
    let request = continue_request_body(&router).await;
    let ContinueRequest::Decrypted {
        plaintext,
        encrypted,
    } = request
    else {
        panic!("decrypted continuation expected");
    };
    assert_eq!(
        plaintext.get(&ContentRef::Message("history-1".to_owned())),
        Some(&"history text".to_owned())
    );
    let payload = encrypted
        .get(&ContentRef::RunInput(Uuid::nil().to_string()))
        .expect("run input content is sealed");
    assert_eq!(
        router_client
            .context
            .e2ee_keys
            .decrypt_payload(payload)
            .expect("sealed content decrypts"),
        "run input"
    );
}

#[tokio::test]
async fn duplicate_tool_requests_do_not_prompt_twice() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);
    let event = TurnEvent::ToolCallRequested {
        call_id: "call-1".to_owned(),
        name: "shell".to_owned(),
        arguments: serde_json::json!({ "command": "cargo test -p smista-cli" }),
        requires_approval: ToolApproval::Ask,
    };

    assert!(
        router_client
            .on_exec_stream_event(Ok(event.clone()))
            .await
            .is_none()
    );
    assert!(
        router_client
            .on_exec_stream_event(Ok(event))
            .await
            .is_none()
    );

    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::ApprovalPrompt(prompt) if prompt.id == "call-1"
    ));
    assert!(msg_rx.try_recv().is_err());
}

#[tokio::test]
async fn invalid_shell_approval_request_prompts_without_alias() {
    let (mut router_client, mut msg_rx) = router_client_with_receiver(State::Streaming);

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::ToolCallRequested {
            call_id: "call-1".to_owned(),
            name: "shell".to_owned(),
            arguments: serde_json::json!({ "command": "&& rm -rf target" }),
            requires_approval: ToolApproval::Ask,
        }))
        .await;

    assert!(next_stream.is_none());
    assert_eq!(router_client.state, State::AwaitingTool);
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::ApprovalPrompt(msg::ApprovalPrompt {
            id: "call-1".to_owned(),
            title: "Approve shell".to_owned(),
            detail: "&& rm -rf target".to_owned(),
            tool_name: Some("shell".to_owned()),
            wildcard_alias: None,
        })
    );
}

#[tokio::test]
async fn awaiting_tool_with_all_results_opens_continue_stream() {
    let events = vec![TurnEvent::TurnEnd(Box::new(defaults::turn()))];
    let router = MockRouter::builder()
        .respond(Endpoint::ContinueRun, sse(&events))
        .start()
        .await;
    let (mut router_client, _msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    let next_stream = router_client
        .on_exec_stream_event(Ok(TurnEvent::TurnEnd(Box::new(TurnResponse {
            outcome: TurnOutcome::AwaitingTool {
                tool_requests: vec![ToolRequest {
                    call_id: "call-1".to_owned(),
                    name: "read_file".to_owned(),
                    arguments: serde_json::json!({ "path": "missing.txt" }),
                    requires_approval: ToolApproval::Allow,
                }],
                to_encrypt: Default::default(),
                trace_id: "trace-1".to_owned(),
            },
            allowed_continuations: Vec::new(),
        }))))
        .await;

    assert!(next_stream.is_some());
    assert_eq!(router_client.state, State::Streaming);
    assert!(router_client.pending_tool_prompts.is_empty());
    assert!(router_client.pending_tool_results.is_empty());
}

#[tokio::test]
async fn execute_with_encrypted_tool_usage_seals_tool_result_and_pending_content() {
    let execute_events = vec![TurnEvent::TurnEnd(Box::new(TurnResponse {
        outcome: TurnOutcome::AwaitingTool {
            tool_requests: vec![ToolRequest {
                call_id: "call-1".to_owned(),
                name: "read_file".to_owned(),
                arguments: serde_json::json!({ "path": "README.md" }),
                requires_approval: ToolApproval::Allow,
            }],
            to_encrypt: [(
                ContentRef::Message("message-1".to_owned()),
                "tool request transcript".to_owned(),
            )]
            .into_iter()
            .collect(),
            trace_id: "trace-1".to_owned(),
        },
        allowed_continuations: Vec::new(),
    }))];
    let router = MockRouter::builder()
        .respond(Endpoint::Execute, sse(&execute_events))
        .respond(
            Endpoint::ContinueRun,
            sse(&[TurnEvent::TurnEnd(Box::new(defaults::turn()))]),
        )
        .start()
        .await;
    let cwd = temp_cwd();
    tokio::fs::write(cwd.join("README.md"), "encrypted tool output")
        .await
        .expect("tool fixture is written");
    let credentials = file_credentials(&cwd);
    let (mut router_client, mut msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    let key_id = router_client
        .context
        .e2ee_keys
        .create_key()
        .expect("E2EE key is created");
    router_client.session = Some(session_info(
        Uuid::nil(),
        "Encrypted session",
        Some(key_id.clone()),
    ));

    router_client
        .execute("read the file".to_owned(), HashSet::new(), false, None)
        .await;

    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Thinking);
    assert!(matches!(recv_msg(&mut msg_rx).await, Msg::AssistantTurn(_)));
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);
    let request = continue_request_body(&router).await;
    let ContinueRequest::ToolResults { results, encrypted } = request else {
        panic!("tool results continuation expected");
    };
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, "call-1");
    assert_eq!(results[0].content, "encrypted tool output");
    assert!(!results[0].is_error);

    let tool_payload = encrypted
        .get(&ContentRef::ToolCall("call-1".to_owned()))
        .expect("tool result content is sealed");
    assert_eq!(
        router_client
            .context
            .e2ee_keys
            .decrypt_payload(tool_payload)
            .expect("tool result decrypts"),
        "encrypted tool output"
    );
    let pending_payload = encrypted
        .get(&ContentRef::Message("message-1".to_owned()))
        .expect("pending router content is sealed");
    assert_eq!(
        router_client
            .context
            .e2ee_keys
            .decrypt_payload(pending_payload)
            .expect("pending content decrypts"),
        "tool request transcript"
    );
}

#[tokio::test]
async fn execute_runs_tool_and_continues_to_completed_turn() {
    let execute_events = vec![TurnEvent::TurnEnd(Box::new(TurnResponse {
        outcome: TurnOutcome::AwaitingTool {
            tool_requests: vec![ToolRequest {
                call_id: "call-1".to_owned(),
                name: "read_file".to_owned(),
                arguments: serde_json::json!({ "path": "README.md" }),
                requires_approval: ToolApproval::Allow,
            }],
            to_encrypt: Default::default(),
            trace_id: "trace-1".to_owned(),
        },
        allowed_continuations: Vec::new(),
    }))];
    let continue_events = vec![TurnEvent::TurnEnd(Box::new(completed_turn_response(
        "I read the file.",
        "trace-2",
    )))];
    let router = MockRouter::builder()
        .respond(Endpoint::Execute, sse(&execute_events))
        .respond(Endpoint::ContinueRun, sse(&continue_events))
        .start()
        .await;
    let cwd = temp_cwd();
    tokio::fs::write(cwd.join("README.md"), "plain tool output")
        .await
        .expect("tool fixture is written");
    let credentials = file_credentials(&cwd);
    let (mut router_client, mut msg_rx) =
        router_client_for_mock_with_credentials(&router, cwd, credentials).await;
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    router_client
        .execute("read the file".to_owned(), HashSet::new(), false, None)
        .await;

    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Thinking);
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::AssistantTurn(msg::AssistantTurn {
            message: "I read the file.".to_owned(),
            trace_id: Some("trace-2".to_owned()),
        })
    );
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);

    let requests = router.received_requests().await;
    let execute_index = requests
        .iter()
        .position(|request| request.url.path().ends_with("/execute"))
        .expect("execute request was sent");
    let continue_index = requests
        .iter()
        .position(|request| request.url.path().ends_with("/continue"))
        .expect("continue request was sent");
    assert!(
        execute_index < continue_index,
        "execute must precede the tool-results continuation"
    );

    let request = requests[continue_index]
        .body_json::<ContinueRequest>()
        .expect("continue request body decodes");
    let ContinueRequest::ToolResults { results, encrypted } = request else {
        panic!("tool results continuation expected");
    };
    assert!(encrypted.is_empty());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, "call-1");
    assert_eq!(results[0].content, "plain tool output");
    assert!(!results[0].is_error);
}

#[tokio::test]
async fn execute_streams_deltas_and_reuses_active_session() {
    let events = vec![
        TurnEvent::TextDelta {
            delta: "Here ".to_owned(),
        },
        TurnEvent::ReasoningDelta {
            delta: "thinking".to_owned(),
        },
        TurnEvent::TurnEnd(Box::new(defaults::turn())),
    ];
    let router = MockRouter::builder()
        .respond(Endpoint::Execute, sse(&events))
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.session = Some(session_info(Uuid::nil(), "Existing session", None));

    router_client
        .execute("continue this".to_owned(), HashSet::new(), false, None)
        .await;

    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Thinking);
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::StreamedContentChunk("Here ".to_owned())
    );
    assert_eq!(
        recv_msg(&mut msg_rx).await,
        Msg::StreamedReasoningChunk("thinking".to_owned())
    );
    assert!(matches!(recv_msg(&mut msg_rx).await, Msg::AssistantTurn(_)));
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);

    let requests = router.received_requests().await;
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/api/v1/sessions"),
        "active execute must reuse the current session instead of creating a new one"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.url.path().ends_with("/execute"))
    );
}

#[tokio::test]
async fn preview_emits_summary_and_reuses_active_session() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.session = Some(session_info(Uuid::nil(), "Existing session", None));

    router_client
        .preview("preview this".to_owned(), HashSet::new(), true, None)
        .await;

    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Thinking);
    assert!(matches!(
        recv_msg(&mut msg_rx).await,
        Msg::Preview(summary)
            if summary.task_type == "review"
                && summary.provider == "openai"
                && summary.model == "gpt-5.5-thinking"
                && summary.classification_source == "inferred"
                && summary.classification_reason == "keyword matched"
                && summary.classification_confidence.as_deref() == Some("high")
                && summary.matched_rule.as_deref()
                    == Some("task.review -> openai/gpt-5.5-thinking")
                && summary.included_context == ["current git diff", "AGENTS.md"]
                && summary.excluded_context == [".env"]
                && summary.estimated_cost_min == "0.03"
                && summary.estimated_cost_max == "0.09"
                && summary.estimated_cost_currency == "USD"
                && summary.required_permissions.len() == 2
                && summary.required_permissions[1].permission == "write_files"
                && summary.required_permissions[1].mode == "ask"
    ));
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);

    let requests = router.received_requests().await;
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/api/v1/sessions"),
        "active preview must reuse the current session instead of creating a new one"
    );
    assert!(
        requests
            .iter()
            .any(|request| request.url.path().ends_with("/preview"))
    );
}

#[tokio::test]
async fn tool_executor_runs_builtin_read_file() {
    let cwd = temp_cwd();
    tokio::fs::write(cwd.join("README.md"), "hello")
        .await
        .expect("test file is written");
    let executor = ToolExecutor::new(cwd);

    let result = executor
        .execute(ToolCall {
            call_id: "call-1".to_owned(),
            name: "read_file".to_owned(),
            arguments: serde_json::json!({ "path": "README.md" }),
            decision: None,
        })
        .await;

    assert_eq!(result.call_id, "call-1");
    assert_eq!(result.content, "hello");
    assert!(!result.is_error);
}

#[tokio::test]
async fn tool_executor_runs_builtin_write_file() {
    let cwd = temp_cwd();
    let executor = ToolExecutor::new(cwd.clone());

    let result = executor
        .execute(ToolCall {
            call_id: "call-1".to_owned(),
            name: "write_file".to_owned(),
            arguments: serde_json::json!({
                "path": "src/generated.rs",
                "content": "pub const VALUE: u8 = 7;\n"
            }),
            decision: None,
        })
        .await;

    assert!(!result.is_error);
    assert_eq!(
        tokio::fs::read_to_string(cwd.join("src/generated.rs"))
            .await
            .expect("written file is readable"),
        "pub const VALUE: u8 = 7;\n"
    );
}

#[tokio::test]
async fn tool_executor_runs_builtin_edit_file() {
    let cwd = temp_cwd();
    tokio::fs::write(cwd.join("lib.rs"), "fn value() -> u8 { 1 }\n")
        .await
        .expect("test file is written");
    let executor = ToolExecutor::new(cwd.clone());

    let result = executor
        .execute(ToolCall {
            call_id: "call-1".to_owned(),
            name: "edit_file".to_owned(),
            arguments: serde_json::json!({
                "path": "lib.rs",
                "old": "1",
                "new": "2"
            }),
            decision: None,
        })
        .await;

    assert!(!result.is_error);
    assert_eq!(
        tokio::fs::read_to_string(cwd.join("lib.rs"))
            .await
            .expect("edited file is readable"),
        "fn value() -> u8 { 2 }\n"
    );
}

#[tokio::test]
async fn tool_executor_rejects_paths_outside_workspace() {
    let cwd = temp_cwd();
    let outside = cwd
        .parent()
        .expect("temporary directory has a parent")
        .join("smista-outside-file.txt");
    let outside_write = cwd
        .parent()
        .expect("temporary directory has a parent")
        .join(format!(
            "{}-outside-write.txt",
            cwd.file_name()
                .expect("temporary directory has a final component")
                .to_string_lossy()
        ));
    tokio::fs::write(&outside, "secret")
        .await
        .expect("outside fixture is written");
    let executor = ToolExecutor::new(cwd.clone());

    let read_result = executor
        .execute(ToolCall {
            call_id: "read".to_owned(),
            name: "read_file".to_owned(),
            arguments: serde_json::json!({ "path": outside }),
            decision: None,
        })
        .await;
    let write_result = executor
        .execute(ToolCall {
            call_id: "write".to_owned(),
            name: "write_file".to_owned(),
            arguments: serde_json::json!({
                "path": outside_write.clone(),
                "content": "escaped"
            }),
            decision: None,
        })
        .await;

    assert!(read_result.is_error);
    assert!(read_result.content.contains("outside the workspace"));
    assert!(write_result.is_error);
    assert!(write_result.content.contains("outside the workspace"));
    assert!(
        tokio::fs::metadata(&outside_write).await.is_err(),
        "write_file must not create files outside the workspace"
    );
}

#[tokio::test]
async fn tool_executor_reports_unsupported_tools_missing_arguments_and_shell_failures() {
    let cwd = temp_cwd();
    let executor = ToolExecutor::new(cwd);

    let unsupported = executor
        .execute(ToolCall {
            call_id: "unsupported".to_owned(),
            name: "unknown".to_owned(),
            arguments: serde_json::json!({}),
            decision: None,
        })
        .await;
    let missing_argument = executor
        .execute(ToolCall {
            call_id: "missing".to_owned(),
            name: "read_file".to_owned(),
            arguments: serde_json::json!({}),
            decision: None,
        })
        .await;
    let shell_failure = executor
        .execute(ToolCall {
            call_id: "shell".to_owned(),
            name: "shell".to_owned(),
            arguments: serde_json::json!({ "command": "exit 7" }),
            decision: Some(ApiApprovalDecision::Approved),
        })
        .await;

    assert!(unsupported.is_error);
    assert!(unsupported.content.contains("unsupported tool"));
    assert!(missing_argument.is_error);
    assert!(
        missing_argument
            .content
            .contains("missing string argument `path`")
    );
    assert!(shell_failure.is_error);
    assert!(shell_failure.content.contains("exited with"));
    assert_eq!(shell_failure.decision, Some(ApiApprovalDecision::Approved));
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
    assert_eq!(router_client.session_id(), None);
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
    assert_eq!(models[0].id, "qwen2.5-coder:7b");
    assert_eq!(models[0].display_name, "qwen2.5-coder:7b");

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
async fn init_new_session_encrypts_by_default_and_records_session_info_key_id() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::CreateSession,
            ResponseTemplate::new(201)
                .set_body_json(created_session_response(Some("kf_response".to_owned()))),
        )
        .start()
        .await;
    let (mut router_client, _msg_rx) = router_client_for_mock(&router).await;

    let session_id = router_client
        .init_new_session("Refactor the auth middleware")
        .await
        .expect("session creation succeeds");

    assert_eq!(session_id, Uuid::nil());
    let session = router_client
        .session
        .as_ref()
        .expect("session info is recorded");
    assert_eq!(session.id, Uuid::nil());
    assert_eq!(session.title, "Created session");
    assert_eq!(session.key_id.as_deref(), Some("kf_response"));

    let requests = router.received_requests().await;
    let request = requests
        .iter()
        .find(|request| request.url.path() == "/api/v1/sessions")
        .expect("create session request was sent");
    let body = request
        .body_json::<CreateSessionRequest>()
        .expect("create session body decodes");
    assert_eq!(body.title, "Refactor the auth middleware");
    assert!(
        body.key_id.is_some(),
        "default session creation must send a key id"
    );
}

#[tokio::test]
async fn init_new_session_creates_plaintext_session_when_encryption_is_disabled() {
    let router = MockRouter::builder()
        .respond(
            Endpoint::CreateSession,
            ResponseTemplate::new(201).set_body_json(created_session_response(None)),
        )
        .start()
        .await;
    let (mut router_client, _msg_rx) = router_client_for_mock(&router).await;
    let mut config = Config::default();
    config.local.encrypt_sessions = Some(false);
    router_client.context.config = Arc::new(config);

    router_client
        .init_new_session("Refactor encrypted auth middleware")
        .await
        .expect("session creation succeeds");

    let session = router_client
        .session
        .as_ref()
        .expect("session info is recorded");
    assert_eq!(session.id, Uuid::nil());
    assert_eq!(session.title, "Created session");
    assert_eq!(session.key_id, None);

    let requests = router.received_requests().await;
    let request = requests
        .iter()
        .find(|request| request.url.path() == "/api/v1/sessions")
        .expect("create session request was sent");
    let body = request
        .body_json::<CreateSessionRequest>()
        .expect("create session body decodes");
    assert_eq!(body.key_id, None);
}

#[tokio::test]
async fn resume_session_maps_messages_and_resets_state() {
    let router = MockRouter::start().await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::AwaitingTool;
    router_client.session = Some(session_info(Uuid::nil(), "Old session", None));
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
    assert_eq!(router_client.session_id(), Some(Uuid::nil()));
    let session_info = router_client
        .session
        .as_ref()
        .expect("session info is recorded");
    assert_eq!(session_info.title, "Refactor auth middleware");
    assert_eq!(session_info.key_id, None);
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
    router_client.session = Some(session_info(Uuid::nil(), "Usage session", None));

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
    router_client.session = Some(session_info(Uuid::nil(), "Trace session", None));

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
    router_client.session = Some(session_info(Uuid::nil(), "Trace session", Some(key_id)));

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
    router_client.session = Some(session_info(Uuid::nil(), "Trace session", None));

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
    router_client.session = Some(session_info(Uuid::nil(), "Trace session", None));

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
    router_client.session = Some(session_info(Uuid::nil(), "Trace session", Some(key_id)));

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
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));
    router_client
        .approvals
        .approve("git commit -m test")
        .expect("approval records");

    router_client.clear_session().await;

    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session, None);
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
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    router_client.clear_session().await;

    assert_error_contains(&mut msg_rx, "Failed to terminate active run").await;
    assert_eq!(recv_msg(&mut msg_rx).await, Msg::Idle);
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session, None);
}

#[tokio::test]
async fn resume_session_reports_interrupt_errors_then_loads_session() {
    let router = MockRouter::builder()
        .endpoint_status(Endpoint::ContinueRun, EndpointStatus::NotFound)
        .start()
        .await;
    let (mut router_client, mut msg_rx) = router_client_for_mock(&router).await;
    router_client.state = State::Streaming;
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));

    router_client.resume_session(Uuid::nil()).await;

    assert_error_contains(&mut msg_rx, "Failed to terminate active run").await;
    let Msg::ResumedSession(session) = recv_msg(&mut msg_rx).await else {
        panic!("resumed session message expected");
    };
    assert_eq!(session.id, Uuid::nil());
    assert_eq!(router_client.state, State::Idle);
    assert_eq!(router_client.session_id(), Some(Uuid::nil()));
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
    assert_eq!(
        router_client
            .session
            .as_ref()
            .expect("session info is recorded")
            .key_id
            .as_deref(),
        Some("kf_ab12")
    );
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

    router_client.session = Some(session_info(Uuid::nil(), "Usage session", None));
    router_client.get_usage().await;
    assert_error_contains(&mut msg_rx, "Failed to get usage statistics").await;
    router_client.session = None;

    router_client.resume_session(Uuid::nil()).await;
    assert_error_contains(&mut msg_rx, "Failed to get session").await;

    router_client.session = Some(session_info(Uuid::nil(), "Trace session", None));
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

#[test]
fn should_name_commands_and_truncate_overlong_titles() {
    assert_eq!(
        command_name(&Cmd::Execute {
            prompt: "run".to_owned(),
            files: HashSet::default(),
            plan: false,
            explicit_model: None,
        }),
        "execute"
    );
    assert_eq!(command_name(&Cmd::Clear), "clear");
    assert_eq!(command_name(&Cmd::ListModels), "list_models");
    assert_eq!(command_name(&Cmd::ListProviders), "list_providers");
    assert_eq!(command_name(&Cmd::ListSessions), "list_sessions");
    assert_eq!(
        command_name(&Cmd::ResumeSession(Uuid::nil())),
        "resume_session"
    );
    assert_eq!(command_name(&Cmd::GetUsage), "get_usage");
    assert_eq!(command_name(&Cmd::GetTrace), "get_trace");
    assert_eq!(
        command_name(&Cmd::Preview {
            prompt: "preview".to_owned(),
            files: HashSet::default(),
            plan: true,
            explicit_model: None,
        }),
        "preview"
    );
    assert_eq!(command_name(&Cmd::GetRouterStatus), "get_router_status");
    assert_eq!(
        command_name(&Cmd::Continue(cmd::ContinueExecution::ToolResults {
            results: Vec::new(),
        })),
        "continue_tool_results"
    );
    assert_eq!(
        continuation_name(&cmd::ContinueExecution::ApprovalDecisions {
            decisions: Vec::new(),
        }),
        "continue_approval_decisions"
    );
    assert_eq!(
        continuation_name(&cmd::ContinueExecution::Inject {
            messages: Vec::new(),
        }),
        "continue_inject"
    );
    assert_eq!(
        continuation_name(&cmd::ContinueExecution::Break),
        "continue_break"
    );
    assert_eq!(session_title("  short   title  "), "short title");
    assert_eq!(session_title(&"word ".repeat(40)), "");
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

fn router_client_with_session_state(state: State) -> RouterClient {
    let mut router_client = router_client_with_state(state);
    router_client.session = Some(session_info(Uuid::nil(), "Active session", None));
    router_client
}

fn router_client_with_receiver(state: State) -> (RouterClient, Receiver<Msg>) {
    let exit = CancellationToken::new();
    let context = app_context(exit);
    let (_cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(1);
    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(8);
    let mut router_client = RouterClient::new(cmd_rx, msg_tx, context);
    router_client.state = state;

    (router_client, msg_rx)
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
        config: Arc::new(Config::default()),
        cwd: cwd.clone(),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        exit,
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

async fn continue_request_body(router: &MockRouter) -> ContinueRequest {
    router
        .received_requests()
        .await
        .into_iter()
        .find(|request| request.url.path().ends_with("/continue"))
        .expect("continue request was sent")
        .body_json()
        .expect("continue request body decodes")
}

fn completed_turn_response(message: &str, trace_id: &str) -> TurnResponse {
    TurnResponse {
        outcome: TurnOutcome::Completed(Box::new(CompletedTurn {
            message: smista_sdk::core::message::Message {
                role: MessageRole::Assistant,
                content: message.to_owned(),
                provider: Some(CoreProvider::Anthropic),
                model: Some("claude-sonnet".to_owned()),
            },
            classification: smista_sdk::core::policy::Classification {
                intent: TaskIntent::Chat,
                source: smista_sdk::core::policy::IntentSource::Inferred,
                reason: "test fixture".to_owned(),
                matched_rule: None,
                confidence: Some(smista_sdk::core::policy::Confidence::Low),
            },
            routing: smista_sdk::core::api::RoutingOutcome {
                task_type: TaskIntent::Chat,
                provider: CoreProvider::Anthropic,
                model: "claude-sonnet".to_owned(),
                matched_rule: None,
                fallback_used: false,
                override_used: false,
            },
            context: ContextOutcome {
                included: Vec::new(),
                excluded: Vec::new(),
            },
            usage: smista_sdk::core::usage::Usage::default(),
            to_encrypt: std::collections::BTreeMap::new(),
            trace_id: trace_id.to_owned(),
        })),
        allowed_continuations: Vec::new(),
    }
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

fn session_info(id: Uuid, title: &str, key_id: Option<String>) -> SessionInfo {
    SessionInfo {
        id,
        title: title.to_owned(),
        key_id,
    }
}

fn created_session_response(key_id: Option<String>) -> CreateSessionResponse {
    CreateSessionResponse {
        session: SessionSummary {
            id: Uuid::nil(),
            title: Some("Created session".to_owned()),
            scope: Some("/tmp/project".to_owned()),
            encrypted: key_id.is_some(),
            key_id,
            created_at: "2026-05-25T09:00:00Z".parse().expect("timestamp parses"),
            updated_at: "2026-05-25T09:00:00Z".parse().expect("timestamp parses"),
            archived: false,
        },
    }
}

fn session_response_with_messages(messages: Vec<SessionMessageDetail>) -> GetSessionResponse {
    GetSessionResponse {
        session: SessionDetail {
            id: Uuid::nil(),
            title: "Encrypted transcript".to_owned(),
            scope: None,
            key_id: Some("kf_ab12".to_owned()),
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
        config: Arc::new(Config::default()),
        cwd: cwd.clone(),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        exit,
        router_client: Arc::new(router_client),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
    }
}
