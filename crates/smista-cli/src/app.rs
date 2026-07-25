//! Main smista.ai cli application

mod input_listener;
#[cfg(test)]
mod integration_tests;
pub mod log;
mod router_client;
mod tui;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use smista_sdk::client::ReqwestClient;
use tokio::sync::mpsc::{Receiver, Sender};
#[cfg(test)]
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use self::log::AppLogSink;
use crate::app::input_listener::{InputEvent, InputListener};
use crate::app::router_client::{Cmd, Msg, RouterClient};
use crate::app::tui::{ClearableBackend, Tui};
use crate::config::Config;
use crate::credentials::E2eeKeysCredentials;
use crate::skills::SkillStore;

const TUI_REFRESH_INTERVAL: Duration = Duration::from_millis(120);

/// Shared CLI application dependencies.
///
/// The context is cloned into each worker so the input listener, router client,
/// and TUI can share configuration, credentials, the authenticated router
/// client, and the cancellation token without owning each other.
#[derive(Clone)]
pub struct AppContext {
    /// Effective CLI configuration.
    pub config: Arc<Config>,
    /// Working directory used for project-local config, secrets, and skills.
    pub cwd: PathBuf,
    /// Storage and crypto helper for session E2EE keys.
    pub e2ee_keys: Arc<E2eeKeysCredentials>,
    /// Shared cancellation token used to shut down all workers.
    pub exit: CancellationToken,
    /// Bounded formatted log entries shared with the terminal UI.
    pub logs: AppLogSink,
    /// Authenticated router HTTP client.
    pub router_client: Arc<ReqwestClient>,
    /// Discovered project and global skills.
    pub skills_store: Arc<SkillStore>,
}

/// Dependencies needed by the application run loop.
struct RunLoopArgs<B: ClearableBackend> {
    cmd_tx: Sender<Cmd>,
    input_event_rx: Receiver<InputEvent>,
    initial_prompt: Option<String>,
    input_listener: JoinHandle<()>,
    msg_rx: Receiver<Msg>,
    router_client: JoinHandle<()>,
    #[cfg(test)]
    snapshot_tx: Option<watch::Sender<tui::State>>,
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
        // setup tui
        tracing::debug!("Setting up tui");
        let (tui, _terminal_restore) = Tui::new(self.context.clone(), initial_prompt.clone())?;
        tracing::debug!("Tui setup complete");
        // setup event listener after TUI initialization so crossterm's event stream cannot consume
        // cursor-position responses used by Ratatui's inline viewport setup.
        tracing::debug!("Setting up input listener");
        let input_listener = InputListener::new(self.context.exit.clone(), input_event_tx).run();
        tracing::debug!("Input listener setup complete");
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
            #[cfg(test)]
            snapshot_tx: None,
            tui,
        })
        .await
    }

    /// Starts the run loop with an interactive deterministic test driver.
    #[cfg(test)]
    fn mock(context: AppContext) -> AppTestDriver {
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
        let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(100);
        let (input_event_tx, input_event_rx) = tokio::sync::mpsc::channel(100);
        let (snapshot_tx, snapshot_rx) = watch::channel(tui::State::default());

        let input_exit = context.exit.clone();
        let input_listener = tokio::spawn(async move {
            input_exit.cancelled().await;
        });
        let tui = Tui::new_test(context.clone());
        let router_client = RouterClient::new(cmd_rx, msg_tx, context.clone()).run();
        let exit = context.exit.clone();

        let handle = tokio::spawn(async move {
            App { context }
                .run_loop(RunLoopArgs {
                    cmd_tx,
                    initial_prompt: None,
                    input_event_rx,
                    input_listener,
                    msg_rx,
                    router_client,
                    snapshot_tx: Some(snapshot_tx),
                    tui,
                })
                .await
                .expect("Run loop failed");
        });

        AppTestDriver {
            exit,
            handle,
            input_event_tx,
            snapshot_rx,
        }
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
    async fn run_loop<B: ClearableBackend>(
        self,
        RunLoopArgs {
            cmd_tx,
            initial_prompt,
            mut input_event_rx,
            input_listener,
            mut msg_rx,
            router_client,
            #[cfg(test)]
            snapshot_tx,
            mut tui,
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

        // view the TUI before entering the loop to ensure the initial state is rendered
        tui.view()?;
        #[cfg(test)]
        publish_snapshot(snapshot_tx.as_ref(), &tui);
        let mut refresh_tick = tokio::time::interval(TUI_REFRESH_INTERVAL);
        refresh_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

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
                    match tui.handle_input_event(input_event) {
                        Ok(Some(cmd)) => {
                            tracing::debug!(
                                command = command_name(&cmd),
                                "sending command to router client"
                            );
                            cmd_tx.send(cmd).await?;
                        }
                        Ok(None) => {
                            tracing::debug!("no command produced by input event");
                        }
                        Err(err) => {
                            tracing::error!(
                                error = %err,
                                "failed to handle input event"
                            );
                        }
                    }
                    #[cfg(test)]
                    publish_snapshot(snapshot_tx.as_ref(), &tui);
                }
                _ = refresh_tick.tick() => {
                    if let Err(err) = tui.refresh() {
                        tracing::error!(
                            error = %err,
                            "failed to refresh tui"
                        );
                    }
                }
                Some(msg) = msg_rx.recv() => {
                    tracing::debug!(
                        message = message_name(&msg),
                        "received message from router client"
                    );
                    if let Err(err) = tui.handle_client_msg(msg) {
                        tracing::error!(
                            error = %err,
                            "failed to handle client message"
                        );
                    }
                    #[cfg(test)]
                    publish_snapshot(snapshot_tx.as_ref(), &tui);
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

#[cfg(test)]
fn publish_snapshot<B>(snapshot_tx: Option<&watch::Sender<tui::State>>, tui: &Tui<B>)
where
    B: ClearableBackend,
{
    if let Some(snapshot_tx) = snapshot_tx {
        snapshot_tx.send_replace(tui.snapshot());
    }
}

/// Interactive app-level test driver.
#[cfg(test)]
struct AppTestDriver {
    exit: CancellationToken,
    handle: JoinHandle<()>,
    input_event_tx: Sender<InputEvent>,
    snapshot_rx: watch::Receiver<tui::State>,
}

#[cfg(test)]
impl AppTestDriver {
    /// Sends one decoded terminal event through the app input path.
    async fn send(&self, event: InputEvent) {
        self.input_event_tx
            .send(event)
            .await
            .expect("test app accepts input events");
    }

    /// Pastes and submits one prompt or slash command.
    async fn submit(&self, input: &str) {
        self.send(InputEvent::Paste(input.to_owned())).await;
        self.send(InputEvent::Enter).await;
    }

    /// Waits for a state predicate with a bounded timeout.
    async fn wait_for<F>(&mut self, predicate: F) -> tui::State
    where
        F: Fn(&tui::State) -> bool,
    {
        const TEST_TIMEOUT: Duration = Duration::from_secs(5);

        tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                let snapshot = self.snapshot_rx.borrow_and_update().clone();
                if predicate(&snapshot) {
                    return snapshot;
                }
                self.snapshot_rx
                    .changed()
                    .await
                    .expect("test app snapshot channel remains open");
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "timed out waiting for TUI state; last snapshot: {:?}",
                self.snapshot_rx.borrow()
            )
        })
    }

    /// Stops the app and waits for every worker to shut down.
    async fn shutdown(self) {
        const TEST_TIMEOUT: Duration = Duration::from_secs(5);

        self.exit.cancel();
        tokio::time::timeout(TEST_TIMEOUT, self.handle)
            .await
            .expect("test app stops after cancellation")
            .expect("test app run loop does not panic");
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
        Msg::ApprovalPrompt(_) => "approval_prompt",
        Msg::AssistantTurn(_) => "assistant_turn",
        Msg::Error(_) => "error",
        Msg::Idle => "idle",
        Msg::Interrupted => "interrupted",
        Msg::ModelsList(_) => "models_list",
        Msg::Preview(_) => "preview",
        Msg::ProvidersList(_) => "providers_list",
        Msg::ResumedSession(_) => "resumed_session",
        Msg::RouterStatus(_) => "router_status",
        Msg::SessionClosed { .. } => "session_closed",
        Msg::SessionsList(_) => "sessions_list",
        Msg::StreamedContentChunk(_) => "streamed_content_chunk",
        Msg::StreamedReasoningChunk(_) => "streamed_reasoning_chunk",
        Msg::Thinking => "thinking",
        Msg::ToolCallStarted(_) => "tool_call_started",
        Msg::Trace(_) => "trace",
        Msg::Usage(_) => "usage",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smista_sdk::client::{ReqwestClient, RouterClientConfig};
    use tokio_util::sync::CancellationToken;
    use url::Url;

    use super::*;
    use crate::credentials::{CredentialBackend, CredentialsStorage, E2eeKeysCredentials};

    #[tokio::test]
    async fn should_terminate_when_cancelled() {
        let exit = CancellationToken::new();
        let app = App::mock(app_context(exit.clone()));

        exit.cancel();

        app.shutdown().await;
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
            logs: AppLogSink::new(),
            router_client: Arc::new(router_client),
            skills_store: Arc::new(SkillStore::discover(&cwd)),
        }
    }
}
