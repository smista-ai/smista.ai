//! Centralized, typed entrypoint for loading router runtime configuration.

use std::path::Path;

use serde::Deserialize;

use super::model::RouterConfig;

/// The on-disk `router.toml` document.
///
/// The runtime configuration lives under a top-level `[router]` table, so the
/// file is parsed through this wrapper and the inner [`RouterConfig`] is
/// returned. A missing `[router]` table yields the default configuration.
#[derive(Debug, Default, Deserialize)]
struct RouterDocument {
    #[serde(default)]
    router: RouterConfig,
}

/// Errors raised while loading router configuration.
#[derive(Debug, thiserror::Error)]
pub enum RouterConfigError {
    /// The config file existed but could not be read.
    #[error("failed to read router config {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: String,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// The config file was not valid TOML.
    #[error("invalid router config {path}: {source}")]
    Parse {
        /// Path that failed to parse.
        path: String,
        /// Underlying parse error.
        source: toml::de::Error,
    },
}

/// Parses a `router.toml` from `contents`.
///
/// # Errors
///
/// Returns [`RouterConfigError::Parse`] if `contents` is not valid TOML. Never
/// panics.
pub fn parse(contents: &str, path: &str) -> Result<RouterConfig, RouterConfigError> {
    tracing::trace!(config.path = %path, "parsing router config {{config.path}}");
    toml::from_str::<RouterDocument>(contents)
        .map(|document| document.router)
        .map_err(|source| {
            tracing::error!(
                config.path = %path,
                error.message = %source,
                "failed to parse router config {{config.path}}"
            );
            RouterConfigError::Parse {
                path: path.to_string(),
                source,
            }
        })
}

/// Reads `router.toml` from `path`, returning [`RouterConfig::default`] if absent.
///
/// # Errors
///
/// Returns [`RouterConfigError::Io`] if the file exists but cannot be read, or
/// [`RouterConfigError::Parse`] if it is invalid TOML.
pub fn load(path: &Path) -> Result<RouterConfig, RouterConfigError> {
    tracing::debug!(config.path = %path.display(), "loading router config {{config.path}}");
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let config = parse(&contents, &path.display().to_string())?;
            tracing::debug!(config.path = %path.display(), "loaded router config {{config.path}}");
            Ok(config)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(
                config.path = %path.display(),
                "router config {{config.path}} not found; using defaults"
            );
            Ok(RouterConfig::default())
        }
        Err(source) => {
            tracing::error!(
                config.path = %path.display(),
                error.message = %source,
                "failed to read router config {{config.path}}"
            );
            Err(RouterConfigError::Io {
                path: path.display().to_string(),
                source,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_valid_router_toml() {
        let config = parse("[router]\nport = 9000\n", "test").unwrap();
        assert_eq!(config.port, 9000);
    }

    #[test]
    fn should_default_when_router_table_absent() {
        let config = parse("", "test").unwrap();
        assert_eq!(config, RouterConfig::default());
    }

    #[test]
    fn should_return_parse_error_not_panic_on_invalid_toml() {
        let err = parse("== nope ==", "test").unwrap_err();
        assert!(matches!(err, RouterConfigError::Parse { .. }));
    }

    #[test]
    fn should_default_when_file_absent() {
        let config = load(Path::new("/no/such/router.toml")).unwrap();
        assert_eq!(config, RouterConfig::default());
    }

    #[test]
    fn should_parse_complete_example_fixture() {
        let config = parse(
            include_str!("../../tests/fixtures/router.toml"),
            "router.toml",
        )
        .unwrap();

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 7331);
        assert_eq!(config.storage.database, "local");
        assert_eq!(config.storage.path.as_deref(), Some(".smista/db"));
        assert_eq!(config.auth.token_ttl_seconds, 86_400);
        assert_eq!(config.limits.max_request_body_bytes, 10_485_760);
        assert_eq!(config.limits.provider_timeout_ms, 180_000);
        assert_eq!(config.logging.format, "compact");
        assert!(config.logging.redact_secrets);
        assert!(config.ollama.enabled);
        assert_eq!(config.ollama.models.preload.len(), 2);
        assert!(!config.ollama.models.allow_pull);
    }
}
