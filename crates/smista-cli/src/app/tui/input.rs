use super::Tui;
use crate::app::input_listener::InputEvent;
use crate::app::router_client::Cmd;
use crate::app::tui::state::HistoryEntry;

impl<B> Tui<B>
where
    B: ratatui::backend::Backend,
{
    /// Handles one input event and optionally produces a router command.
    ///
    /// Returns `Some(Cmd)` if a command is produced and should be sent to the router, or `None` if no command is produced.
    pub(in crate::app::tui) fn on_input(&mut self, event: InputEvent) -> Option<Cmd> {
        match event {
            InputEvent::Backspace => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.backspace();
                }
            }
            InputEvent::Newline => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.push('\n');
                }
            }
            InputEvent::Enter => {
                let message = self
                    .state
                    .active_component
                    .console_mut()
                    .and_then(|console| {
                        let input = console.prompt.input();
                        (!input.is_empty()).then(|| {
                            console.prompt.clear();
                            input
                        })
                    });
                if let Some(message) = message {
                    self.state.push_history(HistoryEntry::UserMessage(message));
                }
            }
            InputEvent::Tab => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.next_suggestion();
                }
            }
            InputEvent::Paste(content) => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.push_str(&content);
                }
            }
            InputEvent::Delete => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.delete();
                }
            }
            InputEvent::Left => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.move_left();
                }
            }
            InputEvent::Right => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.move_right();
                }
            }
            InputEvent::Up => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.move_up();
                }
            }
            InputEvent::Down => {
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.move_down();
                }
            }
            InputEvent::Interrupt => {
                tracing::debug!("input event is interrupt, cancelling application run loop");
                self.context.exit.cancel();
            }
            InputEvent::Char(char) => {
                tracing::debug!("input event is character, pushing to prompt input state");
                if let Some(console) = self.state.active_component.console_mut() {
                    console.prompt.push(char);
                }
            }
            _ => {
                tracing::debug!("input event is not handled by the TUI scaffold");
            }
        }

        None
    }
}
