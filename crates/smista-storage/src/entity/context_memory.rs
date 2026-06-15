//! Context memory tables.
//!
//! A [`ContextMemory`] is a per-session, model-populated memory owned by a
//! session and a user. It cascades on session delete (no SurrealDB FK cascade —
//! `delete_session` removes `context_memory WHERE session = $session` and its
//! `_content` row). The fact lives in the paired [`ContextMemoryContent`] table
//! under the same record id.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Session, Table, User, record_uuid};
use crate::types::SecretContent;

/// A per-session, model-populated memory owned by a session and a user.
///
/// `key` is an optional topic that lets an update target an existing fact;
/// keyless rows accumulate freely. The fact lives in [`ContextMemoryContent`].
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct ContextMemory {
    /// Unique identifier for the memory.
    pub id: RecordId,
    /// Session the memory belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Topic; lets an update target a fact.
    pub key: Option<String>,
    /// When the fact was first recorded.
    pub created_at: DateTime<Utc>,
    /// When the fact was last changed.
    pub updated_at: DateTime<Utc>,
}

impl ContextMemory {
    /// Builds a context memory in `session_id` owned by `user_id` under the
    /// record key `id`.
    ///
    /// `created_at` and `updated_at` are stamped with the current time. Lets a
    /// caller mint a memory from plain [`Uuid`]s without naming SurrealDB's
    /// record id type.
    pub fn new(id: Uuid, session_id: Uuid, user_id: Uuid, key: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: RecordId::new(Self::name(), id.to_string()),
            session: RecordId::new(Session::name(), session_id.to_string()),
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

impl Table for ContextMemory {
    fn name() -> &'static str {
        "context_memory"
    }
}

/// The fact of a [`ContextMemory`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct ContextMemoryContent {
    /// Record id, identical to the owning [`ContextMemory`].
    pub id: RecordId,
    /// The remembered fact, in clear or sealed for an encrypted session.
    pub content: SecretContent,
}

impl ContextMemoryContent {
    /// Builds the content row paired with the [`ContextMemory`] of the same `id`.
    pub fn new(id: Uuid, content: SecretContent) -> Self {
        Self {
            id: RecordId::new(Self::name(), id.to_string()),
            content,
        }
    }
}

impl Table for ContextMemoryContent {
    fn name() -> &'static str {
        "context_memory_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_context_memory() {
        let id = RecordId::new(ContextMemory::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let memory = ContextMemory {
            id,
            session: session.clone(),
            user: user.clone(),
            key: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), memory).await;
    }

    #[test]
    fn should_build_context_memory_from_uuids_and_expose_its_uuid() {
        let id = Uuid::now_v7();
        let session_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let memory = ContextMemory::new(id, session_id, user_id, None);

        assert_eq!(memory.uuid(), id);
        assert_eq!(
            memory.session,
            RecordId::new(Session::name(), session_id.to_string())
        );
        assert_eq!(
            memory.user,
            RecordId::new(User::name(), user_id.to_string())
        );
        assert_eq!(memory.key, None);
    }

    #[tokio::test]
    async fn should_store_and_read_context_memory_content() {
        let id = RecordId::new(
            ContextMemoryContent::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let content = ContextMemoryContent {
            id,
            content: SecretContent::plaintext("the user is refactoring storage"),
        };

        crate::tests::roundtrip(content).await;
    }
}
