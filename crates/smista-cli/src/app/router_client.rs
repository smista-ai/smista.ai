//! Worker-side router API integration for the interactive CLI.
//!
//! The router client owns the command receiver, sends UI messages, keeps the
//! current session state, executes client-side tools, and handles end-to-end
//! encryption continuations required by `smista-router`.

mod approvals;
pub mod cmd;
mod handler;
pub mod msg;
mod state;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashSet};

use smista_sdk::client::Client;
use smista_sdk::core::api::{ContentRef, CreateSessionRequest, ToolRequest, ToolResult};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use uuid::Uuid;

use self::approvals::ApprovalsStorage;
pub use self::cmd::Cmd;
pub use self::msg::Msg;
use self::state::State;
use crate::app::AppContext;

const MAX_SESSION_TITLE_LENGTH: usize = 100;

/// Worker responsible for communicating with `smista-router`.
///
/// This scaffold owns the command receiver and message sender, but it does not
/// execute router requests yet. Future tasks will translate [`Cmd`] values into
/// authenticated HTTP calls and emit [`Msg`] updates.
pub struct RouterClient {
    /// Storage for pending approvals, indexed by approval id.
    approvals: ApprovalsStorage,
    /// Channel to receive commands from the UI.
    cmd_rx: Receiver<Cmd>,
    /// Application context, including router client and cancellation token.
    context: AppContext,
    /// Channel to send messages to the UI.
    msg_tx: Sender<Msg>,
    /// Tool calls that have run locally and are waiting for a turn end.
    pending_tool_results: BTreeMap<String, ToolResult>,
    /// Tool calls waiting for an explicit user approval decision.
    pending_tool_requests: BTreeMap<String, ToolRequest>,
    /// Tool calls already surfaced to the user for approval.
    pending_tool_prompts: HashSet<String>,
    /// Router-authored content that must be sealed on the next continuation.
    pending_seals: BTreeMap<ContentRef, String>,
    /// Current session id, if any.
    session: Option<SessionInfo>,
    /// Current state of the router client.
    state: State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionInfo {
    id: Uuid,
    title: String,
    key_id: Option<String>,
}

impl RouterClient {
    /// Creates a router client worker from its command and message channels.
    #[must_use]
    pub fn new(cmd_rx: Receiver<Cmd>, msg_tx: Sender<Msg>, context: AppContext) -> Self {
        Self {
            approvals: ApprovalsStorage::new(),
            cmd_rx,
            msg_tx,
            pending_tool_results: BTreeMap::new(),
            pending_tool_requests: BTreeMap::new(),
            pending_tool_prompts: HashSet::new(),
            pending_seals: BTreeMap::new(),
            context,
            session: None,
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
                    tracing::debug!(command = command_name(&cmd), "received command");
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
                    explicit_model,
                },
            ) => {
                self.execute(prompt, files, plan, explicit_model).await;
                true
            }
            (
                _,
                Cmd::Preview {
                    prompt,
                    files,
                    plan,
                    explicit_model,
                },
            ) => {
                self.preview(prompt, files, plan, explicit_model).await;
                true
            }
            (_, Cmd::Continue(continue_execution)) if self.session.is_some() => {
                self.continue_execution(continue_execution).await
            }
            (_, Cmd::ListModels) => {
                self.list_models().await;
                true
            }
            (_, Cmd::ListProviders) => {
                self.list_providers().await;
                true
            }
            (_, Cmd::ListSessions) => {
                self.list_sessions().await;
                true
            }
            (_, Cmd::ResumeSession(session_id)) => {
                self.resume_session(session_id).await;
                true
            }
            (_, Cmd::GetRouterStatus) => {
                self.get_router_status().await;
                true
            }
            (_, Cmd::GetUsage) => {
                self.get_usage().await;
                true
            }
            (_, Cmd::GetTrace) => {
                self.get_traces().await;
                true
            }
            (_, Cmd::Clear) => {
                self.clear_session().await;
                true
            }
            (state, cmd) => {
                tracing::warn!(
                    command = command_name(&cmd),
                    ?state,
                    "received command in state, ignoring"
                );
                false
            }
        }
    }

    /// Initializes a new session with the router client.
    ///
    /// If successful, sets the `session_id` field to the new session's ID. If a session is already active, it will be replaced.
    /// The new `session_id` is returned in the `Ok` variant of the result.
    /// If an error occurs during session initialization, it is returned in the `Err` variant.
    async fn init_new_session(&mut self, prompt: &str) -> anyhow::Result<Uuid> {
        tracing::debug!("initializing a new session");

        let key_id = if self.context.config.local.encrypt_sessions() {
            tracing::debug!("e2ee is enabled, creating a new encryption key for the session");
            Some(self.context.e2ee_keys.create_key()?)
        } else {
            None
        };
        let title = session_title(prompt);

        tracing::debug!(
            encrypted = key_id.is_some(),
            title.bytes = title.len(),
            "creating session",
        );
        let response = self
            .context
            .router_client
            .create_session(CreateSessionRequest {
                title: title.clone(),
                scope: Some(self.scope()),
                key_id,
            })
            .await?;

        tracing::debug!(
            "created new session with id: {session_id}",
            session_id = response.session.id
        );
        self.session = Some(SessionInfo {
            id: response.session.id,
            title: response.session.title.unwrap_or(title),
            key_id: response.session.key_id,
        });

        Ok(response.session.id)
    }

    /// Returns the session ID of the current session, if any.
    fn session_id(&self) -> Option<Uuid> {
        self.session.as_ref().map(|info| info.id)
    }

    /// Returns the key ID associated with the current session, if any.
    fn key_id(&self) -> Option<&str> {
        self.session
            .as_ref()
            .and_then(|info| info.key_id.as_deref())
    }

    /// Computes the session list scope from the current working directory.
    #[must_use]
    fn scope(&self) -> String {
        self.context.cwd.to_string_lossy().to_string()
    }
}

fn session_title(prompt: &str) -> String {
    prompt
        .split_ascii_whitespace()
        .try_fold(String::new(), |mut acc, word| {
            if acc.len() + word.len() + 1 > MAX_SESSION_TITLE_LENGTH {
                Err(())
            } else {
                if !acc.is_empty() {
                    acc.push(' ');
                }
                acc.push_str(word);
                Ok(acc)
            }
        })
        .unwrap_or_else(|_| String::new())
}

fn command_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Execute { .. } => "execute",
        Cmd::Continue(continue_execution) => continuation_name(continue_execution),
        Cmd::Clear => "clear",
        Cmd::ListModels => "list_models",
        Cmd::ListProviders => "list_providers",
        Cmd::ListSessions => "list_sessions",
        Cmd::ResumeSession(_) => "resume_session",
        Cmd::GetUsage => "get_usage",
        Cmd::GetTrace => "get_trace",
        Cmd::Preview { .. } => "preview",
        Cmd::GetRouterStatus => "get_router_status",
    }
}

fn continuation_name(continue_execution: &cmd::ContinueExecution) -> &'static str {
    match continue_execution {
        cmd::ContinueExecution::ToolResults { .. } => "continue_tool_results",
        cmd::ContinueExecution::ApprovalDecisions { .. } => "continue_approval_decisions",
        cmd::ContinueExecution::Inject { .. } => "continue_inject",
        cmd::ContinueExecution::Break => "continue_break",
    }
}
