//! Generation parameters applied to a model invocation.
//!
//! [`ModelParameters`] carries the common sampling knobs — temperature, top-p
//! and a token cap — alongside any provider-specific options. The common knobs
//! are typed; everything else is preserved verbatim in [`ModelParameters::extra`]
//! so a provider's bespoke settings survive a round-trip through config without
//! smista-core needing to know about them.
//!
//! # Examples
//!
//! ```
//! use smista_core::model::ModelParameters;
//!
//! let params = ModelParameters {
//!     temperature: Some(0.2),
//!     ..Default::default()
//! };
//! let json = serde_json::to_string(&params).unwrap();
//! assert_eq!(json, r#"{"temperature":0.2}"#);
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Sampling and generation parameters for a model invocation.
///
/// The typed fields are omitted from serialization when unset. Any keys not
/// matching a typed field are collected into [`Self::extra`] and re-emitted as
/// they were, so provider-specific options pass through untouched.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ModelParameters {
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling probability mass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Provider-specific knobs, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_empty() {
        let params = ModelParameters::default();
        assert_eq!(serde_json::to_string(&params).unwrap(), "{}");
    }

    #[test]
    fn should_omit_unset_typed_fields() {
        let params = ModelParameters {
            max_tokens: Some(256),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            r#"{"max_tokens":256}"#
        );
    }

    #[test]
    fn should_preserve_provider_specific_keys() {
        let json = r#"{"temperature":0.7,"reasoning_effort":"high"}"#;
        let params: ModelParameters = serde_json::from_str(json).unwrap();
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(
            params.extra.get("reasoning_effort"),
            Some(&Value::String("high".to_string()))
        );
    }

    #[test]
    fn should_roundtrip_extra_verbatim() {
        let json = r#"{"top_p":0.9,"seed":42}"#;
        let params: ModelParameters = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&params).unwrap(), json);
    }
}
