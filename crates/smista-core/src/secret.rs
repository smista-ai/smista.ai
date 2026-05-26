//! Reference to a secret by key name.
//!
//! A [`SecretRef`] names a secret to resolve at runtime — for example from
//! `.smista/secrets`, an environment variable, or the OS keychain. It carries
//! only the *key name*, never the secret value: inline secret values will be
//! rejected by configuration validation (#4). Resolving a `SecretRef` into an
//! actual value is the responsibility of the consuming binary (see
//! `smista-cli`'s `config::secrets`).
//!
//! # Examples
//!
//! ```
//! use smista_core::secret::SecretRef;
//!
//! let reference = SecretRef::new("openai_api_key");
//! assert_eq!(reference.key(), "openai_api_key");
//! ```

use serde::{Deserialize, Serialize};

/// A reference to a secret by its key name.
///
/// Serializes transparently as the inner string, so in TOML a secret reference
/// is written as a plain key name (e.g. `api_key_ref = "openai_api_key"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// Creates a secret reference for the given key name.
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Returns the key name this reference points to.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.0
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
    fn should_serialize_transparently_as_string() {
        assert_eq!(
            serde_json::to_string(&SecretRef::new("openai_api_key")).unwrap(),
            "\"openai_api_key\""
        );
    }

    #[test]
    fn should_deserialize_from_a_plain_string() {
        assert_eq!(
            serde_json::from_str::<SecretRef>("\"k\"").unwrap(),
            SecretRef::new("k")
        );
    }
}
