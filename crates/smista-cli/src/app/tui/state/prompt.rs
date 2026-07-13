//! Prompt state for the main TUI component.

use std::path::PathBuf;

mod command;
mod file_autocomplete;
mod text;

use self::command::command_prompt;
pub use self::command::{Command, CommandPromptState};
pub use self::file_autocomplete::FileAutocompleteState;
pub use self::text::TextPromptState;

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
    /// A text prompt with an active file mention at the end of the input.
    FileAutocomplete(FileAutocompleteState),
}

impl PromptState {
    /// Inserts a character at the cursor.
    pub fn push(&mut self, char: char) {
        match self {
            Self::Empty if char == '/' => {
                *self = command_prompt("");
            }
            Self::Empty if char == '@' => {
                *self =
                    Self::FileAutocomplete(FileAutocompleteState::new("@".to_owned(), 1, false));
            }
            Self::Empty => {
                *self = Self::Text(TextPromptState::new(char.to_string()));
            }
            Self::Text(state) if char == '@' && state.cursor == char_len(&state.input) => {
                state.insert(char);
                *self = Self::FileAutocomplete(FileAutocompleteState::new(
                    state.input.clone(),
                    state.input.len(),
                    false,
                ));
            }
            Self::Text(state) => state.insert(char),
            Self::FileAutocomplete(state) if char.is_whitespace() => {
                state.insert(char);
                *self = state_without_file_autocomplete(state);
            }
            Self::FileAutocomplete(state) => state.insert(char),
            Self::Command(state)
                if char == '@'
                    && state.command == Command::Preview
                    && state.cursor == char_len(&state.input) =>
            {
                state.insert(char);
                let input = format!("/{}", state.input);
                let mention_start = input.len();
                *self =
                    Self::FileAutocomplete(FileAutocompleteState::new(input, mention_start, true));
            }
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

    /// Replaces the prompt with renderable input text.
    pub fn replace_with_input(&mut self, input: impl AsRef<str>) {
        let input = input.as_ref();
        *self = if input.is_empty() {
            Self::Empty
        } else if let Some(command) = input.strip_prefix('/') {
            command_prompt(command)
        } else {
            Self::Text(TextPromptState::new(input.to_owned()))
        };
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
            Self::FileAutocomplete(state) => {
                let removed = state.backspace();
                if state.input.is_empty() {
                    *self = Self::Empty;
                } else if state.cursor_byte_index() < state.mention_start {
                    *self = state_without_file_autocomplete(state);
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
            Self::FileAutocomplete(state) => state.delete(),
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
            Self::FileAutocomplete(state) => state.input.clone(),
            Self::Command(state) => format!("/{}", state.input),
        }
    }

    /// Returns the cursor position in renderable prompt characters.
    #[must_use]
    pub fn cursor_position(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Text(state) => state.cursor,
            Self::FileAutocomplete(state) => state.cursor,
            Self::Command(state) => state.cursor + 1,
        }
    }

    /// Moves the cursor one character to the left.
    pub fn move_left(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_left(),
            Self::FileAutocomplete(state) => {
                state.move_left();
                *self = state_without_file_autocomplete(state);
            }
            Self::Command(state) => state.move_left(),
        }
    }

    /// Moves the cursor one character to the right.
    pub fn move_right(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_right(),
            Self::FileAutocomplete(state) => {
                state.move_right();
                if state.cursor != char_len(&state.input) {
                    *self = state_without_file_autocomplete(state);
                }
            }
            Self::Command(state) => state.move_right(),
        }
    }

    /// Moves the cursor to the beginning of the prompt.
    pub fn move_home(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_home(),
            Self::FileAutocomplete(state) => {
                state.move_home();
                *self = state_without_file_autocomplete(state);
            }
            Self::Command(state) => state.move_home(),
        }
    }

    /// Moves the cursor to the end of the prompt.
    pub fn move_end(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_end(),
            Self::FileAutocomplete(state) => state.move_end(),
            Self::Command(state) => state.move_end(),
        }
    }

    /// Moves the cursor to the same column on the previous logical row.
    pub fn move_up(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_up(),
            Self::FileAutocomplete(state) => {
                state.move_up();
                if state.cursor != char_len(&state.input) {
                    *self = state_without_file_autocomplete(state);
                }
            }
            Self::Command(state) => state.move_up(),
        }
    }

    /// Moves the cursor to the same column on the next logical row.
    pub fn move_down(&mut self) {
        match self {
            Self::Empty => {}
            Self::Text(state) => state.move_down(),
            Self::FileAutocomplete(state) => {
                state.move_down();
                if state.cursor != char_len(&state.input) {
                    *self = state_without_file_autocomplete(state);
                }
            }
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
            Self::FileAutocomplete(state) => state.current_suggestion(),
            _ => None,
        }
    }

    /// Accepts the selected command suggestion.
    pub fn accept_suggestion(&mut self) -> bool {
        match self {
            Self::Command(state) => state.accept_suggestion(),
            Self::FileAutocomplete(state) => state.accept_suggestion(),
            _ => false,
        }
    }

    /// Returns `true` when a file mention is being completed.
    #[must_use]
    pub fn is_file_autocomplete_active(&self) -> bool {
        matches!(self, Self::FileAutocomplete(_))
    }

    /// Returns `true` when file completion belongs to a slash command.
    #[must_use]
    pub fn is_command_file_autocomplete_active(&self) -> bool {
        matches!(self, Self::FileAutocomplete(state) if state.command)
    }

    /// Returns the path text after the active mention's triggering `@`.
    #[must_use]
    pub fn file_autocomplete_query(&self) -> Option<&str> {
        match self {
            Self::FileAutocomplete(state) => Some(state.query()),
            _ => None,
        }
    }

    /// Replaces file matches and keeps the selected path when it is still present.
    pub fn replace_file_matches(&mut self, matches: Vec<PathBuf>) {
        if let Self::FileAutocomplete(state) = self {
            state.replace_matches(matches);
        }
    }

    /// Selects the next file match, wrapping at the end.
    pub fn next_file_match(&mut self) {
        if let Self::FileAutocomplete(state) = self {
            state.next_match();
        }
    }

    /// Selects the previous file match, wrapping at the beginning.
    pub fn previous_file_match(&mut self) {
        if let Self::FileAutocomplete(state) = self {
            state.previous_match();
        }
    }

    /// Cancels file completion without changing the prompt text.
    pub fn cancel_file_autocomplete(&mut self) -> bool {
        let Self::FileAutocomplete(state) = self else {
            return false;
        };
        *self = state_without_file_autocomplete(state);
        true
    }

    /// Returns text input for ordinary and file-autocomplete prompts.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(state) => Some(state.text()),
            Self::FileAutocomplete(state) if !state.command => Some(state.text()),
            Self::FileAutocomplete(_) => None,
            Self::Empty | Self::Command(_) => None,
        }
    }

    fn command_input(&self) -> String {
        match self {
            Self::Command(state) => state.input.clone(),
            _ => String::new(),
        }
    }
}

fn state_without_file_autocomplete(state: &FileAutocompleteState) -> PromptState {
    if state.command {
        let mut prompt = command_prompt(state.input.strip_prefix('/').unwrap_or(&state.input));
        if let PromptState::Command(command) = &mut prompt {
            command.cursor = state.cursor.saturating_sub(1).min(char_len(&command.input));
        }
        prompt
    } else {
        PromptState::Text(TextPromptState {
            input: state.input.clone(),
            cursor: state.cursor,
        })
    }
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

#[cfg(test)]
mod tests {
    use std::path::{MAIN_SEPARATOR, PathBuf};

    use super::{Command, PromptState, TextPromptState};
    use crate::app::tui::state::prompt::command::{
        COMMAND_CHAT, COMMAND_CLEAR, COMMAND_LOG, COMMAND_MODEL, COMMAND_PLAN, COMMAND_PREVIEW,
        COMMAND_PROVIDERS, COMMAND_QUIT, COMMAND_RESUME, COMMAND_SKILLS, COMMAND_STATUS,
    };

    const HELLO_INPUT: &str = "hello";
    const HELLO_SUFFIX: &str = "ello";
    const MODEL_SUGGESTION: &str = "/model";
    const PROVIDERS_SUGGESTION: &str = "/providers";
    const RESUME_SUGGESTION: &str = "/resume";
    const QUIT_COMMAND_INPUT: &str = "quit now";
    const QUIT_INPUT: &str = "/quit now";
    const QUIT_ARGUMENTS: &str = "now";
    const QUIT_SUGGESTION: &str = "/quit";
    const SKILLS_SUGGESTION: &str = "/skills";
    const SESSION_ID: &str = "00000000-0000-0000-0000-000000000001";

    #[test]
    fn at_sign_at_end_starts_file_autocomplete() {
        let mut empty = PromptState::default();
        empty.push('@');

        assert!(empty.is_file_autocomplete_active());
        assert_eq!(empty.input(), "@");
        assert_eq!(empty.file_autocomplete_query(), Some(""));

        let mut after_text = PromptState::default();
        after_text.push_str("review ");
        after_text.push('@');

        assert!(after_text.is_file_autocomplete_active());
        assert_eq!(after_text.input(), "review @");
        assert_eq!(after_text.file_autocomplete_query(), Some(""));
    }

    #[test]
    fn at_sign_away_from_end_and_inside_command_does_not_start_file_autocomplete() {
        let mut text = PromptState::default();
        text.push_str("ac");
        text.move_left();
        text.push('@');

        assert!(!text.is_file_autocomplete_active());
        assert_eq!(text.input(), "a@c");

        let mut command = PromptState::default();
        command.push_str("/model @file");

        assert!(!command.is_file_autocomplete_active());
        assert!(matches!(command, PromptState::Command(_)));
    }

    #[test]
    fn at_sign_in_preview_arguments_starts_file_autocomplete() {
        let mut state = PromptState::default();
        state.push_str("/preview review @sr");

        assert!(state.is_file_autocomplete_active());
        assert!(state.is_command_file_autocomplete_active());
        assert_eq!(state.input(), "/preview review @sr");
        assert_eq!(state.file_autocomplete_query(), Some("sr"));

        state.replace_file_matches(vec![PathBuf::from("src/lib.rs")]);
        assert_eq!(
            state.current_suggestion(),
            Some("/preview review @src/lib.rs")
        );
        assert!(state.accept_suggestion());
        assert_eq!(state.input(), "/preview review @src/lib.rs");
    }

    #[test]
    fn ending_preview_file_completion_restores_command_state() {
        for action in [' ', '\t', '\n'] {
            let mut state = PromptState::default();
            state.push_str("/preview review @src");
            state.push(action);
            let expected_args = format!("review @src{action}");

            assert!(!state.is_file_autocomplete_active());
            assert_eq!(state.command(), Some(&Command::Preview));
            assert_eq!(state.command_args(), Some(expected_args.as_str()));
        }

        let mut state = PromptState::default();
        state.push_str("/preview review @src");
        assert!(state.cancel_file_autocomplete());
        assert_eq!(state.command(), Some(&Command::Preview));
        assert_eq!(state.command_args(), Some("review @src"));
    }

    #[test]
    fn file_autocomplete_query_tracks_path_characters() {
        let mut state = PromptState::default();
        state.push_str("review @src");
        state.push(MAIN_SEPARATOR);
        state.push_str("lib@rs");

        assert_eq!(
            state.file_autocomplete_query(),
            Some(format!("src{MAIN_SEPARATOR}lib@rs").as_str())
        );

        assert_eq!(state.backspace(), Some('s'));
        assert_eq!(
            state.file_autocomplete_query(),
            Some(format!("src{MAIN_SEPARATOR}lib@r").as_str())
        );
    }

    #[test]
    fn file_matches_are_sorted_and_navigation_wraps() {
        let mut state = PromptState::default();
        state.push('@');
        state.replace_file_matches(vec![PathBuf::from("b"), PathBuf::from("a")]);

        assert_eq!(state.current_suggestion(), Some("@a"));
        state.next_file_match();
        assert_eq!(state.current_suggestion(), Some("@b"));
        state.next_file_match();
        assert_eq!(state.current_suggestion(), Some("@a"));
        state.previous_file_match();
        assert_eq!(state.current_suggestion(), Some("@b"));
    }

    #[test]
    fn single_file_match_stays_selected_in_both_directions() {
        let mut state = PromptState::default();
        state.push('@');
        state.replace_file_matches(vec![PathBuf::from("only")]);

        state.next_file_match();
        assert_eq!(state.current_suggestion(), Some("@only"));
        state.previous_file_match();
        assert_eq!(state.current_suggestion(), Some("@only"));
    }

    #[test]
    fn replacing_file_matches_preserves_or_resets_selection() {
        let mut state = PromptState::default();
        state.push('@');
        state.replace_file_matches(vec![PathBuf::from("a"), PathBuf::from("b")]);
        state.next_file_match();

        state.replace_file_matches(vec![PathBuf::from("c"), PathBuf::from("b")]);
        assert_eq!(state.current_suggestion(), Some("@b"));

        state.replace_file_matches(vec![PathBuf::from("d"), PathBuf::from("c")]);
        assert_eq!(state.current_suggestion(), Some("@c"));
    }

    #[test]
    fn empty_file_matches_make_navigation_and_acceptance_noops() {
        let mut state = PromptState::default();
        state.push_str("review @missing");

        state.next_file_match();
        state.previous_file_match();

        assert_eq!(state.current_suggestion(), None);
        assert!(!state.accept_suggestion());
        assert_eq!(state.input(), "review @missing");
    }

    #[test]
    fn accepting_file_match_replaces_only_active_path() {
        let mut state = PromptState::default();
        state.push_str("review @sr");
        state.replace_file_matches(vec![PathBuf::from("src/lib.rs")]);

        assert!(state.accept_suggestion());
        assert_eq!(state.input(), "review @src/lib.rs");
        assert_eq!(state.file_autocomplete_query(), Some("src/lib.rs"));
        assert_eq!(state.current_suggestion(), None);
    }

    #[test]
    fn accepting_directory_preserves_trailing_separator() {
        let mut state = PromptState::default();
        state.push('@');
        let mut directory = PathBuf::from("src");
        directory.push("");
        state.replace_file_matches(vec![directory]);

        assert!(state.accept_suggestion());
        assert_eq!(state.input(), format!("@src{MAIN_SEPARATOR}"));
    }

    #[test]
    fn whitespace_ends_file_autocomplete_without_losing_input() {
        for whitespace in [' ', '\n'] {
            let mut state = PromptState::default();
            state.push_str("@src");
            state.push(whitespace);

            assert!(!state.is_file_autocomplete_active());
            assert_eq!(state.input(), format!("@src{whitespace}"));
        }
    }

    #[test]
    fn backspace_keeps_autocomplete_until_trigger_is_removed() {
        let mut state = PromptState::default();
        state.push_str("prefix @ab");

        assert_eq!(state.backspace(), Some('b'));
        assert_eq!(state.file_autocomplete_query(), Some("a"));
        assert_eq!(state.backspace(), Some('a'));
        assert_eq!(state.file_autocomplete_query(), Some(""));
        assert_eq!(state.backspace(), Some('@'));

        assert!(!state.is_file_autocomplete_active());
        assert_eq!(state.input(), "prefix ");

        let mut empty = PromptState::default();
        empty.push('@');
        assert_eq!(empty.backspace(), Some('@'));
        assert_eq!(empty, PromptState::Empty);
    }

    #[test]
    fn cursor_movement_and_escape_cancel_file_autocomplete() {
        for movement in [PromptState::move_left, PromptState::move_home] {
            let mut state = PromptState::default();
            state.push_str("review @src");
            movement(&mut state);

            assert!(!state.is_file_autocomplete_active());
            assert_eq!(state.input(), "review @src");
        }

        let mut state = PromptState::default();
        state.push_str("review @src");
        assert!(state.cancel_file_autocomplete());
        assert_eq!(state.input(), "review @src");
        assert!(!state.is_file_autocomplete_active());
    }

    #[test]
    fn clearing_and_replacing_input_discard_file_matches() {
        let mut state = PromptState::default();
        state.push('@');
        state.replace_file_matches(vec![PathBuf::from("src")]);

        state.replace_with_input("restored @src");
        assert!(!state.is_file_autocomplete_active());
        assert_eq!(state.current_suggestion(), None);

        state.clear();
        assert_eq!(state, PromptState::Empty);
    }

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
    fn log_command_parses_offset_and_limit() {
        let mut state = PromptState::default();

        state.push_str("/log 10 25");

        assert_eq!(state.command(), Some(&Command::Log));
        assert_eq!(state.command().map(Command::input_name), Some(COMMAND_LOG));
        let PromptState::Command(command) = state else {
            panic!("command prompt expected");
        };
        assert_eq!(
            command.resolved(),
            (Command::Log, vec!["10".to_owned(), "25".to_owned()])
        );
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
    fn replace_with_input_rebuilds_prompt_state() {
        let mut state = PromptState::default();

        state.replace_with_input(QUIT_INPUT);
        assert_eq!(state.command(), Some(&Command::Quit));
        assert_eq!(state.command_args(), Some(QUIT_ARGUMENTS));
        assert_eq!(state.cursor_position(), QUIT_INPUT.chars().count());

        state.replace_with_input(HELLO_INPUT);
        assert_eq!(state.input(), HELLO_INPUT);
        assert_eq!(state.command(), None);

        state.replace_with_input("");
        assert_eq!(state, PromptState::Empty);
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
    fn command_suggestion_includes_model_command() {
        let mut state = PromptState::default();

        state.push_str("/mod");

        assert_eq!(state.current_suggestion(), Some(MODEL_SUGGESTION));
    }

    #[test]
    fn command_suggestion_includes_providers_command() {
        let mut state = PromptState::default();

        state.push_str("/prov");

        assert_eq!(state.current_suggestion(), Some(PROVIDERS_SUGGESTION));
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
    fn should_parse_chat() {
        let mut state = PromptState::default();

        state.push_str("/chat");

        assert_eq!(state.command(), Some(&Command::Chat));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/chat");
        assert_eq!(state.command().map(Command::input_name), Some(COMMAND_CHAT));
    }

    #[test]
    fn should_parse_clear() {
        let mut state = PromptState::default();

        state.push_str("/clear");

        assert_eq!(state.command(), Some(&Command::Clear));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/clear");
        assert_eq!(
            state.command().map(Command::input_name),
            Some(COMMAND_CLEAR)
        );
    }

    #[test]
    fn should_parse_model() {
        let mut state = PromptState::default();

        state.push_str("/model");

        assert_eq!(state.command(), Some(&Command::Model));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/model");
        assert_eq!(
            state.command().map(Command::input_name),
            Some(COMMAND_MODEL)
        );
    }

    #[test]
    fn should_parse_plan() {
        let mut state = PromptState::default();

        state.push_str("/plan");

        assert_eq!(state.command(), Some(&Command::Plan));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/plan");
        assert_eq!(state.command().map(Command::input_name), Some(COMMAND_PLAN));
    }

    #[test]
    fn should_parse_preview() {
        let mut state = PromptState::default();

        state.push_str("/preview");

        assert_eq!(state.command(), Some(&Command::Preview));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/preview");
        assert_eq!(
            state.command().map(Command::input_name),
            Some(COMMAND_PREVIEW)
        );
    }

    #[test]
    fn should_parse_providers() {
        let mut state = PromptState::default();

        state.push_str("/providers");

        assert_eq!(state.command(), Some(&Command::Providers));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/providers");
        assert_eq!(
            state.command().map(Command::input_name),
            Some(COMMAND_PROVIDERS)
        );
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
    fn should_parse_status() {
        let mut state = PromptState::default();

        state.push_str("/status");

        assert_eq!(state.command(), Some(&Command::Status));
        assert_eq!(state.command_args(), Some(""));
        assert_eq!(state.input(), "/status");
        assert_eq!(
            state.command().map(Command::input_name),
            Some(COMMAND_STATUS)
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
