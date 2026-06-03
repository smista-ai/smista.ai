//! SurrealDB [`Database`] trait implementation.

mod backend;
mod options;
mod schema;

use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Namespace;

#[doc(inline)]
pub use self::backend::{RemoteOptions, SurrealBackend};
#[doc(inline)]
pub use self::options::SurrealOptions;
use crate::database::Database;
use crate::{StorageError, StorageResult};

/// [`Database`] implementation backed by SurrealDB.
pub struct SurrealDatabase(Surreal<Any>);

impl SurrealDatabase {
    /// Connects to (or embeds) a SurrealDB database from the given options.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the embedded directory
    /// cannot be created, the connection or namespace selection fails,
    /// authentication is rejected, or the schema migration fails.
    pub async fn new(options: SurrealOptions) -> StorageResult<Self> {
        options.backend.prepare().await?;

        let endpoint = options.backend.endpoint();
        tracing::debug!(db.endpoint = %endpoint, "connecting to SurrealDB at {{db.endpoint}}");

        let db = surrealdb::engine::any::connect(&endpoint).await?;
        db.use_ns(&options.namespace).use_db(&options.db).await?;
        tracing::debug!(
            db.namespace = %options.namespace,
            db.name = %options.db,
            "using namespace {{db.namespace}} and database {{db.name}}"
        );

        if let Some((username, password)) = options.backend.credentials() {
            tracing::debug!(db.username = %username, "signing in to SurrealDB as {{db.username}}");
            db.signin(Namespace {
                namespace: options.namespace,
                username: username.to_string(),
                password: password.to_string(),
            })
            .await?;
        }

        schema::apply(&db).await?;

        Ok(Self(db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn memory_db() -> SurrealDatabase {
        SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize database")
    }

    #[tokio::test]
    async fn new_applies_schema_and_is_idempotent() {
        // The first connect applies the migration to an empty database.
        let db = memory_db().await;

        // Re-running the migration on the same connection is a no-op, proving
        // every `DEFINE ... IF NOT EXISTS` statement is idempotent.
        schema::apply(&db.0)
            .await
            .expect("schema migration is not idempotent");
    }

    #[tokio::test]
    async fn embedded_surrealkv_creates_dir_and_applies_schema() {
        let root = tempfile::tempdir().expect("failed to create temp dir");
        // A nested, not-yet-existing directory proves `prepare` creates it.
        let db_dir = root.path().join("surrealkv");

        // Connecting opens the SurrealKV store on disk and applies the schema.
        let db = SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Embedded {
                db_dir: db_dir.clone(),
            },
        })
        .await
        .expect("failed to open embedded database");

        assert!(db_dir.is_dir(), "embedded data directory was not created");

        // Re-applying the migration confirms it landed on the persisted store.
        schema::apply(&db.0)
            .await
            .expect("schema migration is not idempotent");
    }
}
