//! Centralized, typed entrypoint for loading and merging CLI configuration.

use std::path::Path;

use super::layers::{ConfigLayer, merge};
use super::model::Config;
use super::paths::{global_config_toml, project_config_toml};

/// Errors raised while loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A config file existed but could not be read.
    #[error("failed to read config file {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: String,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// A config file was not valid TOML.
    #[error("invalid config file {path}: {source}")]
    Parse {
        /// Path that failed to parse.
        path: String,
        /// Underlying parse error.
        source: toml::de::Error,
    },
}

/// Parses a single `config.toml` from `contents`.
///
/// # Errors
///
/// Returns [`ConfigError::Parse`] if `contents` is not valid TOML. Never panics.
pub fn parse(contents: &str, path: &str) -> Result<Config, ConfigError> {
    toml::from_str(contents).map_err(|source| ConfigError::Parse {
        path: path.to_string(),
        source,
    })
}

/// Reads a `config.toml` from `path`, returning [`Config::default`] if absent.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] if the file exists but cannot be read, or
/// [`ConfigError::Parse`] if it is invalid TOML.
fn read_layer(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents, &path.display().to_string()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(source) => Err(ConfigError::Io {
            path: path.display().to_string(),
            source,
        }),
    }
}

/// Loads and merges the global and project configuration layers for `cwd`.
///
/// A `runtime` override, if given, is applied as the highest layer. Missing
/// files are treated as empty layers. The result is the fully merged [`Config`].
///
/// # Errors
///
/// Propagates [`ConfigError`] from any layer that exists but cannot be read or
/// parsed.
pub fn load(cwd: &Path, runtime: Option<Config>) -> Result<Config, ConfigError> {
    let mut layers = vec![(ConfigLayer::SystemDefaults, Config::default())];

    if let Some(global) = global_config_toml() {
        layers.push((ConfigLayer::Global, read_layer(&global)?));
    }
    layers.push((ConfigLayer::Project, read_layer(&project_config_toml(cwd))?));

    if let Some(runtime) = runtime {
        layers.push((ConfigLayer::RuntimeOverride, runtime));
    }

    Ok(merge(layers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_valid_toml() {
        let config = parse("[router]\nauto_start = true\n", "test").unwrap();
        assert!(config.router.auto_start);
    }

    #[test]
    fn should_return_parse_error_not_panic_on_invalid_toml() {
        let err = parse("this is = = invalid", "test").unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn should_default_when_no_files_present() {
        let dir = std::env::temp_dir().join(format!("smista-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // No global is created in temp; project file is absent under this cwd.
        let config = load(&dir, None).unwrap();
        assert_eq!(config.router, Default::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn should_apply_runtime_override_as_highest_layer() {
        let dir = std::env::temp_dir().join(format!("smista-cfg-rt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut runtime = Config::default();
        runtime.router.url = Some("http://runtime".to_string());
        let config = load(&dir, Some(runtime)).unwrap();
        assert_eq!(config.router.url.as_deref(), Some("http://runtime"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn should_parse_complete_example_fixture() {
        use smista_core::model::Provider;
        use smista_core::policy::PermissionMode;
        use smista_core::secret::SecretRef;

        let config = parse(
            include_str!("../../tests/fixtures/config.toml"),
            "config.toml",
        )
        .unwrap();

        // Providers and models.
        assert_eq!(config.providers.len(), 3);
        // Each authenticated provider references its key with the ${secret:NAME}
        // form; the OpenAI reference resolves the OPENAI_API_KEY env var first.
        assert_eq!(
            SecretRef::parse(
                config.providers[&Provider::OpenAI]
                    .api_key
                    .as_deref()
                    .unwrap()
            ),
            Some(SecretRef::new("OPENAI_API_KEY"))
        );
        assert_eq!(
            SecretRef::parse(
                config.providers[&Provider::Anthropic]
                    .api_key
                    .as_deref()
                    .unwrap()
            ),
            Some(SecretRef::new("anthropic"))
        );
        assert_eq!(
            config.models["openai/gpt-5.5-thinking"].max_context_tokens,
            200_000
        );

        // Routing: 4 rules plus a default route.
        assert_eq!(config.routing.rules.len(), 4);
        let default = config.routing.default.as_ref().unwrap();
        assert_eq!(default.model.to_string(), "openai/gpt-5.5-mini");
        let local_rule = &config.routing.rules[0];
        assert_eq!(local_rule.priority, 5);
        assert!(local_rule.local_only);

        // Classification.
        assert_eq!(config.classification.rules.len(), 2);

        // Tool permissions and privacy.
        assert_eq!(config.tools.mode_for("network"), Some(PermissionMode::Deny));
        assert_eq!(config.privacy.remote.mode(), PermissionMode::Ask);
        assert_eq!(config.privacy.local.mode(), PermissionMode::Allow);

        // Router client and local preferences.
        assert_eq!(config.router.connect_timeout_ms, Some(5000));
        assert_eq!(config.local.stream, Some(true));
    }
}
