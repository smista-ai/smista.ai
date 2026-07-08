//! Approvals router client module.

use std::collections::HashSet;
use std::str::FromStr;

use self::alias::CommandAlias;

mod alias;

/// In-memory storage for actions approved for the current session.
#[derive(Debug, Clone, Default)]
pub struct ApprovalsStorage {
    approvals: HashSet<CommandAlias>,
}

impl ApprovalsStorage {
    /// Creates an empty approval store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the wildcard alias shown when prompting for `command`.
    ///
    /// # Errors
    ///
    /// Returns an error when `command` cannot produce an alias.
    pub fn alias_for(&self, command: &str) -> anyhow::Result<String> {
        tracing::debug!(r#"finding alias for command "{command}""#);
        let alias = CommandAlias::from_str(command).map(|alias| alias.to_string())?;
        tracing::debug!(r#"alias for command "{command}" is "{alias}""#);

        Ok(alias)
    }

    /// Returns whether `command` matches an approved session alias.
    ///
    /// # Errors
    ///
    /// Returns an error when `command` cannot produce an alias.
    pub fn approved(&self, command: &str) -> anyhow::Result<bool> {
        tracing::debug!(r#"checking if command is approved: "{command}""#);
        let alias = CommandAlias::from_str(command)?;

        Ok(self.approvals.contains(&alias))
    }

    /// Records `command` as approved for the current session.
    ///
    /// # Errors
    ///
    /// Returns an error when `command` cannot produce an alias.
    pub fn approve(&mut self, command: &str) -> anyhow::Result<()> {
        tracing::debug!(r#"approving command: "{command}""#);
        let alias = CommandAlias::from_str(command)?;
        self.approvals.insert(alias);

        Ok(())
    }

    /// Removes every approval recorded for the current session.
    pub fn clear(&mut self) {
        tracing::debug!("clearing all approvals");
        self.approvals.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_action_and_reports_later_matching_action_as_approved() {
        let mut storage = ApprovalsStorage::new();

        storage
            .approve(r#"git commit -a -m "initial""#)
            .expect("approval is recorded");

        assert!(
            storage
                .approved(r#"git commit --amend -m "follow-up""#)
                .expect("approval lookup succeeds")
        );
    }

    #[test]
    fn does_not_approve_different_aliases() {
        let mut storage = ApprovalsStorage::new();

        storage
            .approve(r#"git commit -a -m "initial""#)
            .expect("approval is recorded");

        assert!(
            !storage
                .approved("git status")
                .expect("approval lookup succeeds")
        );
    }

    #[test]
    fn returns_clear_alias_for_user_prompting() {
        let storage = ApprovalsStorage::new();

        let alias = storage
            .alias_for(r#"cargo test --package smista-cli parser"#)
            .expect("alias is generated");

        assert_eq!(alias, "cargo test *");
    }

    #[test]
    fn clears_session_approvals() {
        let mut storage = ApprovalsStorage::new();

        storage
            .approve("npm run build -- --release")
            .expect("approval is recorded");
        storage.clear();

        assert!(
            !storage
                .approved("npm run build -- --debug")
                .expect("approval lookup succeeds")
        );
    }

    #[test]
    fn reports_invalid_alias_input() {
        let mut storage = ApprovalsStorage::new();

        let error = storage
            .approve("&& rm -rf target")
            .expect_err("invalid aliases fail");

        assert_eq!(error.to_string(), "command alias has no command token");
    }
}
