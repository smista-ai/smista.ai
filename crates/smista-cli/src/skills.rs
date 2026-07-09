//! Skill discovery and lazy reading from `SKILL.md` files.
//!
//! A skill is a directory containing a `SKILL.md` descriptor. The **directory
//! name is the skill's identity** — the key other components resolve against —
//! while the descriptor's YAML front matter (`name`, `description`) is advisory
//! metadata. This mirrors how agent tooling such as Claude Code keys skills:
//! the front matter `name` is expected to match the directory, and a mismatch
//! is reported as a warning rather than changing the identity.
//!
//! Discovery follows *progressive disclosure*: only the cheap metadata
//! (`description` plus the directory path) is read and held in memory eagerly.
//! The behavioral Markdown body — potentially large — is read on demand by
//! [`SkillStore::load`] when a skill is actually invoked, never kept resident.
//!
//! Skills are discovered from two locations, project first then global:
//!
//! - `<cwd>/.agents/skills` — project skills (higher precedence).
//! - `~/.agents/skills` — global, cross-tool skills.
//!
//! A name found in the project location wins; the global directory of the same
//! name is not even read again. Directories without a `SKILL.md` are ignored
//! and recorded as a [`SkillWarning`].
//!
//! The module is split by responsibility: [`entry`] holds the per-skill record,
//! [`store`] the discovery and lookup logic, [`warning`] and [`error`] the
//! reported findings, and [`front_matter`] the `SKILL.md` parsing helpers.

mod entry;
mod error;
mod front_matter;
mod store;
mod warning;

use std::path::{Path, PathBuf};

pub use entry::SkillEntry;
pub use error::SkillError;
use smista_sdk::core::paths::{home_agents_dir, project_agents_dir};
pub use store::SkillStore;
pub use warning::SkillWarning;

/// Name of the descriptor file every skill directory must contain.
const SKILL_FILE: &str = "SKILL.md";
/// Skills directory name within an agents directory.
const SKILLS_DIR: &str = "skills";

/// Returns the project skills directory: `<cwd>/.agents/skills`.
///
/// Skills are shared across agent tools, so they live under the cross-tool
/// `.agents` directory rather than under smista's own `.smista` directory.
#[must_use]
fn skills_dir(cwd: &Path) -> PathBuf {
    project_agents_dir(cwd).join(SKILLS_DIR)
}

/// Returns the global skills directory: `~/.agents/skills`, if resolvable.
///
/// Like the project skills directory (see [`skills_dir`]), global skills live
/// under the cross-tool `~/.agents` directory. Project skills take precedence
/// over these.
#[must_use]
fn global_skills_dir() -> Option<PathBuf> {
    home_agents_dir().map(|dir| dir.join(SKILLS_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_place_project_skills_under_agents() {
        let path = skills_dir(Path::new("/repo"));
        assert_eq!(path, Path::new("/repo").join(".agents").join("skills"));
    }

    #[test]
    fn should_build_global_skills_under_home_agents() {
        if let Some(path) = global_skills_dir() {
            assert!(path.ends_with(Path::new(".agents").join("skills")));
        }
    }
}
