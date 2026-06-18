//! This module exposes the authentication logic and interfaces for smista-router

mod apikey;
mod hash;

use secrecy::{ExposeSecret as _, SecretString};
use smista_storage::database::Database as _;
use smista_storage::database::surreal::SurrealDatabase;
use smista_storage::entity::User;
use uuid::Uuid;

use crate::auth::apikey::ApiKeyIssuer;
use crate::auth::hash::SecretHasher;

/// The [`Authenticator`] struct is responsible for handling authentication logic, in particular:
///
/// - Bootstrapping a new user with an API key
/// - Validating an API key for a given user to ensure that the request is authorized
/// - Issuing a session token for a user after successful authentication
/// - Cleaning up sessions after logout
#[derive(Debug, Clone)]
pub struct Authenticator {
    storage: SurrealDatabase,
}

impl Authenticator {
    /// Creates a new instance of [`Authenticator`] with the provided [`SurrealDatabase`] storage.
    pub fn new(storage: SurrealDatabase) -> Self {
        Self { storage }
    }

    /// Bootstraps a new user by generating a unique user ID and an API key issued by the [`ApiKeyIssuer`].
    ///
    /// The user is then registered in the database with the generated user ID and API key.
    ///
    /// Returns a [`BootstrappedUser`] containing the user ID and the generated API key.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by POST /auth/bootstrap endpoint (#149)")
    )]
    pub async fn bootstrap_user(&self) -> anyhow::Result<BootstrappedUser> {
        let user_id = Uuid::now_v7();
        tracing::debug!("Bootstrapping new user with ID: {user_id}");

        let api_key = ApiKeyIssuer::generate_api_key_v1(&user_id);
        let api_key_hash = SecretHasher::hash(&api_key)?;

        self.storage
            .create_user(User::new(user_id, api_key_hash.expose_secret().to_string()))
            .await?;
        tracing::info!("Successfully bootstrapped new user with ID: {user_id}");

        Ok(BootstrappedUser { user_id, api_key })
    }
}

/// Result of bootstrapping a new user, containing the user ID and the generated API key.
#[derive(Debug, Clone)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by POST /auth/bootstrap endpoint (#149)")
)]
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

        Authenticator::new(database)
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
            SecretHasher::verify(
                &bootstrapped.api_key,
                &SecretString::from(user.api_key_hash)
            )
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
}
