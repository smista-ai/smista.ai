//! Reusable execution profiles loaded from `SKILL.md` files.
//!
//! A [`Skill`] is a named, reproducible execution profile: the task-specific
//! instructions the CLI discovers from `~/.agents/skills` and `.smista/skills`.
//! Each skill directory holds a `SKILL.md` whose front matter supplies the
//! machine-readable [`Skill::name`] and whose Markdown body becomes the
//! behavioral [`Skill::content`].
//!
//! The router never discovers skills itself; the CLI resolves them and sends
//! them as execute-request context, so this type stays provider-agnostic and
//! serialization-friendly. It is shared with the HTTP API ([`crate::api`]).
//!
//! # Examples
//!
//! ```
//! use smista_core::skill::Skill;
//!
//! let skill = Skill {
//!     name: "code-review".to_string(),
//!     content: "Focus on correctness, maintainability and security.".to_string(),
//! };
//! let json = serde_json::to_string(&skill).unwrap();
//! assert!(json.contains("\"name\":\"code-review\""));
//! ```

use serde::{Deserialize, Serialize};

/// A reusable execution profile defined by a `SKILL.md` file.
///
/// [`name`](Self::name) comes from the file's front matter; [`content`](Self::content)
/// is its Markdown body, which frames the model's behavior for the task. The
/// same type carries a skill whether it was explicitly invoked or merely offered
/// for the model to activate; the two cases differ by where the skill travels in
/// the request, not by shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Skill {
    /// Machine-readable identifier used to resolve the skill by name.
    pub name: String,
    /// The `SKILL.md` body: the behavioral instructions for the model.
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Skill {
        Skill {
            name: "code-review".to_string(),
            content: "Report findings by severity.".to_string(),
        }
    }

    #[test]
    fn should_serialize_with_snake_case_fields() {
        assert_eq!(
            serde_json::to_value(sample()).unwrap(),
            serde_json::json!({
                "name": "code-review",
                "content": "Report findings by severity.",
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
