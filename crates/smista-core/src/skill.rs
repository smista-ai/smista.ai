//! Reusable execution profiles loaded from `SKILL.md` files.
//!
//! A [`Skill`] is a named, reproducible execution profile: task-specific
//! instructions the CLI discovers from `~/.agents/skills` and `.smista/skills`.
//! Each skill directory holds a `SKILL.md` whose front matter supplies the
//! machine-readable [`Skill::name`] and [`Skill::description`], and whose
//! Markdown body becomes the behavioral [`Skill::instructions`].
//!
//! The router never discovers skills itself; the CLI resolves them and sends
//! the relevant ones as execute-request context, so this type stays
//! provider-agnostic and serialization-friendly. It is shared with the HTTP
//! API ([`crate::api`]).
//!
//! # Examples
//!
//! ```
//! use smista_core::skill::Skill;
//!
//! let skill = Skill {
//!     name: "code-review".to_string(),
//!     description: "Review code changes for correctness and style.".to_string(),
//!     instructions: "Focus on correctness, maintainability and security.".to_string(),
//! };
//! let json = serde_json::to_string(&skill).unwrap();
//! assert!(json.contains("\"name\":\"code-review\""));
//! ```

use serde::{Deserialize, Serialize};

/// A reusable execution profile defined by a `SKILL.md` file.
///
/// `name` and `description` come from the file's front matter; `instructions`
/// is its Markdown body, which frames the model's behavior for the task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct Skill {
    /// Machine-readable identifier used to resolve the skill by name.
    pub name: String,
    /// Human-readable summary of what the skill does.
    pub description: String,
    /// Behavioral instructions from the skill's Markdown body.
    pub instructions: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Skill {
        Skill {
            name: "code-review".to_string(),
            description: "Review code changes.".to_string(),
            instructions: "Report findings by severity.".to_string(),
        }
    }

    #[test]
    fn should_serialize_with_snake_case_fields() {
        assert_eq!(
            serde_json::to_value(sample()).unwrap(),
            serde_json::json!({
                "name": "code-review",
                "description": "Review code changes.",
                "instructions": "Report findings by severity.",
            })
        );
    }

    #[test]
    fn should_roundtrip_serde() {
        let skill = sample();
        let json = serde_json::to_string(&skill).unwrap();
        assert_eq!(serde_json::from_str::<Skill>(&json).unwrap(), skill);
    }
}
