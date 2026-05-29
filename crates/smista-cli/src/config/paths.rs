//! Concrete `config.toml`-related file paths, composed on core primitives.
//!
//! All paths are built with [`std::path::PathBuf`] joins so they are correct on
//! both POSIX and Windows.

use std::path::{Path, PathBuf};

use smista_sdk::core::paths::{global_config_dir, home_smista_dir, project_dir};

/// Configuration file name within a configuration directory.
const CONFIG_FILE: &str = "config.toml";
/// Secrets file name within the project directory.
const SECRETS_FILE: &str = "secrets";
/// Skills directory name within the project directory.
const SKILLS_DIR: &str = "skills";
/// Plans directory name within the project directory.
const PLANS_DIR: &str = "plans";

/// Returns the global `config.toml` path, if the global directory resolves.
#[must_use]
pub fn global_config_toml() -> Option<PathBuf> {
    global_config_dir().map(|dir| dir.join(CONFIG_FILE))
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

/// Returns the global secrets file path: `~/.smista/secrets`, if the home
/// directory resolves.
///
/// Per the specification the global secrets file lives under the home `.smista`
/// directory on every platform, independent of the platform config directory
/// used for `config.toml`.
#[must_use]
pub fn global_secrets_file() -> Option<PathBuf> {
    home_smista_dir().map(|dir| dir.join(SECRETS_FILE))
}

/// Returns the project skills directory: `<cwd>/.smista/skills`.
#[must_use]
pub fn skills_dir(cwd: &Path) -> PathBuf {
    project_dir(cwd).join(SKILLS_DIR)
}

/// Returns the project plans directory: `<cwd>/.smista/plans`.
#[must_use]
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
    fn should_build_global_secrets_under_home_smista() {
        if let Some(path) = global_secrets_file() {
            assert!(path.ends_with(Path::new(".smista").join("secrets")));
        }
    }

    #[test]
    fn should_build_skills_and_plans_dirs() {
        assert!(skills_dir(Path::new("/repo")).ends_with("skills"));
        assert!(plans_dir(Path::new("/repo")).ends_with("plans"));
    }
}
