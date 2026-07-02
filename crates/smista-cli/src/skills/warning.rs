//! Advisory findings produced while discovering skills.

use std::path::PathBuf;

/// An advisory finding produced while discovering skills.
///
/// Warnings never block discovery; they surface skill directories that were
/// skipped or that look misconfigured.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillWarning {
    /// A `SKILL.md` exists but its YAML front matter could not be parsed.
    #[error("skill at {} has invalid front matter: {reason}", .dir.display())]
    InvalidFrontMatter {
        /// Directory of the offending skill.
        dir: PathBuf,
        /// Human-readable reason the front matter was rejected.
        reason: String,
    },
    /// A `SKILL.md` was found but declares no `description`.
    #[error("skill at {} is missing a `description` in its front matter", .dir.display())]
    MissingDescription {
        /// Directory of the skill missing a description.
        dir: PathBuf,
    },
    /// A directory under a skills root has no `SKILL.md` and was ignored.
    #[error("directory {} has no SKILL.md and was ignored", .dir.display())]
    MissingSkillFile {
        /// Directory that was skipped.
        dir: PathBuf,
    },
    /// The front matter `name` differs from the directory name.
    #[error("skill at {} declares name `{declared}` which differs from its directory name", .dir.display())]
    NameMismatch {
        /// Directory of the skill.
        dir: PathBuf,
        /// The name declared in the front matter.
        declared: String,
    },
}
