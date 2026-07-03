//! executor for `smista config check`.

use std::path::Path;

use crate::args::ConfigScope;

/// Validates the selected configuration file.
///
/// Router configuration is validated with the router validator. Global and
/// project CLI configuration are validated with the CLI validator.
///
/// # Errors
///
/// Returns an error when the path cannot be resolved, the file cannot be read or
/// parsed, or validation reports errors.
pub fn run(path: Option<&Path>, scope: ConfigScope) -> anyhow::Result<()> {
    let path = super::resolve_config_path_by_scope(path, scope)?;
    tracing::debug!("resolved config path: {path}", path = path.display());

    if matches!(scope, ConfigScope::Router) {
        validate_router_config(&path)
    } else {
        validate_cli_config(&path)
    }
}

fn validate_router_config(path: &Path) -> anyhow::Result<()> {
    tracing::debug!(
        "validating router configuration at {path}",
        path = path.display()
    );

    let config = smista_router::config::load(path)?;
    let validation_report = smista_router::config::validate::validate(&config);
    if !validation_report.is_ok() {
        anyhow::bail!(
            "router configuration validation failed: {report}",
            report = validation_report.to_human()
        );
    }

    if validation_report.warnings().is_empty() {
        println!("Router configuration is valid.");
    } else {
        for warning in validation_report.warnings() {
            println!("Warning: {warning}", warning = warning.to_human());
        }
    }

    Ok(())
}

fn validate_cli_config(path: &Path) -> anyhow::Result<()> {
    tracing::debug!(
        "validating CLI configuration at {path}",
        path = path.display()
    );

    crate::config::load_and_validate_at(path).map(|_| ())?;

    println!("CLI configuration is valid.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_check_project_config_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        run(Some(&path), ConfigScope::Project).expect("project config should be valid");
    }

    #[test]
    fn should_check_router_config_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.toml");
        std::fs::write(&path, "[router]\nport = 7331\n")
            .expect("failed to write sample router config");

        run(Some(&path), ConfigScope::Router).expect("router config should be valid");
    }

    #[test]
    fn should_reject_invalid_project_config_from_tempfile() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
            [providers.openai]
            type = "openai"
            api_key = "sk-literal-value"
            "#,
        )
        .expect("failed to write invalid CLI config");

        let error = run(Some(&path), ConfigScope::Project).unwrap_err();

        assert!(error.to_string().contains("validation failed"));
    }
}
