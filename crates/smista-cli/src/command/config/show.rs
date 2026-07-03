//! executor for `smista config show`.

use std::path::Path;

use crate::args::ConfigScope;

const REDACTED: &str = "[redacted]";

/// Prints the effective configuration or one redacted configuration layer.
///
/// Without a scope, the command prints the effective merged CLI configuration.
/// With a scope, it reads that single layer and prints a redacted TOML document.
///
/// # Errors
///
/// Returns an error when a path cannot be resolved, a selected file cannot be
/// read, or a configuration document cannot be parsed or rendered safely.
pub fn run(path: Option<&Path>, scope: Option<ConfigScope>) -> anyhow::Result<()> {
    let output = match scope {
        Some(scope) => render_config_layer(path, scope)?,
        None => render_effective_config(path)?,
    };
    println!("{output}");

    Ok(())
}

fn render_effective_config(path: Option<&Path>) -> anyhow::Result<String> {
    if let Some(path) = path {
        let config = crate::config::load_and_validate_at(path)?;
        return render_cli_config(&config);
    }

    let cwd = std::env::current_dir()?;
    let config = crate::config::load_effective(&cwd)?;
    render_cli_config(&config)
}

fn render_config_layer(path: Option<&Path>, scope: ConfigScope) -> anyhow::Result<String> {
    let path = super::resolve_config_path_by_scope(path, scope)?;
    tracing::debug!("resolved config path: {path}", path = path.display());
    let contents = std::fs::read_to_string(&path).map_err(|source| {
        anyhow::anyhow!(
            "failed to read config file {path}: {source}",
            path = path.display()
        )
    })?;
    redacted_config_document(&contents)
}

fn render_cli_config(config: &crate::config::Config) -> anyhow::Result<String> {
    let mut redacted = config.clone();
    for provider in redacted.providers.values_mut() {
        if provider.api_key.is_some() {
            provider.api_key = Some(REDACTED.to_string());
        }
    }
    Ok(toml::to_string_pretty(&redacted)?)
}

fn redacted_config_document(contents: &str) -> anyhow::Result<String> {
    let mut document = toml::from_str::<toml::Value>(contents)?;
    redact_value(&mut document);
    Ok(toml::to_string_pretty(&document)?)
}

fn redact_value(value: &mut toml::Value) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                if is_sensitive_key(key) {
                    *value = toml::Value::String(REDACTED.to_string());
                } else {
                    redact_value(value);
                }
            }
        }
        toml::Value::Array(values) => {
            for value in values {
                redact_value(value);
            }
        }
        toml::Value::String(_)
        | toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "api_key" || key == "password" || key == "secret" || key.ends_with("_secret")
}

#[cfg(test)]
mod tests {
    use smista_sdk::core::model::Provider;

    use super::*;

    #[test]
    fn should_redact_api_keys_from_raw_config_documents() {
        let output = redacted_config_document(
            r#"
                [providers.openai]
                api_key = "sk-live-secret"
            "#,
        )
        .expect("failed to render redacted config document");

        assert!(output.contains("api_key"));
        assert!(output.contains("[redacted]"));
        assert!(!output.contains("sk-live-secret"));
    }

    #[test]
    fn should_redact_api_keys_from_effective_cli_config() {
        let mut config = crate::config::Config::default();
        config
            .providers
            .get_mut(&Provider::Ollama)
            .expect("default Ollama provider should exist")
            .api_key = Some("sk-live-secret".to_string());

        let output = render_cli_config(&config).expect("failed to render CLI config");

        assert!(output.contains("api_key"));
        assert!(output.contains("[redacted]"));
        assert!(!output.contains("sk-live-secret"));
    }

    #[test]
    fn should_redact_nested_sensitive_keys_case_insensitively() {
        let output = redacted_config_document(
            r#"
                [storage]
                Password = "database-secret"
            "#,
        )
        .expect("failed to render redacted config document");

        assert!(output.contains("[redacted]"));
        assert!(!output.contains("database-secret"));
    }

    #[test]
    fn should_show_effective_config_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        run(Some(&path), None).expect("effective config should render");
    }

    #[test]
    fn should_show_redacted_layer_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
            [providers.openai]
            api_key = "sk-live-secret"
            "#,
        )
        .expect("failed to write sample CLI config");

        run(Some(&path), Some(ConfigScope::Project)).expect("layer config should render");
    }

    #[test]
    fn should_report_missing_layer_file_from_tempfile_path() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("missing.toml");

        let error = run(Some(&path), Some(ConfigScope::Project)).unwrap_err();

        assert!(error.to_string().contains("failed to read config file"));
    }
}
