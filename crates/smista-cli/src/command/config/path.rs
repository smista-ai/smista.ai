//! executor for `smista config path`.

use std::path::{Path, PathBuf};

use crate::args::ConfigScope;

/// Prints one configuration path or the default path table.
///
/// With a scope, this prints only that scope's path for script-friendly use.
/// Without a scope, it lists the router, global CLI, and project CLI paths and
/// marks each one as existing or missing.
///
/// # Errors
///
/// Returns an error when a selected path cannot be resolved or when the current
/// directory cannot be read for the all-paths view.
pub fn run(path: Option<&Path>, scope: Option<ConfigScope>) -> anyhow::Result<()> {
    if let Some(scope) = scope {
        let path = super::resolve_config_path_by_scope(path, scope)?;
        tracing::debug!("resolved config path: {path}", path = path.display());
        println!("{path}", path = path.display());
        return Ok(());
    }

    if let Some(path) = path {
        println!("{path}", path = path.display());
        return Ok(());
    }

    let cwd = std::env::current_dir()?;
    for (label, path) in default_config_paths(&cwd)? {
        println!("{}", format_config_path_line(label, &path, path.exists()));
    }

    Ok(())
}

fn default_config_paths(cwd: &Path) -> anyhow::Result<Vec<(&'static str, PathBuf)>> {
    Ok(vec![
        (
            "router",
            super::default_config_path_by_scope(ConfigScope::Router, cwd)?,
        ),
        (
            "global",
            super::default_config_path_by_scope(ConfigScope::Global, cwd)?,
        ),
        (
            "project",
            super::default_config_path_by_scope(ConfigScope::Project, cwd)?,
        ),
    ])
}

fn format_config_path_line(label: &str, path: &Path, exists: bool) -> String {
    let state = if exists { "exists" } else { "missing" };
    format!("{label:<7} {path} ({state})", path = path.display())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn should_mark_an_existing_config_path() {
        let line = format_config_path_line("project", Path::new("/repo/.smista/config.toml"), true);

        assert!(line.contains("project"));
        assert!(line.contains("/repo/.smista/config.toml"));
        assert!(line.contains("exists"));
    }

    #[test]
    fn should_mark_a_missing_config_path() {
        let line = format_config_path_line(
            "router",
            Path::new("/home/user/.config/smista/router.toml"),
            false,
        );

        assert!(line.contains("router"));
        assert!(line.contains("missing"));
    }

    #[test]
    fn should_resolve_default_project_path_from_tempdir() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");

        let paths = default_config_paths(dir.path()).expect("failed to resolve default paths");

        let project_path = paths
            .into_iter()
            .find_map(|(label, path)| (label == "project").then_some(path))
            .expect("project path should be present");
        assert_eq!(project_path, dir.path().join(".smista").join("config.toml"));
    }

    #[test]
    fn should_print_explicit_tempfile_path_without_scope() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        run(Some(&path), None).expect("explicit path should print successfully");
    }

    #[test]
    fn should_print_scoped_tempfile_path() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[router]\nauto_start = true\n")
            .expect("failed to write sample CLI config");

        run(Some(&path), Some(ConfigScope::Project)).expect("scoped path should print");
    }
}
