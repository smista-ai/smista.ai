//! Terminal input worker for the interactive CLI.

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt as _;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
pub mod mock;

/// Input events emitted by the input listener.
///
/// These variants keep terminal-specific input details out of the application
/// run loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    /// Interrupt signal, such as Ctrl+C.
    Interrupt,
    /// Printable character input.
    Char(char),
    /// Pasted terminal content.
    Paste(String),
    /// Terminal resize notification.
    Resize,
}

impl InputEvent {
    /// Returns a stable non-sensitive label for this input event.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Interrupt => "interrupt",
            Self::Char(_) => "char",
            Self::Paste(_) => "paste",
            Self::Resize => "resize",
        }
    }
}

/// Ratatui keyboard input listener.
///
/// The worker reads terminal events until cancellation and forwards the subset
/// understood by the application as [`InputEvent`] values.
pub struct InputListener {
    exit: CancellationToken,
    tx: Sender<InputEvent>,
}

impl InputListener {
    /// Creates an input listener bound to the shared cancellation token.
    #[must_use]
    pub fn new(exit: CancellationToken, tx: Sender<InputEvent>) -> Self {
        Self { exit, tx }
    }

    /// Spawns the input listener task.
    #[must_use]
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(self.run_loop())
    }

    async fn run_loop(self) {
        tracing::debug!("InputListener started");

        let mut events = EventStream::new();

        loop {
            tokio::select! {
                _ = self.exit.cancelled() => {
                    tracing::debug!("InputListener received cancellation");
                    break;
                }
                maybe_event = events.next() => {
                    match maybe_event {
                        None => {
                            tracing::debug!("InputListener event stream closed");
                            break;
                        }
                        Some(Err(error)) => {
                            tracing::error!("InputListener event stream error: {error}");
                        }
                        Some(Ok(event)) => {
                            self.handle_event(event).await;
                        }
                    }
                }
            }
        }

        tracing::info!("InputListener stopped");
    }

    async fn handle_event(&self, event: Event) {
        let out_event = decode_event(event);
        let Some(out_event) = out_event else {
            tracing::trace!("InputListener received unhandled terminal event");
            return;
        };

        match &out_event {
            InputEvent::Interrupt => tracing::trace!("InputListener received interrupt"),
            InputEvent::Char(_) => tracing::trace!("InputListener received character input"),
            InputEvent::Paste(_) => tracing::trace!("InputListener received pasted content"),
            InputEvent::Resize => tracing::trace!("InputListener received terminal resize"),
        }

        if let Err(err) = self.tx.send(out_event).await {
            tracing::error!("InputListener failed to send event: {err}; exiting");
            self.exit.cancel();
        }
    }
}

fn decode_event(event: Event) -> Option<InputEvent> {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            kind: KeyEventKind::Press,
            state: _,
        }) if modifiers.intersects(KeyModifiers::CONTROL) => Some(InputEvent::Interrupt),
        Event::Key(KeyEvent {
            code: KeyCode::Char(char),
            modifiers,
            kind: KeyEventKind::Press,
            state: _,
        }) if modifiers.is_empty() => Some(InputEvent::Char(char)),
        Event::Paste(content) => Some(InputEvent::Paste(content)),
        Event::Resize(_, _) => Some(InputEvent::Resize),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{InputEvent, decode_event};

    #[test]
    fn should_decode_supported_terminal_events() {
        let events = [
            (
                Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
                InputEvent::Interrupt,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty())),
                InputEvent::Char('x'),
            ),
            (
                Event::Paste("hello".to_owned()),
                InputEvent::Paste("hello".to_owned()),
            ),
            (Event::Resize(80, 24), InputEvent::Resize),
        ];

        for (terminal_event, expected_event) in events {
            assert_eq!(Some(expected_event), decode_event(terminal_event));
        }
    }

    #[test]
    fn should_ignore_unsupported_terminal_events() {
        let events = [
            Event::FocusGained,
            Event::Key(KeyEvent::new_with_kind(
                KeyCode::Char('x'),
                KeyModifiers::empty(),
                KeyEventKind::Release,
            )),
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT)),
        ];

        for terminal_event in events {
            assert_eq!(None, decode_event(terminal_event));
        }
    }
}
