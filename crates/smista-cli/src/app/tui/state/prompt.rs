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
    Command {
        /// The raw input after the slash prefix.
        input: String,
        /// The command name exactly as typed.
        command_name: String,
        command: Command,
        args: String,
        suggestions: Vec<String>,
    },
    /// A text prompt, contains the text input by the user.
    Text(String),
}

impl PromptState {
    /// Pushes a character into the prompt.
    pub fn push(&mut self, char: char) {
        match self {
            Self::Empty if char == '/' => {
                *self = command_prompt("");
            }
            Self::Empty => {
                *self = Self::Text(char.to_string());
            }
            Self::Text(text) => text.push(char),
            Self::Command { .. } => {
                let mut input = self.command_input();
                input.push(char);
                *self = command_prompt(input);
            }
        }
    }

    /// Pushes a string into the prompt.
    pub fn push_str(&mut self, input: &str) {
        for char in input.chars() {
            self.push(char);
        }
    }

    /// Removes the last character from the prompt.
    pub fn pop(&mut self) -> Option<char> {
        match self {
            Self::Empty => None,
            Self::Text(text) => {
                let removed = text.pop();
                if text.is_empty() {
                    *self = Self::Empty;
                }
                removed
            }
            Self::Command { .. } => {
                let mut input = self.command_input();
                let removed = input.pop();
                if input.is_empty() {
                    *self = Self::Empty;
                } else {
                    *self = command_prompt(input);
                }
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
            Self::Text(text) => text.clone(),
            Self::Command { .. } => format!("/{}", self.command_input()),
        }
    }

    /// Returns `true` when the prompt is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    fn command_input(&self) -> String {
        let Self::Command { input, .. } = self else {
            return String::new();
        };

        input.clone()
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

    PromptState::Command {
        input: input.to_owned(),
        command_name: command.to_owned(),
        command: parse_command(command),
        args: args.to_owned(),
        suggestions: command_suggestions(command),
    }
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
    COMMAND_SPECS
        .iter()
        .filter_map(|(name, _)| name.starts_with(prefix).then_some(format!("/{name}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{COMMAND_MODELS, COMMAND_QUIT, Command, PromptState};

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

        assert_eq!(state, PromptState::Text(HELLO_INPUT.to_owned()));
        assert_eq!(state.input(), HELLO_INPUT);
    }

    #[test]
    fn slash_starts_command_prompt() {
        let mut state = PromptState::default();

        state.push('/');
        state.push_str(COMMAND_MODELS);

        assert_eq!(
            state,
            PromptState::Command {
                input: COMMAND_MODELS.to_owned(),
                command_name: COMMAND_MODELS.to_owned(),
                command: Command::ListModels,
                args: String::new(),
                suggestions: vec![MODELS_SUGGESTION.to_owned()],
            }
        );
        assert_eq!(state.input(), MODELS_SUGGESTION);
    }

    #[test]
    fn command_prompt_keeps_arguments() {
        let mut state = PromptState::default();

        state.push_str(QUIT_INPUT);

        assert_eq!(
            state,
            PromptState::Command {
                input: QUIT_COMMAND_INPUT.to_owned(),
                command_name: COMMAND_QUIT.to_owned(),
                command: Command::Quit,
                args: QUIT_ARGUMENTS.to_owned(),
                suggestions: vec![QUIT_SUGGESTION.to_owned()],
            }
        );
    }

    #[test]
    fn pop_clears_prompt_after_last_character() {
        let mut state = PromptState::default();

        state.push('x');
        assert_eq!(state.pop(), Some('x'));

        assert_eq!(state, PromptState::Empty);
    }
}
