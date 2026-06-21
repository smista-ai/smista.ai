//! Reference to a specific model offered by a provider.
//!
//! A [`ModelReference`] pairs a [`Provider`] with a model name, identifying a
//! single model uniquely — for example `anthropic/claude-sonnet`.
//!
//! References serialize to, and parse from, the compact `provider/model` form.
//! The same form is produced by [`Display`] and parsed by [`FromStr`], so the
//! textual representation is stable across config files, the CLI and the HTTP
//! API.
//!
//! # Examples
//!
//! ```
//! use std::str::FromStr;
//!
//! use smista_core::model::{ModelReference, Provider};
//!
//! let reference = ModelReference::from_str("anthropic/claude-sonnet").unwrap();
//! assert_eq!(reference.provider, Provider::Anthropic);
//! assert_eq!(reference.model, "claude-sonnet");
//! assert_eq!(reference.to_string(), "anthropic/claude-sonnet");
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Provider;
use crate::error::{CoreError, ParseError};

/// A reference to a specific model offered by a provider.
///
/// The textual form is `provider/model`, e.g. `anthropic/claude-sonnet`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[cfg_attr(feature = "ts", ts(type = "string"))]
pub struct ModelReference {
    /// Provider that offers this model.
    pub provider: Provider,
    /// Model name, as defined by the provider.
    pub model: String,
}

/// Separator between the provider and model in the textual form.
const SEPARATOR: char = '/';

impl Display for ModelReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.provider, self.model)
    }
}

impl FromStr for ModelReference {
    type Err = CoreError;

    /// Parses the `provider/model` form, as produced by [`Display`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidModelReference`] if `s` lacks a `/`
    /// separator or has an empty provider or model part, and
    /// [`ParseError::UnknownProvider`] if the provider part is unknown.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        tracing::trace!(model.reference = %s, "parsing model reference {{model.reference}}");
        let Some((provider, model)) = s.split_once(SEPARATOR) else {
            tracing::warn!(
                model.reference = %s,
                "model reference {{model.reference}} is missing the `provider/model` separator"
            );
            return Err(ParseError::InvalidModelReference(s.to_string()).into());
        };
        if provider.is_empty() || model.is_empty() {
            tracing::warn!(
                model.reference = %s,
                "model reference {{model.reference}} has an empty provider or model part"
            );
            return Err(ParseError::InvalidModelReference(s.to_string()).into());
        }

        let provider = provider.parse().inspect_err(|source| {
            tracing::warn!(
                model.provider = %provider,
                error.message = %source,
                "model reference names unknown provider {{model.provider}}"
            );
        })?;
        Ok(Self {
            provider,
            model: model.to_string(),
        })
    }
}

impl Serialize for ModelReference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ModelReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ModelReferenceVisitor;

        impl Visitor<'_> for ModelReferenceVisitor {
            type Value = ModelReference;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a model reference in the form `provider/model`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value.parse().map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(ModelReferenceVisitor)
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for ModelReference {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::SchemaType::new(
                utoipa::openapi::schema::Type::String,
            ))
            .description(Some(
                "A model reference in the form `provider/model`, e.g. `anthropic/claude-sonnet`.",
            ))
            .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for ModelReference {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ModelReference")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> ModelReference {
        ModelReference {
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
        }
    }

    #[test]
    fn should_display_as_provider_slash_model() {
        assert_eq!(reference().to_string(), "anthropic/claude-sonnet");
    }

    #[test]
    fn should_parse_provider_slash_model() {
        assert_eq!(
            ModelReference::from_str("anthropic/claude-sonnet"),
            Ok(reference())
        );
    }

    #[test]
    fn should_keep_slashes_in_model_name() {
        let parsed = ModelReference::from_str("ollama/library/llama3").unwrap();
        assert_eq!(parsed.provider, Provider::Ollama);
        assert_eq!(parsed.model, "library/llama3");
    }

    #[test]
    fn should_parse_openai_compatible_reference() {
        // The provider segment carries the scheme tag and instance name; the
        // first `/` still splits provider from model.
        let parsed = ModelReference::from_str("openai-compat:my-vllm/llama-3.1-70b").unwrap();
        assert_eq!(
            parsed.provider,
            Provider::OpenAICompatible("my-vllm".to_string())
        );
        assert_eq!(parsed.model, "llama-3.1-70b");
    }

    #[test]
    fn should_roundtrip_openai_compatible_reference() {
        let reference = ModelReference {
            provider: Provider::OpenAICompatible("my-vllm".to_string()),
            model: "llama-3.1-70b".to_string(),
        };
        assert_eq!(
            ModelReference::from_str(&reference.to_string()),
            Ok(reference)
        );
    }

    #[test]
    fn should_roundtrip_display_and_from_str() {
        assert_eq!(
            ModelReference::from_str(&reference().to_string()),
            Ok(reference())
        );
    }

    #[test]
    fn should_serialize_to_provider_slash_model() {
        assert_eq!(
            serde_json::to_string(&reference()).unwrap(),
            "\"anthropic/claude-sonnet\""
        );
    }

    #[test]
    fn should_deserialize_from_provider_slash_model() {
        assert_eq!(
            serde_json::from_str::<ModelReference>("\"anthropic/claude-sonnet\"").unwrap(),
            reference()
        );
    }

    #[test]
    fn should_roundtrip_serde() {
        let json = serde_json::to_string(&reference()).unwrap();
        assert_eq!(
            serde_json::from_str::<ModelReference>(&json).unwrap(),
            reference()
        );
    }

    #[test]
    fn should_reject_missing_separator() {
        let err = ModelReference::from_str("claude-sonnet").unwrap_err();
        assert!(matches!(
            err,
            CoreError::Parse(ParseError::InvalidModelReference(_))
        ));
    }

    #[test]
    fn should_reject_empty_provider() {
        assert!(ModelReference::from_str("/claude-sonnet").is_err());
    }

    #[test]
    fn should_reject_empty_model() {
        assert!(ModelReference::from_str("anthropic/").is_err());
    }

    #[test]
    fn should_reject_unknown_provider() {
        let err = ModelReference::from_str("cohere/command").unwrap_err();
        assert!(matches!(
            err,
            CoreError::Parse(ParseError::UnknownProvider(_))
        ));
    }

    #[test]
    fn should_order_by_provider_then_model() {
        let mut refs = [
            ModelReference::from_str("openai/b").unwrap(),
            ModelReference::from_str("anthropic/z").unwrap(),
            ModelReference::from_str("anthropic/a").unwrap(),
        ]
        .to_vec();
        refs.sort();
        assert_eq!(
            refs.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["anthropic/a", "anthropic/z", "openai/b"]
        );
    }
}
