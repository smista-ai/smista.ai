//! Authentication token table.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

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
