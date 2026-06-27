//! Session table.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Table, record_uuid};
use crate::entity::User;

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

impl Session {
    /// Creates a new session for the given user.
    pub fn new(id: Uuid, user: Uuid, title: Option<String>, key_id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: RecordId::new(Self::name(), id.to_string()),
            user: RecordId::new(User::table(), user.to_string()),
            title,
            encrypted: key_id.is_some(),
            key_id,
            created_at: now,
            updated_at: now,
            archived_at: None,
        }
    }

    /// Returns this session's UUIDv7 key.
    pub fn uuid(&self) -> Uuid {
        record_uuid(&self.id)
    }
}

impl Table for Session {
    fn name() -> &'static str {
        "session"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn should_build_a_plaintext_session_without_a_key_id() {
        let id = Uuid::now_v7();
        let user = Uuid::now_v7();
        let session = Session::new(id, user, Some("My session".to_string()), None);

        assert_eq!(session.uuid(), id);
        assert_eq!(session.user, RecordId::new(User::name(), user.to_string()));
        assert_eq!(session.title, Some("My session".to_string()));
        // No key id means the session is not end-to-end encrypted.
        assert!(!session.encrypted);
        assert_eq!(session.key_id, None);
        // A fresh session is created and updated at the same instant and is not
        // archived.
        assert_eq!(session.created_at, session.updated_at);
        assert_eq!(session.archived_at, None);
    }

    #[test]
    fn should_mark_a_session_encrypted_when_a_key_id_is_present() {
        let session = Session::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            None,
            Some("kf_ab12".to_string()),
        );

        // The key id alone drives the encrypted flag; no separate argument sets
        // it, so the two can never disagree.
        assert!(session.encrypted);
        assert_eq!(session.key_id, Some("kf_ab12".to_string()));
        assert_eq!(session.title, None);
    }

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
