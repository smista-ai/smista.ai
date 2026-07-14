//! TUI (Terminal User Interface) module for smista-cli.

mod input;
mod state;
#[cfg(test)]
mod tests;
mod view;

use std::io::{Stdout, Write};

use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{Clear, ClearType};
#[cfg(test)]
use ratatui::backend::TestBackend;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{TerminalOptions, Viewport};

use self::state::State;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::{Cmd, Msg};
use crate::app::{AppContext, command_name};

const INLINE_VIEWPORT_HEIGHT: u16 = 8;

/// Backend operations needed to clear an inline terminal and its scrollback.
pub(super) trait ClearableBackend: Backend {
    /// Clears the visible screen and content committed above the inline viewport.
    fn clear_scrollback(&mut self) -> Result<(), Self::Error>;
}

impl<W> ClearableBackend for CrosstermBackend<W>
where
    W: Write,
{
    fn clear_scrollback(&mut self) -> std::io::Result<()> {
        crossterm::execute!(self, Clear(ClearType::Purge), Clear(ClearType::All))
    }
}

#[cfg(test)]
impl ClearableBackend for TestBackend {
    fn clear_scrollback(&mut self) -> Result<(), Self::Error> {
        self.clear()
    }
}

/// Terminal UI facade for the interactive client.
///
/// The TUI currently accepts events and router messages as no-ops so the worker
/// topology can be tested before rendering and key handling are implemented.
pub struct Tui<B: Backend> {
    /// The application context for the TUI.
    context: AppContext,
    /// Number of transcript entries already inserted above the inline viewport.
    printed_history_entries: usize,
    /// The TUI state for rendering and input handling.
    state: State,
    /// The terminal instance for rendering.
    terminal: ratatui::Terminal<B>,
}

/// Restores the process terminal when the interactive TUI session ends.
#[derive(Debug)]
#[must_use = "dropping this guard restores the process terminal"]
pub struct TerminalRestoreGuard {
    keyboard_enhancements_enabled: bool,
}

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        if self.keyboard_enhancements_enabled
            && let Err(err) = disable_keyboard_enhancements()
        {
            tracing::debug!("failed to disable keyboard enhancements: {err}");
        }
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
        let terminal = ratatui::try_init_with_options(TerminalOptions {
            viewport: Viewport::Inline(INLINE_VIEWPORT_HEIGHT),
        })?;
        let keyboard_enhancements_enabled = match enable_keyboard_enhancements() {
            Ok(()) => true,
            Err(err) => {
                tracing::debug!("failed to enable keyboard enhancements: {err}");
                false
            }
        };
        tracing::debug!("terminal initialized");
        if let Some(prompt) = initial_prompt {
            tracing::debug!(
                prompt.bytes = prompt.len(),
                "initial prompt will be dispatched by the application run loop",
            );
        }
        let tui = Self {
            context,
            printed_history_entries: 0,
            state: State::default(),
            terminal,
        };

        Ok((
            tui,
            TerminalRestoreGuard {
                keyboard_enhancements_enabled,
            },
        ))
    }
}

fn enable_keyboard_enhancements() -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
}

fn disable_keyboard_enhancements() -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, PopKeyboardEnhancementFlags)
}

#[cfg(test)]
impl Tui<TestBackend> {
    /// Creates a TUI scaffold backed by an in-memory test terminal.
    #[must_use]
    pub(super) fn new_test(context: AppContext) -> Self {
        let terminal = ratatui::Terminal::with_options(
            TestBackend::new(80, 24),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_VIEWPORT_HEIGHT),
            },
        )
        .expect("test backend terminal is infallible");

        let mut tui = Self {
            context,
            printed_history_entries: 0,
            state: State::default(),
            terminal,
        };
        tui.view().expect("test backend terminal is infallible");

        tui
    }
}

impl<B: ClearableBackend> Tui<B> {
    /// Handles one input event and optionally produces a router command.
    ///
    /// This scaffold does not map keys to commands yet.
    pub fn handle_input_event(&mut self, event: InputEvent) -> anyhow::Result<Option<Cmd>> {
        let event_kind = event.kind();
        tracing::trace!(
            input.event = event_kind,
            "handling input event {{input.event}}",
        );

        let cmd = self.on_input(event);
        tracing::trace!(
            "event {event_kind} has produced command {cmd}",
            cmd = cmd.as_ref().map(|cmd| command_name(cmd)).unwrap_or("none")
        );
        self.view()?;

        Ok(cmd)
    }

    /// Handles one router message and optionally produces a follow-up command.
    ///
    /// This scaffold does not update UI state yet.
    pub fn handle_client_msg(&mut self, msg: Msg) -> anyhow::Result<()> {
        let clear_terminal = matches!(msg, Msg::SessionClosed { .. });
        tracing::trace!(
            message = super::message_name(&msg),
            "handling client message"
        );

        // apply the message to the TUI state
        self.state.apply_msg(msg);

        if clear_terminal {
            self.clear_terminal()?;
        }

        // then view
        self.view()
    }

    /// Redraws the TUI when transient state needs a visual refresh.
    ///
    /// Returns `true` when a frame was rendered.
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal draw operation fails.
    pub fn refresh(&mut self) -> anyhow::Result<bool> {
        if !self.state.router.needs_refresh() {
            return Ok(false);
        }

        self.render_view(false)?;
        Ok(true)
    }

    fn insert_transcript_entries(&mut self) -> anyhow::Result<bool> {
        if self.printed_history_entries > self.state.history.len() {
            self.printed_history_entries = 0;
        }

        if self.printed_history_entries >= self.state.history.len() {
            return Ok(false);
        }

        let width = self
            .terminal
            .size()
            .map_err(|err| anyhow::anyhow!("failed to read terminal size: {err}"))?
            .width;
        let lines = view::console::history_lines_for_width(
            &self.state.history[self.printed_history_entries..],
            width,
        );
        self.printed_history_entries = self.state.history.len();
        if lines.is_empty() {
            return Ok(false);
        }

        let height = view::console::history_rendered_height(&lines, width);
        self.terminal
            .insert_before(height, |buffer| {
                view::console::render_history_lines(lines, buffer);
            })
            .map_err(|err| anyhow::anyhow!("failed to insert transcript entries: {err}"))?;

        Ok(true)
    }

    fn clear_terminal(&mut self) -> anyhow::Result<()> {
        self.terminal
            .backend_mut()
            .clear_scrollback()
            .map_err(|err| anyhow::anyhow!("failed to clear terminal scrollback: {err}"))?;
        self.terminal
            .clear()
            .map_err(|err| anyhow::anyhow!("failed to clear terminal viewport: {err}"))?;
        self.printed_history_entries = 0;
        Ok(())
    }

    fn try_pin_inline_viewport_to_bottom(&mut self) -> anyhow::Result<()> {
        self.terminal
            .autoresize()
            .map_err(|err| anyhow::anyhow!("failed to resize inline viewport: {err}"))?;
        let terminal_height = self
            .terminal
            .size()
            .map_err(|err| anyhow::anyhow!("failed to read terminal size: {err}"))?
            .height;
        let viewport_bottom = self.terminal.get_frame().area().bottom();
        let missing_lines = terminal_height.saturating_sub(viewport_bottom);
        if missing_lines == 0 {
            return Ok(());
        }

        self.terminal
            .insert_before(missing_lines, |_| {})
            .map_err(|err| anyhow::anyhow!("failed to pin inline viewport: {err}"))?;

        Ok(())
    }
}
