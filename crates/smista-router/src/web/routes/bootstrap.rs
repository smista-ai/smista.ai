//! `POST /api/v1/auth/bootstrap` — create a user and issue a long-lived API key.
//!
//! Public endpoint: it needs no authentication, as it mints the first
//! credential a caller ever holds. It creates a new user, persists it with the
//! API key stored only as a hash, and returns the new user ID together with the
//! plaintext API key as a [`BootstrapResponse`].
//! The plaintext key is shown only in this response and can never be retrieved
//! again, so the caller must store it.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use secrecy::ExposeSecret;
use smista_core::api::BootstrapResponse;

use crate::web::AppState;
use crate::web::routes::ApiResult;

/// Handles `POST /api/v1/auth/bootstrap`.
pub(crate) async fn bootstrap(State(state): State<AppState>) -> ApiResult<BootstrapResponse> {
    // bootstrap user with the authenticator
    let user = state.authenticator.bootstrap_user().await?;

    Ok((
        StatusCode::CREATED,
        Json(BootstrapResponse {
            user_id: user.user_id.to_string(),
            api_key: user.api_key.expose_secret().to_string(),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::http::StatusCode;
    use secrecy::SecretString;
    use smista_storage::database::Database as _;
    use uuid::Uuid;

    use crate::auth::Authenticator;
    use crate::web::test_support::{post, send, test_router_with_database};

    /// The prefix every v1 API key carries, per `docs/api/http-api.md`.
    const API_KEY_PREFIX: &str = "sk-smista-api01-";

    #[tokio::test]
    async fn should_create_a_user_and_return_201_with_credentials() {
        // No `Authorization` header is sent: the endpoint is public.
        let (router, _db) = test_router_with_database().await;
        let (status, body) = send(router, post("/api/v1/auth/bootstrap")).await;

        assert_eq!(status, StatusCode::CREATED);

        let user_id = body["user_id"].as_str().expect("user_id missing");
        let api_key = body["api_key"].as_str().expect("api_key missing");
        assert!(!user_id.is_empty());
        assert!(
            api_key.starts_with(API_KEY_PREFIX),
            "api_key must use the {API_KEY_PREFIX} prefix"
        );

        // The body carries exactly the two documented fields and no other
        // secret values.
        let object = body.as_object().expect("body is not a JSON object");
        assert_eq!(object.len(), 2);
        assert!(object.contains_key("user_id"));
        assert!(object.contains_key("api_key"));
    }

    #[tokio::test]
    async fn should_persist_the_user_with_the_api_key_stored_as_a_hash() {
        let (router, db) = test_router_with_database().await;
        let (status, body) = send(router, post("/api/v1/auth/bootstrap")).await;
        assert_eq!(status, StatusCode::CREATED);

        let user_id =
            Uuid::parse_str(body["user_id"].as_str().expect("user_id missing")).expect("bad uuid");
        let api_key = body["api_key"].as_str().expect("api_key missing");

        let user = db
            .get_user(user_id)
            .await
            .expect("failed to read user")
            .expect("bootstrapped user was not persisted");

        // The plaintext key is never stored: only a hash, which is not the key.
        assert_ne!(user.api_key_hash, api_key);
        assert!(
            user.api_key_hash.starts_with("$argon2"),
            "the stored credential must be an argon2 hash"
        );
    }

    #[tokio::test]
    async fn should_return_a_key_that_can_sign_in() {
        let (router, db) = test_router_with_database().await;
        let (status, body) = send(router, post("/api/v1/auth/bootstrap")).await;
        assert_eq!(status, StatusCode::CREATED);

        let user_id =
            Uuid::parse_str(body["user_id"].as_str().expect("user_id missing")).expect("bad uuid");
        let api_key = SecretString::from(body["api_key"].as_str().expect("api_key missing"));

        // The persisted hash verifies against the issued key end to end.
        let authenticator = Authenticator::new(db, Duration::from_secs(3600));
        let session = authenticator
            .sign_in(&api_key)
            .await
            .expect("the issued key failed to sign in");
        assert_eq!(session.user_id, user_id);
    }

    #[tokio::test]
    async fn should_issue_distinct_credentials_on_each_call() {
        let (router, _db) = test_router_with_database().await;
        let (_, first) = send(router.clone(), post("/api/v1/auth/bootstrap")).await;
        let (_, second) = send(router, post("/api/v1/auth/bootstrap")).await;

        assert_ne!(first["user_id"], second["user_id"]);
        assert_ne!(first["api_key"], second["api_key"]);
    }
}
