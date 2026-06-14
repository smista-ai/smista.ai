//! Session table.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

/// A resumable user interaction with smista.ai.
///
/// Each session belongs to exactly one user, who is the only identity allowed
/// to access it. Session ids are globally unique.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct Session {
    /// Globally unique session id.
    pub id: RecordId,
    /// Owning user.
    pub user: RecordId,
    /// Human-readable session title.
    pub title: Option<String>,
    /// Whether the session's content is end-to-end encrypted.
    ///
    /// Fixed when the session is created and never changed afterwards: flipping
    /// it would orphan content the router cannot re-key. When `true`, every
    /// paired `_content` row in the session stores a sealed
    /// [`SecretContent::Encrypted`](crate::types::SecretContent::Encrypted)
    /// payload sealed under the key named by [`key_id`](Self::key_id).
    pub encrypted: bool,
    /// Fingerprint of the per-session client key, when [`encrypted`](Self::encrypted).
    ///
    /// `None` for a non-encrypted session. The key itself never reaches storage
    /// or the router; only this identifier is persisted.
    pub key_id: Option<String>,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the session was archived, if applicable.
    pub archived_at: Option<DateTime<Utc>>,
}

impl Table for Session {
    fn name() -> &'static str {
        "session"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session() {
        let id = RecordId::new(Session::name(), uuid::Uuid::now_v7().to_string());
        let user = RecordId::new("user", uuid::Uuid::now_v7().to_string());
        let session = Session {
            id,
            user: user.clone(),
            title: Some("My session".to_string()),
            encrypted: false,
            key_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        };

        crate::tests::fk_roundtrip(crate::tests::user(user), session).await;
    }
}
