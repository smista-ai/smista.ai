//! The aggregate CLI configuration and its client-side sections.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smista_core::model::Provider;
use smista_core::policy::{ClassificationConfig, PrivacyPolicy, RoutingPolicy, ToolsConfig};

/// The merged CLI/policy configuration loaded from `config.toml`.
///
/// Every section defaults to empty so a missing or partial file is valid; the
/// layered merge (`super::layers`) combines layers per section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Configured providers, keyed by provider identity.
    pub providers: BTreeMap<Provider, ProviderConfig>,
    /// Known models, keyed by their `provider/model` reference string.
    pub models: BTreeMap<String, ModelConfig>,
    /// Routing policy.
    pub routing: RoutingPolicy,
    /// Task-classification configuration.
    pub classification: ClassificationConfig,
    /// Tool permissions.
    pub tools: ToolsConfig,
    /// Privacy constraints.
    pub privacy: PrivacyPolicy,
    /// Router client connection settings.
    pub router: RouterClientConfig,
    /// Uncommitted local preferences (highest non-runtime layer).
    #[serde(rename = "local_preferences")]
    pub local: LocalPreferences,
}

/// How a provider is configured client-side.
///
/// Mirrors a `[providers.<id>]` table. The `type` key names the provider kind.
/// The credential lives in `api_key`, which holds a `${secret:NAME}` reference
/// resolved against the secret sources (see `super::secrets`) — an environment
/// variable named `NAME` first, then the `.smista/secrets` files — or, where
/// allowed, an inline literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider kind (serialized as `type`).
    #[serde(rename = "type")]
    pub kind: Provider,
    /// Base URL for the provider endpoint, if non-default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// API key value: a `${secret:NAME}` reference or, where allowed, a literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// A model and its capabilities, as declared in `config.toml`.
///
/// Mirrors a `[models."provider/model"]` table. This is the user-authored
/// configuration shape; the router expands it into a runtime descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Provider that offers the model.
    pub provider: Provider,
    /// Model name, as defined by the provider.
    pub name: String,
    /// Whether the model requires an API key.
    #[serde(default)]
    pub requires_api_key: bool,
    /// Whether the model runs locally.
    #[serde(default)]
    pub local: bool,
    /// Whether the model supports streaming responses.
    #[serde(default)]
    pub supports_streaming: bool,
    /// Whether the model supports tool calls.
    #[serde(default)]
    pub supports_tools: bool,
    /// Whether the model supports structured JSON output.
    #[serde(default)]
    pub supports_json_output: bool,
    /// Whether the model supports explicit reasoning.
    #[serde(default)]
    pub supports_reasoning: bool,
    /// Maximum number of context tokens the model accepts.
    pub max_context_tokens: u32,
}

/// Source from which the router auth credential is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthSource {
    /// OS keychain.
    #[default]
    Keychain,
    /// Environment variable.
    Env,
    /// On-disk file.
    File,
    /// External credential helper.
    Helper,
}

/// Client-side settings for connecting to smista-router.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RouterClientConfig {
    /// Router base URL, e.g. `http://127.0.0.1:7331`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Whether to start a local router automatically when none is reachable.
    #[serde(default)]
    pub auto_start: bool,
    /// Connection timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_ms: Option<u64>,
    /// Request timeout in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_timeout_ms: Option<u64>,
    /// Where the router auth credential is read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_source: Option<AuthSource>,
}

/// Uncommitted local preferences. The only layer that is not version-controlled.
///
/// All fields are optional so that an unset preference defers to lower layers
/// during the merge.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LocalPreferences {
    /// Apply file writes without prompting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_apply: Option<bool>,
    /// Stream model output when supported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Use only local models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_only: Option<bool>,
    /// Forbid network access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_network: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_empty_config_to_default() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn should_parse_provider_with_type_and_secret_reference() {
        use smista_core::secret::SecretRef;

        let toml = r#"
            [providers.openai]
            type = "openai"
            base_url = "https://api.openai.com/v1"
            api_key = "${secret:openai_api_key}"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let openai = config.providers.get(&Provider::OpenAI).unwrap();
        assert_eq!(openai.kind, Provider::OpenAI);
        assert_eq!(
            openai.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            SecretRef::parse(openai.api_key.as_deref().unwrap()),
            Some(SecretRef::new("openai_api_key"))
        );
    }

    #[test]
    fn should_parse_model_capabilities() {
        let toml = r#"
            [models."openai/gpt-5.5-thinking"]
            provider = "openai"
            name = "gpt-5.5-thinking"
            requires_api_key = true
            local = false
            supports_streaming = true
            supports_tools = true
            supports_json_output = true
            supports_reasoning = true
            max_context_tokens = 200000
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let model = config.models.get("openai/gpt-5.5-thinking").unwrap();
        assert_eq!(model.provider, Provider::OpenAI);
        assert_eq!(model.name, "gpt-5.5-thinking");
        assert!(model.requires_api_key);
        assert!(model.supports_tools);
        assert_eq!(model.max_context_tokens, 200_000);
    }

    #[test]
    fn should_default_model_capability_flags_to_false() {
        let toml = r#"
            [models."ollama/qwen2.5-coder"]
            provider = "ollama"
            name = "qwen2.5-coder"
            local = true
            max_context_tokens = 32768
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let model = config.models.get("ollama/qwen2.5-coder").unwrap();
        assert!(model.local);
        assert!(!model.requires_api_key);
        assert!(!model.supports_tools);
        assert!(!model.supports_json_output);
    }

    #[test]
    fn should_default_auth_source_to_keychain() {
        assert_eq!(AuthSource::default(), AuthSource::Keychain);
    }

    #[test]
    fn should_parse_router_client_section() {
        let toml = r#"
            [router]
            url = "http://127.0.0.1:7331"
            auto_start = true
            auth_source = "env"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.router.url.as_deref(), Some("http://127.0.0.1:7331"));
        assert!(config.router.auto_start);
        assert_eq!(config.router.auth_source, Some(AuthSource::Env));
    }

    #[test]
    fn should_parse_local_preferences_section() {
        let toml = r#"
            [local_preferences]
            auto_apply = false
            stream = true
            local_only = false
            no_network = false
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.local.auto_apply, Some(false));
        assert_eq!(config.local.stream, Some(true));
    }

    #[test]
    fn should_leave_local_prefs_unset_by_default() {
        assert_eq!(Config::default().local, LocalPreferences::default());
        assert!(Config::default().local.auto_apply.is_none());
    }
}
