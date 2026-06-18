//! Authentication token table.

use std::time::Duration;

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Table, record_uuid};
use crate::entity::User;

/// A short-lived authentication session for the router.
///
/// The CLI uses an auth token after signing in with its user id and API key.
/// The raw token is never stored — only its hash. Expired or revoked tokens are
/// rejected and cleaned up over time.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct AuthToken {
    /// Unique identifier for the token.
    pub id: RecordId,
    /// Owning user.
    pub user: RecordId,
    /// Hash of the issued token.
    pub token_hash: String,
    /// When the token was issued.
    pub created_at: DateTime<Utc>,
    /// When the token expires.
    pub expires_at: DateTime<Utc>,
    /// When the token was revoked, if applicable.
    pub revoked_at: Option<DateTime<Utc>>,
}

impl AuthToken {
    /// Creates a new [`AuthToken`] owned by `user`, identified by `id` and storing `token_hash`.
    ///
    /// The token expires `duration` after creation and starts out not revoked.
    pub fn new(id: Uuid, token_hash: String, user: &User, duration: Duration) -> Self {
        let now = Utc::now();
        let expires_at = now + duration;

        Self {
            id: RecordId::new(Self::name(), id.to_string()),
            user: user.id.clone(),
            token_hash,
            created_at: now,
            expires_at,
            revoked_at: None,
        }
    }

    /// Returns the [`Uuid`] of the user that owns this token.
    ///
    /// This recovers the owning user's id from the stored record reference so
    /// callers can scope a request to that user without depending on SurrealDB's
    /// [`RecordId`] type.
    pub fn user_id(&self) -> Uuid {
        record_uuid(&self.user)
    }
}

impl Table for AuthToken {
    fn name() -> &'static str {
        "auth_token"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_auth_token() {
        let id = RecordId::new(AuthToken::name(), uuid::Uuid::now_v7().to_string());
        let user = RecordId::new("user", uuid::Uuid::now_v7().to_string());
        let token = AuthToken {
            id,
            user: user.clone(),
            token_hash: "hashed_token".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now(),
            revoked_at: None,
        };

        crate::tests::fk_roundtrip(crate::tests::user(user), token).await;
    }
}
