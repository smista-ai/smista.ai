//! LLM provider identity.
//!
//! A [`Provider`] names the backend a model is served by. It is used in
//! configuration and routing to associate a model with its provider — for
//! example `anthropic/claude-sonnet` pairs the [`Provider::Anthropic`]
//! provider with a model name.
//!
//! The three built-in providers serialize to their bare lowercase name (e.g.
//! [`Provider::OpenAI`] becomes `"openai"`). A generic OpenAI-compatible
//! endpoint is a named instance that serializes as `openai-compat:<name>` (e.g.
//! `openai-compat:my-vllm`). The same form is produced by [`Display`] and
//! parsed by [`FromStr`], so the textual representation is stable across config
//! files, the CLI and the HTTP API.
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

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
#[cfg(feature = "surrealdb")]
use surrealdb_types::{Kind, SurrealValue, Value};

use crate::error::{CoreError, ParseError};

/// The identifier of a provider, used for configuration and routing.
///
/// The three built-in providers render as their bare lowercase name. A generic
/// OpenAI-compatible endpoint is a named instance that renders as
/// `openai-compat:<name>`, so several such endpoints can be configured and
/// routed to independently.
///
/// The type is intentionally not `Copy`: the named variant owns a `String`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[cfg_attr(feature = "ts", ts(type = "string"))]
pub enum Provider {
    /// Anthropic, serving the Claude family of models.
    Anthropic,
    /// Google Gemini, serving the Gemini family of models.
    Gemini,
    /// OpenAI, serving the GPT family of models.
    OpenAI,
    /// Ollama, serving local models.
    Ollama,
    /// A generic OpenAI-compatible endpoint (vLLM, LM Studio, a gateway, …),
    /// identified by its user-chosen instance name. Renders as
    /// `openai-compat:<name>`.
    OpenAICompatible(String),
}

/// Scheme tag that prefixes a named OpenAI-compatible instance in its textual
/// form, separated from the instance name by [`TAG_SEPARATOR`].
///
/// Chosen over a bare `openai` (which would read as if the endpoint *were*
/// OpenAI) and over a generic `compat` (which would be ambiguous if another
/// compatible family is ever added).
const OPENAI_COMPATIBLE_TAG: &str = "openai-compat";

/// Separator between the scheme tag and the instance name in a provider's
/// textual form: the `:` in `openai-compat:my-vllm`. Distinct from the `/` that
/// separates a provider from a model in a model reference.
const TAG_SEPARATOR: char = ':';

impl Provider {
    /// Returns whether `name` is a valid OpenAI-compatible instance name.
    ///
    /// Instance names are restricted to non-empty runs of lowercase ASCII
    /// letters, digits, `-` and `_`. The restriction keeps a name safe to use
    /// unquoted and free of the `/` and `:` characters that delimit a model
    /// reference.
    fn is_valid_instance_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    }

    /// Parses a provider from its exact canonical (lowercase) textual form.
    ///
    /// Unlike [`FromStr`], this performs no case folding: the input must already
    /// match the form produced by [`Display`]. It backs the strict
    /// [`Deserialize`] implementation, where configuration and stored values are
    /// expected to be canonical.
    fn from_canonical(s: &str) -> Result<Self, CoreError> {
        match s {
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            "openai" => Ok(Self::OpenAI),
            "ollama" => Ok(Self::Ollama),
            other => match other.split_once(TAG_SEPARATOR) {
                Some((OPENAI_COMPATIBLE_TAG, name)) if Self::is_valid_instance_name(name) => {
                    Ok(Self::OpenAICompatible(name.to_string()))
                }
                Some((OPENAI_COMPATIBLE_TAG, name)) => {
                    Err(ParseError::InvalidProviderName(name.to_string()).into())
                }
                _ => Err(ParseError::UnknownProvider(other.to_string()).into()),
            },
        }
    }
}

impl Display for Provider {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anthropic => f.write_str("anthropic"),
            Self::Gemini => f.write_str("gemini"),
            Self::OpenAI => f.write_str("openai"),
            Self::Ollama => f.write_str("ollama"),
            Self::OpenAICompatible(name) => {
                write!(f, "{OPENAI_COMPATIBLE_TAG}{TAG_SEPARATOR}{name}")
            }
        }
    }
}

impl FromStr for Provider {
    type Err = CoreError;

    /// Parses a provider name, as produced by [`Display`], case-insensitively.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::UnknownProvider`] if `s` names no known provider,
    /// or [`ParseError::InvalidProviderName`] if an `openai-compat:` instance
    /// name contains unsupported characters.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_canonical(&s.to_ascii_lowercase())
    }
}

impl Serialize for Provider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Provider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ProviderVisitor;

        impl Visitor<'_> for ProviderVisitor {
            type Value = Provider;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a provider name such as `openai` or `openai-compat:<name>`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Provider::from_canonical(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(ProviderVisitor)
    }
}

#[cfg(feature = "surrealdb")]
impl SurrealValue for Provider {
    fn kind_of() -> Kind {
        Kind::String
    }

    fn is_value(value: &Value) -> bool {
        matches!(value, Value::String(_))
    }

    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }

    fn from_value(value: Value) -> Result<Self, surrealdb_types::Error> {
        match value {
            Value::String(value) => value.parse().map_err(|error: CoreError| {
                surrealdb_types::Error::serialization(
                    error.to_string(),
                    surrealdb_types::SerializationError::Deserialization,
                )
            }),
            other => Err(surrealdb_types::conversion_error(Kind::String, other)),
        }
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for Provider {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::SchemaType::new(
                utoipa::openapi::schema::Type::String,
            ))
            .description(Some(
                "A provider identifier, e.g. `anthropic`, `openai`, or `openai-compat:<name>`.",
            ))
            .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for Provider {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Provider")
    }
}

/// A provider that is available for routing.
///
/// This is the shape returned by `GET /llm/providers`, which lists only the
/// providers that are currently available — those configured with usable
/// credentials (or a base URL, for local providers). A provider that is not
/// available is omitted from the listing entirely, so this descriptor carries
/// no availability flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ProviderDescriptor {
    /// The provider this descriptor refers to.
    pub id: Provider,
    /// Human-friendly name shown to users.
    pub display_name: String,
    /// Whether the provider serves its models locally, with no request leaving
    /// the host or network.
    ///
    /// This is the provider-level source of truth for locality: every model the
    /// provider offers inherits it, so a provider can never report itself local
    /// while a model it advertises routes to the cloud. The router uses it as a
    /// routing discriminant to keep sensitive work on local models.
    pub local: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Provider; 4] = [
        Provider::Anthropic,
        Provider::Gemini,
        Provider::OpenAI,
        Provider::Ollama,
    ];

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
    fn should_parse_and_render_gemini() {
        assert_eq!(Provider::from_str("gemini"), Ok(Provider::Gemini));
        assert_eq!(Provider::Gemini.to_string(), "gemini");
        assert_eq!(
            serde_json::to_string(&Provider::Gemini).unwrap(),
            "\"gemini\""
        );
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
            local: false,
        };
        assert_eq!(
            serde_json::to_value(&descriptor).unwrap(),
            serde_json::json!({
                "id": "anthropic",
                "display_name": "Anthropic",
                "local": false,
            })
        );
    }

    #[test]
    fn should_roundtrip_provider_descriptor() {
        let descriptor = ProviderDescriptor {
            id: Provider::Ollama,
            display_name: "Ollama".to_string(),
            local: true,
        };
        let json = serde_json::to_string(&descriptor).unwrap();
        assert_eq!(
            serde_json::from_str::<ProviderDescriptor>(&json).unwrap(),
            descriptor
        );
    }

    #[test]
    fn should_display_openai_compatible_with_scheme_tag() {
        let provider = Provider::OpenAICompatible("my-vllm".to_string());
        assert_eq!(provider.to_string(), "openai-compat:my-vllm");
    }

    #[test]
    fn should_parse_openai_compatible_instance() {
        assert_eq!(
            Provider::from_str("openai-compat:my-vllm"),
            Ok(Provider::OpenAICompatible("my-vllm".to_string()))
        );
    }

    #[test]
    fn should_roundtrip_openai_compatible_serde() {
        let provider = Provider::OpenAICompatible("lmstudio_1".to_string());
        let json = serde_json::to_string(&provider).unwrap();
        assert_eq!(json, "\"openai-compat:lmstudio_1\"");
        assert_eq!(serde_json::from_str::<Provider>(&json).unwrap(), provider);
    }

    #[test]
    fn should_parse_openai_compatible_case_insensitively() {
        assert_eq!(
            Provider::from_str("OpenAI-Compat:my-vllm"),
            Ok(Provider::OpenAICompatible("my-vllm".to_string()))
        );
    }

    #[test]
    fn should_reject_openai_compatible_without_instance_name() {
        // The bare tag with no name is not a usable identity.
        let err = Provider::from_str("openai-compat:").unwrap_err();
        assert!(matches!(
            err,
            CoreError::Parse(ParseError::InvalidProviderName(_))
        ));
    }

    #[test]
    fn should_reject_bare_scheme_tag_as_unknown_provider() {
        // Without the `:` separator the tag is just an unknown provider name.
        let err = Provider::from_str("openai-compat").unwrap_err();
        assert!(matches!(
            err,
            CoreError::Parse(ParseError::UnknownProvider(_))
        ));
    }

    #[test]
    fn should_reject_instance_name_with_unsupported_characters() {
        let err = Provider::from_str("openai-compat:bad name").unwrap_err();
        assert!(matches!(
            err,
            CoreError::Parse(ParseError::InvalidProviderName(_))
        ));
    }

    #[test]
    fn should_order_builtins_before_openai_compatible() {
        let mut providers = [
            Provider::OpenAICompatible("z".to_string()),
            Provider::Ollama,
            Provider::Anthropic,
        ];
        providers.sort();
        assert_eq!(
            providers,
            [
                Provider::Anthropic,
                Provider::Ollama,
                Provider::OpenAICompatible("z".to_string()),
            ]
        );
    }
}
