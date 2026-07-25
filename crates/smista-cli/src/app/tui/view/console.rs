//! Console component for the TUI application.

use std::env;
use std::path::Path;
use std::time::Duration;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget, Wrap};
use smista_sdk::core::model::ModelReference;

use crate::app::log::{AppLogEntry, LogLevel};
use crate::app::router_client::msg::{ApprovalPrompt, PreviewSummary, TraceEvent};
use crate::app::tui::state::{ConsoleState, ExecutionTurn, HistoryEntry, PromptState, RouterState};

const EDIT_FILE_TOOL: &str = "edit_file";
const HISTORY_ENTRY_VERTICAL_PADDING: usize = 2;
const INPUT_VERTICAL_PADDING: usize = 2;
const DARK_USER_MESSAGE_ALPHA: f32 = 0.24;
const DEFAULT_INPUT_BACKGROUND: Color = Color::Rgb(35, 35, 35);
const LIGHT_USER_MESSAGE_ALPHA: f32 = 0.04;
const PROMPT_MARKER_WIDTH: usize = 2;
const READ_FILE_TOOL: &str = "read_file";
const WRITE_FILE_TOOL: &str = "write_file";
const PROMPT_MARKER: &str = "› ";
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn view_console<'a>(
    frame: &mut Frame<'a>,
    console: &ConsoleState,
    execution_turn: Option<&ExecutionTurn>,
    preferred_model: Option<&ModelReference>,
    router_state: RouterState,
    cwd: &Path,
    plan_mode: bool,
) {
    let live_lines = live_lines(console, execution_turn);
    let input_height = input_area_height(
        console,
        execution_turn,
        live_lines.len(),
        frame.area().width,
    );
    let status_line = status_line(router_state, execution_turn);
    let status_height = u16::from(status_line.is_some());
    let [_history_area, status_area, input_area, footer_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(status_height),
        Constraint::Length(input_height),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    Clear.render(frame.area(), frame.buffer_mut());

    if let Some(status_line) = status_line {
        Paragraph::new(status_line)
            .style(palette().input_area)
            .render(status_area, frame.buffer_mut());
    }

    Paragraph::new(padded_input_lines(live_lines))
        .style(palette().input_area)
        .wrap(Wrap { trim: false })
        .render(input_area, frame.buffer_mut());

    Paragraph::new(footer_line(preferred_model, router_state, cwd, plan_mode))
        .style(palette().input_area)
        .render(footer_area, frame.buffer_mut());

    if let Some(cursor_position) = prompt_cursor_position(console, execution_turn, input_area) {
        frame.set_cursor_position(cursor_position);
    }
}

fn status_line(
    router_state: RouterState,
    execution_turn: Option<&ExecutionTurn>,
) -> Option<Line<'static>> {
    if matches!(execution_turn, Some(ExecutionTurn::Approval(_))) {
        return None;
    }

    match router_state {
        RouterState::Idle => None,
        RouterState::Interrupted => Some(Line::from(Span::styled(
            "Interrupted. What should we do instead?",
            palette().interrupted.add_modifier(Modifier::BOLD),
        ))),
        RouterState::Thinking(started_at) => {
            let elapsed = started_at.elapsed();
            Some(Line::from(vec![
                Span::styled(spinner_frame(elapsed), palette().spinner),
                Span::raw(" "),
                Span::styled(
                    format!("Working ({}s - esc to interrupt)", elapsed.as_secs()),
                    palette().working,
                ),
            ]))
        }
    }
}

fn spinner_frame(elapsed: Duration) -> &'static str {
    let index = (elapsed.as_millis() / 120) as usize % SPINNER_FRAMES.len();
    SPINNER_FRAMES[index]
}

pub(in crate::app::tui) fn render_history_lines(lines: Vec<Line<'static>>, buffer: &mut Buffer) {
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(buffer.area, buffer);
}

pub(in crate::app::tui) fn history_rendered_height(lines: &[Line<'_>], width: u16) -> u16 {
    let height = Paragraph::new(lines.to_vec())
        .wrap(Wrap { trim: false })
        .line_count(width);
    u16::try_from(height).unwrap_or(u16::MAX)
}

pub(in crate::app::tui) fn history_lines_for_width(
    entries: &[HistoryEntry],
    width: u16,
) -> Vec<Line<'static>> {
    entries
        .iter()
        .flat_map(|entry| padded_history_entry_lines(entry, width))
        .collect::<Vec<_>>()
}

fn padded_history_entry_lines(entry: &HistoryEntry, width: u16) -> Vec<Line<'static>> {
    let entry_lines = history_entry_lines(entry, width);
    let mut lines = Vec::with_capacity(entry_lines.len() + HISTORY_ENTRY_VERTICAL_PADDING);
    lines.push(Line::raw(""));
    lines.extend(entry_lines);
    lines.push(Line::raw(""));
    lines
}

fn history_entry_lines(entry: &HistoryEntry, width: u16) -> Vec<Line<'static>> {
    match entry {
        HistoryEntry::ApprovalRequest(prompt) => approval_history_lines(prompt),
        HistoryEntry::AssistantMessage(message) => plain_block(message, palette().assistant),
        HistoryEntry::Diff { added, removed } => diff_lines(added, removed),
        HistoryEntry::Error(error) => prefixed_block("ERROR", error, palette().error),
        HistoryEntry::Log(entries) => log_lines(entries),
        HistoryEntry::Notice(message) => prefixed_block("Notice", message, palette().notice),
        HistoryEntry::Preview(preview) => preview_lines(preview),
        HistoryEntry::Reasoning(message) => prefixed_block("Thinking", message, palette().dim),
        HistoryEntry::ToolCall { name, input } => tool_call_lines(name, input),
        HistoryEntry::ToolResult { name, output } => tool_result_lines(name, output),
        HistoryEntry::Trace(trace) => trace_lines(trace),
        HistoryEntry::UserMessage(message) => user_message_lines(message, width),
    }
}

fn live_lines(
    console: &ConsoleState,
    execution_turn: Option<&ExecutionTurn>,
) -> Vec<Line<'static>> {
    match execution_turn {
        None => prompt_lines(&console.prompt),
        Some(ExecutionTurn::Streaming { content, reasoning }) => {
            streaming_lines(content, reasoning)
        }
        Some(ExecutionTurn::ToolCall(tool_call)) => vec![Line::from(vec![
            Span::styled("tool", palette().tool),
            Span::raw(" "),
            Span::raw(tool_call.name.clone()),
            Span::styled(" running", palette().dim),
        ])],
        Some(ExecutionTurn::Approval(prompt)) => approval_live_lines(console, prompt),
    }
}

fn prompt_lines(prompt: &PromptState) -> Vec<Line<'static>> {
    let input = prompt.input();
    if input.is_empty() {
        let spans = vec![
            Span::styled(PROMPT_MARKER, palette().prompt),
            Span::styled("Type a message or /command", palette().placeholder),
        ];
        return vec![Line::from(spans)];
    }

    let input_lines = input.split('\n').collect::<Vec<_>>();
    let last_index = input_lines.len().saturating_sub(1);
    input_lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let marker = if index == 0 {
                PROMPT_MARKER.to_owned()
            } else {
                " ".repeat(PROMPT_MARKER_WIDTH)
            };
            let mut spans = vec![
                Span::styled(marker, palette().prompt),
                Span::styled(line.to_owned(), palette().input_text),
            ];

            if index == last_index
                && let Some(suggestion) = prompt.current_suggestion()
                && let Some(suffix) = suggestion.strip_prefix(&input)
                && !suffix.is_empty()
            {
                spans.push(Span::styled(suffix.to_owned(), palette().placeholder));
            }

            Line::from(spans)
        })
        .collect()
}

fn prompt_content_height(prompt: &PromptState, width: u16) -> usize {
    let width = usize::from(width).max(1);
    let input = prompt.input();
    if input.is_empty() {
        return PROMPT_MARKER_WIDTH.max(1).div_ceil(width);
    }

    input
        .split('\n')
        .map(|line| {
            (PROMPT_MARKER_WIDTH + line.chars().count())
                .max(1)
                .div_ceil(width)
        })
        .sum()
}

fn prompt_cursor_position(
    console: &ConsoleState,
    execution_turn: Option<&ExecutionTurn>,
    input_area: Rect,
) -> Option<(u16, u16)> {
    if execution_turn.is_some() || input_area.is_empty() {
        return None;
    }

    let content_width = usize::from(input_area.width).max(1);
    let input = console.prompt.input();
    let cursor = console.prompt.cursor_position();
    let byte_index = char_to_byte_index(&input, cursor);
    let input_before_cursor = &input[..byte_index];
    let input_lines = input_before_cursor.split('\n').collect::<Vec<_>>();
    let previous_rows = input_lines
        .iter()
        .take(input_lines.len().saturating_sub(1))
        .map(|line| wrapped_row_count(PROMPT_MARKER_WIDTH + line.chars().count(), content_width))
        .sum::<usize>();
    let last_line_width =
        PROMPT_MARKER_WIDTH + input_lines.last().map_or(0, |line| line.chars().count());
    let content_rows = usize::from(input_area.height.saturating_sub(2)).max(1);
    let row_offset =
        (previous_rows + (last_line_width / content_width)).min(content_rows.saturating_sub(1));
    let column_offset = last_line_width % content_width;

    let x = input_area
        .x
        .saturating_add(u16::try_from(column_offset).unwrap_or(u16::MAX))
        .min(
            input_area
                .x
                .saturating_add(input_area.width.saturating_sub(1)),
        );
    let y = input_area
        .y
        .saturating_add(1)
        .saturating_add(u16::try_from(row_offset).unwrap_or(u16::MAX))
        .min(
            input_area
                .y
                .saturating_add(input_area.height.saturating_sub(1)),
        );

    Some((x, y))
}

fn wrapped_row_count(width: usize, content_width: usize) -> usize {
    width.max(1).div_ceil(content_width)
}

fn char_to_byte_index(input: &str, char_index: usize) -> usize {
    input
        .char_indices()
        .map(|(byte_index, _)| byte_index)
        .nth(char_index)
        .unwrap_or(input.len())
}

fn input_area_height(
    console: &ConsoleState,
    execution_turn: Option<&ExecutionTurn>,
    live_line_count: usize,
    width: u16,
) -> u16 {
    let content_height = match execution_turn {
        None => prompt_content_height(&console.prompt, width),
        Some(_) => live_line_count.max(1),
    };
    u16::try_from(content_height + INPUT_VERTICAL_PADDING).unwrap_or(u16::MAX)
}

fn padded_input_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    let mut padded = Vec::with_capacity(lines.len() + INPUT_VERTICAL_PADDING);
    padded.push(Line::raw(""));
    padded.extend(lines);
    padded.push(Line::raw(""));
    padded
}

fn streaming_lines(content: &str, reasoning: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !reasoning.is_empty() {
        lines.extend(prefixed_block("thinking", reasoning, palette().dim));
    }
    if !content.is_empty() {
        lines.extend(plain_block(content, palette().assistant));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("thinking", palette().dim)));
    }

    lines
}

fn approval_live_lines(console: &ConsoleState, prompt: &ApprovalPrompt) -> Vec<Line<'static>> {
    let options = approval_options(prompt);
    let selected = console.approval_option_index(options.len());
    let mut lines = vec![
        Line::from(Span::styled(prompt.title.clone(), palette().approval_title)),
        Line::from(Span::styled(
            prompt.detail.clone(),
            palette().approval_detail,
        )),
    ];

    lines.push(Line::raw(""));
    lines.extend(options.into_iter().enumerate().map(|(index, option)| {
        let style = if index == selected {
            palette().selected_option
        } else {
            palette().option
        };
        Line::from(Span::styled(option, style))
    }));

    lines
}

fn approval_options(prompt: &ApprovalPrompt) -> Vec<String> {
    let once = "1. Yes, proceed (y)".to_owned();
    let reject = "No, and tell Smista what to do differently (n)";

    if prompt.tool_name.as_deref() == Some(READ_FILE_TOOL) {
        return vec![
            once,
            "2. Yes, and don't ask again for reading files (a)".to_owned(),
            format!("3. {reject}"),
        ];
    }

    if prompt.tool_name.as_deref().is_some_and(tool_changes_files) {
        return vec![
            once,
            "2. Yes, and don't ask again for writing files (a)".to_owned(),
            format!("3. {reject}"),
        ];
    }

    if let Some(alias) = &prompt.wildcard_alias {
        return vec![
            once,
            format!("2. Yes, and don't ask again for commands that start with `{alias}` (a)"),
            format!("3. {reject}"),
        ];
    }

    vec![once, format!("2. {reject}")]
}

fn tool_changes_files(name: &str) -> bool {
    matches!(name, WRITE_FILE_TOOL | EDIT_FILE_TOOL)
}

fn footer_line(
    preferred_model: Option<&ModelReference>,
    router_state: RouterState,
    cwd: &Path,
    plan_mode: bool,
) -> Line<'static> {
    let model = preferred_model
        .map(ToString::to_string)
        .unwrap_or_else(|| "auto".to_owned());
    let (mode, mode_style) = if plan_mode {
        ("Plan mode", palette().footer_plan_mode)
    } else {
        ("Chat mode", palette().footer_chat_mode)
    };
    let router_state_style = match router_state {
        RouterState::Idle => palette().footer_router_idle,
        RouterState::Interrupted => palette().footer_router_interrupted,
        RouterState::Thinking(_) => palette().footer_router_thinking,
    };

    Line::from(vec![
        Span::raw("  "),
        Span::styled(model, palette().footer_model),
        Span::styled(" - ", palette().dim),
        Span::styled(cwd.display().to_string(), palette().footer_cwd),
        Span::styled(" - ", palette().dim),
        Span::styled(mode, mode_style),
        Span::styled(" - ", palette().dim),
        Span::styled(router_state.kind(), router_state_style),
    ])
}

fn prefixed_block(prefix: &str, value: &str, style: Style) -> Vec<Line<'static>> {
    block_lines(value, |index, line| {
        if index == 0 {
            Line::from(vec![
                Span::styled(format!("{prefix}: "), style.add_modifier(Modifier::BOLD)),
                Span::styled(line.to_owned(), style),
            ])
        } else {
            Line::from(vec![
                Span::styled(" ".repeat(prefix.len() + 2), palette().dim),
                Span::styled(line.to_owned(), style),
            ])
        }
    })
}

fn user_message_lines(value: &str, width: u16) -> Vec<Line<'static>> {
    let surface = palette().input_area;
    let mut lines = Vec::new();
    lines.push(user_surface_line("", width, surface));
    lines.extend(block_lines(value, |_index, line| {
        user_surface_line(line, width, surface)
    }));
    lines.push(user_surface_line("", width, surface));
    lines
}

fn user_surface_line(value: &str, width: u16, surface: Style) -> Line<'static> {
    let mut spans = vec![Span::styled(value.to_owned(), palette().input_text)];
    let visible_width = value.chars().count();
    let target_width = usize::from(width);
    if target_width > visible_width {
        spans.push(Span::styled(
            " ".repeat(target_width - visible_width),
            palette().input_text,
        ));
    }

    Line::from(spans).style(surface)
}

fn plain_block(value: &str, style: Style) -> Vec<Line<'static>> {
    block_lines(value, |_index, line| {
        Line::from(Span::styled(line.to_owned(), style))
    })
}

fn block_lines<F>(value: &str, mut line_fn: F) -> Vec<Line<'static>>
where
    F: FnMut(usize, &str) -> Line<'static>,
{
    if value.is_empty() {
        return vec![line_fn(0, "")];
    }

    value
        .lines()
        .enumerate()
        .map(|(index, line)| line_fn(index, line))
        .collect()
}

fn tool_call_lines(name: &str, input: &str) -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::styled("tool: ", palette().tool.add_modifier(Modifier::BOLD)),
        Span::styled(name.to_owned(), palette().tool),
        Span::styled(" ", palette().dim),
        Span::styled(input.to_owned(), palette().dim),
    ])]
}

fn tool_result_lines(name: &str, output: &str) -> Vec<Line<'static>> {
    prefixed_block(&format!("tool {name}"), output, palette().tool_result)
}

fn approval_history_lines(prompt: &ApprovalPrompt) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled("approval: ", palette().approval_title),
        Span::styled(prompt.title.clone(), palette().approval_title),
    ])];
    lines.extend(prefixed_block(
        "detail",
        &prompt.detail,
        palette().approval_detail,
    ));
    lines
}

fn log_lines(entries: &[AppLogEntry]) -> Vec<Line<'static>> {
    entries.iter().map(log_line).collect()
}

fn log_line(entry: &AppLogEntry) -> Line<'static> {
    let (level_name, style) = match entry.level() {
        LogLevel::Error => ("[ERROR]", palette().log_error),
        LogLevel::Warn => ("[WARN]", palette().log_warn),
        LogLevel::Info => ("[INFO]", palette().log_info),
        LogLevel::Debug => ("[DEBUG]", palette().log_debug),
        LogLevel::Trace => ("[TRACE]", palette().log_trace),
    };
    Line::from(vec![
        Span::styled(level_name, style),
        Span::raw(" "),
        Span::styled(entry.message().to_owned(), style),
    ])
}

fn preview_lines(preview: &PreviewSummary) -> Vec<Line<'static>> {
    let confidence = preview.classification_confidence.as_deref().map_or_else(
        || preview.classification_source.clone(),
        |confidence| {
            format!(
                "{} ({confidence} confidence)",
                preview.classification_source
            )
        },
    );
    let mut lines = vec![
        Line::from(Span::styled(
            "Routing preview",
            palette().preview.add_modifier(Modifier::BOLD),
        )),
        preview_detail_line(
            "Model",
            format!("{}/{}", preview.routing.provider, preview.routing.model),
        ),
        preview_detail_line("Task", preview.routing.intent.to_string()),
        preview_detail_line("Classification", confidence),
        preview_detail_line("Reason", preview.classification_reason.clone()),
        preview_detail_line(
            "Matched rule",
            preview
                .routing
                .matched_rule
                .as_deref()
                .unwrap_or("default route"),
        ),
        preview_detail_line(
            "Estimated cost",
            format!(
                "{}-{} {}",
                preview.estimated_cost_min,
                preview.estimated_cost_max,
                preview.estimated_cost_currency
            ),
        ),
    ];
    lines.extend(prefixed_block(
        "Routing reason",
        &preview.routing.reason,
        palette().preview,
    ));
    lines.extend(preview_list_lines(
        "Included context",
        &preview.included_context,
    ));
    lines.extend(preview_list_lines(
        "Excluded context",
        &preview.excluded_context,
    ));
    lines.push(Line::from(Span::styled(
        "Permissions",
        palette().dim.add_modifier(Modifier::BOLD),
    )));
    if preview.required_permissions.is_empty() {
        lines.push(Line::from(Span::styled("  none", palette().dim)));
    } else {
        lines.extend(preview.required_permissions.iter().map(|permission| {
            Line::from(vec![
                Span::styled("  • ", palette().dim),
                Span::styled(permission.permission.clone(), palette().preview),
                Span::styled(format!(" ({})", permission.mode), palette().dim),
            ])
        }));
    }
    lines
}

fn preview_detail_line(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}: "), palette().dim),
        Span::styled(value.into(), palette().preview),
    ])
}

fn preview_list_lines(label: &str, values: &[String]) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        label.to_owned(),
        palette().dim.add_modifier(Modifier::BOLD),
    ))];
    if values.is_empty() {
        lines.push(Line::from(Span::styled("  none", palette().dim)));
    } else {
        lines.extend(values.iter().map(|value| {
            Line::from(vec![
                Span::styled("  • ", palette().dim),
                Span::styled(value.clone(), palette().preview),
            ])
        }));
    }
    lines
}

fn trace_lines(trace: &[TraceEvent]) -> Vec<Line<'static>> {
    trace.iter().map(trace_line).collect()
}

fn trace_line(trace: &TraceEvent) -> Line<'static> {
    Line::from(vec![
        Span::styled(trace.created_at.to_owned(), palette().dim),
        Span::raw(": "),
        Span::styled(trace.task_type.to_owned(), palette().trace),
        Span::raw(" "),
        Span::styled(trace.event_type.to_owned(), palette().trace),
        Span::raw(" ("),
        Span::styled(
            format!("{}/{}", trace.provider, trace.model),
            palette().trace,
        ),
        Span::raw(")"),
        if let Some(matched_rule) = trace.matched_rule.as_deref() {
            Span::raw(format!(" (matched rule: {matched_rule})"))
        } else {
            Span::raw("")
        },
        Span::styled(": ", palette().trace),
        Span::raw(trace.payload.to_owned()),
    ])
}

fn diff_lines(added: &str, removed: &str) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "diff",
        palette().dim.add_modifier(Modifier::BOLD),
    ))];

    lines.extend(numbered_diff_lines('-', removed, palette().diff_removed));
    lines.extend(numbered_diff_lines('+', added, palette().diff_added));
    lines
}

fn numbered_diff_lines(marker: char, value: &str, style: Style) -> Vec<Line<'static>> {
    value
        .lines()
        .enumerate()
        .map(|(index, line)| {
            Line::from(vec![
                Span::styled(format!("{:>7} ", index + 1), palette().line_number),
                Span::styled(format!("{marker} "), style.add_modifier(Modifier::BOLD)),
                Span::styled(line.to_owned(), style),
            ])
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Palette {
    approval_detail: Style,
    approval_title: Style,
    assistant: Style,
    diff_added: Style,
    diff_removed: Style,
    dim: Style,
    error: Style,
    footer_chat_mode: Style,
    footer_cwd: Style,
    footer_model: Style,
    footer_plan_mode: Style,
    footer_router_idle: Style,
    footer_router_interrupted: Style,
    footer_router_thinking: Style,
    input_area: Style,
    input_text: Style,
    interrupted: Style,
    line_number: Style,
    log_debug: Style,
    log_error: Style,
    log_info: Style,
    log_trace: Style,
    log_warn: Style,
    notice: Style,
    option: Style,
    placeholder: Style,
    preview: Style,
    prompt: Style,
    selected_option: Style,
    spinner: Style,
    tool: Style,
    tool_result: Style,
    trace: Style,
    working: Style,
}

fn palette() -> Palette {
    Palette {
        approval_detail: Style::default().fg(Color::Gray),
        approval_title: Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        assistant: Style::default().fg(Color::Reset),
        diff_added: Style::default().fg(Color::Green),
        diff_removed: Style::default().fg(Color::Red),
        dim: Style::default().fg(Color::DarkGray),
        error: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        footer_chat_mode: Style::default().fg(Color::LightBlue),
        footer_cwd: Style::default().fg(Color::LightGreen),
        footer_model: Style::default().fg(Color::LightCyan),
        footer_plan_mode: Style::default()
            .fg(Color::LightMagenta)
            .add_modifier(Modifier::BOLD),
        footer_router_idle: Style::default(),
        footer_router_interrupted: Style::default().fg(Color::LightRed),
        footer_router_thinking: Style::default().fg(Color::LightGreen),
        input_area: input_area_style(),
        input_text: Style::default(),
        interrupted: Style::default().fg(Color::LightRed),
        line_number: Style::default().fg(Color::DarkGray),
        log_debug: Style::default().fg(Color::Gray),
        log_error: Style::default().fg(Color::Red),
        log_info: Style::default().fg(Color::LightGreen),
        log_warn: Style::default().fg(Color::Yellow),
        log_trace: Style::default().fg(Color::DarkGray),
        notice: Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
        option: Style::default().fg(Color::Gray),
        placeholder: Style::default().fg(Color::DarkGray),

        preview: Style::default().fg(Color::Magenta),
        prompt: Style::default()
            .fg(Color::Blue)
            .add_modifier(Modifier::BOLD),
        selected_option: Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        spinner: Style::default().fg(Color::LightCyan),
        tool: Style::default().fg(Color::Cyan),
        tool_result: Style::default().fg(Color::Green),
        trace: Style::default().fg(Color::Blue),
        working: Style::default().fg(Color::DarkGray),
    }
}

fn input_area_style() -> Style {
    terminal_background()
        .map(|background| Style::default().bg(user_message_bg(background)))
        .unwrap_or_else(|| Style::default().bg(DEFAULT_INPUT_BACKGROUND))
}

fn terminal_background() -> Option<(u8, u8, u8)> {
    env::var("COLORFGBG")
        .ok()
        .and_then(|value| parse_colorfgbg_background(&value))
}

fn user_message_bg(terminal_bg: (u8, u8, u8)) -> Color {
    let (overlay, alpha) = if is_light_rgb(terminal_bg) {
        ((0, 0, 0), LIGHT_USER_MESSAGE_ALPHA)
    } else {
        ((255, 255, 255), DARK_USER_MESSAGE_ALPHA)
    };
    let (red, green, blue) = blend_rgb(overlay, terminal_bg, alpha);
    Color::Rgb(red, green, blue)
}

fn blend_rgb(top: (u8, u8, u8), bottom: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    (
        blend_channel(top.0, bottom.0, alpha),
        blend_channel(top.1, bottom.1, alpha),
        blend_channel(top.2, bottom.2, alpha),
    )
}

fn blend_channel(top: u8, bottom: u8, alpha: f32) -> u8 {
    f32::from(top)
        .mul_add(alpha, f32::from(bottom) * (1.0 - alpha))
        .round() as u8
}

fn is_light_rgb((red, green, blue): (u8, u8, u8)) -> bool {
    let luminance =
        (0.299 * f32::from(red)) + (0.587 * f32::from(green)) + (0.114 * f32::from(blue));
    luminance > 128.0
}

fn parse_colorfgbg_background(value: &str) -> Option<(u8, u8, u8)> {
    let color_index = value.rsplit(';').next()?.parse::<u8>().ok()?;
    ansi_color(color_index)
}

fn ansi_color(index: u8) -> Option<(u8, u8, u8)> {
    match index {
        0 => Some((0, 0, 0)),
        1 => Some((205, 0, 0)),
        2 => Some((0, 205, 0)),
        3 => Some((205, 205, 0)),
        4 => Some((0, 0, 238)),
        5 => Some((205, 0, 205)),
        6 => Some((0, 205, 205)),
        7 => Some((229, 229, 229)),
        8 => Some((127, 127, 127)),
        9 => Some((255, 0, 0)),
        10 => Some((0, 255, 0)),
        11 => Some((255, 255, 0)),
        12 => Some((92, 92, 255)),
        13 => Some((255, 0, 255)),
        14 => Some((0, 255, 255)),
        15 => Some((255, 255, 255)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use smista_sdk::core::intent::TaskIntent;
    use smista_sdk::core::model::Provider;
    use smista_sdk::core::routing::RoutingDecision;

    use super::*;
    use crate::app::router_client::msg::PreviewPermissionSummary;

    fn preview_summary() -> PreviewSummary {
        PreviewSummary {
            routing: RoutingDecision {
                intent: TaskIntent::Review,
                provider: Provider::OpenAI,
                model: "gpt-5.5-thinking".to_owned(),
                matched_rule: Some("review route".to_owned()),
                fallback_used: false,
                override_used: false,
                reason: "review intent matched the review route".to_owned(),
            },
            classification_source: "inferred".to_owned(),
            classification_reason: "keyword 'review' matched".to_owned(),
            classification_confidence: Some("high".to_owned()),
            included_context: vec!["current git diff".to_owned(), "AGENTS.md".to_owned()],
            excluded_context: vec![".env".to_owned()],
            estimated_cost_min: "0.03".to_owned(),
            estimated_cost_max: "0.09".to_owned(),
            estimated_cost_currency: "USD".to_owned(),
            required_permissions: vec![PreviewPermissionSummary {
                permission: "write_files".to_owned(),
                mode: "ask".to_owned(),
            }],
        }
    }

    #[test]
    fn diff_history_lines_are_numbered_and_signed() {
        let lines = history_lines_for_width(
            &[HistoryEntry::Diff {
                added: "new line".to_owned(),
                removed: "old line".to_owned(),
            }],
            0,
        );

        assert_eq!(plain_text(&lines[0]), "");
        assert_eq!(plain_text(&lines[1]), "diff");
        assert_eq!(plain_text(&lines[2]), "      1 - old line");
        assert_eq!(plain_text(&lines[3]), "      1 + new line");
        assert_eq!(plain_text(&lines[4]), "");
    }

    #[test]
    fn history_entries_are_padded_vertically() {
        let lines =
            history_lines_for_width(&[HistoryEntry::AssistantMessage("hello".to_owned())], 0);

        assert_eq!(plain_text(&lines[0]), "");
        assert_eq!(plain_text(&lines[1]), "hello");
        assert_eq!(plain_text(&lines[2]), "");
    }

    #[test]
    fn log_history_lines_include_level_and_message() {
        let lines = history_lines_for_width(
            &[HistoryEntry::Log(vec![
                AppLogEntry::new(LogLevel::Error, "failed".to_owned()),
                AppLogEntry::new(LogLevel::Trace, "details".to_owned()),
            ])],
            80,
        );

        assert_eq!(plain_text(&lines[1]), "[ERROR] failed");
        assert_eq!(lines[1].spans[0].style, palette().log_error);
        assert_eq!(plain_text(&lines[2]), "[TRACE] details");
        assert_eq!(lines[2].spans[0].style, palette().log_trace);
    }

    #[test]
    fn trace_history_lines_include_event_metadata_and_payload() {
        let lines = history_lines_for_width(
            &[HistoryEntry::Trace(vec![TraceEvent {
                event_type: "routing_decision",
                task_type: "edit",
                provider: "openai".to_owned(),
                model: "gpt-4.1".to_owned(),
                matched_rule: Some("code edits".to_owned()),
                created_at: "2026-07-08T11:00:00Z".to_owned(),
                payload: "selected by policy".to_owned(),
            }])],
            80,
        );

        assert_eq!(
            plain_text(&lines[1]),
            "2026-07-08T11:00:00Z: edit routing_decision (openai/gpt-4.1) \
             (matched rule: code edits): selected by policy"
        );
    }

    #[test]
    fn preview_history_is_a_detailed_multiline_report() {
        let lines = history_lines_for_width(&[HistoryEntry::Preview(preview_summary())], 80);
        let text = lines.iter().map(plain_text).collect::<Vec<_>>();

        assert!(text.contains(&"Routing preview".to_owned()));
        assert!(text.contains(&"Model: openai/gpt-5.5-thinking".to_owned()));
        assert!(text.contains(&"Task: review".to_owned()));
        assert!(text.contains(&"Classification: inferred (high confidence)".to_owned()));
        assert!(text.contains(&"Reason: keyword 'review' matched".to_owned()));
        assert!(text.contains(&"Matched rule: review route".to_owned()));
        assert!(
            text.contains(&"Routing reason: review intent matched the review route".to_owned())
        );
        assert!(text.contains(&"Estimated cost: 0.03-0.09 USD".to_owned()));
        assert!(text.contains(&"  • current git diff".to_owned()));
        assert!(text.contains(&"  • .env".to_owned()));
        assert!(text.contains(&"  • write_files (ask)".to_owned()));
    }

    #[test]
    fn history_height_accounts_for_wrapped_rows() {
        let lines = vec![Line::raw("123456 123456 123456")];

        assert_eq!(history_rendered_height(&lines, 10), 3);
        assert_eq!(history_rendered_height(&[Line::raw("")], 8), 1);
    }

    #[test]
    fn user_history_uses_input_surface_style() {
        let lines = history_lines_for_width(&[HistoryEntry::UserMessage("hello".to_owned())], 0);

        assert_eq!(plain_text(&lines[1]), "");
        assert_eq!(plain_text(&lines[2]), "hello");
        assert_eq!(plain_text(&lines[3]), "");
        assert_eq!(lines[2].style, input_area_style());
    }

    #[test]
    fn user_history_surface_padding_uses_render_width() {
        let lines = history_lines_for_width(&[HistoryEntry::UserMessage("hi".to_owned())], 5);

        assert_eq!(plain_text(&lines[1]), "     ");
        assert_eq!(plain_text(&lines[2]), "hi   ");
        assert_eq!(plain_text(&lines[3]), "     ");
    }

    #[test]
    fn input_surface_is_subtle_on_dark_terminal_background() {
        assert_eq!(user_message_bg((0, 0, 0)), Color::Rgb(61, 61, 61));
    }

    #[test]
    fn input_surface_is_lighter_on_light_terminal_background() {
        assert_eq!(user_message_bg((240, 240, 240)), Color::Rgb(230, 230, 230));
    }

    #[test]
    fn colorfgbg_background_uses_last_color_index() {
        assert_eq!(parse_colorfgbg_background("15;0"), Some((0, 0, 0)));
        assert_eq!(parse_colorfgbg_background("0;15"), Some((255, 255, 255)));
        assert_eq!(parse_colorfgbg_background("not-a-color"), None);
    }

    #[test]
    fn unknown_terminal_background_uses_fallback_input_background() {
        assert_eq!(
            parse_colorfgbg_background("not-a-color")
                .map(|background| Style::default().bg(user_message_bg(background)))
                .unwrap_or_else(|| Style::default().bg(DEFAULT_INPUT_BACKGROUND)),
            Style::default().bg(DEFAULT_INPUT_BACKGROUND)
        );
    }

    #[test]
    fn file_approvals_offer_numbered_session_choices() {
        let read_prompt = ApprovalPrompt {
            id: "read-1".to_owned(),
            title: "Approve read_file".to_owned(),
            detail: "Cargo.toml".to_owned(),
            tool_name: Some(READ_FILE_TOOL.to_owned()),
            wildcard_alias: None,
        };
        let prompt = ApprovalPrompt {
            id: "edit-1".to_owned(),
            title: "Approve edit_file".to_owned(),
            detail: "src/lib.rs".to_owned(),
            tool_name: Some(EDIT_FILE_TOOL.to_owned()),
            wildcard_alias: None,
        };

        assert_eq!(
            approval_options(&read_prompt),
            vec![
                "1. Yes, proceed (y)".to_owned(),
                "2. Yes, and don't ask again for reading files (a)".to_owned(),
                "3. No, and tell Smista what to do differently (n)".to_owned(),
            ]
        );
        assert_eq!(
            approval_options(&prompt),
            vec![
                "1. Yes, proceed (y)".to_owned(),
                "2. Yes, and don't ask again for writing files (a)".to_owned(),
                "3. No, and tell Smista what to do differently (n)".to_owned(),
            ]
        );
    }

    #[test]
    fn command_approvals_name_the_session_alias() {
        let prompt = ApprovalPrompt {
            id: "shell-1".to_owned(),
            title: "Approve shell".to_owned(),
            detail: "cargo test -p smista-cli".to_owned(),
            tool_name: Some("shell".to_owned()),
            wildcard_alias: Some("cargo test *".to_owned()),
        };

        assert_eq!(
            approval_options(&prompt),
            vec![
                "1. Yes, proceed (y)".to_owned(),
                "2. Yes, and don't ask again for commands that start with `cargo test *` (a)"
                    .to_owned(),
                "3. No, and tell Smista what to do differently (n)".to_owned(),
            ]
        );
    }

    #[test]
    fn thinking_status_line_shows_elapsed_interrupt_hint() {
        let line = status_line(
            RouterState::Thinking(Instant::now() - Duration::from_secs(3)),
            None,
        )
        .expect("thinking router state renders a status line");

        assert!(plain_text(&line).contains("Working (3s - esc to interrupt)"));
    }

    #[test]
    fn interrupted_status_line_is_red() {
        let line = status_line(RouterState::Interrupted, None)
            .expect("interrupted router state renders a status line");

        assert_eq!(plain_text(&line), "Interrupted. What should we do instead?");
        assert_eq!(line.spans[0].style.fg, Some(Color::LightRed));
    }

    #[test]
    fn approval_prompt_suppresses_working_status() {
        let prompt = ApprovalPrompt {
            id: "read-1".to_owned(),
            title: "Approve read_file".to_owned(),
            detail: "Path: Cargo.toml".to_owned(),
            tool_name: Some("read_file".to_owned()),
            wildcard_alias: None,
        };

        assert!(
            status_line(
                RouterState::Thinking(Instant::now()),
                Some(&ExecutionTurn::Approval(prompt)),
            )
            .is_none()
        );
    }

    #[test]
    fn footer_entries_have_separate_bright_colors() {
        let cwd = Path::new("/tmp/workspace");
        let line = footer_line(None, RouterState::Interrupted, cwd, false);

        assert_eq!(
            plain_text(&line),
            "  auto - /tmp/workspace - Chat mode - Interrupted"
        );
        assert_eq!(line.spans[1].style.fg, Some(Color::LightCyan));
        assert_eq!(line.spans[3].style.fg, Some(Color::LightGreen));
        assert_eq!(line.spans[5].style.fg, Some(Color::LightBlue));
        assert_eq!(line.spans[7].style.fg, Some(Color::LightRed));
    }

    #[test]
    fn footer_entries_have_separate_bright_colors_in_plan_mode() {
        let cwd = Path::new("/tmp/workspace");
        let line = footer_line(None, RouterState::Idle, cwd, true);

        assert_eq!(
            plain_text(&line),
            "  auto - /tmp/workspace - Plan mode - Idle"
        );
        assert_eq!(line.spans[1].style.fg, Some(Color::LightCyan));
        assert_eq!(line.spans[3].style.fg, Some(Color::LightGreen));
        assert_eq!(line.spans[5].style.fg, Some(Color::LightMagenta));
        assert_eq!(line.spans[7].style.fg, None);
    }

    #[test]
    fn prompt_lines_render_newlines_as_rows() {
        let mut prompt = PromptState::default();
        prompt.push_str("a\nb");

        let lines = prompt_lines(&prompt);

        assert_eq!(plain_text(&lines[0]), "› a");
        assert_eq!(plain_text(&lines[1]), "  b");
    }

    #[test]
    fn prompt_lines_render_only_file_suggestion_suffix_as_placeholder() {
        let mut prompt = PromptState::default();
        prompt.push_str("review @sr");
        prompt.replace_file_matches(vec![PathBuf::from("src/lib.rs")]);

        let lines = prompt_lines(&prompt);

        assert_eq!(plain_text(&lines[0]), "› review @src/lib.rs");
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[1].content.as_ref(), "review @sr");
        assert_eq!(lines[0].spans[1].style, palette().input_text);
        assert_eq!(lines[0].spans[2].content.as_ref(), "c/lib.rs");
        assert_eq!(lines[0].spans[2].style, palette().placeholder);
    }

    #[test]
    fn prompt_lines_render_absolute_and_directory_suggestions() {
        let mut absolute = PromptState::default();
        absolute.push_str("@/tm");
        absolute.replace_file_matches(vec![PathBuf::from("/tmp/outside.rs")]);
        assert_eq!(
            plain_text(&prompt_lines(&absolute)[0]),
            "› @/tmp/outside.rs"
        );

        let mut directory = PathBuf::from("src");
        directory.push("");
        let mut relative = PromptState::default();
        relative.push_str("@s");
        relative.replace_file_matches(vec![directory]);
        assert_eq!(
            plain_text(&prompt_lines(&relative)[0]),
            format!("› @src{}", std::path::MAIN_SEPARATOR)
        );
    }

    #[test]
    fn prompt_lines_omit_empty_or_exact_file_suggestions() {
        let mut prompt = PromptState::default();
        prompt.push_str("@file.rs");
        assert_eq!(prompt_lines(&prompt)[0].spans.len(), 2);

        prompt.replace_file_matches(vec![PathBuf::from("file.rs")]);
        assert_eq!(prompt_lines(&prompt)[0].spans.len(), 2);
    }

    #[test]
    fn file_suggestion_does_not_move_prompt_cursor() {
        let mut console = ConsoleState::default();
        console.prompt.push_str("@sr");
        console
            .prompt
            .replace_file_matches(vec![PathBuf::from("src/a/long/path.rs")]);

        let position = prompt_cursor_position(&console, None, Rect::new(0, 0, 10, 4));

        assert_eq!(position, Some((5, 1)));
    }

    #[test]
    fn prompt_cursor_moves_to_last_input_row() {
        let mut console = ConsoleState::default();
        console.prompt.push_str("a\nb");

        let position = prompt_cursor_position(&console, None, Rect::new(0, 0, 20, 5));

        assert_eq!(position, Some((3, 2)));
    }

    #[test]
    fn prompt_cursor_uses_prompt_cursor_position() {
        let mut console = ConsoleState::default();
        console.prompt.push_str("abc");
        console.prompt.move_left();

        let position = prompt_cursor_position(&console, None, Rect::new(0, 0, 20, 5));

        assert_eq!(position, Some((4, 1)));
    }

    fn plain_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }
}
