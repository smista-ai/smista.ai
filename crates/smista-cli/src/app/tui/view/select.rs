//! Selectable list component for replacement TUI views.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState as RatatuiListState, Widget,
};

use crate::app::router_client::msg::{Provider, SessionListItem, TraceEvent};
use crate::app::tui::state::{ListState, ModelListEntry};
use crate::skills::SkillEntry;

const FOOTER: &str = "enter select  esc close";
const LOCAL_PROVIDER: &str = "local";
const MODEL_AUTO: &str = "auto";
const REMOTE_PROVIDER: &str = "remote";
const UNTITLED_SESSION: &str = "Untitled session";

pub(in crate::app::tui) fn view_select<'a, T, F>(
    frame: &mut Frame<'a>,
    title: &'static str,
    empty_message: &'static str,
    state: &ListState<T>,
    render_entry: F,
) where
    F: Fn(&T) -> Line<'static>,
{
    let [list_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    Clear.render(frame.area(), frame.buffer_mut());

    let lines = if state.is_empty() {
        vec![ListItem::new(Line::styled(
            empty_message,
            palette().placeholder,
        ))]
    } else {
        state
            .entries()
            .iter()
            .map(|entry| ListItem::new(render_entry(entry)))
            .collect()
    };
    let list = List::new(lines)
        .block(Block::new().title(title).borders(Borders::BOTTOM))
        .scroll_padding(8)
        .highlight_style(palette().selected.add_modifier(Modifier::BOLD))
        .highlight_symbol("› ");
    let mut widget_state = RatatuiListState::default()
        .with_selected((!state.is_empty()).then_some(state.current_index()));

    frame.render_stateful_widget(list, list_area, &mut widget_state);
    frame.render_widget(Line::styled(FOOTER, palette().footer), footer_area);
}

pub(in crate::app::tui) fn string_line(entry: &str) -> Line<'static> {
    Line::from(entry.to_owned())
}

pub(in crate::app::tui) fn skill_line((name, entry): &(String, SkillEntry)) -> Line<'static> {
    Line::from(vec![
        Span::styled(name.to_owned(), palette().name),
        Span::raw(" "),
        Span::styled(entry.description().to_owned(), palette().dim),
    ])
}

pub(in crate::app::tui) fn model_line(entry: &ModelListEntry) -> Line<'static> {
    let ModelListEntry::Model(model) = entry else {
        return Line::from(vec![
            Span::styled(MODEL_AUTO, palette().name),
            Span::raw(" "),
            Span::styled("deterministic routing", palette().metadata),
        ]);
    };

    let max_output_tokens = model
        .max_output_tokens
        .map_or_else(|| "unbounded".to_owned(), |tokens| tokens.to_string());

    Line::from(vec![
        Span::styled(model.display_name.clone(), palette().name),
        Span::raw(" "),
        Span::styled(model.provider.clone(), palette().metadata),
        Span::raw(" "),
        Span::styled(
            format!(
                "{} ctx / {} out",
                model.max_context_tokens, max_output_tokens
            ),
            palette().metadata,
        ),
    ])
}

pub(in crate::app::tui) fn provider_line(provider: &Provider) -> Line<'static> {
    let kind = if provider.local {
        LOCAL_PROVIDER
    } else {
        REMOTE_PROVIDER
    };

    Line::from(vec![
        Span::styled(provider.name.clone(), palette().name),
        Span::raw(" "),
        Span::styled(kind, palette().metadata),
    ])
}

pub(in crate::app::tui) fn session_line(session: &SessionListItem) -> Line<'static> {
    let title = session.title.as_deref().unwrap_or(UNTITLED_SESSION);
    let scope = session.scope.as_deref().unwrap_or("default");

    Line::from(vec![
        Span::styled(title.to_owned(), palette().name),
        Span::raw(" "),
        Span::styled(scope.to_owned(), palette().metadata),
        Span::raw(" "),
        Span::styled(session.updated_at.clone(), palette().dim),
        Span::raw(" "),
        Span::styled(session.id.to_string(), palette().id),
    ])
}

pub(in crate::app::tui) fn trace_line(trace: &TraceEvent) -> Line<'static> {
    Line::from(vec![
        Span::styled(trace.event_type, palette().name),
        Span::raw(" "),
        Span::styled(trace.task_type, palette().metadata),
        Span::raw(" "),
        Span::styled(format!("{}/{}", trace.provider, trace.model), palette().dim),
        Span::raw(" "),
        Span::styled(trace.created_at.clone(), palette().id),
    ])
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    dim: Style,
    footer: Style,
    id: Style,
    metadata: Style,
    name: Style,
    placeholder: Style,
    selected: Style,
}

fn palette() -> Palette {
    Palette {
        dim: Style::default().fg(Color::DarkGray),
        footer: Style::default().fg(Color::DarkGray),
        id: Style::default().fg(Color::DarkGray),
        metadata: Style::default().fg(Color::Cyan),
        name: Style::default().fg(Color::White),
        placeholder: Style::default().fg(Color::DarkGray),
        selected: Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use uuid::Uuid;

    use super::*;
    use crate::app::router_client::msg::{Model, Provider, TraceEvent};
    use crate::skills::SkillStore;

    const SESSION_UPDATED_AT: &str = "2026-07-08T10:00:00Z";
    const SKILL_DESCRIPTION: &str = "Review code changes.";
    const SKILL_NAME: &str = "code-review";
    const TRACE_CREATED_AT: &str = "2026-07-08T11:00:00Z";

    #[test]
    fn view_select_renders_entries_and_footer() {
        let mut terminal = Terminal::new(TestBackend::new(40, 6)).expect("terminal builds");
        let state = ListState::with_current_index(vec!["first".to_owned(), "second".to_owned()], 1);

        terminal
            .draw(|frame| {
                view_select(frame, "Items", "No items", &state, |entry| {
                    string_line(entry)
                })
            })
            .expect("select view renders");

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Items"));
        assert!(screen.contains("first"));
        assert!(screen.contains("second"));
        assert!(screen.contains("enter select"));
    }

    #[test]
    fn view_select_renders_empty_message_without_selection() {
        let mut terminal = Terminal::new(TestBackend::new(40, 5)).expect("terminal builds");
        let state = ListState::<String>::default();

        terminal
            .draw(|frame| {
                view_select(frame, "Items", "No items", &state, |entry| {
                    string_line(entry)
                })
            })
            .expect("empty select view renders");

        let screen = terminal.backend().to_string();
        assert!(screen.contains("Items"));
        assert!(screen.contains("No items"));
        assert!(screen.contains("enter select"));
    }

    #[test]
    fn string_line_renders_the_entry_text() {
        let rendered = string_line("log entry")
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(rendered, "log entry");
    }

    #[test]
    fn skill_line_renders_explicit_name_and_description() {
        let root = tempfile::tempdir().expect("temporary directory is created");
        write_skill(root.path(), SKILL_NAME, SKILL_DESCRIPTION);
        let store = SkillStore::discover(root.path());
        let entry = store.get(SKILL_NAME).expect("skill is discovered").clone();

        let rendered = line_text(skill_line(&(SKILL_NAME.to_owned(), entry)));

        assert!(rendered.contains(SKILL_NAME));
        assert!(rendered.contains(SKILL_DESCRIPTION));
    }

    #[test]
    fn model_line_includes_model_metadata() {
        let model = Model {
            reference: smista_sdk::core::model::ModelReference {
                provider: smista_sdk::core::model::Provider::OpenAI,
                model: "gpt-4.1".to_owned(),
            },
            provider: "openai".to_owned(),
            id: "gpt-4.1".to_owned(),
            display_name: "GPT-4.1".to_owned(),
            max_context_tokens: 128_000,
            max_output_tokens: Some(16_000),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        };

        let rendered = line_text(model_line(&ModelListEntry::Model(model)));

        assert!(rendered.contains("GPT-4.1"));
        assert!(rendered.contains("openai"));
        assert!(rendered.contains("128000 ctx / 16000 out"));
    }

    #[test]
    fn model_line_renders_auto_entry() {
        let rendered = line_text(model_line(&ModelListEntry::Auto));

        assert!(rendered.contains("auto"));
        assert!(rendered.contains("deterministic routing"));
    }

    #[test]
    fn model_line_renders_unbounded_output() {
        let model = Model {
            reference: smista_sdk::core::model::ModelReference {
                provider: smista_sdk::core::model::Provider::OpenAI,
                model: "gpt-4.1".to_owned(),
            },
            provider: "local".to_owned(),
            id: "model".to_owned(),
            display_name: "Local Model".to_owned(),
            max_context_tokens: 4096,
            max_output_tokens: None,
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
        };

        assert!(
            line_text(model_line(&ModelListEntry::Model(model)))
                .contains("4096 ctx / unbounded out")
        );
    }

    #[test]
    fn provider_line_distinguishes_local_and_remote() {
        let local = Provider {
            name: "ollama".to_owned(),
            local: true,
        };
        let remote = Provider {
            name: "openai".to_owned(),
            local: false,
        };

        assert!(line_text(provider_line(&local)).contains("local"));
        assert!(line_text(provider_line(&remote)).contains("remote"));
    }

    #[test]
    fn session_line_includes_title_metadata_and_id() {
        let id = Uuid::from_u128(42);
        let session = SessionListItem {
            id,
            title: Some("Investigate bug".to_owned()),
            scope: Some("project".to_owned()),
            updated_at: SESSION_UPDATED_AT.to_owned(),
        };

        let rendered = session_line(&session)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains("Investigate bug"));
        assert!(rendered.contains("project"));
        assert!(rendered.contains(SESSION_UPDATED_AT));
        assert!(rendered.contains(&id.to_string()));
    }

    #[test]
    fn session_line_uses_fallbacks_for_missing_title_and_scope() {
        let session = SessionListItem {
            id: Uuid::nil(),
            title: None,
            scope: None,
            updated_at: SESSION_UPDATED_AT.to_owned(),
        };

        let rendered = session_line(&session)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(rendered.contains(UNTITLED_SESSION));
        assert!(rendered.contains("default"));
    }

    #[test]
    fn trace_line_includes_event_metadata() {
        let trace = TraceEvent {
            event_type: "route",
            task_type: "code",
            provider: "openai".to_owned(),
            model: "gpt-4.1".to_owned(),
            matched_rule: None,
            created_at: TRACE_CREATED_AT.to_owned(),
            payload: "{}".to_owned(),
        };

        let rendered = line_text(trace_line(&trace));

        assert!(rendered.contains("route"));
        assert!(rendered.contains("code"));
        assert!(rendered.contains("openai/gpt-4.1"));
        assert!(rendered.contains(TRACE_CREATED_AT));
    }

    #[test]
    fn selected_row_style_preserves_entry_colors() {
        assert_eq!(palette().selected.fg, None);
    }

    fn line_text(line: Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn write_skill(root: &std::path::Path, name: &str, description: &str) {
        let skill_dir = root.join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).expect("skill directory is created");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nUse this skill.\n"),
        )
        .expect("skill descriptor is written");
    }
}
