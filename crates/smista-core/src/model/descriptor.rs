//! Full description of a model and what it can do.
//!
//! A [`ModelDescriptor`] is the canonical record of a single model: its
//! identity, capabilities, limits, costs and default parameters. The router
//! uses it to decide whether a model can serve a task via
//! [`ModelDescriptor::can_handle`], and the API in #7 serializes it for clients.
//!
//! # Examples
//!
//! ```
//! use smista_core::model::{
//!     ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters,
//!     Provider, RoutingRequirements,
//! };
//!
//! let descriptor = ModelDescriptor {
//!     provider: Provider::Anthropic,
//!     model: "claude-sonnet".to_string(),
//!     display_name: None,
//!     local: false,
//!     auth: ModelAuthRequirement::ApiKey,
//!     capabilities: ModelCapabilities {
//!         tools: true,
//!         ..Default::default()
//!     },
//!     max_context_tokens: 200_000,
//!     max_output_tokens: Some(8_192),
//!     input_cost_per_million_tokens: None,
//!     output_cost_per_million_tokens: None,
//!     default_parameters: ModelParameters::default(),
//!     provider_options: None,
//! };
//!
//! let requirements = RoutingRequirements {
//!     tools: true,
//!     ..Default::default()
//! };
//! assert!(descriptor.can_handle(&requirements).is_ok());
//! assert!(descriptor.requires_api_key());
//! ```

use serde::{Deserialize, Serialize};

use super::{
    ModelAuthRequirement, ModelCapabilities, ModelParameters, ModelReference, Provider,
    RoutingRequirements,
};
use crate::error::CapabilityError;

/// A complete description of a model offered by a provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Provider that offers the model.
    pub provider: Provider,
    /// Model name, as defined by the provider.
    pub model: String,
    /// Human-friendly name, if different from `model`.
    pub display_name: Option<String>,
    /// Whether the model runs locally (no network call).
    pub local: bool,
    /// How the model is authenticated.
    pub auth: ModelAuthRequirement,
    /// What the model can do.
    pub capabilities: ModelCapabilities,
    /// Maximum number of context tokens the model accepts.
    pub max_context_tokens: u32,
    /// Maximum number of tokens the model emits, if bounded.
    pub max_output_tokens: Option<u32>,
    /// Input price per million tokens, in `default_parameters`' currency.
    pub input_cost_per_million_tokens: Option<f64>,
    /// Output price per million tokens.
    pub output_cost_per_million_tokens: Option<f64>,
    /// Default generation parameters applied when none are supplied.
    pub default_parameters: ModelParameters,
    /// Provider-specific options, preserved verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_options: Option<serde_json::Value>,
}

impl ModelDescriptor {
    /// Returns a [`ModelReference`] identifying this model.
    #[must_use]
    pub fn reference(&self) -> ModelReference {
        ModelReference {
            provider: self.provider,
            model: self.model.clone(),
        }
    }

    /// Returns whether the model cannot be used without an API key.
    ///
    /// Derived from [`Self::auth`]; see
    /// [`ModelAuthRequirement::requires_api_key`].
    #[must_use]
    pub fn requires_api_key(&self) -> bool {
        self.auth.requires_api_key()
    }

    /// Returns whether an input of `estimated_tokens` fits the context window.
    #[must_use]
    pub fn fits_context(&self, estimated_tokens: u64) -> bool {
        estimated_tokens <= u64::from(self.max_context_tokens)
    }

    /// Checks that the model can serve a task with the given requirements.
    ///
    /// Each required capability must be supported, and the estimated input — if
    /// given — must fit the context window. A task that requires tool calls,
    /// for instance, cannot be handled by a model without the `tools`
    /// capability.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::MissingCapability`] for the first required
    /// capability the model lacks, or [`CapabilityError::ContextWindowExceeded`]
    /// if the estimated input does not fit the context window.
    pub fn can_handle(&self, requirements: &RoutingRequirements) -> Result<(), CapabilityError> {
        if let Some(missing) = requirements
            .required()
            .find(|capability| !self.capabilities.supports(*capability))
        {
            return Err(CapabilityError::MissingCapability(missing));
        }

        if let Some(estimated_tokens) = requirements.estimated_tokens
            && !self.fits_context(estimated_tokens)
        {
            return Err(CapabilityError::ContextWindowExceeded {
                estimated_tokens,
                max_context_tokens: self.max_context_tokens,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Capability;

    fn descriptor() -> ModelDescriptor {
        ModelDescriptor {
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
            display_name: Some("Claude Sonnet".to_string()),
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                ..Default::default()
            },
            max_context_tokens: 200_000,
            max_output_tokens: Some(8_192),
            input_cost_per_million_tokens: Some(3.0),
            output_cost_per_million_tokens: Some(15.0),
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    #[test]
    fn should_build_reference_from_descriptor() {
        assert_eq!(
            descriptor().reference().to_string(),
            "anthropic/claude-sonnet"
        );
    }

    #[test]
    fn should_derive_requires_api_key_from_auth() {
        assert!(descriptor().requires_api_key());
    }

    #[test]
    fn should_handle_task_within_capabilities_and_context() {
        let requirements = RoutingRequirements {
            tools: true,
            estimated_tokens: Some(100_000),
            ..Default::default()
        };
        assert_eq!(descriptor().can_handle(&requirements), Ok(()));
    }

    #[test]
    fn should_reject_task_needing_unsupported_capability() {
        let requirements = RoutingRequirements {
            images: true,
            ..Default::default()
        };
        assert_eq!(
            descriptor().can_handle(&requirements),
            Err(CapabilityError::MissingCapability(Capability::Images))
        );
    }

    #[test]
    fn should_reject_task_exceeding_context_window() {
        let requirements = RoutingRequirements {
            estimated_tokens: Some(200_001),
            ..Default::default()
        };
        assert_eq!(
            descriptor().can_handle(&requirements),
            Err(CapabilityError::ContextWindowExceeded {
                estimated_tokens: 200_001,
                max_context_tokens: 200_000,
            })
        );
    }

    #[test]
    fn should_report_context_fit() {
        let descriptor = descriptor();
        assert!(descriptor.fits_context(200_000));
        assert!(!descriptor.fits_context(200_001));
    }

    #[test]
    fn should_omit_provider_options_when_absent() {
        let json = serde_json::to_value(descriptor()).unwrap();
        assert!(json.get("provider_options").is_none());
    }
}
