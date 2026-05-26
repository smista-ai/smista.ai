//! Reference to a secret by key name.
//!
//! A [`SecretRef`] names a secret to resolve at runtime — for example from
//! `.smista/secrets`, an environment variable, or the OS keychain. It carries
//! only the *key name*, never the secret value.
//!
//! In configuration a secret is referenced inline with the interpolation form
//! `${secret:NAME}`, which may appear in any string field (for example
//! `api_key = "${secret:openai}"`). [`SecretRef::parse`] recognizes that form
//! and extracts `NAME`; a value that is not a placeholder is treated by the
//! consuming binary as an inline literal. Resolving a `SecretRef` into an actual
//! value is the responsibility of the consuming binary (see `smista-cli`'s
//! `config::secrets`).
//!
//! # Examples
//!
//! ```
//! use smista_core::secret::SecretRef;
//!
//! let reference = SecretRef::parse("${secret:openai}").unwrap();
//! assert_eq!(reference.key(), "openai");
//! assert_eq!(reference.to_string(), "${secret:openai}");
//! assert!(SecretRef::parse("sk-literal-value").is_none());
//! ```

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Prefix of the `${secret:NAME}` interpolation form.
const PLACEHOLDER_PREFIX: &str = "${secret:";
/// Suffix of the `${secret:NAME}` interpolation form.
const PLACEHOLDER_SUFFIX: &str = "}";

/// A reference to a secret by its key name.
///
/// Serializes and deserializes as the `${secret:NAME}` interpolation form, so a
/// secret reference round-trips through any string field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretRef(String);

impl SecretRef {
    /// Creates a secret reference for the given key name.
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Parses a `${secret:NAME}` interpolation placeholder into a reference.
    ///
    /// Returns `None` when `raw` is not a placeholder (e.g. an inline literal)
    /// or when the referenced name is empty.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let inner = raw
            .strip_prefix(PLACEHOLDER_PREFIX)?
            .strip_suffix(PLACEHOLDER_SUFFIX)?;
        if inner.is_empty() {
            return None;
        }
        Some(Self(inner.to_string()))
    }

    /// Returns the key name this reference points to.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PLACEHOLDER_PREFIX}{}{PLACEHOLDER_SUFFIX}", self.0)
    }
}

impl Serialize for SecretRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SecretRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SecretRefVisitor;

        impl Visitor<'_> for SecretRefVisitor {
            type Value = SecretRef;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a secret reference of the form ${secret:NAME}")
            }

            fn visit_str<E>(self, value: &str) -> Result<SecretRef, E>
            where
                E: de::Error,
            {
                SecretRef::parse(value)
                    .ok_or_else(|| de::Error::invalid_value(de::Unexpected::Str(value), &self))
            }
        }

        deserializer.deserialize_str(SecretRefVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_expose_key_name() {
        assert_eq!(SecretRef::new("k").key(), "k");
    }

    #[test]
    fn should_parse_placeholder_into_reference() {
        assert_eq!(
            SecretRef::parse("${secret:openai}"),
            Some(SecretRef::new("openai"))
        );
    }

    #[test]
    fn should_not_parse_a_literal_value() {
        assert_eq!(SecretRef::parse("sk-literal-value"), None);
    }

    #[test]
    fn should_not_parse_an_empty_reference() {
        assert_eq!(SecretRef::parse("${secret:}"), None);
    }

    #[test]
    fn should_display_as_placeholder() {
        assert_eq!(SecretRef::new("openai").to_string(), "${secret:openai}");
    }

    #[test]
    fn should_serialize_as_placeholder() {
        assert_eq!(
            serde_json::to_string(&SecretRef::new("openai")).unwrap(),
            "\"${secret:openai}\""
        );
    }

    #[test]
    fn should_deserialize_from_a_placeholder() {
        assert_eq!(
            serde_json::from_str::<SecretRef>("\"${secret:openai}\"").unwrap(),
            SecretRef::new("openai")
        );
    }

    #[test]
    fn should_reject_deserializing_a_literal() {
        assert!(serde_json::from_str::<SecretRef>("\"sk-literal\"").is_err());
    }
}
