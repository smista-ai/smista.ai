//! Errors raised while loading a skill's full definition.

/// Errors raised while loading a skill's full definition.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    /// The skill's `SKILL.md` could not be read.
    #[error("failed to read skill file {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: String,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// No discovered skill matched the requested name.
    #[error("no skill named `{name}` was discovered")]
    NotFound {
        /// The requested skill name.
        name: String,
    },
    /// The skill's front matter was not valid YAML.
    #[error("invalid front matter in {path}: {source}")]
    Parse {
        /// Path whose front matter failed to parse.
        path: String,
        /// Underlying YAML error.
        source: yaml_serde::Error,
    },
}
