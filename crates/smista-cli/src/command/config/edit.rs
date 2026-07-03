//! executor for `smista config edit`.

use std::path::Path;
use std::process::Command;

use crate::args::ConfigScope;

/// Opens the selected configuration file in the user's editor.
///
/// `VISUAL` takes precedence over `EDITOR`; when neither is set, the platform
/// opener from the `open` crate is used as the default.
///
/// # Errors
///
/// Returns an error when the path cannot be resolved, the file does not exist,
/// or the configured editor/opener cannot be launched successfully.
pub fn run(path: Option<&Path>, scope: ConfigScope) -> anyhow::Result<()> {
    run_with_editor(path, scope, selected_editor_from_env())
}

fn run_with_editor(
    path: Option<&Path>,
    scope: ConfigScope,
    editor: Option<String>,
) -> anyhow::Result<()> {
    let path = super::resolve_config_path_by_scope(path, scope)?;
    tracing::debug!("opening config at path: {path}", path = path.display());
    ensure_existing_config(&path, scope)?;

    open_config(&path, editor)?;

    println!("Config file opened in default editor.");

    Ok(())
}

fn ensure_existing_config(path: &Path, scope: ConfigScope) -> anyhow::Result<()> {
    if path.exists() {
        return Ok(());
    }

    anyhow::bail!(
        "configuration file does not exist at {path}; run `smista config init {scope}` to create it",
        path = path.display(),
        scope = scope.value_name()
    );
}

fn selected_editor_from_env() -> Option<String> {
    selected_editor(
        std::env::var("VISUAL").ok().as_deref(),
        std::env::var("EDITOR").ok().as_deref(),
    )
}

fn selected_editor(visual: Option<&str>, editor: Option<&str>) -> Option<String> {
    [visual, editor]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn open_config(path: &Path, editor: Option<String>) -> anyhow::Result<()> {
    let Some(editor) = editor else {
        open::that(path)?;
        return Ok(());
    };

    let mut parts = editor.split_whitespace();
    let command = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("editor command is empty"))?;
    let status = Command::new(command).args(parts).arg(path).status()?;
    if !status.success() {
        anyhow::bail!("editor exited with status {status}");
    }
    Ok(())
}

impl ConfigScope {
    fn value_name(self) -> &'static str {
        match self {
            Self::Router => "router",
            Self::Global => "global",
            Self::Project => "project",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_point_missing_edit_targets_at_config_init() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");

        let error = ensure_existing_config(&path, ConfigScope::Project).unwrap_err();

        assert!(error.to_string().contains("does not exist"));
        assert!(error.to_string().contains("smista config init project"));
    }

    #[test]
    fn should_use_visual_before_editor() {
        let editor = selected_editor(Some("code --wait"), Some("vim"));

        assert_eq!(editor.as_deref(), Some("code --wait"));
    }

    #[test]
    fn should_use_editor_when_visual_is_empty() {
        let editor = selected_editor(Some(" "), Some("vim"));

        assert_eq!(editor.as_deref(), Some("vim"));
    }

    #[test]
    fn should_edit_existing_config_from_tempfile_with_injected_editor() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        run_with_editor(Some(&path), ConfigScope::Project, Some("true".to_string()))
            .expect("editor command should succeed");
    }

    #[test]
    fn should_report_failing_injected_editor() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        let error = run_with_editor(Some(&path), ConfigScope::Project, Some("false".to_string()))
            .unwrap_err();

        assert!(error.to_string().contains("editor exited"));
    }
}
