//! Slash-command state for the interactive prompt.

use super::{PromptState, VerticalDirection, char_len, char_to_byte_index, move_vertical};

pub(super) const COMMAND_EXIT: &str = "exit";
pub(super) const COMMAND_MODEL: &str = "model";
pub(super) const COMMAND_PROVIDERS: &str = "providers";
pub(super) const COMMAND_Q: &str = "q";
pub(super) const COMMAND_QUIT: &str = "quit";
pub(super) const COMMAND_RESUME: &str = "resume";
pub(super) const COMMAND_SKILLS: &str = "skills";
pub(super) const COMMAND_STATUS: &str = "status";

const COMMAND_SPECS: &[(&str, Command)] = &[
    (COMMAND_EXIT, Command::Quit),
    (COMMAND_MODEL, Command::Model),
    (COMMAND_PROVIDERS, Command::Providers),
    (COMMAND_Q, Command::Quit),
    (COMMAND_QUIT, Command::Quit),
    (COMMAND_RESUME, Command::Resume),
    (COMMAND_SKILLS, Command::Skills),
    (COMMAND_STATUS, Command::Status),
];

/// A recognized or in-progress slash command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `/model` command. Without arguments, lists available models. With a model name, sets the current model.
    Model,
    /// `/providers` command. Lists available providers.
    Providers,
    /// `/quit`, `/q`, or `/exit` command, exits the application.
    Quit,
    /// `/resume` command. Lists sessions, or resumes a session when an ID is passed.
    Resume,
    /// `/skills` command, lists available skills.
    Skills,
    /// `/status` command, shows the current status of the router.
    Status,
    /// Still unresolved
    Unresolved(String),
}

impl Command {
    /// Returns the command text currently held in the prompt.
    #[must_use]
    pub fn input_name(&self) -> &str {
        match self {
            Self::Model => COMMAND_MODEL,
            Self::Providers => COMMAND_PROVIDERS,
            Self::Quit => COMMAND_QUIT,
            Self::Resume => COMMAND_RESUME,
            Self::Skills => COMMAND_SKILLS,
            Self::Status => COMMAND_STATUS,
            Self::Unresolved(command) => command,
        }
    }
}

/// State for a slash command prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPromptState {
    pub(super) input: String,
    pub(super) command_name: String,
    pub(super) command: Command,
    pub(super) args: String,
    pub(super) suggestion: Option<String>,
    pub(super) cursor: usize,
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

    pub(super) fn insert(&mut self, char: char) {
        let byte_index = char_to_byte_index(&self.input, self.cursor);
        self.input.insert(byte_index, char);
        self.cursor += 1;
    }

    pub(super) fn backspace(&mut self) -> Option<char> {
        if self.cursor == 0 {
            return None;
        }

        self.cursor -= 1;
        self.remove_at_cursor()
    }

    pub(super) fn delete(&mut self) -> Option<char> {
        self.remove_at_cursor()
    }

    pub(super) fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(super) fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(char_len(&self.input));
    }

    pub(super) fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub(super) fn move_end(&mut self) {
        self.cursor = char_len(&self.input);
    }

    pub(super) fn move_up(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Up);
    }

    pub(super) fn move_down(&mut self) {
        self.cursor = move_vertical(&self.input, self.cursor, VerticalDirection::Down);
    }

    pub(super) fn reparse(&mut self) {
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

pub(super) fn command_prompt(input: impl AsRef<str>) -> PromptState {
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
