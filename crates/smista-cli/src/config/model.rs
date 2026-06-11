//! The aggregate CLI configuration and its client-side sections.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smista_sdk::core::model::Provider;
use smista_sdk::core::policy::{ClassificationConfig, PrivacyPolicy, RoutingPolicy, ToolsConfig};

/// The merged CLI/policy configuration loaded from `config.toml`.
///
/// Every section defaults to empty so a missing or partial file is valid; the
/// layered merge (`super::layers`) combines layers per section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Configured providers, keyed by provider identity.
    pub providers: BTreeMap<Provider, ProviderConfig>,
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
/// Mirrors a `[providers.<id>]` table, keyed by the provider identity (e.g.
/// `openai`, or `openai-compat:my-vllm` for a named OpenAI-compatible instance).
/// The optional `type` key names the provider kind; it is redundant with the
/// table key and may be omitted, which is the norm for `openai-compat:<name>`
/// instances. The credential lives in `api_key`, which holds a `${secret:NAME}`
/// reference resolved against the secret sources (see `super::secrets`) — an
/// environment variable named `NAME` first, then the `.smista/secrets` files —
/// or, where allowed, an inline literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider kind (serialized as `type`). Optional and redundant with the
    /// table key; omit it for `openai-compat:<name>` instances.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<Provider>,
    /// API key value: a `${secret:NAME}` reference or, where allowed, a literal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
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
        use smista_sdk::core::secret::SecretRef;

        let toml = r#"
            [providers.openai]
            type = "openai"
            api_key = "${secret:openai_api_key}"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let openai = config.providers.get(&Provider::OpenAI).unwrap();
        assert_eq!(openai.kind, Some(Provider::OpenAI));
        assert_eq!(
            SecretRef::parse(openai.api_key.as_deref().unwrap()),
            Some(SecretRef::new("openai_api_key"))
        );
    }

    #[test]
    fn should_parse_openai_compatible_instance_keyed_by_identity() {
        // A named instance is keyed by its full identity; `type` is omitted.
        let toml = r#"
            [providers."openai-compat:my-vllm"]
            api_key = "${secret:my_vllm_key}"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let key = Provider::OpenAICompatible("my-vllm".to_string());
        let instance = config.providers.get(&key).unwrap();
        assert!(instance.kind.is_none());
        assert_eq!(instance.api_key.as_deref(), Some("${secret:my_vllm_key}"));
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
