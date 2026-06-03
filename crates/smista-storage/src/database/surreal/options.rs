//! Connection options for the SurrealDB [`Database`](crate::Database).

use super::backend::SurrealBackend;

/// Options for configuring a [`SurrealDatabase`](super::SurrealDatabase) connection.
#[derive(Debug, Clone)]
pub struct SurrealOptions {
    /// The namespace to use for the SurrealDB connection.
    pub namespace: String,
    /// The database name to use for the SurrealDB connection.
    pub db: String,
    /// The backend that selects how and where to connect.
    pub backend: SurrealBackend,
}
