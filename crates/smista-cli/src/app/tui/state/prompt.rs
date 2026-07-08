//! Prompt state for the main TUI component.

const COMMAND_EXIT: &str = "exit";
const COMMAND_MODELS: &str = "models";
const COMMAND_Q: &str = "q";
const COMMAND_QUIT: &str = "quit";

const COMMAND_SPECS: &[(&str, Command)] = &[
    (COMMAND_EXIT, Command::Quit),
    (COMMAND_MODELS, Command::ListModels),
    (COMMAND_Q, Command::Quit),
    (COMMAND_QUIT, Command::Quit),
];

/// State for prompt input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PromptState {
    /// An empty prompt, no input from the user. The default state
    #[default]
    Empty,
    /// A command prompt, contains the text that comes after the command prefix (e.g., `/`).
    Command(CommandPromptState),
    /// A text prompt, contains the text input by the user.
    Text(TextPromptState),
}

impl PromptState {
    /// Inserts a character at the cursor.
    pub fn push(&mut self, char: char) {
        match self {
            Self::Empty if char == '/' => {
                *self = command_prompt("");
            }
            Self::Empty => {
                *self = Self::Text(TextPromptState::new(char.to_string()));
            }
            Self::Text(state) => state.insert(char),
            Self::Command(state) => {
                state.insert(char);
                state.reparse();
            }
        }
    }

    /// Pushes a string into the prompt.
    pub fn push_str(&mut self, input: &str) {
        for char in input.chars() {
            self.push(char);
        }
    }

    /// Removes the character before the cursor.
    pub fn backspace(&mut self) -> Option<char> {
        match self {
            Self::Empty => None,
            Self::Text(state) => {
                let removed = state.backspace();
                if state.input.is_empty() {
                    *self = Self::Empty;
                }
                removed
            }
            Self::Command(state) if state.input.is_empty() && state.cursor == 0 => {
                *self = Self::Empty;
                Some('/')
            }
            Self::Command(state) => {
                let removed = state.backspace();
                state.reparse();
                removed
            }
        }
    }

    /// Removes the character at the cursor.
    pub fn delete(&mut self) -> Option<char> {
        match self {
            Self::Empty => None,
            Self::Text(state) => {
                let removed = state.delete();
                if state.input.is_empty() {
                    *self = Self::Empty;
                }
                removed
            }
            Self::Command(state) => {
                let removed = state.delete();
                state.reparse();
                removed
            }
        }
    }

    /// Clears prompt input.
    pub fn clear(&mut self) {
        *self = Self::Empty;
    }

    /// Returns the renderable input text.
    #[must_use]
    pub fn input(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(state) => state.input.clone(),
            Self::Command(state) => format!("/{}", state.input),
        }
    }

    /// Returns the cursor position in renderable prompt characters.
    #[must_use]
    pub fn cursor_position(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Text(state) => state.cursor,
            Self::Command(state) => state.cursor + 1,
        }
    }

    /// Moves the cursor one character to the left.
    pub fn move_left(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_left(),
            Self::Command(state) => state.move_left(),
        }
    }

    /// Moves the cursor one character to the right.
    pub fn move_right(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_right(),
            Self::Command(state) => state.move_right(),
        }
    }

    /// Moves the cursor to the same column on the previous logical row.
    pub fn move_up(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_up(),
            Self::Command(state) => state.move_up(),
        }
    }

    /// Moves the cursor to the same column on the next logical row.
    pub fn move_down(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_down(),
            Self::Command(state) => state.move_down(),
        }
    }

    /// Returns `true` when the prompt is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Returns the parsed command when the prompt contains a slash command.
    #[must_use]
    pub fn command(&self) -> Option<&Command> {
        match self {
            Self::Command(state) => Some(&state.command),
            _ => None,
        }
    }

    /// Returns command arguments when the prompt contains a slash command.
    #[must_use]
    pub fn command_args(&self) -> Option<&str> {
        match self {
            Self::Command(state) => Some(&state.args),
            _ => None,
        }
    }

    /// Returns the selected command suggestion, if any.
    #[must_use]
    pub fn current_suggestion(&self) -> Option<&str> {
        match self {
            Self::Command(state) => state.current_suggestion(),
            _ => None,
        }
    }

    /// Advances the selected command suggestion.
    pub fn next_suggestion(&mut self) {
        if let Self::Command(state) = self {
            state.next_suggestion();
        }
    }

    fn command_input(&self) -> String {
        match self {
            Self::Command(state) => state.input.clone(),
            _ => String::new(),
        }
    }
}

/// State for a text prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPromptState {
    input: String,
    cursor: usize,
}

impl TextPromptState {
    fn new(input: String) -> Self {
        let cursor = input.chars().count();
        Self { input, cursor }
    }

    fn insert(&mut self, char: char) {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        self.input.insert(byte_index, char);
        self.cursor += 1;
    }

    fn backspace(&mut self) -> Option<char> {
        if self.cursor == 0 {
            return None;
        }

        self.cursor -= 1;
        self.remove_at_cursor()
    }

    fn delete(&mut self) -> Option<char> {
        self.remove_at_cursor()
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(char_len(&self.input));
    }

    fn move_up(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Up);
    }

    fn move_down(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Down);
    }

    fn remove_at_cursor(&mut self) -> Option<char> {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        let char = self.input[byte_index..].chars().next()?;
        self.input.remove(byte_index);
        Some(char)
    }
}

/// State for a slash command prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPromptState {
    input: String,
    command_name: String,
    command: Command,
    args: String,
    suggestions: Vec<String>,
    index: usize,
    cursor: usize,
}

impl CommandPromptState {
    /// Returns the raw command input without the slash prefix.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Returns the command name exactly as typed.
    #[must_use]
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    /// Returns the parsed command.
    #[must_use]
    pub fn command(&self) -> &Command {
        &self.command
    }

    /// Returns command arguments.
    #[must_use]
    pub fn args(&self) -> &str {
        &self.args
    }

    /// Returns the selected command suggestion.
    #[must_use]
    pub fn current_suggestion(&self) -> Option<&str> {
        self.suggestions
            .get(self.index.min(self.suggestions.len().saturating_sub(1)))
            .map(String::as_str)
    }

    /// Advances to the next command suggestion.
    pub fn next_suggestion(&mut self) {
        if !self.suggestions.is_empty() {
            self.index = (self.index + 1) % self.suggestions.len();
        }
    }

    fn insert(&mut self, char: char) {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        self.input.insert(byte_index, char);
        self.cursor += 1;
    }

    fn backspace(&mut self) -> Option<char> {
        if self.cursor == 0 {
            return None;
        }

        self.cursor -= 1;
        self.remove_at_cursor()
    }

    fn delete(&mut self) -> Option<char> {
        self.remove_at_cursor()
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(char_len(&self.input));
    }

    fn move_up(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Up);
    }

    fn move_down(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Down);
    }

    fn reparse(&mut self) {
        let (command, args) = split_command_input(&self.input);
        self.command_name = command.to_owned();
        self.command = parse_command(command);
        self.args = args.to_owned();
        self.suggestions = command_suggestions(command);
        self.index = 0;
        self.cursor = self.cursor.min(char_len(&self.input));
    }

    fn remove_at_cursor(&mut self) -> Option<char> {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        let char = self.input[byte_index..].chars().next()?;
        self.input.remove(byte_index);
        Some(char)
    }
}

/// A recognized or in-progress slash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `/models` command, lists the available models.
    ListModels,
    /// `/quit`, `/q`, or `/exit` command, exits the application.
    Quit,
    /// Still unresolved
    Unresolved(String),
}

impl Command {
    /// Returns the command text currently held in the prompt.
    #[must_use]
    pub fn input_name(&self) -> &str {
        match self {
            Self::ListModels => COMMAND_MODELS,
            Self::Quit => COMMAND_QUIT,
            Self::Unresolved(command) => command,
        }
    }
}

fn command_prompt(input: impl AsRef<str>) -> PromptState {
    let input = input.as_ref();
    let (command, args) = split_command_input(input);

    PromptState::Command(CommandPromptState {
        input: input.to_owned(),
        command_name: command.to_owned(),
        command: parse_command(command),
        args: args.to_owned(),
        suggestions: command_suggestions(command),
        index: 0,
        cursor: input.chars().count(),
    })
}

enum VerticalDirection {
    Up,
    Down,
}

fn move_vertical(input: &str, cursor: usize, direction: VerticalDirection) -> usize {
    let rows = logical_rows(input);
    let Some((row_index, column)) = row_column_for_cursor(&rows, cursor) else {
        return cursor;
    };

    let target_row_index = match direction {
        VerticalDirection::Up => row_index.checked_sub(1),
        VerticalDirection::Down => (row_index + 1 < rows.len()).then_some(row_index + 1),
    };
    let Some(target_row_index) = target_row_index else {
        return cursor;
    };
    let target_row = rows[target_row_index];
    target_row.start + column.min(target_row.len)
}

#[derive(Clone, Copy)]
struct LogicalRow {
    start: usize,
    len: usize,
}

fn logical_rows(input: &str) -> Vec<LogicalRow> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut len = 0;

    for char in input.chars() {
        if char == '\n' {
            rows.push(LogicalRow { start, len });
            start += len + 1;
            len = 0;
        } else {
            len += 1;
        }
    }

    rows.push(LogicalRow { start, len });
    rows
}

fn row_column_for_cursor(rows: &[LogicalRow], cursor: usize) -> Option<(usize, usize)> {
    rows.iter().enumerate().find_map(|(index, row)| {
        (cursor >= row.start && cursor <= row.start + row.len)
            .then_some((index, cursor - row.start))
    })
}

fn char_to_byte_index(input: &str, char_index: usize) -> usize {
    input
        .char_indices()
        .map(|(byte_index, _)| byte_index)
        .nth(char_index)
        .unwrap_or(input.len())
}

fn char_len(input: &str) -> usize {
    input.chars().count()
}

fn split_command_input(input: &str) -> (&str, &str) {
    input
        .split_once(char::is_whitespace)
        .map_or((input, ""), |(command, args)| (command, args.trim_start()))
}

fn parse_command(command: &str) -> Command {
    COMMAND_SPECS
        .iter()
        .find_map(|(name, parsed)| (*name == command).then_some(parsed.clone()))
        .unwrap_or_else(|| Command::Unresolved(command.to_owned()))
}

fn command_suggestions(prefix: &str) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }

    COMMAND_SPECS
        .iter()
        .filter_map(|(name, _)| name.starts_with(prefix).then_some(format!("/{name}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{COMMAND_MODELS, COMMAND_QUIT, Command, PromptState, TextPromptState};

    const HELLO_INPUT: &str = "hello";
    const HELLO_SUFFIX: &str = "ello";
    const MODELS_SUGGESTION: &str = "/models";
    const QUIT_COMMAND_INPUT: &str = "quit now";
    const QUIT_INPUT: &str = "/quit now";
    const QUIT_ARGUMENTS: &str = "now";
    const QUIT_SUGGESTION: &str = "/quit";

    #[test]
    fn push_regular_character_starts_text_prompt() {
        let mut state = PromptState::default();

        state.push('h');
        state.push_str(HELLO_SUFFIX);

        assert_eq!(
            state,
            PromptState::Text(TextPromptState {
                input: HELLO_INPUT.to_owned(),
                cursor: HELLO_INPUT.chars().count(),
            })
        );
        assert_eq!(state.input(), HELLO_INPUT);
    }

    #[test]
    fn slash_starts_command_prompt() {
        let mut state = PromptState::default();

        state.push('/');
        state.push_str(COMMAND_MODELS);

        assert_eq!(
            state,
            PromptState::Command(super::CommandPromptState {
                input: COMMAND_MODELS.to_owned(),
                command_name: COMMAND_MODELS.to_owned(),
                command: Command::ListModels,
                args: String::new(),
                suggestions: vec![MODELS_SUGGESTION.to_owned()],
                index: 0,
                cursor: COMMAND_MODELS.chars().count(),
            })
        );
        assert_eq!(state.input(), MODELS_SUGGESTION);
        assert_eq!(state.current_suggestion(), Some(MODELS_SUGGESTION));
    }

    #[test]
    fn command_prompt_keeps_arguments() {
        let mut state = PromptState::default();

        state.push_str(QUIT_INPUT);

        assert_eq!(
            state,
            PromptState::Command(super::CommandPromptState {
                input: QUIT_COMMAND_INPUT.to_owned(),
                command_name: COMMAND_QUIT.to_owned(),
                command: Command::Quit,
                args: QUIT_ARGUMENTS.to_owned(),
                suggestions: vec![QUIT_SUGGESTION.to_owned()],
                index: 0,
                cursor: QUIT_COMMAND_INPUT.chars().count(),
            })
        );
    }

    #[test]
    fn tab_cycles_command_suggestions() {
        let mut state = PromptState::default();

        state.push('/');
        assert_eq!(state.current_suggestion(), None);
        state.push('q');
        assert_eq!(state.current_suggestion(), Some("/q"));
        state.next_suggestion();
        assert_eq!(state.current_suggestion(), Some("/quit"));
    }

    #[test]
    fn empty_command_has_no_suggestion() {
        let mut state = PromptState::default();

        state.push('/');

        assert_eq!(state.current_suggestion(), None);
    }

    #[test]
    fn backspace_clears_prompt_after_last_character() {
        let mut state = PromptState::default();

        state.push('x');
        assert_eq!(state.backspace(), Some('x'));

        assert_eq!(state, PromptState::Empty);
    }

    #[test]
    fn backspace_keeps_empty_command_prompt_after_removing_command_text() {
        let mut state = PromptState::default();

        state.push_str("/e");
        assert_eq!(state.backspace(), Some('e'));

        assert_eq!(state.input(), "/");
        assert_eq!(state.current_suggestion(), None);
    }

    #[test]
    fn backspace_empty_command_prompt_removes_slash() {
        let mut state = PromptState::default();

        state.push('/');
        assert_eq!(state.backspace(), Some('/'));

        assert_eq!(state, PromptState::Empty);
    }

    #[test]
    fn push_inserts_at_cursor_position() {
        let mut state = PromptState::default();

        state.push_str("ac");
        state.move_left();
        state.push('b');

        assert_eq!(state.input(), "abc");
        assert_eq!(state.cursor_position(), 2);
    }

    #[test]
    fn delete_removes_character_at_cursor() {
        let mut state = PromptState::default();

        state.push_str("abc");
        state.move_left();
        assert_eq!(state.delete(), Some('c'));

        assert_eq!(state.input(), "ab");
        assert_eq!(state.cursor_position(), 2);
    }

    #[test]
    fn backspace_removes_character_before_cursor() {
        let mut state = PromptState::default();

        state.push_str("abc");
        state.move_left();
        assert_eq!(state.backspace(), Some('b'));

        assert_eq!(state.input(), "ac");
        assert_eq!(state.cursor_position(), 1);
    }

    #[test]
    fn arrows_move_cursor_across_prompt() {
        let mut state = PromptState::default();

        state.push_str("abc");
        state.move_left();
        state.move_left();
        assert_eq!(state.cursor_position(), 1);
        state.move_right();
        assert_eq!(state.cursor_position(), 2);
    }

    #[test]
    fn up_and_down_move_between_logical_rows() {
        let mut state = PromptState::default();

        state.push_str("ab\ncd");
        assert_eq!(state.cursor_position(), 5);
        state.move_up();
        assert_eq!(state.cursor_position(), 2);
        state.move_down();
        assert_eq!(state.cursor_position(), 5);
    }

    #[test]
    fn command_cursor_position_includes_slash_prefix() {
        let mut state = PromptState::default();

        state.push_str("/ab");
        state.move_left();
        state.push('c');

        assert_eq!(state.input(), "/acb");
        assert_eq!(state.cursor_position(), 3);
    }
}
