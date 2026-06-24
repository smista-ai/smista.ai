//! This module exposes a helper function to build a [`SurrealDatabase`] instance from the [`StorageConfig`].

use smista_storage::database::surreal::{
    RemoteOptions, SurrealBackend, SurrealDatabase, SurrealOptions,
};

use crate::config::{StorageConfig, StorageMode};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("invalid configuration: {0}")]
    ConfigInvalid(&'static str),
    #[error("invalid URL scheme: {0}")]
    InvalidScheme(String),
    #[error(transparent)]
    Storage(#[from] smista_storage::StorageError),
}

/// Builds a [`SurrealDatabase`] instance from the given [`StorageConfig`].
///
/// At the moment only a [`SurrealDatabase`] is supported, even if the `engine` params exists on [`StorageConfig`] to allow for future extensibility.
/// In case extension is needed, we have to change the [`Database`](smista_storage::database::Database) trait to use `async_trait` to allow v-table.
pub async fn build_storage(config: &StorageConfig) -> Result<SurrealDatabase, StorageError> {
    let options = match config.mode {
        StorageMode::Embedded => {
            let path = config
                .path
                .clone()
                .or(crate::config::paths::db_path())
                .ok_or(StorageError::ConfigInvalid(
                    "path must be specified for `Embedded` engine",
                ))?;
            tracing::debug!(
                "Using embedded SurrealDB storage with path: {path}",
                path = path.display()
            );
            SurrealOptions {
                namespace: config.namespace.clone(),
                db: config.database.clone(),
                backend: SurrealBackend::Embedded { db_dir: path },
            }
        }
        StorageMode::Remote => {
            let url = config.url.clone().ok_or(StorageError::ConfigInvalid(
                "url must be specified for `Remote` engine",
            ))?;
            let username = config.username.clone();
            tracing::debug!("Using remote SurrealDB storage with URL: {url}");
            let password = config.password.clone();
            let remote_options = RemoteOptions {
                url: url.to_string(),
                username,
                password,
            };

            match url.scheme() {
                "ws" | "wss" => SurrealOptions {
                    namespace: config.namespace.clone(),
                    db: config.database.clone(),
                    backend: SurrealBackend::WebSocket(remote_options),
                },
                "http" | "https" => SurrealOptions {
                    namespace: config.namespace.clone(),
                    db: config.database.clone(),
                    backend: SurrealBackend::Http(remote_options),
                },
                scheme => {
                    return Err(StorageError::InvalidScheme(format!(
                        "unsupported URL scheme {scheme} for SurrealDB remote storage"
                    )));
                }
            }
        }
    };

    SurrealDatabase::new(options)
        .await
        .map_err(StorageError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn should_build_embedded_storage_from_explicit_path() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let config = StorageConfig {
            path: Some(dir.path().join("db")),
            ..StorageConfig::default()
        };

        build_storage(&config)
            .await
            .expect("failed to build embedded storage");
    }

    #[tokio::test]
    async fn should_fail_remote_storage_without_url() {
        let config = StorageConfig {
            mode: StorageMode::Remote,
            ..StorageConfig::default()
        };

        let err = build_storage(&config)
            .await
            .expect_err("building remote storage without a URL did not fail");

        assert!(
            err.to_string().contains("url must be specified"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn should_fail_remote_storage_with_unsupported_scheme() {
        let config = StorageConfig {
            mode: StorageMode::Remote,
            url: Some(
                "ftp://db.internal:8000"
                    .parse()
                    .expect("failed to parse URL"),
            ),
            ..StorageConfig::default()
        };

        let err = build_storage(&config)
            .await
            .expect_err("building remote storage with an unsupported scheme did not fail");

        assert!(
            err.to_string().contains("unsupported URL scheme"),
            "unexpected error: {err}"
        );
    }
}
