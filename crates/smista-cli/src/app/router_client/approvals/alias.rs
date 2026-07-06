use std::fmt;
use std::str::FromStr;

use anyhow::bail;

/// Session-scoped wildcard alias for a command action.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandAlias {
    /// Executable or shell command name.
    pub command: String,
    /// Positional arguments that identify the command kind.
    pub args: Vec<String>,
}

impl fmt::Display for CommandAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.args.is_empty() {
            write!(f, "{command} *", command = self.command)
        } else {
            write!(
                f,
                "{command} {args} *",
                command = self.command,
                args = self.args.join(" ")
            )
        }
    }
}

impl FromStr for CommandAlias {
    type Err = anyhow::Error;

    /// Parses a shell command into a session wildcard alias.
    ///
    /// Parsing keeps the command and plain positional words, then stops at
    /// options, quoted strings, shell operators, or unsupported characters.
    ///
    /// # Errors
    ///
    /// Returns an error when `s` is empty or starts with an alias boundary.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            bail!("command alias is empty");
        }

        let mut tokens = s.split_whitespace();
        let Some(first) = tokens.next() else {
            bail!("command alias is empty");
        };
        let Some(command) = alias_part(first) else {
            bail!("command alias has no command token");
        };
        let mut args = Vec::new();

        for token in tokens {
            let Some(arg) = alias_part(token) else {
                break;
            };
            args.push(arg.to_owned());

            if arg.len() < token.len() {
                break;
            }
        }

        Ok(Self {
            command: command.to_owned(),
            args,
        })
    }
}

/// Returns the safe prefix of a command token.
///
/// A missing prefix marks the token as a parser boundary.
fn alias_part(token: &str) -> Option<&str> {
    if token.starts_with('-') {
        return None;
    }

    let end = token
        .char_indices()
        .find_map(|(index, value)| (!is_alias_char(value)).then_some(index))
        .unwrap_or(token.len());

    (end > 0).then_some(&token[..end])
}

/// Returns whether `value` can be part of a command alias token.
fn is_alias_char(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | '.' | '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_command_and_plain_arguments_before_options() {
        let alias =
            CommandAlias::from_str(r#"git commit -a -m "message""#).expect("git command aliases");

        assert_eq!(alias.command, "git");
        assert_eq!(alias.args, ["commit"]);
        assert_eq!(alias.to_string(), "git commit *");
    }

    #[test]
    fn parses_all_plain_arguments_until_a_boundary() {
        let alias =
            CommandAlias::from_str("npm run build && rm -rf target").expect("npm command aliases");

        assert_eq!(alias.command, "npm");
        assert_eq!(alias.args, ["run", "build"]);
        assert_eq!(alias.to_string(), "npm run build *");
    }

    #[test]
    fn keeps_safe_token_prefix_before_punctuation_boundary() {
        let alias =
            CommandAlias::from_str("git status; rm -rf target").expect("git command aliases");

        assert_eq!(alias.command, "git");
        assert_eq!(alias.args, ["status"]);
        assert_eq!(alias.to_string(), "git status *");
    }

    #[test]
    fn parses_command_paths_and_collapses_whitespace() {
        let alias = CommandAlias::from_str("  ./scripts/release.sh   publish  ")
            .expect("script command aliases");

        assert_eq!(alias.command, "./scripts/release.sh");
        assert_eq!(alias.args, ["publish"]);
        assert_eq!(alias.to_string(), "./scripts/release.sh publish *");
    }

    #[test]
    fn stops_before_quoted_strings() {
        let alias = CommandAlias::from_str(r#"echo "hello world""#).expect("echo command aliases");

        assert_eq!(alias.command, "echo");
        assert!(alias.args.is_empty());
        assert_eq!(alias.to_string(), "echo *");
    }

    #[test]
    fn stops_before_short_options() {
        let alias = CommandAlias::from_str("rm -rf target").expect("rm command aliases");

        assert_eq!(alias.command, "rm");
        assert!(alias.args.is_empty());
        assert_eq!(alias.to_string(), "rm *");
    }

    #[test]
    fn stops_before_long_options() {
        let alias = CommandAlias::from_str("cargo test --package smista-cli parser")
            .expect("cargo command aliases");

        assert_eq!(alias.command, "cargo");
        assert_eq!(alias.args, ["test"]);
        assert_eq!(alias.to_string(), "cargo test *");
    }

    #[test]
    fn rejects_empty_commands() {
        let error = CommandAlias::from_str("   ").expect_err("empty commands fail");

        assert_eq!(error.to_string(), "command alias is empty");
    }

    #[test]
    fn rejects_commands_that_start_with_a_boundary() {
        let error = CommandAlias::from_str("-rf target").expect_err("option-only commands fail");

        assert_eq!(error.to_string(), "command alias has no command token");
    }
}
