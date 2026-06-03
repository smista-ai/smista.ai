//! SurrealDB backend selection and its connection endpoint.

use std::path::PathBuf;

use secrecy::{ExposeSecret, SecretString};

/// The SurrealDB backend, selecting the engine and its location at runtime.
///
/// Each variant maps to a connection endpoint scheme: the embedded SurrealKV
/// engine, an in-memory engine for tests, or a remote instance over HTTP or
/// WebSocket. The engine is erased to a single `Surreal<Any>` at connect time.
#[derive(Debug, Clone)]
pub enum SurrealBackend {
    /// Embedded SurrealKV engine, storing data under the given directory.
    Embedded { db_dir: PathBuf },
    /// In-memory engine, intended for tests; nothing is persisted.
    Memory,
    /// Remote connection to a SurrealDB instance over HTTP.
    Http(RemoteOptions),
    /// Remote connection to a SurrealDB instance over WebSocket.
    WebSocket(RemoteOptions),
}

/// Options for connecting to a remote SurrealDB instance (HTTP or WebSocket).
#[derive(Debug, Clone)]
pub struct RemoteOptions {
    /// The URL of the remote SurrealDB instance (e.g., "http://localhost:8000").
    pub url: String,
    /// Optional username for authentication with the remote instance.
    pub username: Option<String>,
    /// Optional password for authentication with the remote instance.
    pub password: Option<SecretString>,
}

impl SurrealBackend {
    /// Prepares the local environment required before connecting.
    ///
    /// The embedded backend creates its data directory if it is missing; the
    /// in-memory and remote backends need no preparation. SurrealDB normalizes
    /// the path into the `surrealkv://` endpoint, including Windows drive
    /// letters and backslashes, so the directory is the only local setup.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`](crate::StorageError::Io) if the embedded
    /// data directory cannot be created.
    pub(super) async fn prepare(&self) -> crate::StorageResult<()> {
        if let Self::Embedded { db_dir } = self {
            tracing::debug!(db.path = %db_dir.display(), "creating embedded SurrealDB directory {{db.path}}");
            tokio::fs::create_dir_all(db_dir).await?;
        }
        Ok(())
    }

    /// Builds the connection endpoint string for this backend.
    pub(super) fn endpoint(&self) -> String {
        match self {
            Self::Embedded { db_dir } => format!("surrealkv://{path}", path = db_dir.display()),
            Self::Memory => "mem://".to_string(),
            Self::Http(options) | Self::WebSocket(options) => options.url.clone(),
        }
    }

    /// Retrieves the connection credentials, if any are configured.
    ///
    /// The password is exposed as a borrowed `&str` only here, at the point of
    /// use, and is otherwise kept in a [`SecretString`].
    pub(super) fn credentials(&self) -> Option<(&str, &str)> {
        match self {
            Self::Embedded { .. } | Self::Memory => None,
            Self::Http(options) | Self::WebSocket(options) => {
                match (&options.username, &options.password) {
                    (Some(username), Some(password)) => {
                        Some((username.as_str(), password.expose_secret()))
                    }
                    _ => None,
                }
            }
        }
    }
}
