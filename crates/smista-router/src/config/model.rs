//! Router runtime configuration types and their defaults.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smista_core::model::Provider;

/// Top-level router runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterConfig {
    /// Bind host.
    pub host: String,
    /// Bind port.
    pub port: u16,
    /// Storage backend configuration.
    pub storage: StorageConfig,
    /// Authentication configuration.
    pub auth: RouterAuthConfig,
    /// Request and execution limits.
    pub limits: RouterLimits,
    /// Logging configuration.
    pub logging: LoggingConfig,
    /// CORS configuration.
    pub cors: CorsConfig,
    /// Data-retention configuration.
    pub retention: RetentionConfig,
    /// Ollama backend configuration.
    pub ollama: OllamaConfig,
    /// Per-provider endpoint configuration (e.g. a custom OpenAI base URL).
    pub providers: BTreeMap<Provider, RouterProviderConfig>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 7331,
            storage: StorageConfig::default(),
            auth: RouterAuthConfig::default(),
            limits: RouterLimits::default(),
            logging: LoggingConfig::default(),
            cors: CorsConfig::default(),
            retention: RetentionConfig::default(),
            ollama: OllamaConfig::default(),
            providers: BTreeMap::new(),
        }
    }
}

/// Per-provider endpoint configuration on the router.
///
/// The router is what connects to providers, so endpoint details live here, not
/// in the CLI. An absent `base_url` means the provider's fixed default endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterProviderConfig {
    /// Base URL for the provider endpoint, if non-default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Storage engine identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageEngine {
    /// SurrealDB.
    #[default]
    Surrealdb,
}

/// Storage deployment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    /// Embedded, on-disk database.
    #[default]
    Embedded,
    /// Remote database server.
    Remote,
}

/// Storage backend configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Storage engine.
    pub engine: StorageEngine,
    /// Deployment mode.
    pub mode: StorageMode,
    /// Database file path (embedded mode).
    pub path: Option<String>,
    /// Database URL (remote mode).
    pub url: Option<String>,
    /// Namespace.
    pub namespace: String,
    /// Database name.
    pub database: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            engine: StorageEngine::default(),
            mode: StorageMode::default(),
            path: None,
            url: None,
            namespace: "smista".to_string(),
            database: "local".to_string(),
        }
    }
}

/// Authentication configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterAuthConfig {
    /// Session token time-to-live, in seconds.
    pub token_ttl_seconds: u64,
    /// API key version segment (e.g. `01`).
    pub api_key_version: String,
    /// Whether local API-key bootstrap is enabled.
    pub local_bootstrap_enabled: bool,
}

impl Default for RouterAuthConfig {
    fn default() -> Self {
        Self {
            token_ttl_seconds: 86_400,
            api_key_version: "01".to_string(),
            local_bootstrap_enabled: true,
        }
    }
}

/// Request and execution limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouterLimits {
    /// Maximum request body size, in bytes.
    pub max_request_body_bytes: u64,
    /// Maximum context size, in bytes.
    pub max_context_bytes: u64,
    /// Maximum concurrent requests.
    pub max_concurrent_requests: u32,
    /// Overall request timeout, in milliseconds.
    pub request_timeout_ms: u64,
    /// Provider call timeout, in milliseconds.
    pub provider_timeout_ms: u64,
    /// Tool execution timeout, in milliseconds.
    pub tool_timeout_ms: u64,
}

impl Default for RouterLimits {
    fn default() -> Self {
        Self {
            max_request_body_bytes: 10_485_760,
            max_context_bytes: 5_242_880,
            max_concurrent_requests: 8,
            request_timeout_ms: 120_000,
            provider_timeout_ms: 180_000,
            tool_timeout_ms: 60_000,
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level filter.
    pub level: String,
    /// Log output format.
    pub format: String,
    /// Whether to redact secrets from logs.
    pub redact_secrets: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "compact".to_string(),
            redact_secrets: true,
        }
    }
}

/// CORS configuration. Disabled by default.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Whether CORS is enabled.
    pub enabled: bool,
    /// Allowed origins when enabled.
    pub allowed_origins: Vec<String>,
}

/// Data-retention configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetentionConfig {
    /// Days to retain traces.
    pub trace_retention_days: u32,
    /// Days to retain sessions.
    pub session_retention_days: u32,
    /// Days to retain deleted sessions before purge.
    pub deleted_session_retention_days: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            trace_retention_days: 90,
            session_retention_days: 365,
            deleted_session_retention_days: 30,
        }
    }
}

/// Ollama backend configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    /// Whether the Ollama backend is enabled.
    pub enabled: bool,
    /// Ollama base URL.
    pub base_url: String,
    /// Whether to auto-discover installed models.
    pub auto_discover_models: bool,
    /// Whether to health-check Ollama at startup.
    pub startup_healthcheck: bool,
    /// Whether a failed startup health-check aborts startup.
    pub startup_required: bool,
    /// Model list refresh interval, in seconds.
    pub model_refresh_interval_seconds: u64,
    /// Concurrency and timeout limits.
    pub limits: OllamaLimits,
    /// Model allow/preload configuration.
    pub models: OllamaModels,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:11434".to_string(),
            auto_discover_models: true,
            startup_healthcheck: true,
            startup_required: false,
            model_refresh_interval_seconds: 300,
            limits: OllamaLimits::default(),
            models: OllamaModels::default(),
        }
    }
}

/// Ollama concurrency and timeout limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaLimits {
    /// Maximum concurrent Ollama requests.
    pub max_concurrent_requests: u32,
    /// Request timeout, in milliseconds.
    pub request_timeout_ms: u64,
    /// Model pull timeout, in milliseconds.
    pub pull_timeout_ms: u64,
}

impl Default for OllamaLimits {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 4,
            request_timeout_ms: 180_000,
            pull_timeout_ms: 600_000,
        }
    }
}

/// Ollama model preload/allow configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaModels {
    /// Models pulled/warmed at startup.
    pub preload: Vec<String>,
    /// Whether the router may pull models on demand.
    pub allow_pull: bool,
    /// Allowed models; empty means all discovered models are allowed.
    pub allowed_models: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_host_and_port() {
        let config = RouterConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 7331);
    }

    #[test]
    fn should_disable_cors_by_default() {
        assert!(!CorsConfig::default().enabled);
    }

    #[test]
    fn should_redact_secrets_by_default() {
        assert!(LoggingConfig::default().redact_secrets);
    }

    #[test]
    fn should_set_ollama_safe_defaults() {
        let ollama = OllamaConfig::default();
        assert!(!ollama.enabled);
        assert!(ollama.auto_discover_models);
        assert!(ollama.startup_healthcheck);
        assert!(!ollama.startup_required);
        assert!(!ollama.models.allow_pull);
        assert!(ollama.models.allowed_models.is_empty());
    }

    #[test]
    fn should_deserialize_empty_to_default() {
        let config: RouterConfig = toml::from_str("").unwrap();
        assert_eq!(config, RouterConfig::default());
    }

    #[test]
    fn should_roundtrip_full_config() {
        let config = RouterConfig::default();
        let toml = toml::to_string(&config).unwrap();
        assert_eq!(toml::from_str::<RouterConfig>(&toml).unwrap(), config);
    }

    #[test]
    fn should_default_to_empty_providers() {
        assert!(RouterConfig::default().providers.is_empty());
    }

    #[test]
    fn should_parse_provider_base_url_override() {
        let toml = r#"
            [providers.openai]
            base_url = "https://proxy.internal/v1"
        "#;
        let config: RouterConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            config.providers[&Provider::OpenAI].base_url.as_deref(),
            Some("https://proxy.internal/v1")
        );
    }

    #[test]
    fn should_override_nested_ollama_value() {
        let toml = r#"
            [ollama]
            enabled = true
            [ollama.models]
            allow_pull = true
        "#;
        let config: RouterConfig = toml::from_str(toml).unwrap();
        assert!(config.ollama.enabled);
        assert!(config.ollama.models.allow_pull);
        // Untouched nested defaults remain.
        assert_eq!(config.ollama.base_url, "http://127.0.0.1:11434");
    }
}
