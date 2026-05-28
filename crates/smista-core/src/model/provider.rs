//! LLM provider identity.
//!
//! A [`Provider`] names the backend a model is served by. It is used in
//! configuration and routing to associate a model with its provider — for
//! example `anthropic/claude-sonnet` pairs the [`Provider::Anthropic`]
//! provider with a model name.
//!
//! Provider identifiers serialize to their lowercase name (e.g.
//! [`Provider::OpenAI`] becomes `"openai"`). The same lowercase form is
//! produced by [`Display`] and parsed by [`FromStr`], so the textual
//! representation is stable across config files, the CLI and the HTTP API.
//!
//! # Examples
//!
//! ```
//! use std::str::FromStr;
//!
//! use smista_core::model::Provider;
//!
//! let provider = Provider::from_str("openai").unwrap();
//! assert_eq!(provider, Provider::OpenAI);
//! assert_eq!(provider.to_string(), "openai");
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, ParseError};

/// The identifier of a provider, used for configuration and routing.
///
/// Each variant serializes to its lowercase name.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Provider {
    /// Anthropic, serving the Claude family of models.
    Anthropic,
    /// OpenAI, serving the GPT family of models.
    OpenAI,
    /// Ollama, serving local models.
    Ollama,
}

impl Provider {
    /// Returns the lowercase string representation of the provider.
    ///
    /// This is the same form used for serialization, [`Display`] and
    /// [`FromStr`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAI => "openai",
            Self::Ollama => "ollama",
        }
    }
}

impl Display for Provider {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Provider {
    type Err = CoreError;

    /// Parses the lowercase name of a provider, as produced by [`Display`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownProvider`] if `s` is not a known provider
    /// name.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &s.to_ascii_lowercase() as &str {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::OpenAI),
            "ollama" => Ok(Self::Ollama),
            other => Err(ParseError::UnknownProvider(other.to_string()).into()),
        }
    }
}

/// A provider together with its runtime configuration state.
///
/// This is the shape returned by `GET /llm/providers`: it pairs a provider's
/// identity and display name with whether usable credentials (or a base URL,
/// for local providers) are currently configured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ProviderDescriptor {
    /// The provider this descriptor refers to.
    pub id: Provider,
    /// Human-friendly name shown to users.
    pub display_name: String,
    /// Whether credentials or a base URL are configured for the provider.
    pub configured: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Provider; 3] = [Provider::Anthropic, Provider::OpenAI, Provider::Ollama];

    #[test]
    fn should_serialize_to_lowercase_name() {
        assert_eq!(
            serde_json::to_string(&Provider::OpenAI).unwrap(),
            "\"openai\""
        );
    }

    #[test]
    fn should_deserialize_from_lowercase_name() {
        assert_eq!(
            serde_json::from_str::<Provider>("\"anthropic\"").unwrap(),
            Provider::Anthropic
        );
    }

    #[test]
    fn should_reject_unknown_provider_on_deserialize() {
        assert!(serde_json::from_str::<Provider>("\"cohere\"").is_err());
    }

    #[test]
    fn should_reject_uppercase_on_deserialize() {
        assert!(serde_json::from_str::<Provider>("\"OpenAI\"").is_err());
    }

    #[test]
    fn should_roundtrip_serde_for_every_variant() {
        for provider in ALL {
            let json = serde_json::to_string(&provider).unwrap();
            let parsed: Provider = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, provider);
        }
    }

    #[test]
    fn should_roundtrip_display_and_from_str_for_every_variant() {
        for provider in ALL {
            assert_eq!(Provider::from_str(&provider.to_string()), Ok(provider));
        }
    }

    #[test]
    fn should_parse_case_insensitively() {
        assert_eq!(Provider::from_str("OpenAI"), Ok(Provider::OpenAI));
        assert_eq!(Provider::from_str("ANTHROPIC"), Ok(Provider::Anthropic));
    }

    #[test]
    fn should_match_display_with_serde_representation() {
        for provider in ALL {
            let json = serde_json::to_string(&provider).unwrap();
            assert_eq!(json, format!("\"{provider}\""));
        }
    }

    #[test]
    fn should_fail_to_parse_unknown_provider() {
        let err = Provider::from_str("cohere").unwrap_err();
        if let CoreError::Parse(ParseError::UnknownProvider(unknown)) = err {
            assert_eq!(unknown, "cohere");
        } else {
            panic!("Expected UnknownProvider error");
        }
    }

    #[test]
    fn should_serialize_provider_descriptor() {
        let descriptor = ProviderDescriptor {
            id: Provider::Anthropic,
            display_name: "Anthropic".to_string(),
            configured: true,
        };
        assert_eq!(
            serde_json::to_value(&descriptor).unwrap(),
            serde_json::json!({
                "id": "anthropic",
                "display_name": "Anthropic",
                "configured": true,
            })
        );
    }

    #[test]
    fn should_roundtrip_provider_descriptor() {
        let descriptor = ProviderDescriptor {
            id: Provider::Ollama,
            display_name: "Ollama".to_string(),
            configured: false,
        };
        let json = serde_json::to_string(&descriptor).unwrap();
        assert_eq!(
            serde_json::from_str::<ProviderDescriptor>(&json).unwrap(),
            descriptor
        );
    }
}
