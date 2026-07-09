//! Prompt state for the main TUI component.

const COMMAND_EXIT: &str = "exit";
const COMMAND_Q: &str = "q";
const COMMAND_QUIT: &str = "quit";
const COMMAND_RESUME: &str = "resume";
const COMMAND_SKILLS: &str = "skills";

const COMMAND_SPECS: &[(&str, Command)] = &[
    (COMMAND_EXIT, Command::Quit),
    (COMMAND_Q, Command::Quit),
    (COMMAND_QUIT, Command::Quit),
    (COMMAND_RESUME, Command::Resume),
    (COMMAND_SKILLS, Command::Skills),
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

    /// Moves the cursor to the beginning of the prompt.
    pub fn move_home(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_home(),
            Self::Command(state) => state.move_home(),
        }
    }

    /// Moves the cursor to the end of the prompt.
    pub fn move_end(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_end(),
            Self::Command(state) => state.move_end(),
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

    /// Accepts the selected command suggestion.
    pub fn accept_suggestion(&mut self) -> bool {
        match self {
            Self::Command(state) => state.accept_suggestion(),
            _ => false,
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

    pub fn text(&self) -> &str {
        &self.input
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

    fn move_home(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = char_len(&self.input);
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
    suggestion: Option<String>,
    cursor: usize,
}

impl CommandPromptState {
    /// Returns the raw command input without the slash prefix.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Returns the resolved command and arguments.
    #[must_use]
    pub fn resolved(&self) -> (Command, Vec<String>) {
        (self.command.clone(), parsed_args(&self.args))
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
        self.suggestion.as_deref()
    }

    /// Accepts the currently selected command suggestion.
    pub fn accept_suggestion(&mut self) -> bool {
        if self.cursor != char_len(&self.command_name) {
            return false;
        }

        let Some(suggestion) = self.suggestion.clone() else {
            return false;
        };
        let Some(command) = suggestion.strip_prefix('/') else {
            return false;
        };

        self.input = if self.args.is_empty() {
            command.to_owned()
        } else {
            format!("{command} {}", self.args)
        };
        self.cursor = self.input.chars().count();
        self.reparse();
        true
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

    fn move_home(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = char_len(&self.input);
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
        self.suggestion = command_suggestion(command);
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
    /// `/resume` command. Lists sessions, or resumes a session when an ID is passed.
    Resume,
    /// `/skills` command, lists available skills.
    Skills,
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
            Self::Quit => COMMAND_QUIT,
            Self::Resume => COMMAND_RESUME,
            Self::Skills => COMMAND_SKILLS,
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
        suggestion: command_suggestion(command),
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

fn parsed_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(ToOwned::to_owned).collect()
}

fn parse_command(command: &str) -> Command {
    COMMAND_SPECS
        .iter()
        .find_map(|(name, parsed)| (*name == command).then_some(parsed.clone()))
        .unwrap_or_else(|| Command::Unresolved(command.to_owned()))
}

fn command_suggestion(prefix: &str) -> Option<String> {
    if prefix.is_empty() {
        return None;
    }

    COMMAND_SPECS.iter().find_map(|(name, _)| {
        (name.starts_with(prefix) && *name != prefix).then_some(format!("/{name}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_QUIT, COMMAND_RESUME, COMMAND_SKILLS, Command, PromptState, TextPromptState,
    };

    const HELLO_INPUT: &str = "hello";
    const HELLO_SUFFIX: &str = "ello";
    const RESUME_SUGGESTION: &str = "/resume";
    const QUIT_COMMAND_INPUT: &str = "quit now";
    const QUIT_INPUT: &str = "/quit now";
    const QUIT_ARGUMENTS: &str = "now";
    const QUIT_SUGGESTION: &str = "/quit";
    const SKILLS_SUGGESTION: &str = "/skills";
    const SESSION_ID: &str = "00000000-0000-0000-0000-000000000001";

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
        state.push_str(COMMAND_RESUME);

        assert_eq!(
            state,
            PromptState::Command(super::CommandPromptState {
                input: COMMAND_RESUME.to_owned(),
                command_name: COMMAND_RESUME.to_owned(),
                command: Command::Resume,
                args: String::new(),
                suggestion: None,
                cursor: COMMAND_RESUME.chars().count(),
            })
        );
        assert_eq!(state.input(), RESUME_SUGGESTION);
        assert_eq!(state.current_suggestion(), None);
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
                suggestion: None,
                cursor: QUIT_COMMAND_INPUT.chars().count(),
            })
        );
    }

    #[test]
    fn command_prompt_trims_extra_argument_spacing() {
        let mut state = PromptState::default();

        state.push_str("/resume    ");
        state.push_str(SESSION_ID);

        assert_eq!(state.command(), Some(&Command::Resume));
        assert_eq!(state.command_args(), Some(SESSION_ID));
    }

    #[test]
    fn quit_aliases_parse_as_quit() {
        for input in ["/q", "/exit"] {
            let mut state = PromptState::default();

            state.push_str(input);

            assert_eq!(state.command(), Some(&Command::Quit));
            assert_eq!(state.command().map(Command::input_name), Some(COMMAND_QUIT));
        }
    }

    #[test]
    fn command_suggestions_are_visible_while_typing() {
        let mut state = PromptState::default();

        state.push('/');
        assert_eq!(state.current_suggestion(), None);
        state.push('q');
        assert_eq!(state.current_suggestion(), Some("/quit"));
    }

    #[test]
    fn command_suggestion_uses_first_matching_command() {
        let mut state = PromptState::default();

        state.push_str("/e");
        assert_eq!(state.current_suggestion(), Some("/exit"));
    }

    #[test]
    fn command_suggestion_includes_skills_command() {
        let mut state = PromptState::default();

        state.push_str("/s");

        assert_eq!(state.current_suggestion(), Some(SKILLS_SUGGESTION));
    }

    #[test]
    fn accepts_command_suggestion() {
        let mut state = PromptState::default();

        state.push_str("/q");

        assert!(state.accept_suggestion());
        assert_eq!(state.input(), "/quit");
        assert_eq!(state.current_suggestion(), None);
    }

    #[test]
    fn does_not_accept_suggestion_inside_command_word() {
        let mut state = PromptState::default();

        state.push_str("/qu");
        state.move_left();

        assert!(!state.accept_suggestion());
        assert_eq!(state.input(), "/qu");
        assert_eq!(state.current_suggestion(), Some("/quit"));
    }

    #[test]
    fn empty_command_has_no_suggestion() {
        let mut state = PromptState::default();

        state.push('/');

        assert_eq!(state.current_suggestion(), None);
    }

    #[test]
    fn empty_prompt_editing_is_noop() {
        let mut state = PromptState::default();

        assert!(state.is_empty());
        assert_eq!(state.backspace(), None);
        assert_eq!(state.delete(), None);
        state.move_left();
        state.move_right();
        state.move_home();
        state.move_end();
        state.move_up();
        state.move_down();

        assert_eq!(state.cursor_position(), 0);
        assert_eq!(state.command(), None);
        assert_eq!(state.command_args(), None);
        assert_eq!(state.current_suggestion(), None);
        assert!(!state.accept_suggestion());
        assert_eq!(state.command_input(), "");
    }

    #[test]
    fn command_accessors_expose_command_and_args() {
        let mut state = PromptState::default();

        state.push_str(QUIT_INPUT);

        assert!(!state.is_empty());
        assert_eq!(state.command(), Some(&Command::Quit));
        assert_eq!(state.command_args(), Some(QUIT_ARGUMENTS));
        assert_eq!(state.command_input(), QUIT_COMMAND_INPUT);

        let PromptState::Command(command) = &state else {
            panic!("command prompt expected");
        };
        assert_eq!(command.input(), QUIT_COMMAND_INPUT);
        assert_eq!(command.command_name(), COMMAND_QUIT);
        assert_eq!(command.command(), &Command::Quit);
        assert_eq!(command.args(), QUIT_ARGUMENTS);
        assert_eq!(command.command().input_name(), COMMAND_QUIT);

        state.clear();
        state.push_str("/unknown");
        assert_eq!(state.command().map(Command::input_name), Some("unknown"));
    }

    #[test]
    fn command_delete_reparses_command_name() {
        let mut state = PromptState::default();

        state.push_str("/resume now");
        state.move_home();
        assert_eq!(state.delete(), Some('r'));

        assert_eq!(
            state.command(),
            Some(&Command::Unresolved("esume".to_owned()))
        );
        assert_eq!(state.command_args(), Some("now"));
        assert_eq!(state.input(), "/esume now");
    }

    #[test]
    fn command_backspace_at_command_start_is_noop() {
        let mut state = PromptState::default();

        state.push_str("/resume");
        state.move_home();

        assert_eq!(state.backspace(), None);
        assert_eq!(state.input(), "/resume");
        assert_eq!(state.command(), Some(&Command::Resume));
    }

    #[test]
    fn accepting_command_suggestion_preserves_arguments() {
        let mut state = PromptState::default();

        state.push_str("/q now");
        state.move_home();
        state.move_right();

        assert!(state.accept_suggestion());
        assert_eq!(state.input(), QUIT_INPUT);
        assert_eq!(state.command_args(), Some(QUIT_ARGUMENTS));
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
    fn delete_at_end_is_noop() {
        let mut state = PromptState::default();

        state.push_str("abc");

        assert_eq!(state.delete(), None);
        assert_eq!(state.input(), "abc");

        state.clear();
        state.push_str("/resume");

        assert_eq!(state.delete(), None);
        assert_eq!(state.input(), "/resume");
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
    fn home_and_end_move_cursor_to_prompt_boundaries() {
        let mut state = PromptState::default();

        state.push_str("abc");
        state.move_home();
        assert_eq!(state.cursor_position(), 0);
        state.move_end();
        assert_eq!(state.cursor_position(), 3);

        state.clear();
        state.push_str("/quit");
        state.move_home();
        assert_eq!(state.cursor_position(), 1);
        state.move_end();
        assert_eq!(state.cursor_position(), 5);
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

    #[test]
    fn unicode_prompt_editing_uses_character_positions() {
        let mut state = PromptState::default();

        state.push_str("aé");
        state.move_left();
        state.push('b');

        assert_eq!(state.input(), "abé");
        assert_eq!(state.cursor_position(), 2);
        assert_eq!(state.delete(), Some('é'));
        assert_eq!(state.input(), "ab");
    }

    #[test]
    fn vertical_movement_at_boundaries_is_noop() {
        let mut state = PromptState::default();

        state.push_str("ab\ncd");
        state.move_home();
        state.move_up();
        assert_eq!(state.cursor_position(), 0);

        state.move_end();
        state.move_down();
        assert_eq!(state.cursor_position(), 5);
    }

    #[test]
    fn should_parse_resume() {
        let mut state = PromptState::default();

        state.push_str("/resume");

        assert_eq!(state.command(), Some(&Command::Resume));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/resume");
    }

    #[test]
    fn should_parse_skills() {
        let mut state = PromptState::default();

        state.push_str("/skills");

        assert_eq!(state.command(), Some(&Command::Skills));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/skills");
        assert_eq!(
            state.command().map(Command::input_name),
            Some(COMMAND_SKILLS)
        );
    }

    #[test]
    fn should_return_resolved_command_and_args() {
        let mut state = PromptState::default();

        state.push_str("/resume ");
        state.push_str(SESSION_ID);

        let PromptState::Command(command) = state else {
            panic!("command prompt expected");
        };
        assert_eq!(
            command.resolved(),
            (Command::Resume, vec![SESSION_ID.to_owned()])
        );
    }
}
