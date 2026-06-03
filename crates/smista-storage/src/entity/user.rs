//! User table.
//!
//! The [`User`] is the root of ownership in the schema: every session, token
//! and user-owned memory references a user, and ownership checks are enforced at
//! the query boundary. A user is metadata-only — it has no paired `_content`
//! table.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

/// An identity that can own sessions.
///
/// In local-first deployments a user may be created locally with no SaaS
/// account; the same entity can later represent a remote account. The raw API
/// key is never stored — only its hash. A user has a single API key.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct User {
    /// Unique identifier for the user.
    pub id: RecordId,
    /// Api key hash for authentication.
    pub api_key_hash: String,
    /// Created at timestamp.
    pub created_at: DateTime<Utc>,
    /// Updated at timestamp.
    pub updated_at: DateTime<Utc>,
    /// Timestamp of when the user was disabled, if applicable.
    pub disabled_at: Option<DateTime<Utc>>,
}

impl Table for User {
    fn name() -> &'static str {
        "user"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_user() {
        let id = uuid::Uuid::now_v7();
        let id = RecordId::new(User::name(), id.to_string());
        let user = User {
            id,
            api_key_hash: "hashed_api_key".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        };

        crate::tests::roundtrip(user).await;
    }
}
