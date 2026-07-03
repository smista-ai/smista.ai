//! Config-file scaffolding for `smista config init`.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::args::ConfigScope;

/// Creates a starter configuration file for `scope`.
///
/// When `path` is absent, the default location for the selected configuration
/// scope is used. Project initialization also ensures `.smista/.gitignore`
/// ignores the local `secrets` file.
///
/// # Errors
///
/// Returns an error when the default path cannot be resolved, the target file
/// exists and `force` is `false`, the parent directory cannot be created, or the
/// starter template cannot be serialized or written.
pub fn init(scope: ConfigScope, path: Option<&Path>, force: bool) -> anyhow::Result<()> {
    let path = super::resolve_config_path_by_scope(path, scope)?;

    match scope {
        ConfigScope::Router => init_router(path, force),
        ConfigScope::Global => init_global(path, force),
        ConfigScope::Project => init_project(path, force),
    }
}

fn init_router(path: PathBuf, force: bool) -> anyhow::Result<()> {
    tracing::debug!(
        "Initializing router configuration at {path}",
        path = path.display()
    );

    let toml = router_template()?;
    tracing::debug!(
        "Writing router configuration to {path}",
        path = path.display()
    );
    write_config_file(&path, &toml, force)?;

    println!(
        "Router configuration initialized at {path}",
        path = path.display()
    );

    Ok(())
}

fn init_global(path: PathBuf, force: bool) -> anyhow::Result<()> {
    tracing::debug!(
        "Initializing global CLI configuration at {path}",
        path = path.display()
    );

    init_cli_config(&path, force)?;

    println!(
        "Global CLI configuration initialized at {path}",
        path = path.display()
    );

    Ok(())
}

fn init_project(path: PathBuf, force: bool) -> anyhow::Result<()> {
    tracing::debug!(
        "Initializing project CLI configuration at {path}",
        path = path.display()
    );

    let parent = path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine parent directory of project configuration file {path}",
            path = path.display()
        )
    })?;
    init_cli_config(&path, force)?;

    let gitignore = parent.join(".gitignore");
    tracing::debug!(
        "Ensuring project secrets are ignored at {gitignore}",
        gitignore = gitignore.display()
    );
    ensure_project_gitignore(&gitignore)?;

    println!(
        "Project CLI configuration initialized at {path}",
        path = path.display()
    );

    Ok(())
}

fn init_cli_config(path: &Path, force: bool) -> anyhow::Result<()> {
    let toml = cli_template()?;
    tracing::debug!("Writing CLI configuration to {path}", path = path.display());
    write_config_file(path, &toml, force)
}

fn cli_template() -> anyhow::Result<String> {
    let config = crate::config::Config::default();
    let toml = toml::to_string_pretty(&config)?;

    Ok(format!(
        "# Starter smista.ai CLI configuration.\n\
         # Edit this file to configure routing, providers, privacy, tools, and router access.\n\n\
         {toml}"
    ))
}

fn router_template() -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct RouterDocument<'a> {
        router: &'a smista_router::config::RouterConfig,
    }

    let config = smista_router::config::RouterConfig::default();
    let toml = toml::to_string_pretty(&RouterDocument { router: &config })?;

    Ok(format!(
        "# Starter smista.ai router runtime configuration.\n\
         # Edit this file to configure HTTP binding, auth, storage, providers, and telemetry.\n\n\
         {toml}"
    ))
}

fn write_config_file(path: &Path, contents: &str, force: bool) -> anyhow::Result<()> {
    ensure_parent(path)?;

    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }

    let mut file = options.open(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!(
                "configuration already exists at {path}; pass --force to replace it",
                path = path.display()
            )
        } else {
            anyhow::Error::new(source)
        }
    })?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

fn ensure_parent(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tracing::debug!(
            "Creating parent directory {parent}",
            parent = parent.display()
        );
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn ensure_project_gitignore(path: &Path) -> anyhow::Result<()> {
    ensure_parent(path)?;

    let entry = crate::config::paths::SECRETS_FILE;
    let current = match std::fs::read_to_string(path) {
        Ok(current) => current,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => return Err(source.into()),
    };
    if current.lines().any(|line| line == entry) {
        return Ok(());
    }

    let mut updated = current;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(entry);
    updated.push('\n');
    std::fs::write(path, updated)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn should_write_project_config_and_gitignore() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join(".smista").join("config.toml");

        init(ConfigScope::Project, Some(&path), false).expect("failed to init project config");

        assert_valid_cli_config(&path);
        assert_eq!(
            fs::read_to_string(dir.path().join(".smista").join(".gitignore"))
                .expect("failed to read project gitignore"),
            "secrets\n"
        );
    }

    #[test]
    fn should_append_secrets_to_existing_project_gitignore() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join(".smista").join("config.toml");
        let gitignore = dir.path().join(".smista").join(".gitignore");
        fs::create_dir_all(gitignore.parent().expect("gitignore should have parent"))
            .expect("failed to create project config dir");
        fs::write(&gitignore, "plans\n").expect("failed to write project gitignore");

        init(ConfigScope::Project, Some(&path), false).expect("failed to init project config");

        assert_eq!(
            fs::read_to_string(gitignore).expect("failed to read project gitignore"),
            "plans\nsecrets\n"
        );
    }

    #[test]
    fn should_not_duplicate_existing_project_gitignore_entry() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join(".smista").join("config.toml");
        let gitignore = dir.path().join(".smista").join(".gitignore");
        fs::create_dir_all(gitignore.parent().expect("gitignore should have parent"))
            .expect("failed to create project config dir");
        fs::write(&gitignore, "secrets\n").expect("failed to write project gitignore");

        init(ConfigScope::Project, Some(&path), false).expect("failed to init project config");

        assert_eq!(
            fs::read_to_string(gitignore).expect("failed to read project gitignore"),
            "secrets\n"
        );
    }

    #[test]
    fn should_write_global_cli_config() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");

        init(ConfigScope::Global, Some(&path), false).expect("failed to init global config");

        assert_valid_cli_config(&path);
    }

    #[test]
    fn should_write_router_config_under_router_table() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("router.toml");

        init(ConfigScope::Router, Some(&path), false).expect("failed to init router config");

        let contents = fs::read_to_string(&path).expect("failed to read router config");
        assert!(contents.contains("[router]"));
        smista_router::config::parse(&contents, &path.display().to_string())
            .expect("router config should parse");
    }

    #[test]
    fn should_create_missing_parent_folders() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("nested").join("config.toml");

        init(ConfigScope::Global, Some(&path), false).expect("failed to init global config");

        assert!(path.exists());
    }

    #[test]
    fn should_not_overwrite_existing_config_without_force() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        fs::write(&path, "sentinel").expect("failed to write existing config");

        let error = init(ConfigScope::Global, Some(&path), false).unwrap_err();

        assert!(
            error.to_string().contains("configuration already exists"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read_to_string(path).expect("failed to read existing config"),
            "sentinel"
        );
    }

    #[test]
    fn should_overwrite_existing_config_with_force() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("config.toml");
        fs::write(&path, "sentinel").expect("failed to write existing config");

        init(ConfigScope::Global, Some(&path), true).expect("failed to force init global config");

        assert_valid_cli_config(&path);
        assert_ne!(
            fs::read_to_string(path).expect("failed to read overwritten config"),
            "sentinel"
        );
    }

    fn assert_valid_cli_config(path: &Path) {
        let contents = fs::read_to_string(path).expect("failed to read CLI config");
        let config: crate::config::Config =
            toml::from_str(&contents).expect("CLI config should parse");

        assert_eq!(config, crate::config::Config::default());
    }
}
