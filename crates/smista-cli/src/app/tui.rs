use std::io::Stdout;

#[cfg(test)]
use ratatui::backend::TestBackend;
use ratatui::backend::{Backend, CrosstermBackend};

use crate::app::AppContext;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::{Cmd, Msg};

/// Terminal UI facade for the interactive client.
///
/// The TUI currently accepts events and router messages as no-ops so the worker
/// topology can be tested before rendering and key handling are implemented.
pub struct Tui<B: Backend> {
    /// The application context for the TUI.
    context: AppContext,
    /// The terminal instance for rendering.
    #[expect(
        dead_code,
        reason = "The TUI terminal is reserved for future rendering and command handling."
    )]
    terminal: ratatui::Terminal<B>,
}

/// Restores the process terminal when the interactive TUI session ends.
#[derive(Debug)]
#[must_use = "dropping this guard restores the process terminal"]
pub struct TerminalRestoreGuard;

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        if let Err(err) = ratatui::init::try_restore() {
            tracing::error!("failed to restore terminal: {err}");
        }
    }
}

impl Tui<CrosstermBackend<Stdout>> {
    /// Creates a TUI scaffold backed by the process terminal.
    ///
    /// `initial_prompt` is logged here and dispatched by the application run
    /// loop after the router client starts.
    ///
    /// # Errors
    ///
    /// Returns an error if the process terminal cannot be initialized.
    pub fn new(
        context: AppContext,
        initial_prompt: Option<String>,
    ) -> anyhow::Result<(Self, TerminalRestoreGuard)> {
        tracing::debug!("initializing terminal");
        let terminal = ratatui::try_init()?;
        tracing::debug!("terminal initialized");
        if let Some(prompt) = initial_prompt {
            tracing::debug!(
                prompt.bytes = prompt.len(),
                "initial prompt will be dispatched by the application run loop",
            );
        }
        Ok((Self { context, terminal }, TerminalRestoreGuard))
    }
}

#[cfg(test)]
impl Tui<TestBackend> {
    /// Creates a TUI scaffold backed by an in-memory test terminal.
    #[must_use]
    pub(super) fn new_test(context: AppContext) -> Self {
        let terminal = ratatui::Terminal::new(TestBackend::new(80, 24))
            .expect("test backend terminal is infallible");

        Self { context, terminal }
    }
}

impl<B: Backend> Tui<B> {
    /// Handles one input event and optionally produces a router command.
    ///
    /// This scaffold does not map keys to commands yet.
    #[must_use]
    pub fn handle_input_event(&self, event: InputEvent) -> Option<Cmd> {
        tracing::debug!(
            input.event = event.kind(),
            "handling input event {{input.event}}",
        );

        if let InputEvent::Interrupt = event {
            self.context.exit.cancel();
        }

        None
    }

    /// Handles one router message and optionally produces a follow-up command.
    ///
    /// This scaffold does not update UI state yet.
    #[must_use]
    pub fn handle_client_msg(&self, msg: Msg) -> Option<Cmd> {
        tracing::debug!(message = message_name(&msg), "handling client message");
        None
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
