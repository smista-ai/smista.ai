//! Main smista.ai cli application

mod input_listener;
mod router_client;
mod tui;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use ratatui::backend::Backend;
use smista_sdk::client::ReqwestClient;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::app::input_listener::{InputEvent, InputListener};
use crate::app::router_client::{Cmd, Msg, RouterClient};
use crate::app::tui::Tui;
use crate::config::Config;
use crate::credentials::{ApiKeyStorage, E2eeKeysCredentials, ProvidersCredentials};
use crate::skills::SkillStore;

/// Shared CLI application dependencies.
///
/// The context is cloned into each worker so the input listener, router client,
/// and TUI can share configuration, credentials, the authenticated router
/// client, and the cancellation token without owning each other.
#[expect(
    dead_code,
    reason = "Most context fields are scaffolded before the workers consume them."
)]
#[derive(Clone)]
pub struct AppContext {
    /// Storage for the router API key.
    pub api_key: Arc<ApiKeyStorage>,
    /// Effective CLI configuration.
    pub config: Arc<Config>,
    /// Working directory used for project-local config, secrets, and skills.
    pub cwd: PathBuf,
    /// Storage and crypto helper for session E2EE keys.
    pub e2ee_keys: Arc<E2eeKeysCredentials>,
    /// Shared cancellation token used to shut down all workers.
    pub exit: CancellationToken,
    /// Storage for provider API keys.
    pub providers_credentials: Arc<ProvidersCredentials>,
    /// Authenticated router HTTP client.
    pub router_client: Arc<ReqwestClient>,
    /// Discovered project and global skills.
    pub skills_store: Arc<SkillStore>,
}

/// Dependencies needed by the application run loop.
struct RunLoopArgs<B: Backend> {
    cmd_tx: Sender<Cmd>,
    input_event_rx: Receiver<InputEvent>,
    initial_prompt: Option<String>,
    input_listener: JoinHandle<()>,
    msg_rx: Receiver<Msg>,
    router_client: JoinHandle<()>,
    tui: Tui<B>,
}

/// Interactive CLI application coordinator.
///
/// [`App`] owns the worker lifecycle and routes messages between the input
/// listener, TUI, and router client through bounded Tokio channels.
pub struct App {
    context: AppContext,
}

impl App {
    /// Creates an application coordinator from shared context.
    #[must_use]
    pub fn new(context: AppContext) -> Self {
        Self { context }
    }

    /// Starts the worker skeleton and runs until the shared cancellation token fires.
    ///
    /// The optional `initial_prompt` is forwarded to the router client as the
    /// first scaffold command. Real rendering and router execution are added by
    /// later tasks.
    ///
    /// # Errors
    ///
    /// Returns an error if a worker task panics or if a channel send fails while
    /// the loop is still active.
    pub async fn run(self, initial_prompt: Option<String>) -> anyhow::Result<()> {
        tracing::info!("Starting smista.ai cli application");
        // setup channels
        tracing::debug!("Setting up channels for message and input events");
        let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(100);
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
        let (input_event_tx, input_event_rx) = tokio::sync::mpsc::channel(100);
        tracing::debug!("Channels setup complete");
        // setup event listener
        tracing::debug!("Setting up input listener");
        let input_listener = InputListener::new(self.context.exit.clone(), input_event_tx).run();
        tracing::debug!("Input listener setup complete");
        // setup tui
        tracing::debug!("Setting up tui");
        let (tui, _terminal_restore) = Tui::new(self.context.clone(), initial_prompt.clone())?;
        tracing::debug!("Tui setup complete");
        // setup router client
        tracing::debug!("Setting up router client");
        let router_client = RouterClient::new(cmd_rx, msg_tx, self.context.clone()).run();
        tracing::debug!("Router client setup complete");

        self.run_loop(RunLoopArgs {
            cmd_tx,
            initial_prompt,
            input_event_rx,
            input_listener,
            msg_rx,
            router_client,
            tui,
        })
        .await
    }

    /// Starts the run loop with a deterministic test input listener.
    #[cfg(test)]
    pub fn mock(context: AppContext, input_events: Vec<InputEvent>) -> JoinHandle<()> {
        use std::time::Duration;

        use crate::app::input_listener::mock::MockInputListener;

        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
        let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(100);
        let (input_event_tx, input_event_rx) = tokio::sync::mpsc::channel(100);

        let input_listener = MockInputListener::new(
            input_events,
            context.exit.clone(),
            Duration::from_millis(100),
            input_event_tx,
        )
        .run();
        let tui = Tui::new_test(context.clone());
        let router_client = RouterClient::new(cmd_rx, msg_tx, context.clone()).run();

        tokio::spawn(async move {
            App { context }
                .run_loop(RunLoopArgs {
                    cmd_tx,
                    initial_prompt: None,
                    input_event_rx,
                    input_listener,
                    msg_rx,
                    router_client,
                    tui,
                })
                .await
                .expect("Run loop failed");
        })
    }

    /// Routes events and router messages until cancellation.
    ///
    /// This loop intentionally contains no real UI or router behavior yet. Its
    /// job is to prove the worker topology and shutdown flow.
    ///
    /// # Errors
    ///
    /// Returns an error if forwarding a command fails before shutdown, or if a
    /// worker task panics while being joined.
    async fn run_loop<B: Backend>(
        self,
        RunLoopArgs {
            cmd_tx,
            initial_prompt,
            mut input_event_rx,
            input_listener,
            mut msg_rx,
            router_client,
            tui,
        }: RunLoopArgs<B>,
    ) -> anyhow::Result<()> {
        tracing::debug!("starting run loop");

        // if there is an initial prompt, send it to the router client
        if let Some(prompt) = initial_prompt {
            tracing::debug!(
                prompt.bytes = prompt.len(),
                "sending initial prompt to router client",
            );
            cmd_tx
                .send(Cmd::Execute {
                    prompt,
                    files: HashSet::default(),
                    plan: false,
                    explicit_model: None,
                })
                .await?;
        }

        loop {
            tokio::select! {
                _ = self.context.exit.cancelled() => {
                    break;
                }
                Some(input_event) = input_event_rx.recv() => {
                    tracing::debug!(
                        input.event = input_event.kind(),
                        "received input event {{input.event}}",
                    );
                    if let Some(cmd) = tui.handle_input_event(input_event) {
                        tracing::debug!(
                            command = command_name(&cmd),
                            "sending command to router client"
                        );
                        cmd_tx.send(cmd).await?;
                    }
                }
                Some(msg) = msg_rx.recv() => {
                    tracing::debug!(
                        message = message_name(&msg),
                        "received message from router client"
                    );
                    if let Some(cmd) = tui.handle_client_msg(msg) {
                        tracing::debug!(
                            command = command_name(&cmd),
                            "sending command to router client"
                        );
                        cmd_tx.send(cmd).await?;
                    }
                }
            }
        }

        // join the input listener and router client tasks to ensure they have completed
        tracing::debug!("joining input listener");
        input_listener.await?;
        tracing::debug!("Input listener task joined; joining router client task");
        router_client.await?;

        tracing::info!("shutting down run loop");

        Ok(())
    }
}

fn command_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::Execute { .. } => "execute",
        Cmd::Continue(_) => "continue",
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

fn message_name(msg: &Msg) -> &'static str {
    match msg {
        Msg::AssistantTurn(_) => "assistant_turn",
        Msg::StreamedContentChunk(_) => "streamed_content_chunk",
        Msg::StreamedReasoningChunk(_) => "streamed_reasoning_chunk",
        Msg::ToolCallStarted(_) => "tool_call_started",
        Msg::ApprovalPrompt(_) => "approval_prompt",
        Msg::ModelsList(_) => "models_list",
        Msg::ProvidersList(_) => "providers_list",
        Msg::SessionsList(_) => "sessions_list",
        Msg::ResumedSession(_) => "resumed_session",
        Msg::Usage(_) => "usage",
        Msg::Trace(_) => "trace",
        Msg::Preview(_) => "preview",
        Msg::RouterStatus(_) => "router_status",
        Msg::Error(_) => "error",
        Msg::Idle => "idle",
        Msg::Thinking => "thinking",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use smista_sdk::client::{ReqwestClient, RouterClientConfig};
    use tokio_util::sync::CancellationToken;
    use url::Url;

    use super::*;
    use crate::credentials::{
        ApiKeyStorage, CredentialBackend, CredentialsStorage, E2eeKeysCredentials,
        ProvidersCredentials,
    };

    #[tokio::test]
    async fn should_terminate_when_cancelled() {
        let exit = CancellationToken::new();
        let app = App::mock(app_context(exit.clone()), Vec::new());

        exit.cancel();

        tokio::time::timeout(Duration::from_secs(1), app)
            .await
            .expect("app run loop stops after cancellation")
            .expect("app run loop does not panic during cancellation");
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
