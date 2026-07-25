//! Session usage replacement view.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Widget};
use smista_sdk::core::api::SessionUsageResponse;
use smista_sdk::core::usage::Usage;

use crate::app::tui::state::UsageState;

const EMPTY_USAGE: &str = "No token usage recorded";
const FOOTER: &str = "esc close";
const SECTION_MODELS: &str = "By model";
const SECTION_TASKS: &str = "By task";
const TITLE: &str = "Usage";

pub(in crate::app::tui) fn view<'a>(frame: &mut Frame<'a>, state: &UsageState) {
    let [usage_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    Clear.render(frame.area(), frame.buffer_mut());

    let list = List::new(usage_lines(state.usage()))
        .block(Block::new().title(TITLE).borders(Borders::BOTTOM));

    frame.render_widget(list, usage_area);
    frame.render_widget(Line::styled(FOOTER, palette().footer), footer_area);
}

fn usage_lines(response: &SessionUsageResponse) -> Vec<ListItem<'static>> {
    let colors = palette();
    let session_tokens = session_token_count(response);
    let mut lines = vec![ListItem::new(Line::styled(
        "Total",
        colors.section.add_modifier(Modifier::BOLD),
    ))];

    if has_token_usage(&response.total) {
        lines.push(ListItem::new(Line::from(stat_spans(&response.total))));
    } else {
        lines.push(ListItem::new(Line::styled(EMPTY_USAGE, colors.placeholder)));
    }

    let model_lines = response
        .by_model
        .iter()
        .filter(|entry| has_token_usage(&entry.usage))
        .map(|entry| {
            entry_line(
                format!(
                    "{provider}/{model}",
                    provider = entry.provider,
                    model = entry.model
                ),
                &entry.usage,
                entry.request_count,
                session_tokens,
            )
        })
        .collect::<Vec<_>>();
    append_section(&mut lines, SECTION_MODELS, model_lines);

    let task_lines = response
        .by_task_type
        .iter()
        .filter(|entry| has_token_usage(&entry.usage))
        .map(|entry| {
            entry_line(
                entry.task_type.to_string(),
                &entry.usage,
                entry.request_count,
                session_tokens,
            )
        })
        .collect::<Vec<_>>();
    append_section(&mut lines, SECTION_TASKS, task_lines);

    lines
}

fn append_section(
    lines: &mut Vec<ListItem<'static>>,
    title: &'static str,
    entries: Vec<ListItem<'static>>,
) {
    if entries.is_empty() {
        return;
    }

    lines.push(ListItem::new(Line::default()));
    lines.push(ListItem::new(Line::styled(
        title,
        palette().section.add_modifier(Modifier::BOLD),
    )));
    lines.extend(entries);
}

fn entry_line(
    name: String,
    usage: &Usage,
    request_count: u32,
    session_tokens: u64,
) -> ListItem<'static> {
    let colors = palette();
    let percentage = percentage(token_count(usage), session_tokens);
    let requests = if request_count == 1 {
        "1 request".to_owned()
    } else {
        format!("{request_count} requests")
    };

    let mut spans = vec![
        Span::styled(format!("{percentage:>5.1}%"), colors.percentage),
        Span::raw("  "),
        Span::styled(name, colors.name.add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {requests} · "), colors.metadata),
    ];
    spans.extend(stat_spans(usage));

    ListItem::new(Line::from(spans))
}

fn stat_spans(usage: &Usage) -> Vec<Span<'static>> {
    let colors = palette();
    let mut stats = vec![format!("{count} tokens", count = token_count(usage))];
    push_token_stat(&mut stats, usage.input_tokens, "input");
    push_token_stat(&mut stats, usage.cached_tokens, "cached");
    push_token_stat(&mut stats, usage.output_tokens, "output");
    push_token_stat(&mut stats, usage.reasoning_tokens, "reasoning");

    let mut spans = vec![Span::styled(stats.join(" · "), colors.metadata)];
    if let Some(cost) = usage.actual_cost {
        spans.push(Span::styled(
            format!(" · {} {cost} actual", usage.currency()),
            colors.cost,
        ));
    }
    if let Some(cost) = usage.estimated_cost {
        spans.push(Span::styled(
            format!(" · {} {cost} estimated", usage.currency()),
            colors.cost,
        ));
    }
    spans
}

fn push_token_stat(stats: &mut Vec<String>, tokens: Option<u64>, label: &str) {
    if let Some(tokens) = tokens.filter(|tokens| *tokens > 0) {
        stats.push(format!("{tokens} {label}"));
    }
}

fn session_token_count(response: &SessionUsageResponse) -> u64 {
    let total = token_count(&response.total);
    if total > 0 {
        return total;
    }

    let by_model = response
        .by_model
        .iter()
        .map(|entry| token_count(&entry.usage))
        .fold(0_u64, u64::saturating_add);
    if by_model > 0 {
        return by_model;
    }

    response
        .by_task_type
        .iter()
        .map(|entry| token_count(&entry.usage))
        .fold(0_u64, u64::saturating_add)
}

fn has_token_usage(usage: &Usage) -> bool {
    [
        usage.input_tokens,
        usage.output_tokens,
        usage.cached_tokens,
        usage.reasoning_tokens,
        usage.total_tokens,
    ]
    .into_iter()
    .flatten()
    .any(|tokens| tokens > 0)
}

fn token_count(usage: &Usage) -> u64 {
    if let Some(total) = usage.total_tokens.filter(|tokens| *tokens > 0) {
        return total;
    }

    let input_and_output = usage
        .input_tokens
        .unwrap_or_default()
        .saturating_add(usage.output_tokens.unwrap_or_default());
    if input_and_output > 0 {
        return input_and_output;
    }

    usage
        .cached_tokens
        .unwrap_or_default()
        .max(usage.reasoning_tokens.unwrap_or_default())
}

fn percentage(tokens: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }

    100.0 * tokens as f64 / total as f64
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    cost: Style,
    footer: Style,
    metadata: Style,
    name: Style,
    percentage: Style,
    placeholder: Style,
    section: Style,
}

fn palette() -> Palette {
    Palette {
        cost: Style::default().fg(Color::Green),
        footer: Style::default().fg(Color::DarkGray),
        metadata: Style::default().fg(Color::DarkGray),
        name: Style::default().fg(Color::White),
        percentage: Style::default().fg(Color::Cyan),
        placeholder: Style::default().fg(Color::DarkGray),
        section: Style::default().fg(Color::White),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use rust_decimal::Decimal;
    use smista_sdk::core::api::{ModelUsage, TaskTypeUsage};
    use smista_sdk::core::intent::TaskIntent;
    use smista_sdk::core::model::Provider;

    use super::*;

    fn usage(input_tokens: u64, output_tokens: u64, total_tokens: Option<u64>) -> Usage {
        Usage {
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            total_tokens,
            ..Default::default()
        }
    }

    #[test]
    fn view_renders_usage_buffer_split_by_model_and_task() {
        let response = SessionUsageResponse {
            total: Usage {
                estimated_cost: Some(Decimal::new(42, 2)),
                currency: Some("USD".to_owned()),
                ..usage(12_000, 4_200, Some(16_200))
            },
            by_model: vec![
                ModelUsage {
                    provider: Provider::OpenAI,
                    model: "gpt-5.5-thinking".to_owned(),
                    usage: Usage {
                        estimated_cost: Some(Decimal::new(31, 2)),
                        currency: Some("USD".to_owned()),
                        ..usage(8_000, 2_200, Some(10_200))
                    },
                    request_count: 3,
                },
                ModelUsage {
                    provider: Provider::Ollama,
                    model: "unused".to_owned(),
                    usage: Usage::default(),
                    request_count: 1,
                },
            ],
            by_task_type: vec![
                TaskTypeUsage {
                    task_type: TaskIntent::Plan,
                    usage: Usage {
                        estimated_cost: Some(Decimal::new(18, 2)),
                        ..usage(4_000, 1_200, None)
                    },
                    request_count: 1,
                },
                TaskTypeUsage {
                    task_type: TaskIntent::Chat,
                    usage: Usage::default(),
                    request_count: 2,
                },
            ],
        };
        let state = UsageState::new(response);
        let mut terminal = Terminal::new(TestBackend::new(110, 12)).expect("terminal builds");

        terminal
            .draw(|frame| view(frame, &state))
            .expect("usage view renders");

        let buffer = terminal.backend().to_string();
        assert!(buffer.contains("Usage"));
        assert!(buffer.contains("16200 tokens · 12000 input · 4200 output · USD 0.42 estimated"));
        assert!(buffer.contains("63.0%  openai/gpt-5.5-thinking"));
        assert!(buffer.contains("32.1%  plan"));
        assert!(buffer.contains("esc close"));
        assert!(!buffer.contains("ollama/unused"));
        assert!(!buffer.contains("chat"));
    }

    #[test]
    fn view_omits_empty_breakdown_sections() {
        let state = UsageState::new(SessionUsageResponse {
            total: Usage::default(),
            by_model: vec![ModelUsage {
                provider: Provider::OpenAI,
                model: "unused".to_owned(),
                usage: Usage {
                    estimated_cost: Some(Decimal::new(10, 2)),
                    ..Default::default()
                },
                request_count: 1,
            }],
            by_task_type: vec![],
        });
        let mut terminal = Terminal::new(TestBackend::new(60, 6)).expect("terminal builds");

        terminal
            .draw(|frame| view(frame, &state))
            .expect("empty usage view renders");

        let buffer = terminal.backend().to_string();
        assert!(buffer.contains(EMPTY_USAGE));
        assert!(!buffer.contains(SECTION_MODELS));
        assert!(!buffer.contains(SECTION_TASKS));
        assert!(!buffer.contains("openai/unused"));
    }

    #[test]
    fn session_total_falls_back_to_breakdown_tokens() {
        let response = SessionUsageResponse {
            total: Usage::default(),
            by_model: vec![ModelUsage {
                provider: Provider::OpenAI,
                model: "gpt".to_owned(),
                usage: usage(75, 25, None),
                request_count: 1,
            }],
            by_task_type: vec![],
        };

        assert_eq!(session_token_count(&response), 100);
        assert_eq!(percentage(25, 100), 25.0);
    }
}
