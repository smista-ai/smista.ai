//! Context memory tables.
//!
//! A [`ContextMemory`] is a per-session, model-populated memory owned by a
//! session and a user. It cascades on session delete (no SurrealDB FK cascade —
//! `delete_session` removes `context_memory WHERE session = $session` and its
//! `_content` row). The fact lives in the paired [`ContextMemoryContent`] table
//! under the same record id.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;
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
