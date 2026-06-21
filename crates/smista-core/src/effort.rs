//! Reasoning effort levels.
//!
//! An [`Effort`] expresses how much reasoning effort a model should spend on a
//! routed task. smista-router attaches the effort declared on the matched
//! routing rule to the model invocation; the provider adapter maps it onto the
//! model's native effort control. Selecting an effort never depends on an LLM.
//!
//! Efforts serialize to their lowercase name (e.g. [`Effort::High`] becomes
//! `"high"`). The same lowercase form is produced by [`Display`] and parsed by
//! [`FromStr`], so the textual representation is stable across config files, the
//! CLI and the HTTP API. The default effort is [`Effort::Medium`].
//!
//! # Examples
//!
//! ```
//! use std::str::FromStr;
//!
//! use smista_core::effort::Effort;
//!
//! let effort = Effort::from_str("high").unwrap();
//! assert_eq!(effort, Effort::High);
//! assert_eq!(effort.to_string(), "high");
//! assert_eq!(Effort::default(), Effort::Medium);
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, ParseError};

/// The reasoning effort a model should spend on a routed task.
///
/// Declared per routing rule and passed through to the model as its effort
/// setting. Each variant serializes to its lowercase name. The default is
/// [`Effort::Medium`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Effort {
    /// Minimal reasoning effort; fastest and cheapest.
    Low,
    /// Balanced reasoning effort. The default when unset.
    #[default]
    Medium,
    /// Elevated reasoning effort.
    High,
    /// Maximum reasoning effort; slowest and most expensive.
    Xhigh,
}

impl Effort {
    /// Returns the lowercase string representation of the effort.
    ///
    /// This is the same form used for serialization, [`Display`] and
    /// [`FromStr`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

impl Display for Effort {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Effort {
    type Err = CoreError;

    /// Parses the lowercase name of an effort, as produced by [`Display`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownEffort`] if `s` is not a known effort name.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s.to_ascii_lowercase() as &str {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::Xhigh),
            other => {
                tracing::warn!(effort.value = %other, "unknown reasoning effort {{effort.value}}");
                Err(ParseError::UnknownEffort(other.to_string()).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Effort; 4] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh];

    #[test]
    fn should_default_to_medium() {
        assert_eq!(Effort::default(), Effort::Medium);
    }

    #[test]
    fn should_serialize_to_lowercase_name() {
        assert_eq!(serde_json::to_string(&Effort::Xhigh).unwrap(), "\"xhigh\"");
    }

    #[test]
    fn should_deserialize_from_lowercase_name() {
        assert_eq!(
            serde_json::from_str::<Effort>("\"high\"").unwrap(),
            Effort::High
        );
    }

    #[test]
    fn should_reject_unknown_effort_on_deserialize() {
        assert!(serde_json::from_str::<Effort>("\"extreme\"").is_err());
    }

    #[test]
    fn should_reject_uppercase_on_deserialize() {
        assert!(serde_json::from_str::<Effort>("\"Low\"").is_err());
    }

    #[test]
    fn should_roundtrip_serde_for_every_variant() {
        for effort in ALL {
            let json = serde_json::to_string(&effort).unwrap();
            let parsed: Effort = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, effort);
        }
    }

    #[test]
    fn should_roundtrip_display_and_from_str_for_every_variant() {
        for effort in ALL {
            assert_eq!(Effort::from_str(&effort.to_string()), Ok(effort));
        }
    }

    #[test]
    fn should_match_display_with_serde_representation() {
        for effort in ALL {
            let json = serde_json::to_string(&effort).unwrap();
            assert_eq!(json, format!("\"{effort}\""));
        }
    }

    #[test]
    fn should_parse_case_insensitively() {
        assert_eq!(Effort::from_str("XHIGH"), Ok(Effort::Xhigh));
    }

    #[test]
    fn should_fail_to_parse_unknown_effort() {
        let err = Effort::from_str("extreme").unwrap_err();
        if let CoreError::Parse(ParseError::UnknownEffort(unknown)) = err {
            assert_eq!(unknown, "extreme");
        } else {
            panic!("Expected UnknownEffort error");
        }
    }
}
