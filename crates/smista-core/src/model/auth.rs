//! Authentication a model requires before it can be used.
//!
//! [`ModelAuthRequirement`] records how a model is authenticated — none, an
//! API key, an optional API key, or a custom scheme named by the provider.
//! Routing and configuration consult it to decide whether a model is usable
//! with the credentials at hand.
//!
//! Whether an API key is *required* is derived from the variant via
//! [`ModelAuthRequirement::requires_api_key`] rather than stored as a separate
//! flag, so the two can never disagree.
//!
//! # Examples
//!
//! ```
//! use smista_core::model::ModelAuthRequirement;
//!
//! assert!(ModelAuthRequirement::ApiKey.requires_api_key());
//! assert!(!ModelAuthRequirement::OptionalApiKey.requires_api_key());
//! assert!(!ModelAuthRequirement::None.requires_api_key());
//! ```

use serde::{Deserialize, Serialize};

/// How a model is authenticated.
///
/// Each variant serializes to its snake_case name; [`Self::Custom`] carries the
/// scheme name as its payload (e.g. `{"custom": "aws-sigv4"}`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ModelAuthRequirement {
    /// No authentication is needed, e.g. a local model.
    #[default]
    None,
    /// An API key is required.
    ApiKey,
    /// An API key may be supplied but is not required.
    OptionalApiKey,
    /// A provider-specific scheme, named by the contained string.
    Custom(String),
}

impl ModelAuthRequirement {
    /// Returns whether the model cannot be used without an API key.
    ///
    /// Only [`Self::ApiKey`] requires one; [`Self::OptionalApiKey`] accepts a
    /// key but does not require it, and [`Self::Custom`] schemes manage their
    /// own credentials.
    #[must_use]
    pub fn requires_api_key(&self) -> bool {
        matches!(self, Self::ApiKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_none() {
        assert_eq!(ModelAuthRequirement::default(), ModelAuthRequirement::None);
    }

    #[test]
    fn should_require_api_key_only_for_api_key_variant() {
        assert!(ModelAuthRequirement::ApiKey.requires_api_key());
        assert!(!ModelAuthRequirement::None.requires_api_key());
        assert!(!ModelAuthRequirement::OptionalApiKey.requires_api_key());
        assert!(!ModelAuthRequirement::Custom("aws-sigv4".to_string()).requires_api_key());
    }

    #[test]
    fn should_serialize_unit_variant_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&ModelAuthRequirement::OptionalApiKey).unwrap(),
            "\"optional_api_key\""
        );
    }

    #[test]
    fn should_serialize_custom_variant_with_payload() {
        assert_eq!(
            serde_json::to_string(&ModelAuthRequirement::Custom("aws-sigv4".to_string())).unwrap(),
            "{\"custom\":\"aws-sigv4\"}"
        );
    }

    #[test]
    fn should_roundtrip_serde() {
        let values = [
            ModelAuthRequirement::None,
            ModelAuthRequirement::ApiKey,
            ModelAuthRequirement::OptionalApiKey,
            ModelAuthRequirement::Custom("aws-sigv4".to_string()),
        ];
        for value in values {
            let json = serde_json::to_string(&value).unwrap();
            let parsed: ModelAuthRequirement = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, value);
        }
    }
}
