//! Terminal input worker for the interactive CLI.

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt as _;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

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
    /// Backspace
    Backspace,
    /// Delete
    Delete,
    /// Home key
    Home,
    /// End key
    End,
    /// Page Up key
    PageUp,
    /// Page Down key
    PageDown,
    /// Enter key
    Enter,
    /// Tab key
    Tab,
    /// Escape key
    Escape,
    /// Arrow key up
    Up,
    /// Arrow key down
    Down,
    /// Arrow key left
    Left,
    /// Arrow key right
    Right,
    /// Modified Enter that inserts a line break into the prompt.
    Newline,
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
            Self::Enter => "enter",
            Self::Tab => "tab",
            Self::Escape => "escape",
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::Backspace => "backspace",
            Self::Delete => "delete",
            Self::Home => "home",
            Self::End => "end",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::Newline => "newline",
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
            InputEvent::Enter => tracing::trace!("InputListener received enter key"),
            InputEvent::Tab => tracing::trace!("InputListener received tab key"),
            InputEvent::Escape => tracing::trace!("InputListener received escape key"),
            InputEvent::Up => tracing::trace!("InputListener received arrow up key"),
            InputEvent::Down => tracing::trace!("InputListener received arrow down key"),
            InputEvent::Left => tracing::trace!("InputListener received arrow left key"),
            InputEvent::Right => tracing::trace!("InputListener received arrow right key"),
            InputEvent::Backspace => tracing::trace!("InputListener received backspace key"),
            InputEvent::Delete => tracing::trace!("InputListener received delete key"),
            InputEvent::Home => tracing::trace!("InputListener received home key"),
            InputEvent::End => tracing::trace!("InputListener received end key"),
            InputEvent::PageUp => tracing::trace!("InputListener received page up key"),
            InputEvent::PageDown => tracing::trace!("InputListener received page down key"),
            InputEvent::Newline => tracing::trace!("InputListener received newline key"),
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
            code: KeyCode::Char('j' | 'm'),
            modifiers,
            kind,
            state: _,
        }) if modifiers.intersects(KeyModifiers::CONTROL) && is_edit_key_event(kind) => {
            Some(InputEvent::Newline)
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char('\n' | '\r'),
            modifiers,
            kind,
            state: _,
        }) if (modifiers.is_empty() || modifiers.intersects(KeyModifiers::CONTROL))
            && is_edit_key_event(kind) =>
        {
            Some(InputEvent::Newline)
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char(' '),
            modifiers,
            kind,
            state: _,
        }) if modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER | KeyModifiers::META)
            && is_edit_key_event(kind) =>
        {
            Some(InputEvent::Newline)
        }
        Event::Key(KeyEvent {
            code: KeyCode::Char(char),
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Char(char)),
        Event::Key(KeyEvent {
            code: KeyCode::Backspace,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Backspace),
        Event::Key(KeyEvent {
            code: KeyCode::Delete,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Delete),
        Event::Key(KeyEvent {
            code: KeyCode::Home,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Home),
        Event::Key(KeyEvent {
            code: KeyCode::End,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::End),
        Event::Key(KeyEvent {
            code: KeyCode::PageUp,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::PageUp),
        Event::Key(KeyEvent {
            code: KeyCode::PageDown,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::PageDown),
        Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers,
            kind: KeyEventKind::Press,
            state: _,
        }) if modifiers.intersects(
            KeyModifiers::ALT
                | KeyModifiers::SUPER
                | KeyModifiers::META
                | KeyModifiers::SHIFT
                | KeyModifiers::CONTROL,
        ) =>
        {
            Some(InputEvent::Newline)
        }
        Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers,
            kind: KeyEventKind::Press,
            state: _,
        }) if modifiers.is_empty() => Some(InputEvent::Enter),
        Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Tab),
        Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers,
            kind: KeyEventKind::Press,
            state: _,
        }) if modifiers.is_empty() => Some(InputEvent::Escape),
        Event::Key(KeyEvent {
            code: KeyCode::Up,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Up),
        Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Down),
        Event::Key(KeyEvent {
            code: KeyCode::Left,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Left),
        Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers,
            kind,
            state: _,
        }) if modifiers.is_empty() && is_edit_key_event(kind) => Some(InputEvent::Right),
        Event::Paste(content) => Some(InputEvent::Paste(content)),
        Event::Resize(_, _) => Some(InputEvent::Resize),
        _ => None,
    }
}

fn is_edit_key_event(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
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
                Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::ALT)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::empty())),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Char('\r'), KeyModifiers::empty())),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty())),
                InputEvent::Backspace,
            ),
            (
                Event::Key(KeyEvent::new_with_kind(
                    KeyCode::Backspace,
                    KeyModifiers::empty(),
                    KeyEventKind::Repeat,
                )),
                InputEvent::Backspace,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Delete, KeyModifiers::empty())),
                InputEvent::Delete,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::empty())),
                InputEvent::Home,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::empty())),
                InputEvent::End,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty())),
                InputEvent::PageUp,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty())),
                InputEvent::PageDown,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())),
                InputEvent::Enter,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::META)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL)),
                InputEvent::Newline,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::empty())),
                InputEvent::Tab,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())),
                InputEvent::Escape,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::empty())),
                InputEvent::Up,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty())),
                InputEvent::Down,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::empty())),
                InputEvent::Left,
            ),
            (
                Event::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty())),
                InputEvent::Right,
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
