//! Platform path primitives for locating smista.ai configuration.
//!
//! These functions return only *directory* locations, never concrete file
//! paths — each binary composes the files it owns on top of them (see
//! `smista-cli`'s and `smista-router`'s `config::paths`). They perform no
//! filesystem IO; they only compute locations.
//!
//! The global configuration directory differs by platform, matching the
//! specification: `~/.config/smista` on POSIX and `~/.smista` on Windows.

use std::path::{Path, PathBuf};

/// Directory name used for project-local and Windows-global configuration.
const SMISTA_DIR: &str = ".smista";
/// Application directory name inside the POSIX config directory.
const APP_DIR: &str = "smista";

/// Returns the global configuration directory, or `None` if the home or
/// platform config directory cannot be determined.
///
/// - POSIX: `~/.config/smista` (the platform config directory plus `smista`).
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
        dirs::config_dir().map(|config| config.join(APP_DIR))
    }
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
    fn should_place_global_dir_under_a_known_root() {
        // The exact root depends on the host, but when resolvable the directory
        // must end with the platform-appropriate suffix.
        if let Some(dir) = global_config_dir() {
            if cfg!(windows) {
                assert!(dir.ends_with(".smista"));
            } else {
                assert!(dir.ends_with("smista"));
            }
        }
    }
}
