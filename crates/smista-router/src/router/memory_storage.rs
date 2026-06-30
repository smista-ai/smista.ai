//! The router's [`MemoryStorage`] backend, backed by [`SurrealDatabase`].
//!
//! [`SurrealMemoryStorage`] adapts the agent-facing [`MemoryStorage`] seam from
//! `smista-providers` onto the storage [`Database`] API. It is a long-lived,
//! shared handle: every method takes the [`MemoryScope`] it acts within, so one
//! instance serves every user and session. Memory handles are the entries'
//! UUIDv7 keys rendered as strings, so the model can forget exactly what it
//! recorded.

use smista_providers::memory::{MemoryRecord, MemoryScope, MemoryStorage};
use smista_storage::StorageError;
use smista_storage::database::Database;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::{ContextMemory, ContextMemoryContent, UserMemory, UserMemoryContent};
use smista_storage::types::SecretContent;
use uuid::Uuid;

/// A [`MemoryStorage`] backend writing through to a [`SurrealDatabase`].
///
/// Holds only the shared database handle; the user and session a call operates
/// on arrive per call as a [`MemoryScope`].
pub struct SurrealMemoryStorage {
    /// The shared storage backend every memory operation reads and writes.
    database: SurrealDatabase,
}

impl From<SurrealDatabase> for SurrealMemoryStorage {
    fn from(database: SurrealDatabase) -> Self {
        Self { database }
    }
}

/// Maps a stored [`UserMemory`] and its content into the agent-facing record.
fn user_record(memory: UserMemory, content: UserMemoryContent) -> MemoryRecord {
    MemoryRecord {
        handle: memory.uuid().to_string(),
        key: memory.key,
        content: content.content,
    }
}

/// Maps a stored [`ContextMemory`] and its content into the agent-facing record.
///
/// The content of an encrypted session reads back blank: storage holds no key,
/// so the sealed envelope has no plaintext here.
fn session_record(memory: ContextMemory, content: ContextMemoryContent) -> MemoryRecord {
    MemoryRecord {
        handle: memory.uuid().to_string(),
        key: memory.key,
        content: content
            .content
            .as_plaintext()
            .unwrap_or_default()
            .to_string(),
    }
}

/// Treats a missing row as success, so forgetting an unknown handle is a no-op.
fn ignore_missing(result: Result<(), StorageError>) -> Result<(), StorageError> {
    match result {
        Ok(()) | Err(StorageError::NotFound) => Ok(()),
        Err(error) => Err(error),
    }
}

impl MemoryStorage for SurrealMemoryStorage {
    type Error = StorageError;

    async fn put_user_memory(
        &self,
        scope: MemoryScope,
        key: Option<String>,
        content: String,
    ) -> Result<MemoryRecord, Self::Error> {
        let id = Uuid::now_v7();
        let memory = UserMemory::new(id, scope.user_id, key);
        let memory_content = UserMemoryContent::new(id, content.clone());
        let stored = self
            .database
            .record_user_memory(scope.user_id, memory, memory_content)
            .await?;
        Ok(MemoryRecord {
            handle: stored.uuid().to_string(),
            key: stored.key,
            content,
        })
    }

    async fn forget_user_memory(
        &self,
        scope: MemoryScope,
        handle: String,
    ) -> Result<(), Self::Error> {
        let Ok(id) = Uuid::parse_str(&handle) else {
            return Ok(());
        };
        ignore_missing(self.database.forget_user_memory(scope.user_id, id).await)
    }

    async fn get_user_memories(
        &self,
        scope: MemoryScope,
        limit: Option<usize>,
    ) -> Result<Vec<MemoryRecord>, Self::Error> {
        let rows = self
            .database
            .list_user_memory_with_content(scope.user_id)
            .await?;
        Ok(rows
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|(memory, content)| user_record(memory, content))
            .collect())
    }

    async fn get_user_memory_by_key(
        &self,
        scope: MemoryScope,
        key: String,
    ) -> Result<Option<MemoryRecord>, Self::Error> {
        let rows = self
            .database
            .list_user_memory_with_content(scope.user_id)
            .await?;
        Ok(rows
            .into_iter()
            .find(|(memory, _)| memory.key.as_deref() == Some(key.as_str()))
            .map(|(memory, content)| user_record(memory, content)))
    }

    async fn put_session_memory(
        &self,
        scope: MemoryScope,
        key: Option<String>,
        content: String,
    ) -> Result<MemoryRecord, Self::Error> {
        let id = Uuid::now_v7();
        let memory = ContextMemory::new(id, scope.session_id, scope.user_id, key);
        let memory_content =
            ContextMemoryContent::new(id, SecretContent::plaintext(content.clone()));
        let stored = self
            .database
            .record_context_memory(scope.user_id, memory, memory_content)
            .await?;
        Ok(MemoryRecord {
            handle: stored.uuid().to_string(),
            key: stored.key,
            content,
        })
    }

    async fn forget_session_memory(
        &self,
        scope: MemoryScope,
        handle: String,
    ) -> Result<(), Self::Error> {
        let Ok(id) = Uuid::parse_str(&handle) else {
            return Ok(());
        };
        ignore_missing(
            self.database
                .forget_context_memory(scope.user_id, scope.session_id, id)
                .await,
        )
    }

    async fn get_session_memories(
        &self,
        scope: MemoryScope,
        limit: Option<usize>,
    ) -> Result<Vec<MemoryRecord>, Self::Error> {
        let rows = self
            .database
            .list_context_memory_with_content(scope.user_id, scope.session_id)
            .await?;
        Ok(rows
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|(memory, content)| session_record(memory, content))
            .collect())
    }

    async fn get_session_memory_by_key(
        &self,
        scope: MemoryScope,
        key: String,
    ) -> Result<Option<MemoryRecord>, Self::Error> {
        let rows = self
            .database
            .list_context_memory_with_content(scope.user_id, scope.session_id)
            .await?;
        Ok(rows
            .into_iter()
            .find(|(memory, _)| memory.key.as_deref() == Some(key.as_str()))
            .map(|(memory, content)| session_record(memory, content)))
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use smista_storage::database::Database;
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use smista_storage::entity::{Session, Table, User};
    use surrealdb::types::RecordId;
    use uuid::Uuid;

    use super::*;

    /// Builds a user owned by `id`.
    fn user(id: Uuid) -> User {
        User {
            id: RecordId::new(User::name(), id.to_string()),
            api_key_hash: format!("hash-{id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        }
    }

    /// Builds a session `id` owned by `user_id`.
    fn session(id: Uuid, user_id: Uuid) -> Session {
        Session {
            id: RecordId::new(Session::name(), id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: None,
            scope: None,
            encrypted: false,
            key_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        }
    }

    /// Spins up an in-memory backend with a user and an owned session, returning
    /// the scoped storage and the [`MemoryScope`] addressing them.
    async fn storage() -> (SurrealMemoryStorage, MemoryScope) {
        let database = SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database");

        let user_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        database
            .create_user(user(user_id))
            .await
            .expect("failed to create user");
        database
            .create_session(session(session_id, user_id))
            .await
            .expect("failed to create session");

        (
            SurrealMemoryStorage::from(database),
            MemoryScope {
                user_id,
                session_id,
            },
        )
    }

    #[tokio::test]
    async fn should_record_and_list_user_memory() {
        let (storage, scope) = storage().await;

        let record = storage
            .put_user_memory(scope, Some("editor".to_string()), "neovim".to_string())
            .await
            .expect("failed to record memory");
        assert_eq!(record.key.as_deref(), Some("editor"));
        assert_eq!(record.content, "neovim");

        let listed = storage
            .get_user_memories(scope, None)
            .await
            .expect("failed to list memories");
        assert_eq!(listed, vec![record]);
    }

    #[tokio::test]
    async fn should_get_user_memory_by_key() {
        let (storage, scope) = storage().await;
        storage
            .put_user_memory(scope, Some("editor".to_string()), "neovim".to_string())
            .await
            .expect("failed to record memory");

        let found = storage
            .get_user_memory_by_key(scope, "editor".to_string())
            .await
            .expect("failed to look up memory")
            .expect("memory not found by key");
        assert_eq!(found.content, "neovim");

        assert!(
            storage
                .get_user_memory_by_key(scope, "missing".to_string())
                .await
                .expect("failed to look up memory")
                .is_none(),
            "unknown key resolved to a memory"
        );
    }

    #[tokio::test]
    async fn should_upsert_keyed_user_memory_in_place() {
        let (storage, scope) = storage().await;
        storage
            .put_user_memory(scope, Some("editor".to_string()), "vim".to_string())
            .await
            .expect("failed to record first memory");
        storage
            .put_user_memory(scope, Some("editor".to_string()), "emacs".to_string())
            .await
            .expect("failed to record second memory");

        let listed = storage
            .get_user_memories(scope, None)
            .await
            .expect("failed to list memories");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].content, "emacs");
    }

    #[tokio::test]
    async fn should_keep_multiple_keyless_user_memories() {
        let (storage, scope) = storage().await;
        storage
            .put_user_memory(scope, None, "a".to_string())
            .await
            .expect("failed to record first memory");
        storage
            .put_user_memory(scope, None, "b".to_string())
            .await
            .expect("failed to record second memory");

        let listed = storage
            .get_user_memories(scope, None)
            .await
            .expect("failed to list memories");
        assert_eq!(listed.len(), 2);
    }

    #[tokio::test]
    async fn should_forget_user_memory_by_handle() {
        let (storage, scope) = storage().await;
        let record = storage
            .put_user_memory(scope, Some("editor".to_string()), "neovim".to_string())
            .await
            .expect("failed to record memory");

        storage
            .forget_user_memory(scope, record.handle)
            .await
            .expect("failed to forget memory");

        assert!(
            storage
                .get_user_memories(scope, None)
                .await
                .expect("failed to list memories")
                .is_empty(),
            "memory survived forget"
        );
    }

    #[tokio::test]
    async fn should_ignore_forgetting_unknown_handle() {
        let (storage, scope) = storage().await;

        storage
            .forget_user_memory(scope, Uuid::now_v7().to_string())
            .await
            .expect("forgetting an absent handle should be a no-op");
        storage
            .forget_user_memory(scope, "not-a-uuid".to_string())
            .await
            .expect("forgetting a malformed handle should be a no-op");
    }

    #[tokio::test]
    async fn should_cap_user_memories_at_limit() {
        let (storage, scope) = storage().await;
        for index in 0..3 {
            storage
                .put_user_memory(scope, None, format!("fact-{index}"))
                .await
                .expect("failed to record memory");
        }

        let listed = storage
            .get_user_memories(scope, Some(2))
            .await
            .expect("failed to list memories");
        assert_eq!(listed.len(), 2);
    }

    #[tokio::test]
    async fn should_isolate_user_and_session_scopes() {
        let (storage, scope) = storage().await;
        storage
            .put_user_memory(scope, Some("u".to_string()), "user-fact".to_string())
            .await
            .expect("failed to record user memory");
        storage
            .put_session_memory(scope, Some("s".to_string()), "session-fact".to_string())
            .await
            .expect("failed to record session memory");

        let user_memories = storage
            .get_user_memories(scope, None)
            .await
            .expect("failed to list user memories");
        assert_eq!(user_memories.len(), 1);
        assert_eq!(user_memories[0].content, "user-fact");

        let session_memories = storage
            .get_session_memories(scope, None)
            .await
            .expect("failed to list session memories");
        assert_eq!(session_memories.len(), 1);
        assert_eq!(session_memories[0].content, "session-fact");
    }

    #[tokio::test]
    async fn should_record_forget_and_look_up_session_memory() {
        let (storage, scope) = storage().await;
        let record = storage
            .put_session_memory(scope, Some("goal".to_string()), "ship it".to_string())
            .await
            .expect("failed to record session memory");

        let found = storage
            .get_session_memory_by_key(scope, "goal".to_string())
            .await
            .expect("failed to look up session memory")
            .expect("session memory not found by key");
        assert_eq!(found.content, "ship it");

        storage
            .forget_session_memory(scope, record.handle)
            .await
            .expect("failed to forget session memory");
        assert!(
            storage
                .get_session_memories(scope, None)
                .await
                .expect("failed to list session memories")
                .is_empty(),
            "session memory survived forget"
        );
    }

    #[tokio::test]
    async fn should_not_read_memory_of_another_user() {
        let (storage, scope) = storage().await;
        storage
            .put_user_memory(scope, Some("editor".to_string()), "neovim".to_string())
            .await
            .expect("failed to record memory");

        let stranger = MemoryScope {
            user_id: Uuid::now_v7(),
            session_id: scope.session_id,
        };
        assert!(
            storage
                .get_user_memories(stranger, None)
                .await
                .expect("failed to list memories")
                .is_empty(),
            "another user's memory was disclosed"
        );
    }
}
