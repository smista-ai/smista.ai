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
//! use smista_core::api::ListModelsResponse;
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
//! };
//! let json = serde_json::to_string(&response).unwrap();
//! assert!(json.contains("\"local\":true"));
//! ```

use serde::{Deserialize, Serialize};

use crate::model::{ModelDescriptor, ProviderDescriptor};

/// Response to `GET /llm/providers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ListProvidersResponse {
    /// Available providers.
    pub providers: Vec<ProviderDescriptor>,
}

/// Response to `GET /llm/models`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ListModelsResponse {
    /// Available models across providers.
    pub models: Vec<ModelDescriptor>,
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
        };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["models"][0]["capabilities"]["streaming"], true);
        assert_eq!(value["models"][0]["local"], true);
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
