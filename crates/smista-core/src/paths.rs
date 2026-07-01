//! Platform path primitives for locating smista.ai configuration.
//!
//! These functions return only *directory* locations, never concrete file
//! paths — each binary composes the files it owns on top of them (see
//! `smista-cli`'s and `smista-router`'s `config::paths`). They perform no
//! filesystem IO; they only compute locations.
//!
//! The global configuration directory differs by platform, matching the
//! specification: `~/.config/smista` on Linux and macOS, and `~/.smista` on
//! Windows. macOS is forced to `~/.config` on purpose: `dirs::config_dir()`
//! there resolves to `~/Library/Application Support`, which is not what the
//! specification wants.

use std::path::{Path, PathBuf};

/// Directory name used for project-local and Windows-global configuration.
const SMISTA_DIR: &str = ".smista";
/// Application directory name inside the POSIX config directory.
const APP_DIR: &str = "smista";
/// Cross-tool agents directory under the home directory.
const AGENTS_DIR: &str = ".agents";
/// POSIX-style config directory name, joined under the home directory on macOS
/// and Linux.
const POSIX_CONFIG_DIR: &str = ".config";

/// Returns the global configuration directory, or `None` if the home or
/// platform config directory cannot be determined.
///
/// - Linux/BSD: `~/.config/smista`.
/// - macOS: `~/.config/smista`. The platform default from `dirs::config_dir()`
///   is `~/Library/Application Support`, so it is overridden to follow the
///   POSIX convention.
/// - Windows: `~/.smista` (the home directory plus `.smista`).
///
/// # Examples
///
/// ```
/// use smista_core::paths::global_config_dir;
///
/// // Present on any platform with a resolvable home directory.
/// let _ = global_config_dir();
/// ```
#[must_use]
pub fn global_config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        dirs::home_dir().map(|home| home.join(SMISTA_DIR))
    } else {
        dirs::home_dir().map(|home| home.join(POSIX_CONFIG_DIR).join(APP_DIR))
    }
}

/// Returns the per-user runtime directory for smista: a short-lived location for
/// process-state files such as a router pidfile.
///
/// Runtime state is ephemeral and must not live alongside persistent
/// configuration, so this resolves a dedicated runtime location rather than the
/// configuration directory:
///
/// - Linux/BSD: `$XDG_RUNTIME_DIR/smista` when the runtime directory is set.
/// - Otherwise (macOS, Windows, or no `XDG_RUNTIME_DIR`): the per-user
///   temporary directory plus `smista`.
///
/// It always resolves to a path a normal user can write to, so it returns a
/// [`PathBuf`] rather than an [`Option`].
///
/// # Examples
///
/// ```
/// use smista_core::paths::runtime_dir;
///
/// assert!(runtime_dir().ends_with("smista"));
/// ```
#[must_use]
pub fn runtime_dir() -> PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

/// Returns the home `.agents` directory: `~/.agents`, or `None` if the home
/// directory cannot be determined.
///
/// This is the cross-tool agents directory, anchored at the home directory on
/// every platform. Global skills live under `~/.agents/skills`; the CLI composes
/// that location on top of this primitive.
///
/// # Examples
///
/// ```
/// use smista_core::paths::home_agents_dir;
///
/// // Present on any platform with a resolvable home directory.
/// let _ = home_agents_dir();
/// ```
#[must_use]
pub fn home_agents_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(AGENTS_DIR))
}

/// Returns the project-local agents directory for `cwd`: `<cwd>/.agents`.
///
/// Skills shared across agent tools live under `.agents` rather than smista's
/// own `.smista` directory, both globally (see [`home_agents_dir`]) and
/// per-project. Project skills live under `<cwd>/.agents/skills`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use smista_core::paths::project_agents_dir;
///
/// let dir = project_agents_dir(Path::new("/work/repo"));
/// assert!(dir.ends_with(".agents"));
/// ```
#[must_use]
pub fn project_agents_dir(cwd: &Path) -> PathBuf {
    cwd.join(AGENTS_DIR)
}

/// Returns the project-local configuration directory for `cwd`: `<cwd>/.smista`.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use smista_core::paths::project_dir;
///
/// let dir = project_dir(Path::new("/work/repo"));
/// assert!(dir.ends_with(".smista"));
/// ```
#[must_use]
pub fn project_dir(cwd: &Path) -> PathBuf {
    cwd.join(SMISTA_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_build_project_dir_under_cwd() {
        let dir = project_dir(Path::new("/work/repo"));
        assert_eq!(dir, Path::new("/work/repo").join(".smista"));
    }

    #[test]
    fn should_end_project_dir_with_smista() {
        assert!(project_dir(Path::new("/anything")).ends_with(".smista"));
    }

    #[test]
    fn should_place_home_agents_dir_under_home() {
        if let Some(dir) = home_agents_dir() {
            assert!(dir.ends_with(".agents"));
        }
    }

    #[test]
    fn should_build_project_agents_dir_under_cwd() {
        let dir = project_agents_dir(Path::new("/work/repo"));
        assert_eq!(dir, Path::new("/work/repo").join(".agents"));
    }

    #[test]
    fn should_place_global_dir_under_a_known_root() {
        if let Some(dir) = global_config_dir() {
            if cfg!(windows) {
                assert!(dir.ends_with(".smista"));
            } else {
                assert!(dir.ends_with(".config/smista"));
            }
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn should_anchor_global_dir_under_dot_config() {
        if let Some(dir) = global_config_dir() {
            assert!(dir.ends_with("smista"));
            assert!(
                dir.parent()
                    .is_some_and(|parent| parent.ends_with(".config"))
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn should_anchor_windows_global_dir_under_dot_smista() {
        if let Some(dir) = global_config_dir() {
            assert!(dir.ends_with(".smista"));
        }
    }

    #[test]
    fn should_place_runtime_dir_under_smista() {
        // Always resolves to a writable location; only the suffix is fixed.
        assert!(runtime_dir().ends_with("smista"));
    }
}
