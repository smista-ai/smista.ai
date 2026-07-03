use std::path::{Path, PathBuf};

use crate::args::{ConfigArgs, ConfigCommand, ConfigScope};

mod check;
mod edit;
mod init;
mod path;
mod show;

/// Runs the selected `smista config` subcommand.
///
/// # Errors
///
/// Returns an error from the selected subcommand when configuration paths cannot
/// be resolved, files cannot be read or written, validation fails, or an editor
/// cannot be opened.
pub fn run(args: ConfigArgs) -> anyhow::Result<()> {
    match args.command {
        ConfigCommand::Check { scope } => check::run(args.path.as_deref(), scope),
        ConfigCommand::Edit { scope } => edit::run(args.path.as_deref(), scope),
        ConfigCommand::Init { scope, force } => init::init(scope, args.path.as_deref(), force),
        ConfigCommand::Path { scope } => path::run(args.path.as_deref(), scope),
        ConfigCommand::Show { scope } => show::run(args.path.as_deref(), scope),
    }
}

/// Resolves the concrete path for a selected configuration scope.
///
/// An explicit `path` always wins. Otherwise the path is computed from the same
/// platform-aware defaults used by `smista config init`.
///
/// # Errors
///
/// Returns an error when the selected scope has no resolvable default path or
/// when the current directory cannot be read for project configuration.
fn resolve_config_path_by_scope(
    path: Option<&Path>,
    scope: ConfigScope,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = path {
        return Ok(path.to_path_buf());
    }

    let cwd = std::env::current_dir()?;
    default_config_path_by_scope(scope, &cwd)
}

/// Returns the default configuration path for `scope` under `cwd`.
///
/// # Errors
///
/// Returns an error when the platform cannot resolve a global configuration
/// directory for router or global CLI configuration.
fn default_config_path_by_scope(scope: ConfigScope, cwd: &Path) -> anyhow::Result<PathBuf> {
    match scope {
        ConfigScope::Router => smista_router::config::paths::router_toml()
            .ok_or_else(|| anyhow::anyhow!("failed to resolve router config")),
        ConfigScope::Global => crate::config::paths::global_config_toml()
            .ok_or_else(|| anyhow::anyhow!("failed to resolve global cli config")),
        ConfigScope::Project => Ok(crate::config::paths::project_config_toml(cwd)),
    }
}
