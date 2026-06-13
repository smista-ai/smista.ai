//! Router runtime configuration types and their defaults.

use std::collections::BTreeMap;
use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use smista_core::model::Provider;
use url::Url;

/// The default [`RouterConfig::host`] value: the IPv4 loopback address.
const DEFAULT_HOST: &str = "127.0.0.1";
/// The default [`RouterConfig::port`] value.
const DEFAULT_PORT: u16 = 7331;

/// The default [`RouterAuthConfig::token_ttl_seconds`] value: 1 day.
const DEFAULT_TOKEN_TTL_SECONDS: u64 = 86_400;
/// The default [`RouterAuthConfig::api_key_version`] value.
const DEFAULT_API_KEY_VERSION: &str = "01";

/// The default [`StorageConfig::namespace`] value.
const DEFAULT_STORAGE_NAMESPACE: &str = "smista";
/// The default [`StorageConfig::database`] value.
const DEFAULT_STORAGE_DATABASE: &str = "local";

/// The default [`RouterLimits::max_request_body_bytes`] value: 10 MiB.
const DEFAULT_MAX_REQUEST_BODY_BYTES: u64 = 10_485_760;
/// The default [`RouterLimits::max_context_bytes`] value: 5 MiB.
const DEFAULT_MAX_CONTEXT_BYTES: u64 = 5_242_880;
/// The default [`RouterLimits::max_concurrent_requests`] value.
const DEFAULT_MAX_CONCURRENT_REQUESTS: u32 = 8;
/// The default [`RouterLimits::request_timeout_ms`] value: 2 minutes.
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 120_000;
/// The default [`RouterLimits::provider_timeout_ms`] value: 3 minutes.
const DEFAULT_PROVIDER_TIMEOUT_MS: u64 = 180_000;
/// The default [`RouterLimits::tool_timeout_ms`] value: 1 minute.
const DEFAULT_TOOL_TIMEOUT_MS: u64 = 60_000;

/// The default [`LoggingConfig::level`] value.
const DEFAULT_LOG_LEVEL: &str = "info";
/// The default [`LoggingConfig::format`] value.
const DEFAULT_LOG_FORMAT: &str = "compact";

/// The default [`RetentionConfig::trace_retention_days`] value: 90 days.
const DEFAULT_TRACE_RETENTION_DAYS: u32 = 90;
/// The default [`RetentionConfig::session_retention_days`] value: 1 year.
const DEFAULT_SESSION_RETENTION_DAYS: u32 = 365;
/// The default [`RetentionConfig::archived_session_retention_days`] value: 30 days.
const DEFAULT_ARCHIVED_SESSION_RETENTION_DAYS: u32 = 30;
/// The default [`RetentionConfig::cleanup_interval_seconds`] value: 1 hour.
const DEFAULT_CLEANUP_INTERVAL_SECONDS: u64 = 3_600;

/// The default [`OllamaConfig::base_url`] value: the local Ollama endpoint.
const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";
/// The default [`OllamaConfig::model_refresh_interval_seconds`] value: 5 minutes.
const DEFAULT_OLLAMA_MODEL_REFRESH_INTERVAL_SECONDS: u64 = 300;

/// The default [`OllamaLimits::max_concurrent_requests`] value.
const DEFAULT_OLLAMA_MAX_CONCURRENT_REQUESTS: u32 = 4;
/// The default [`OllamaLimits::request_timeout_ms`] value: 3 minutes.
const DEFAULT_OLLAMA_REQUEST_TIMEOUT_MS: u64 = 180_000;
/// The default [`OllamaLimits::pull_timeout_ms`] value: 10 minutes.
const DEFAULT_OLLAMA_PULL_TIMEOUT_MS: u64 = 600_000;

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
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
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
    /// Whether the provider runs locally.
    ///
    /// Defaults to `false`. The router surfaces this to consumers so a local
    /// provider can be told apart from a hosted one.
    pub local: bool,
    /// Human-readable display name for the provider, if any.
    ///
    /// When omitted, consumers fall back to the provider's own identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Models advertised for a generic OpenAI-compatible endpoint that exposes
    /// no model-listing API.
    ///
    /// Only consulted for an `openai-compat:<name>` provider, and only as a
    /// fallback when the endpoint reports no models of its own. The built-in
    /// providers and Ollama discover their models at runtime and ignore this.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
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
///
/// `PartialEq`/`Eq` are implemented by hand because [`SecretString`] does not
/// expose a comparison, and `password` is `#[serde(skip_serializing)]` so the
/// credential is never written back out to a config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Storage engine.
    pub engine: StorageEngine,
    /// Deployment mode.
    pub mode: StorageMode,
    /// Database file path (embedded mode).
    pub path: Option<PathBuf>,
    /// Database URL (remote mode).
    pub url: Option<Url>,
    /// Authentication username (remote mode).
    pub username: Option<String>,
    /// Authentication password (remote mode); never serialized.
    #[serde(skip_serializing)]
    pub password: Option<SecretString>,
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
            username: None,
            password: None,
            namespace: DEFAULT_STORAGE_NAMESPACE.to_string(),
            database: DEFAULT_STORAGE_DATABASE.to_string(),
        }
    }
}

impl PartialEq for StorageConfig {
    fn eq(&self, other: &Self) -> bool {
        self.engine == other.engine
            && self.mode == other.mode
            && self.path == other.path
            && self.url == other.url
            && self.username == other.username
            && self.namespace == other.namespace
            && self.database == other.database
            && self.password.as_ref().map(ExposeSecret::expose_secret)
                == other.password.as_ref().map(ExposeSecret::expose_secret)
    }
}

impl Eq for StorageConfig {}

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
            token_ttl_seconds: DEFAULT_TOKEN_TTL_SECONDS,
            api_key_version: DEFAULT_API_KEY_VERSION.to_string(),
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
            max_request_body_bytes: DEFAULT_MAX_REQUEST_BODY_BYTES,
            max_context_bytes: DEFAULT_MAX_CONTEXT_BYTES,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
            request_timeout_ms: DEFAULT_REQUEST_TIMEOUT_MS,
            provider_timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
            tool_timeout_ms: DEFAULT_TOOL_TIMEOUT_MS,
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
            level: DEFAULT_LOG_LEVEL.to_string(),
            format: DEFAULT_LOG_FORMAT.to_string(),
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
    /// Days to retain archived sessions before purge.
    pub archived_session_retention_days: u32,
    /// Cleanup interval, in seconds.
    pub cleanup_interval_seconds: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            trace_retention_days: DEFAULT_TRACE_RETENTION_DAYS,
            session_retention_days: DEFAULT_SESSION_RETENTION_DAYS,
            archived_session_retention_days: DEFAULT_ARCHIVED_SESSION_RETENTION_DAYS,
            cleanup_interval_seconds: DEFAULT_CLEANUP_INTERVAL_SECONDS,
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
    pub base_url: Url,
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
            base_url: DEFAULT_OLLAMA_BASE_URL
                .to_string()
                .parse()
                .expect("invalid default URL"),
            auto_discover_models: true,
            startup_healthcheck: true,
            startup_required: false,
            model_refresh_interval_seconds: DEFAULT_OLLAMA_MODEL_REFRESH_INTERVAL_SECONDS,
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
            max_concurrent_requests: DEFAULT_OLLAMA_MAX_CONCURRENT_REQUESTS,
            request_timeout_ms: DEFAULT_OLLAMA_REQUEST_TIMEOUT_MS,
            pull_timeout_ms: DEFAULT_OLLAMA_PULL_TIMEOUT_MS,
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
    fn should_set_retention_defaults() {
        let retention = RetentionConfig::default();
        assert_eq!(retention.trace_retention_days, 90);
        assert_eq!(retention.session_retention_days, 365);
        assert_eq!(retention.archived_session_retention_days, 30);
        assert_eq!(retention.cleanup_interval_seconds, 3_600);
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
    fn should_parse_remote_storage_credentials() {
        let toml = r#"
            [storage]
            mode = "remote"
            url = "ws://db.internal:8000"
            username = "root"
            password = "s3cret"
        "#;
        let config: RouterConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.storage.username.as_deref(), Some("root"));
        assert_eq!(
            config
                .storage
                .password
                .as_ref()
                .map(ExposeSecret::expose_secret),
            Some("s3cret")
        );
    }

    #[test]
    fn should_never_serialize_storage_password() {
        let config = StorageConfig {
            password: Some(SecretString::from("s3cret")),
            ..StorageConfig::default()
        };
        let toml = toml::to_string(&config).unwrap();
        assert!(!toml.contains("password"));
        assert!(!toml.contains("s3cret"));
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
    fn should_parse_openai_compatible_provider_with_models() {
        let toml = r#"
            [providers."openai-compat:my-vllm"]
            base_url = "http://localhost:8000/v1"
            models = ["llama-3.1-70b"]
        "#;
        let config: RouterConfig = toml::from_str(toml).unwrap();
        let key = Provider::OpenAICompatible("my-vllm".to_string());
        assert_eq!(
            config.providers[&key].base_url.as_deref(),
            Some("http://localhost:8000/v1")
        );
        assert_eq!(config.providers[&key].models, vec!["llama-3.1-70b"]);
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
        // Untouched nested defaults remain. `Url` normalizes an authority-only
        // URL with a trailing slash, so the rendered default carries one.
        assert_eq!(
            config.ollama.base_url.to_string(),
            "http://127.0.0.1:11434/"
        );
    }
}
