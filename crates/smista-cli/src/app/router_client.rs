mod approvals;
pub mod cmd;
pub mod msg;
mod state;

use std::collections::HashMap;
use std::path::PathBuf;

use smista_sdk::client::Client;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use uuid::Uuid;

use self::approvals::ApprovalsStorage;
pub use self::cmd::Cmd;
pub use self::msg::Msg;
use self::state::State;
use crate::app::AppContext;

/// Worker responsible for communicating with `smista-router`.
///
/// This scaffold owns the command receiver and message sender, but it does not
/// execute router requests yet. Future tasks will translate [`Cmd`] values into
/// authenticated HTTP calls and emit [`Msg`] updates.
pub struct RouterClient {
    #[expect(
        dead_code,
        reason = "approvals are used once real router calls are implemented."
    )]
    approvals: ApprovalsStorage,
    cmd_rx: Receiver<Cmd>,
    context: AppContext,
    #[expect(
        dead_code,
        reason = "Router messages are emitted once real router calls are implemented."
    )]
    msg_tx: Sender<Msg>,
    /// Current session id, if any.
    session_id: Option<Uuid>,
    /// current state of the router client
    state: State,
}

impl RouterClient {
    /// Creates a router client worker from its command and message channels.
    #[must_use]
    pub fn new(cmd_rx: Receiver<Cmd>, msg_tx: Sender<Msg>, context: AppContext) -> Self {
        Self {
            approvals: ApprovalsStorage::new(),
            cmd_rx,
            msg_tx,
            context,
            session_id: None,
            state: State::Idle,
        }
    }

    /// Spawns the router client worker task.
    #[must_use]
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(self.run_loop())
    }

    async fn run_loop(mut self) {
        tracing::debug!("RouterClient started");

        loop {
            tokio::select! {
                _ = self.context.exit.cancelled() => {
                    break;
                }
                Some(cmd) = self.cmd_rx.recv() => {
                    tracing::debug!("received command: {cmd:?}");
                    self.handle_cmd(cmd).await;
                }
            }
        }

        // signing out on router.
        if let Err(err) = self.context.router_client.sign_out().await {
            tracing::error!("failed to sign out: {err}");
        } else {
            tracing::debug!("signed out successfully");
        }

        tracing::info!("RouterClient stopped");
    }

    async fn handle_cmd(&mut self, cmd: Cmd) -> bool {
        match (&self.state, cmd) {
            (
                State::Idle,
                Cmd::Execute {
                    prompt,
                    files,
                    plan,
                },
            ) => self.execute(prompt, files, plan).await,
            (
                State::Idle,
                Cmd::Preview {
                    prompt,
                    files,
                    plan,
                },
            ) => self.preview(prompt, files, plan).await,
            (_, Cmd::ListSessions) => placeholder("list sessions on the router for this user"),
            (state, Cmd::ResumeSession(session_id)) => {
                let todo = if matches!(state, State::Idle) {
                    "resume session from idle state"
                } else {
                    "interrupt active run, then resume session"
                };
                tracing::debug!(%session_id, todo, "router client scaffold placeholder");
                true
            }
            (_, Cmd::GetRouterStatus) => placeholder("get router health status"),
            (state, Cmd::GetUsage)
                if self.session_id.is_some() || !matches!(state, State::Idle) =>
            {
                placeholder("get current usage statistics for this session")
            }
            (state, Cmd::GetTrace)
                if self.session_id.is_some() || !matches!(state, State::Idle) =>
            {
                placeholder("get execution trace for this session")
            }
            (
                State::Streaming,
                Cmd::Continue(
                    continue_execution @ (cmd::ContinueExecution::Break
                    | cmd::ContinueExecution::Inject { .. }),
                ),
            ) => self.continue_execution(continue_execution).await,
            (state, Cmd::Continue(continue_execution)) if !matches!(state, State::Idle) => {
                self.continue_execution(continue_execution).await
            }
            (state, Cmd::Clear) if !matches!(state, State::Idle) => {
                placeholder("interrupt active run, then clear current session")
            }
            (State::Idle, Cmd::Clear) => placeholder("clear current idle session"),
            (
                _,
                Cmd::Preview {
                    prompt,
                    files,
                    plan,
                },
            ) => self.preview(prompt, files, plan).await,
            (state, cmd) => {
                tracing::warn!("received command {cmd:?} in state {state:?}, ignoring");
                false
            }
        }
    }

    async fn execute(
        &mut self,
        prompt: String,
        files: HashMap<PathBuf, String>,
        plan: bool,
    ) -> bool {
        tracing::debug!(
            "executing prompt: {prompt} with files: {files:?}, plan: {plan}",
            files = files.keys().collect::<Vec<_>>()
        );
        placeholder("execute prompt through router and process turn response")
    }

    async fn preview(
        &mut self,
        prompt: String,
        files: HashMap<PathBuf, String>,
        plan: bool,
    ) -> bool {
        tracing::debug!(
            "previewing prompt: {prompt} with files: {files:?}, plan: {plan}",
            files = files.keys().collect::<Vec<_>>()
        );
        placeholder("preview deterministic router selection for prompt")
    }

    async fn continue_execution(&mut self, continue_execution: cmd::ContinueExecution) -> bool {
        match (&self.state, continue_execution) {
            (State::AwaitingTool, cmd::ContinueExecution::ToolResults { results }) => {
                tracing::debug!(
                    result.count = results.len(),
                    "router client scaffold placeholder: submit tool results",
                );
                true
            }
            (State::AwaitingApproval, cmd::ContinueExecution::ApprovalDecisions { decisions }) => {
                tracing::debug!(
                    decision.count = decisions.len(),
                    "router client scaffold placeholder: submit approval decisions",
                );
                true
            }
            (
                state @ (State::AwaitingTool | State::AwaitingApproval | State::Streaming),
                cmd::ContinueExecution::Break,
            ) => {
                tracing::debug!(
                    ?state,
                    "router client scaffold placeholder: break active run",
                );
                true
            }
            (
                state @ (State::AwaitingTool | State::AwaitingApproval | State::Streaming),
                cmd::ContinueExecution::Inject { messages },
            ) => {
                tracing::debug!(
                    ?state,
                    message.count = messages.len(),
                    "router client scaffold placeholder: inject user input into active run",
                );
                true
            }
            (state, continue_execution) => {
                tracing::warn!(
                    "received continuation {continue_execution:?} in state {state:?}, ignoring",
                );
                false
            }
        }
    }
}

fn placeholder(todo: &'static str) -> bool {
    tracing::debug!(todo, "router client scaffold placeholder");
    true
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use smista_sdk::client::{ReqwestClient, RouterClientConfig};
    use tokio_util::sync::CancellationToken;
    use url::Url;

    use super::*;
    use crate::config::Config;
    use crate::credentials::{
        ApiKeyStorage, CredentialBackend, CredentialsStorage, E2eeKeysCredentials,
        ProvidersCredentials,
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
                })
                .await;

            assert!(!handled);
        }
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
}
