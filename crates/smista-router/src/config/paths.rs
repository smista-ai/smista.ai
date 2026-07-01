//! Router config file paths, composed on core primitives.

use std::path::PathBuf;

use smista_core::paths::global_config_dir;

/// Router config file name.
const ROUTER_FILE: &str = "router.toml";
/// Default embedded database directory name.
const DB_DIR: &str = "db";

/// Returns the global `router.toml` path, if the global directory resolves.
#[must_use]
pub fn router_toml() -> Option<PathBuf> {
    match global_config_dir().map(|dir| dir.join(ROUTER_FILE)) {
        Some(path) => {
            tracing::debug!(config.path = %path.display(), "resolved router.toml path");
            Some(path)
        }
        None => {
            tracing::warn!("could not resolve global config directory for router.toml");
            None
        }
    }
}

/// Returns the default embedded database path under the global config dir.
#[must_use]
pub fn db_path() -> Option<PathBuf> {
    match global_config_dir().map(|dir| dir.join(DB_DIR)) {
        Some(path) => {
            tracing::debug!(db.path = %path.display(), "resolved embedded database path");
            Some(path)
        }
        None => {
            tracing::warn!("could not resolve global config directory for the database path");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_build_router_toml_under_global_dir() {
        if let Some(path) = router_toml() {
            assert!(path.ends_with("router.toml"));
        }
    }

    #[test]
    fn should_build_db_path_under_global_dir() {
        if let Some(path) = db_path() {
            assert!(path.ends_with("db"));
        }
    }
}
