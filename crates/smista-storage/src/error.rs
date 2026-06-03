//! Storage error types.

/// A result returned by the [`Database`](crate::Database) API.
pub type StorageResult<T> = std::result::Result<T, StorageError>;

/// An error raised by a [`Database`](crate::Database) operation.
///
/// Variants are kept deliberately coarse: callers act on the failure category,
/// not on the underlying SurrealDB detail, which is wrapped in
/// [`StorageError::Backend`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// A uniqueness constraint was violated — for example a duplicate API-key
    /// hash or a keyed memory that already exists for the owner.
    #[error("record already exists")]
    AlreadyExists,

    /// The underlying database returned an error.
    #[error("storage backend error: {0}")]
    Backend(#[from] surrealdb::Error),

    /// The requested record does not exist.
    #[error("record not found")]
    NotFound,

    /// The authenticated user may not access the requested record.
    #[error("operation not permitted for this user")]
    Unauthorized,
}
