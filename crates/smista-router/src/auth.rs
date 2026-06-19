//! This module exposes the authentication logic and interfaces for smista-router

mod apikey;
mod hash;
mod token;

use std::time::Duration;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret as _, SecretString};
use smista_storage::database::Database as _;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::{AuthToken, User};
use uuid::Uuid;

use self::apikey::ApiKeyIssuer;
use self::hash::SecretHasher;
use crate::auth::token::{Token, TokenIssuer};

/// Error type for authentication-related operations, such as user bootstrapping and API key validation.
#[derive(Debug, thiserror::Error)]
pub enum AuthenticatorError {
    #[error("Internal error: {0}")]
    InternalError(anyhow::Error),
    #[error("Invalid API key")]
    InvalidApiKey,
    #[error("Invalid hash format")]
    InvalidHash,
    #[error("Invalid token")]
    InvalidToken,
    #[error("User not found")]
    UserNotFound,
}

type AuthenticationResult<T> = std::result::Result<T, AuthenticatorError>;

/// The [`Authenticator`] struct is responsible for handling authentication logic, in particular:
///
/// - Bootstrapping a new user with an API key
/// - Validating an API key for a given user to ensure that the request is authorized
/// - Issuing a session token for a user after successful authentication
/// - Cleaning up sessions after logout
#[derive(Debug, Clone)]
pub struct Authenticator {
    storage: SurrealDatabase,
    /// How long an issued session token stays valid, sourced from
    /// `router.auth.token_ttl_seconds`.
    token_ttl: Duration,
}

/// A session token issued at sign-in, together with the moment it expires.
#[derive(Debug, Clone)]
pub struct SessionToken {
    /// The expiration time of the session token.
    pub expires_at: DateTime<Utc>,
    /// The session token issued to the user.
    pub token: SecretString,
}

impl Authenticator {
    /// Creates a new [`Authenticator`] over `storage`, issuing tokens that live for `token_ttl`.
    pub fn new(storage: SurrealDatabase, token_ttl: Duration) -> Self {
        Self { storage, token_ttl }
    }

    /// Bootstraps a new user by generating a unique user ID and an API key issued by the [`ApiKeyIssuer`].
    ///
    /// The user is then registered in the database with the generated user ID and API key.
    ///
    /// Returns a [`BootstrappedUser`] containing the user ID and the generated API key.
    pub async fn bootstrap_user(&self) -> AuthenticationResult<BootstrappedUser> {
        let user_id = Uuid::now_v7();
        tracing::debug!("Bootstrapping new user with ID: {user_id}");

        let api_key = ApiKeyIssuer::generate_api_key_v1(&user_id);
        let api_key_hash = SecretHasher::argon2().hash(&api_key)?;

        self.storage
            .create_user(User::new(user_id, api_key_hash.expose_secret().to_string()))
            .await
            .map_err(|e| AuthenticatorError::InternalError(e.into()))?;
        tracing::info!("Successfully bootstrapped new user with ID: {user_id}");

        Ok(BootstrappedUser { user_id, api_key })
    }

    /// Signs in a user by validating the provided API key against the stored hash in the database.
    ///
    /// A session token is issued, stored in the database and returned to the caller if the API key is valid.
    ///
    /// Returns a [`SessionToken`] containing the issued token and its expiration time.
    /// Returns an error if the user is not found or if the API key is invalid.
    pub async fn sign_in(&self, api_key: &SecretString) -> AuthenticationResult<SessionToken> {
        tracing::debug!("parsing user id from api key");
        let user_id = ApiKeyIssuer::parse_user_id(api_key)?;
        tracing::debug!("loading user from database with id {user_id}");

        let user = self
            .storage
            .get_user(user_id)
            .await
            .map_err(|e| AuthenticatorError::InternalError(e.into()))?
            .ok_or(AuthenticatorError::UserNotFound)?;

        tracing::debug!("verifying api key for user {user_id}");
        if !SecretHasher::argon2().verify(api_key, &user.api_key_hash)? {
            tracing::debug!("api key verification failed for user {user_id}");
            return Err(AuthenticatorError::InvalidApiKey);
        }

        // issue token
        let token = TokenIssuer::generate_token();
        let token_str = SecretString::from(token.to_string());
        let token_hash = SecretHasher::sha512_crypt().hash(&token_str)?;
        let token_entity = AuthToken::new(
            token.token_id(),
            token_hash.expose_secret().to_string(),
            &user,
            self.token_ttl,
        );
        let expires_at = token_entity.expires_at;
        self.storage
            .create_token(token_entity)
            .await
            .map_err(|e| AuthenticatorError::InternalError(e.into()))?;

        tracing::info!(
            "successfully signed in user {user_id} and issued token {token}; expires at {expires_at}",
            token = token.token_id()
        );

        Ok(SessionToken {
            expires_at,
            token: token_str,
        })
    }

    /// Authenticates a session token and resolves the user it authenticates.
    ///
    /// The token id is parsed from the presented token, the matching active
    /// token row is loaded by that id (a missing, expired or revoked token is
    /// rejected), and the presented token is verified against the stored hash.
    /// The owning user's id is returned so the caller can scope the request to
    /// that user.
    ///
    /// Returns [`AuthenticatorError::InvalidToken`] when the token is malformed,
    /// unknown, expired, revoked or does not match the stored hash.
    pub async fn authenticate(&self, token: &SecretString) -> AuthenticationResult<Uuid> {
        tracing::debug!("parsing token from string");
        let parsed_token = Token::parse(token.expose_secret())?;
        tracing::debug!("loading active token with id {}", parsed_token.token_id());
        let active_token = self
            .storage
            .get_active_token(parsed_token.token_id())
            .await
            .map_err(|e| AuthenticatorError::InternalError(e.into()))?
            .ok_or(AuthenticatorError::InvalidToken)?;
        tracing::debug!(
            "verifying token hash for token id {}",
            parsed_token.token_id()
        );

        if !SecretHasher::sha512_crypt().verify(token, &active_token.token_hash)? {
            tracing::debug!(
                "token hash verification failed for token id {}",
                parsed_token.token_id()
            );
            return Err(AuthenticatorError::InvalidToken);
        }

        let user_id = active_token.user_id();
        tracing::info!(
            "validated token {} for user {user_id}",
            parsed_token.token_id()
        );

        Ok(user_id)
    }

    /// Signs a user out by revoking the session token they present.
    ///
    /// The token id is parsed from the presented token and the matching row is
    /// revoked, after which [`Authenticator::authenticate`] rejects it.
    ///
    /// Returns [`AuthenticatorError::InvalidToken`] when the token is malformed.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by POST /auth/sign-out endpoint (#150)")
    )]
    pub async fn sign_out(&self, token: &SecretString) -> AuthenticationResult<()> {
        tracing::debug!("parsing token from string");
        let parsed_token = Token::parse(token.expose_secret())?;
        tracing::debug!(
            "revoking token with id {} in the database",
            parsed_token.token_id()
        );
        self.storage
            .revoke_token(parsed_token.token_id())
            .await
            .map_err(|e| AuthenticatorError::InternalError(e.into()))?;
        tracing::info!(
            "successfully revoked token with id {}",
            parsed_token.token_id()
        );

        Ok(())
    }
}

/// Result of bootstrapping a new user, containing the user ID and the generated API key.
#[derive(Debug, Clone)]
pub struct BootstrappedUser {
    pub user_id: Uuid,
    pub api_key: SecretString,
}

#[cfg(test)]
mod tests {

    use smista_storage::database::surreal::{SurrealBackend, SurrealOptions};

    use super::apikey::ApiKeyIssuer;
    use super::hash::SecretHasher;
    use super::*;

    async fn test_authenticator() -> Authenticator {
        let database = SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database");

        Authenticator::new(database, Duration::from_secs(3600))
    }

    #[tokio::test]
    async fn should_bootstrap_user_with_a_v1_api_key() {
        let authenticator = test_authenticator().await;
        let bootstrapped = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");

        assert!(
            bootstrapped
                .api_key
                .expose_secret()
                .starts_with("sk-smista-api01-")
        );
        // The issued key embeds the same user id that was bootstrapped.
        assert_eq!(
            ApiKeyIssuer::parse_user_id(&bootstrapped.api_key).expect("failed to parse user id"),
            bootstrapped.user_id
        );
    }

    #[tokio::test]
    async fn should_persist_only_the_hash_of_the_api_key() {
        let authenticator = test_authenticator().await;
        let bootstrapped = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");

        let user = authenticator
            .storage
            .get_user(bootstrapped.user_id)
            .await
            .expect("failed to read user")
            .expect("bootstrapped user not found");

        // The raw key is never stored; only its hash.
        assert_ne!(user.api_key_hash, bootstrapped.api_key.expose_secret());
        // The stored hash verifies against the issued key.
        assert!(
            SecretHasher::argon2()
                .verify(&bootstrapped.api_key, &user.api_key_hash)
                .expect("failed to verify api key")
        );
    }

    #[tokio::test]
    async fn should_bootstrap_distinct_users() {
        let authenticator = test_authenticator().await;
        let first = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");
        let second = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");

        assert_ne!(first.user_id, second.user_id);
        assert_ne!(
            first.api_key.expose_secret(),
            second.api_key.expose_secret()
        );
    }

    #[tokio::test]
    async fn should_sign_in_with_a_valid_api_key_and_issue_a_usable_token() {
        let authenticator = test_authenticator().await;
        let bootstrapped = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");

        let user_session = authenticator
            .sign_in(&bootstrapped.api_key)
            .await
            .expect("failed to sign in with a valid api key");

        // check expires_at
        let now = Utc::now();
        assert!(user_session.expires_at > now);
        assert!(user_session.expires_at < now + chrono::Duration::seconds(3600));

        // The issued token authenticates the same user that signed in.
        let user_id = authenticator
            .authenticate(&user_session.token)
            .await
            .expect("failed to validate the issued token");
        assert_eq!(user_id, bootstrapped.user_id);
    }

    #[tokio::test]
    async fn should_store_only_the_hash_of_the_issued_token() {
        let authenticator = test_authenticator().await;
        let bootstrapped = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");

        let token = authenticator
            .sign_in(&bootstrapped.api_key)
            .await
            .expect("failed to sign in")
            .token;

        let token_id = Token::parse(token.expose_secret())
            .expect("issued token is malformed")
            .token_id();
        let stored = authenticator
            .storage
            .get_active_token(token_id)
            .await
            .expect("failed to load token")
            .expect("issued token not found");

        // The raw token is never stored, only its salted hash.
        assert_ne!(stored.token_hash, token.expose_secret());
        assert!(stored.token_hash.starts_with("$6$"));
    }

    #[tokio::test]
    async fn should_reject_sign_in_for_an_unknown_user() {
        let authenticator = test_authenticator().await;
        // A well-formed key whose embedded user was never bootstrapped.
        let api_key = ApiKeyIssuer::generate_api_key_v1(&Uuid::now_v7());

        let error = authenticator
            .sign_in(&api_key)
            .await
            .expect_err("sign in succeeded for an unknown user");
        assert!(matches!(error, AuthenticatorError::UserNotFound));
    }

    #[tokio::test]
    async fn should_reject_sign_in_with_a_wrong_api_key() {
        let authenticator = test_authenticator().await;
        let bootstrapped = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");

        // Same user id, different secret: the stored hash will not verify.
        let wrong_key = ApiKeyIssuer::generate_api_key_v1(&bootstrapped.user_id);

        let error = authenticator
            .sign_in(&wrong_key)
            .await
            .expect_err("sign in succeeded with a wrong api key");
        assert!(matches!(error, AuthenticatorError::InvalidApiKey));
    }

    #[tokio::test]
    async fn should_reject_sign_in_with_a_malformed_api_key() {
        let authenticator = test_authenticator().await;
        let malformed = SecretString::from("not-a-smista-api-key");

        let error = authenticator
            .sign_in(&malformed)
            .await
            .expect_err("sign in succeeded with a malformed api key");
        assert!(matches!(error, AuthenticatorError::InvalidApiKey));
    }

    #[tokio::test]
    async fn should_reject_validation_of_a_malformed_token() {
        let authenticator = test_authenticator().await;
        let malformed = SecretString::from("not-a-valid-token");

        let error = authenticator
            .authenticate(&malformed)
            .await
            .expect_err("validation succeeded for a malformed token");
        assert!(matches!(error, AuthenticatorError::InvalidToken));
    }

    #[tokio::test]
    async fn should_reject_validation_of_an_unknown_token() {
        let authenticator = test_authenticator().await;
        // Well-formed, but never issued, so no row exists for its id.
        let unknown = SecretString::from(TokenIssuer::generate_token().to_string());

        let error = authenticator
            .authenticate(&unknown)
            .await
            .expect_err("validation succeeded for an unknown token");
        assert!(matches!(error, AuthenticatorError::InvalidToken));
    }

    #[tokio::test]
    async fn should_reject_validation_after_sign_out() {
        let authenticator = test_authenticator().await;
        let bootstrapped = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap user");
        let token = authenticator
            .sign_in(&bootstrapped.api_key)
            .await
            .expect("failed to sign in")
            .token;

        authenticator
            .sign_out(&token)
            .await
            .expect("failed to sign out");

        let error = authenticator
            .authenticate(&token)
            .await
            .expect_err("a revoked token still validated");
        assert!(matches!(error, AuthenticatorError::InvalidToken));
    }

    #[tokio::test]
    async fn should_reject_sign_out_of_a_malformed_token() {
        let authenticator = test_authenticator().await;
        let malformed = SecretString::from("not-a-valid-token");

        let error = authenticator
            .sign_out(&malformed)
            .await
            .expect_err("sign out succeeded for a malformed token");
        assert!(matches!(error, AuthenticatorError::InvalidToken));
    }
}
