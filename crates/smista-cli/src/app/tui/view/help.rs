//! Slash-command help rendering for the select component.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::tui::state::HelpEntry;

// Rust guideline compliant 2026-07-14.
pub(in crate::app::tui) fn help_line(entry: &HelpEntry) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{usage:<24}", usage = entry.usage()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(entry.description(), Style::default().fg(Color::White)),
    ])
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::tui::state::{ActiveComponentState, State};

    #[test]
    fn help_lines_render_every_command_usage_and_description() {
        let mut state = State::default();
        state.show_help();
        let ActiveComponentState::Help(help) = state.active_component else {
            panic!("help component expected");
        };

        for entry in help.entries() {
            let rendered = help_line(entry)
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();

            assert!(rendered.contains(entry.usage()));
            assert!(rendered.contains(entry.description()));
        }
    }

    #[test]
    fn help_list_scrolls_to_selected_commands_in_short_terminals() {
        let mut app_state = State::default();
        app_state.show_help();
        let ActiveComponentState::Help(mut help) = app_state.active_component else {
            panic!("help component expected");
        };
        help.last();
        let mut terminal = Terminal::new(TestBackend::new(80, 6)).expect("terminal builds");

        terminal
            .draw(|frame| {
                super::super::select::view_select(
                    frame,
                    "Help",
                    "No commands found",
                    &help,
                    help_line,
                );
            })
            .expect("help list renders");

        let screen = terminal.backend().to_string();
        assert!(screen.contains("/usage"));
        assert!(!screen.contains("/chat"));
    }
}
