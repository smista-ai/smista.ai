//! Response bodies for listing providers and models.
//!
//! [`ListProvidersResponse`] answers `GET /llm/providers` by reusing the
//! domain [`ProviderDescriptor`](crate::model::ProviderDescriptor).
//! [`ListModelsResponse`] answers `GET /llm/models` with a flat [`ModelInfo`]
//! per model — the public listing surface, deliberately separate from the
//! richer internal `ModelDescriptor` (which carries auth, costs and parameters
//! tied to user configuration rather than to a model catalog).
//!
//! # Examples
//!
//! ```
//! use smista_core::api::ModelInfo;
//! use smista_core::model::Provider;
//!
//! let info = ModelInfo {
//!     provider: Provider::Ollama,
//!     model: "qwen2.5-coder".to_string(),
//!     local: true,
//!     requires_api_key: false,
//!     supports_streaming: true,
//!     supports_tools: false,
//!     supports_json_output: None,
//!     max_context_tokens: 32_768,
//! };
//! let json = serde_json::to_string(&info).unwrap();
//! assert!(json.contains("\"local\":true"));
//! ```

use serde::{Deserialize, Serialize};

use crate::model::{Provider, ProviderDescriptor};

/// Response to `GET /llm/providers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ListProvidersResponse {
    /// Configured and available providers.
    pub providers: Vec<ProviderDescriptor>,
}

/// Response to `GET /llm/models`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ListModelsResponse {
    /// Available models across providers.
    pub models: Vec<ModelInfo>,
}

/// Flat description of a model in the public catalog.
///
/// Capability flags are reported as flat `supports_*` booleans.
/// `supports_json_output` is omitted for models that do not report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ModelInfo {
    /// Provider that offers the model.
    pub provider: Provider,
    /// Model name.
    pub model: String,
    /// Whether the model runs locally.
    pub local: bool,
    /// Whether the model requires an API key.
    pub requires_api_key: bool,
    /// Whether the model can stream its response.
    pub supports_streaming: bool,
    /// Whether the model can call tools.
    pub supports_tools: bool,
    /// Whether the model can be constrained to emit JSON, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub supports_json_output: Option<bool>,
    /// Maximum number of context tokens the model accepts.
    pub max_context_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_spec_models() {
        let json = r#"{
            "models": [
                {
                    "provider": "openai",
                    "model": "gpt-5.5-thinking",
                    "local": false,
                    "requires_api_key": true,
                    "supports_streaming": true,
                    "supports_tools": true,
                    "supports_json_output": true,
                    "max_context_tokens": 200000
                },
                {
                    "provider": "ollama",
                    "model": "qwen2.5-coder",
                    "local": true,
                    "requires_api_key": false,
                    "supports_streaming": true,
                    "supports_tools": false,
                    "max_context_tokens": 32768
                }
            ]
        }"#;
        let response: ListModelsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.models.len(), 2);
        assert_eq!(response.models[0].supports_json_output, Some(true));
        assert_eq!(response.models[1].supports_json_output, None);
        assert_eq!(response.models[1].max_context_tokens, 32_768);
    }

    #[test]
    fn should_omit_absent_json_output_support() {
        let info = ModelInfo {
            provider: Provider::Ollama,
            model: "qwen2.5-coder".to_string(),
            local: true,
            requires_api_key: false,
            supports_streaming: true,
            supports_tools: false,
            supports_json_output: None,
            max_context_tokens: 32_768,
        };
        let value = serde_json::to_value(&info).unwrap();
        assert!(value.get("supports_json_output").is_none());
    }

    #[test]
    fn should_deserialize_spec_providers() {
        let json = r#"{
            "providers": [
                { "id": "openai", "display_name": "OpenAI", "configured": true },
                { "id": "ollama", "display_name": "Ollama", "configured": true }
            ]
        }"#;
        let response: ListProvidersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.providers.len(), 2);
        assert_eq!(response.providers[0].id, Provider::OpenAI);
    }
}
