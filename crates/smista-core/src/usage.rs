//! Token usage and cost metadata for a model invocation.
//!
//! [`Usage`] records how many tokens an invocation consumed and what it cost.
//! Every field is optional: providers report different subsets, and costs may
//! be estimated up front, reconciled afterwards, or absent for local models.
//! The type is shared with smista-providers, smista-trace and the web API.
//!
//! # Examples
//!
//! ```
//! use smista_core::usage::Usage;
//!
//! let usage = Usage {
//!     input_tokens: Some(1_024),
//!     output_tokens: Some(256),
//!     ..Default::default()
//! };
//! // No currency reported: defaults to USD.
//! assert_eq!(usage.currency(), "USD");
//! ```

use serde::{Deserialize, Serialize};

/// ISO 4217 currency code assumed when a [`Usage`] reports no currency.
const DEFAULT_CURRENCY: &str = "USD";

/// Token usage and cost metadata for a single model invocation.
///
/// All fields are optional and omitted from serialization when unset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens in the input/prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Tokens generated in the output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Input tokens served from a provider cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    /// Tokens spent on reasoning, when reported separately.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Total tokens, when the provider reports a combined count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Cost estimated before the invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    /// Cost reconciled after the invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_cost: Option<f64>,
    /// ISO 4217 currency code for the costs; defaults to `USD` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

impl Usage {
    /// Returns the currency code, defaulting to `USD` when none is reported.
    #[must_use]
    pub fn currency(&self) -> &str {
        self.currency.as_deref().unwrap_or(DEFAULT_CURRENCY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_empty_object() {
        assert_eq!(serde_json::to_string(&Usage::default()).unwrap(), "{}");
    }

    #[test]
    fn should_default_currency_to_usd() {
        assert_eq!(Usage::default().currency(), "USD");
    }

    #[test]
    fn should_use_reported_currency() {
        let usage = Usage {
            currency: Some("EUR".to_string()),
            ..Default::default()
        };
        assert_eq!(usage.currency(), "EUR");
    }

    #[test]
    fn should_omit_unset_fields() {
        let usage = Usage {
            input_tokens: Some(1_024),
            output_tokens: Some(256),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&usage).unwrap(),
            serde_json::json!({ "input_tokens": 1_024, "output_tokens": 256 })
        );
    }

    #[test]
    fn should_roundtrip_serde() {
        let usage = Usage {
            input_tokens: Some(1_024),
            output_tokens: Some(256),
            total_tokens: Some(1_280),
            actual_cost: Some(0.01),
            currency: Some("USD".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert_eq!(serde_json::from_str::<Usage>(&json).unwrap(), usage);
    }
}
