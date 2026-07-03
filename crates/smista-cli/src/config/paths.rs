//! Concrete `config.toml`-related file paths, composed on core primitives.
//!
//! All paths are built with [`std::path::PathBuf`] joins so they are correct on
//! both POSIX and Windows.

use std::path::{Path, PathBuf};

use smista_sdk::core::paths::{global_config_dir, project_dir, runtime_dir};

/// Configuration file name within a configuration directory.
const CONFIG_FILE: &str = "config.toml";
/// Router pidfile name within the global configuration directory.
const ROUTER_PIDFILE: &str = "router.pid";
/// Secrets file name within the project directory.
pub const SECRETS_FILE: &str = "secrets";
/// Plans directory name within the project directory.
const PLANS_DIR: &str = "plans";

/// Returns the global `config.toml` path, if the global directory resolves.
#[must_use]
pub fn global_config_toml() -> Option<PathBuf> {
    global_config_dir().map(|dir| dir.join(CONFIG_FILE))
}

/// Returns the default router pidfile path: `router.pid` under the per-user
/// runtime directory.
///
/// Both `smista start` and `smista stop` fall back to this path when no
/// `--pidfile` is given, so the two always agree on where a locally started
/// router records its process id. A pidfile is ephemeral process state, so it
/// lives under the runtime directory (see [`runtime_dir`]) rather than alongside
/// persistent configuration. That location always resolves and is writable by a
/// normal user on every platform.
#[must_use]
pub fn router_pidfile() -> PathBuf {
    runtime_dir().join(ROUTER_PIDFILE)
}

/// Returns the project `config.toml` path: `<cwd>/.smista/config.toml`.
#[must_use]
pub fn project_config_toml(cwd: &Path) -> PathBuf {
    project_dir(cwd).join(CONFIG_FILE)
}

/// Returns the project secrets file path: `<cwd>/.smista/secrets`.
#[must_use]
pub fn secrets_file(cwd: &Path) -> PathBuf {
    project_dir(cwd).join(SECRETS_FILE)
}

/// Returns the global secrets file path, if the global config directory
/// resolves.
///
/// The secrets file lives beside global CLI configuration under
/// [`global_config_dir`].
#[must_use]
pub fn global_secrets_file() -> Option<PathBuf> {
    global_config_dir().map(|dir| dir.join(SECRETS_FILE))
}

/// Returns the project plans directory: `<cwd>/.smista/plans`.
#[must_use]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "plans storage path is reserved for upcoming plan support"
    )
)]
pub fn plans_dir(cwd: &Path) -> PathBuf {
    project_dir(cwd).join(PLANS_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_place_project_config_under_smista() {
        let path = project_config_toml(Path::new("/repo"));
        assert_eq!(path, Path::new("/repo").join(".smista").join("config.toml"));
    }

    #[test]
    fn should_build_secrets_path() {
        assert!(secrets_file(Path::new("/repo")).ends_with("secrets"));
    }

    #[test]
    fn should_place_project_secrets_under_smista() {
        let path = secrets_file(Path::new("/repo"));
        assert_eq!(path, Path::new("/repo").join(".smista").join("secrets"));
    }

    #[test]
    fn should_build_global_secrets_under_global_config_dir() {
        if let (Some(config_dir), Some(secrets_file)) = (global_config_dir(), global_secrets_file())
        {
            assert_eq!(secrets_file, config_dir.join("secrets"));
        }
    }

    #[test]
    fn should_build_plans_dir() {
        assert!(plans_dir(Path::new("/repo")).ends_with("plans"));
    }

    #[test]
    fn should_build_router_pidfile_under_runtime_dir() {
        assert!(router_pidfile().ends_with("router.pid"));
    }
}
