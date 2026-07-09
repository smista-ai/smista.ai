//! The per-skill record produced by discovery.

use std::path::PathBuf;

use super::SKILL_FILE;

/// A discovered skill: where it lives plus its eagerly-read metadata.
///
/// The behavioral instructions are intentionally absent; they are read on
/// demand via [`SkillStore::load`](super::SkillStore::load).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    /// Directory holding the skill's `SKILL.md`. Its name is the skill identity.
    dir: PathBuf,
    /// Description from the front matter (empty when none was provided).
    description: String,
}

impl SkillEntry {
    /// Creates an entry for a skill discovered at `dir`.
    pub(super) fn new(dir: PathBuf, description: String) -> Self {
        Self { dir, description }
    }

    /// Returns the skill's description from its front matter.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Path to this skill's `SKILL.md` descriptor.
    pub(super) fn skill_file(&self) -> PathBuf {
        self.dir.join(SKILL_FILE)
    }
}
