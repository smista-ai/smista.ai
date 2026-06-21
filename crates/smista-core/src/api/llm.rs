//! Response bodies for listing providers and models.
//!
//! [`ListProvidersResponse`] answers `GET /llm/providers` by reusing the
//! domain [`ProviderDescriptor`](crate::model::ProviderDescriptor), which lists
//! only the providers that are currently available. [`ListModelsResponse`]
//! answers `GET /llm/models` with one [`ModelDescriptor`](crate::model::ModelDescriptor)
//! per available model — the same canonical record the router uses internally.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::{ListModelsResponse, UnavailableProvider};
//! use smista_core::error::ProviderErrorCategory;
//! use smista_core::model::{
//!     ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
//! };
//!
//! let response = ListModelsResponse {
//!     models: vec![ModelDescriptor {
//!         provider: Provider::Ollama,
//!         model: "qwen2.5-coder".to_string(),
//!         display_name: None,
//!         local: true,
//!         auth: ModelAuthRequirement::None,
//!         capabilities: ModelCapabilities {
//!             streaming: true,
//!             ..Default::default()
//!         },
//!         max_context_tokens: 32_768,
//!         max_output_tokens: None,
//!         input_cost_per_million_tokens: None,
//!         output_cost_per_million_tokens: None,
//!         default_parameters: ModelParameters::default(),
//!         provider_options: None,
//!     }],
//!     unavailable: vec![UnavailableProvider {
//!         provider: Provider::Anthropic,
//!         reason: ProviderErrorCategory::MissingCredentials,
//!         message: None,
//!     }],
//! };
//! let json = serde_json::to_string(&response).unwrap();
//! assert!(json.contains("\"local\":true"));
//! assert!(json.contains("\"reason\":\"missing_credentials\""));
//! ```

use serde::{Deserialize, Serialize};

use crate::error::ProviderErrorCategory;
use crate::model::{ModelDescriptor, Provider, ProviderDescriptor};

/// Response to `GET /llm/providers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ListProvidersResponse {
    /// Available providers.
    pub providers: Vec<ProviderDescriptor>,
}

/// Response to `GET /llm/models`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ListModelsResponse {
    /// Available models across providers.
    pub models: Vec<ModelDescriptor>,
    /// Providers omitted from `models` because listing their models failed,
    /// each with the reason it dropped out.
    ///
    /// The router queries every configured provider when listing models. A
    /// provider that cannot be listed — most often because its credentials are
    /// missing or were rejected — is left out of `models` and reported here, so
    /// a caller can tell an incomplete result from a genuinely empty one and act
    /// on the cause. Empty when every configured provider was listed.
    #[serde(default)]
    pub unavailable: Vec<UnavailableProvider>,
}

/// A provider left out of [`ListModelsResponse::models`] because its models
/// could not be listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct UnavailableProvider {
    /// The provider whose models could not be listed.
    pub provider: Provider,
    /// The classification of the failure that left the provider out.
    pub reason: ProviderErrorCategory,
    /// Human-readable, redacted detail describing the failure, when the
    /// provider reported one. Never contains credentials.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Provider;

    #[test]
    fn should_deserialize_spec_models() {
        let json = r#"{
            "models": [
                {
                    "provider": "openai",
                    "model": "gpt-5.5-thinking",
                    "display_name": "GPT-5.5 Thinking",
                    "local": false,
                    "auth": "api_key",
                    "capabilities": { "streaming": true, "tools": true, "json_output": true },
                    "max_context_tokens": 200000,
                    "max_output_tokens": 16384,
                    "input_cost_per_million_tokens": "1.25",
                    "output_cost_per_million_tokens": "10",
                    "default_parameters": {}
                },
                {
                    "provider": "ollama",
                    "model": "qwen2.5-coder",
                    "display_name": null,
                    "local": true,
                    "auth": "none",
                    "capabilities": { "streaming": true },
                    "max_context_tokens": 32768,
                    "max_output_tokens": null,
                    "default_parameters": {}
                }
            ]
        }"#;
        let response: ListModelsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.models.len(), 2);
        assert_eq!(response.models[0].provider, Provider::OpenAI);
        assert!(response.models[0].capabilities.tools);
        assert!(response.models[0].requires_api_key());
        assert!(!response.models[1].requires_api_key());
        assert_eq!(response.models[1].max_context_tokens, 32_768);
    }

    #[test]
    fn should_serialize_capabilities_as_nested_object() {
        let response = ListModelsResponse {
            models: vec![ModelDescriptor {
                provider: Provider::Ollama,
                model: "qwen2.5-coder".to_string(),
                display_name: None,
                local: true,
                auth: crate::model::ModelAuthRequirement::None,
                capabilities: crate::model::ModelCapabilities {
                    streaming: true,
                    ..Default::default()
                },
                max_context_tokens: 32_768,
                max_output_tokens: None,
                input_cost_per_million_tokens: None,
                output_cost_per_million_tokens: None,
                default_parameters: crate::model::ModelParameters::default(),
                provider_options: None,
            }],
            unavailable: Vec::new(),
        };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["models"][0]["capabilities"]["streaming"], true);
        assert_eq!(value["models"][0]["local"], true);
    }

    #[test]
    fn should_default_unavailable_when_absent() {
        let json = r#"{ "models": [] }"#;
        let response: ListModelsResponse = serde_json::from_str(json).unwrap();

        assert!(response.unavailable.is_empty());
    }

    #[test]
    fn should_report_unavailable_providers() {
        let response = ListModelsResponse {
            models: Vec::new(),
            unavailable: vec![UnavailableProvider {
                provider: Provider::Anthropic,
                reason: ProviderErrorCategory::InvalidCredentials,
                message: Some("provider rejected the configured credentials".to_string()),
            }],
        };
        let value = serde_json::to_value(&response).unwrap();

        assert_eq!(value["unavailable"][0]["provider"], "anthropic");
        assert_eq!(value["unavailable"][0]["reason"], "invalid_credentials");
        let parsed: ListModelsResponse =
            serde_json::from_value(value).expect("round-trips through JSON");
        assert_eq!(parsed.unavailable.len(), 1);
        assert_eq!(
            parsed.unavailable[0].reason,
            ProviderErrorCategory::InvalidCredentials
        );
    }

    #[test]
    fn should_deserialize_spec_providers() {
        let json = r#"{
            "providers": [
                { "id": "openai", "display_name": "OpenAI", "local": false },
                { "id": "ollama", "display_name": "Ollama", "local": true }
            ]
        }"#;
        let response: ListProvidersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.providers.len(), 2);
        assert_eq!(response.providers[0].id, Provider::OpenAI);
    }
}
