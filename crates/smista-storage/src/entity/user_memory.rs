//! User memory tables.
//!
//! A [`UserMemory`] is a long-term, model-populated preference owned by a user.
//! It is untouched by session deletion and subject to retention. The fact
//! lives in the paired [`UserMemoryContent`] table under the same record id.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Table, User, record_uuid};

/// A long-term, model-populated preference owned by a user.
///
/// `key` is an optional topic that lets an update target an existing fact;
/// keyless rows accumulate freely. The fact lives in [`UserMemoryContent`].
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct UserMemory {
    /// Unique identifier for the memory.
    pub id: RecordId,
    /// Owner of the memory.
    pub user: RecordId,
    /// Topic; lets an update target a fact.
    pub key: Option<String>,
    /// When the fact was first recorded.
    pub created_at: DateTime<Utc>,
    /// When the fact was last changed.
    pub updated_at: DateTime<Utc>,
}

impl UserMemory {
    /// Builds a user memory owned by `user_id` under the record key `id`.
    ///
    /// `created_at` and `updated_at` are stamped with the current time. Lets a
    /// caller mint a memory from plain [`Uuid`]s without naming SurrealDB's
    /// record id type.
    pub fn new(id: Uuid, user_id: Uuid, key: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: RecordId::new(Self::name(), id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            key,
            created_at: now,
            updated_at: now,
        }
    }

    /// Returns this memory's UUIDv7 key.
    pub fn uuid(&self) -> Uuid {
        record_uuid(&self.id)
    }
}

impl Table for UserMemory {
    fn name() -> &'static str {
        "user_memory"
    }
}

/// The fact of a [`UserMemory`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct UserMemoryContent {
    /// Record id, identical to the owning [`UserMemory`].
    pub id: RecordId,
    /// The remembered fact.
    pub content: String,
}

impl UserMemoryContent {
    /// Builds the content row paired with the [`UserMemory`] of the same `id`.
    pub fn new(id: Uuid, content: String) -> Self {
        Self {
            id: RecordId::new(Self::name(), id.to_string()),
            content,
        }
    }
}

impl Table for UserMemoryContent {
    fn name() -> &'static str {
        "user_memory_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_user_memory() {
        let id = RecordId::new(UserMemory::name(), uuid::Uuid::now_v7().to_string());
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let memory = UserMemory {
            id,
            user: user.clone(),
            key: Some("editor".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        crate::tests::fk_roundtrip(crate::tests::user(user), memory).await;
    }

    #[test]
    fn should_build_user_memory_from_uuids_and_expose_its_uuid() {
        let id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let memory = UserMemory::new(id, user_id, Some("editor".to_string()));

        assert_eq!(memory.uuid(), id);
        assert_eq!(
            memory.user,
            RecordId::new(User::name(), user_id.to_string())
        );
        assert_eq!(memory.key.as_deref(), Some("editor"));
        assert_eq!(memory.created_at, memory.updated_at);
    }

    #[tokio::test]
    async fn should_store_and_read_user_memory_content() {
        let id = RecordId::new(UserMemoryContent::name(), uuid::Uuid::now_v7().to_string());
        let content = UserMemoryContent {
            id,
            content: "prefers tabs".to_string(),
        };

        crate::tests::roundtrip(content).await;
    }
}
