//! Task intents.
//!
//! A [`TaskIntent`] classifies what a user is asking an AI workflow to do at a
//! given step. smista-router detects the intent of an incoming prompt and
//! matches it against the user's routing policy to pick a model
//! deterministically — intent detection never depends on an LLM.
//!
//! Intents serialize to their lowercase name (e.g. [`TaskIntent::Chat`] becomes
//! `"chat"`). The same lowercase form is produced by [`Display`] and parsed by
//! [`FromStr`], so the textual representation is stable across config files, the
//! CLI and the HTTP API.
//!
//! # Examples
//!
//! ```
//! use std::str::FromStr;
//!
//! use smista_core::intent::TaskIntent;
//!
//! let intent = TaskIntent::from_str("review").unwrap();
//! assert_eq!(intent, TaskIntent::Review);
//! assert_eq!(intent.to_string(), "review");
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
#[cfg(feature = "surrealdb")]
use surrealdb_types::SurrealValue;

use crate::error::{CoreError, ParseError};

/// The kind of work a single step of an AI workflow performs.
///
/// Used by smista-router to match a prompt against the user's routing policy.
/// Each variant serializes to its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "surrealdb", derive(SurrealValue))]
#[cfg_attr(feature = "surrealdb", surreal(crate = "::surrealdb_types"))]
#[cfg_attr(feature = "surrealdb", surreal(rename_all = "lowercase", untagged))]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TaskIntent {
    /// Free-form conversation with no specialized objective.
    Chat,
    /// Producing a plan or breaking a task into steps.
    Plan,
    /// Modifying existing code or text.
    Edit,
    /// Assessing code or text and giving feedback.
    Review,
    /// Condensing longer content into a shorter form.
    Summarize,
    /// Crafting or refining a prompt for another model.
    Prompt,
    /// Invoking a named skill or tool-driven capability.
    Skill,
}

impl TaskIntent {
    /// Returns the lowercase string representation of the intent.
    ///
    /// This is the same form used for serialization, [`Display`] and
    /// [`FromStr`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Plan => "plan",
            Self::Edit => "edit",
            Self::Review => "review",
            Self::Summarize => "summarize",
            Self::Prompt => "prompt",
            Self::Skill => "skill",
        }
    }
}

impl Display for TaskIntent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskIntent {
    type Err = CoreError;

    /// Parses the lowercase name of an intent, as produced by [`Display`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownIntent`] if `s` is not a known intent name.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s.to_ascii_lowercase() as &str {
            "chat" => Ok(Self::Chat),
            "plan" => Ok(Self::Plan),
            "edit" => Ok(Self::Edit),
            "review" => Ok(Self::Review),
            "summarize" => Ok(Self::Summarize),
            "prompt" => Ok(Self::Prompt),
            "skill" => Ok(Self::Skill),
            other => {
                tracing::warn!(intent.value = %other, "unknown task intent {{intent.value}}");
                Err(ParseError::UnknownIntent(other.to_string()).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TaskIntent; 7] = [
        TaskIntent::Chat,
        TaskIntent::Plan,
        TaskIntent::Edit,
        TaskIntent::Review,
        TaskIntent::Summarize,
        TaskIntent::Prompt,
        TaskIntent::Skill,
    ];

    #[test]
    fn should_serialize_to_lowercase_name() {
        assert_eq!(
            serde_json::to_string(&TaskIntent::Summarize).unwrap(),
            "\"summarize\""
        );
    }

    #[test]
    fn should_deserialize_from_lowercase_name() {
        assert_eq!(
            serde_json::from_str::<TaskIntent>("\"review\"").unwrap(),
            TaskIntent::Review
        );
    }

    #[test]
    fn should_reject_unknown_intent_on_deserialize() {
        assert!(serde_json::from_str::<TaskIntent>("\"deploy\"").is_err());
    }

    #[test]
    fn should_reject_uppercase_on_deserialize() {
        assert!(serde_json::from_str::<TaskIntent>("\"Chat\"").is_err());
    }

    #[test]
    fn should_roundtrip_serde_for_every_variant() {
        for intent in ALL {
            let json = serde_json::to_string(&intent).unwrap();
            let parsed: TaskIntent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, intent);
        }
    }

    #[test]
    fn should_roundtrip_display_and_from_str_for_every_variant() {
        for intent in ALL {
            assert_eq!(TaskIntent::from_str(&intent.to_string()), Ok(intent));
        }
    }

    #[test]
    fn should_match_display_with_serde_representation() {
        for intent in ALL {
            let json = serde_json::to_string(&intent).unwrap();
            assert_eq!(json, format!("\"{intent}\""));
        }
    }

    #[test]
    fn should_fail_to_parse_unknown_intent() {
        let err = TaskIntent::from_str("deploy").unwrap_err();
        if let CoreError::Parse(ParseError::UnknownIntent(unknown)) = err {
            assert_eq!(unknown, "deploy");
        } else {
            panic!("Expected UnknownIntent error");
        }
    }
}
