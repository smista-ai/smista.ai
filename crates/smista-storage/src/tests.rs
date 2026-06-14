//! Shared test helpers for entity roundtrips.

use chrono::Utc;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::types::{RecordId, SurrealValue};

use crate::entity::{Session, Table, User};

/// Opens a fresh in-memory database scoped to a test namespace.
async fn memory_db() -> Surreal<Db> {
    let db = Surreal::new::<surrealdb::engine::local::Mem>(())
        .await
        .expect("failed to load database");
    db.use_ns("test")
        .use_db("test")
        .await
        .expect("failed to access database");
    db
}

/// Creates `record` and reads it back, asserting the stored row matches.
pub async fn roundtrip<T>(record: T)
where
    T: SurrealValue + Table + PartialEq + std::fmt::Debug,
{
    let db = memory_db().await;

    let record: Option<T> = db
        .create(T::name())
        .content(record)
        .await
        .expect("failed to create record");

    let read_record: Vec<T> = db.select(T::name()).await.expect("failed to read record");
    assert_eq!(read_record.len(), 1);
    assert_eq!(read_record[0], record.unwrap());
}

/// Roundtrips a `child` that references a `parent` through a [`RecordId`].
///
/// The `parent` is created first so the referenced row exists, then `child` is
/// created and read back. Asserting equality on the read-back row proves the
/// reference (a [`RecordId`] field) is persisted and loaded intact — SurrealDB
/// does not enforce foreign keys, so a reference is preserved as a plain record
/// id regardless of whether its target exists.
pub async fn fk_roundtrip<P, C>(parent: P, child: C)
where
    P: SurrealValue + Table,
    C: SurrealValue + Table + PartialEq + std::fmt::Debug,
{
    let db = memory_db().await;

    let _: Option<P> = db
        .create(P::name())
        .content(parent)
        .await
        .expect("failed to create parent");

    let child: Option<C> = db
        .create(C::name())
        .content(child)
        .await
        .expect("failed to create child");

    let read_child: Vec<C> = db.select(C::name()).await.expect("failed to read child");
    assert_eq!(read_child.len(), 1);
    assert_eq!(read_child[0], child.unwrap());
}

/// Roundtrips a bare value type by storing it as the field of a scratch record.
///
/// Value types (those that carry no record id and implement no [`Table`]) cannot
/// be stored on their own, so the value is wrapped in a throwaway record, read
/// back, and compared field for field.
pub async fn value_roundtrip<V>(value: V)
where
    V: SurrealValue + PartialEq + std::fmt::Debug,
{
    /// Throwaway record carrying a single value field.
    #[derive(SurrealValue, PartialEq, Debug)]
    struct Wrapper<V>
    where
        V: SurrealValue,
    {
        value: V,
    }

    let db = memory_db().await;
    let stored: Option<Wrapper<V>> = db
        .create("scratch")
        .content(Wrapper { value })
        .await
        .expect("failed to create record");

    let read: Vec<Wrapper<V>> = db.select("scratch").await.expect("failed to read record");
    assert_eq!(read.len(), 1);
    assert_eq!(read[0], stored.unwrap());
}

/// Builds a minimal [`User`] parent with the given record id, for FK tests.
pub fn user(id: RecordId) -> User {
    User {
        id,
        api_key_hash: "hashed_api_key".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        disabled_at: None,
    }
}

/// Builds a minimal [`Session`] parent owned by `user`, for FK tests.
pub fn session(id: RecordId, user: RecordId) -> Session {
    Session {
        id,
        user,
        title: None,
        encrypted: false,
        key_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        archived_at: None,
    }
}
