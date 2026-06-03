//! User memory tables.
//!
//! A [`UserMemory`] is a long-term, model-populated preference owned by a user.
//! It is untouched by session deletion and subject to retention. The fact
//! lives in the paired [`UserMemoryContent`] table under the same record id.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

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
