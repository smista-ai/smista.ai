//! `POST /api/v1/auth/sign-in` — exchange an API key for a session token.
//!
//! Public endpoint. Takes the `X-Smista-Api-Key` header and returns a
//! [`SignInResponse`] with a short-lived session token and its expiry. The API
//! key already embeds the user id, so no request body is needed.
//!
//! The error response is a structured [`WebError`] whose status and machine
//! readable code are:
//!
//! - `401 missing_credentials` when the `X-Smista-Api-Key` header is absent.
//! - `401 invalid_api_key` when the header is not valid text, or when the key is
//!   malformed, unknown or does not match the stored hash. Reported uniformly so
//!   it never leaks which users exist.
//! - `500 internal_error` when sign-in fails for a server-side reason.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use secrecy::{ExposeSecret as _, SecretString};
use smista_core::api::{ApiErrorCode, SignInResponse};

use crate::web::AppState;
use crate::web::error::WebError;
use crate::web::routes::ApiResult;

/// The request header carrying the API key.
const API_KEY_HEADER: &str = "X-Smista-Api-Key";

/// Handles `POST /api/v1/auth/sign-in`.
pub(crate) async fn sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<SignInResponse> {
    let Some(api_key) = headers.get(API_KEY_HEADER) else {
        return Err(WebError::from_code(
            ApiErrorCode::MissingCredentials,
            "The X-Smista-Api-Key header is missing.",
        ));
    };
    let Ok(api_key) = api_key.to_str().map(SecretString::from) else {
        return Err(WebError::from_code(
            ApiErrorCode::InvalidApiKey,
            "The X-Smista-Api-Key header is not valid text.",
        ));
    };

    let session = state.authenticator.sign_in(&api_key).await?;

    Ok((
        StatusCode::OK,
        Json(SignInResponse {
            token: session.token.expose_secret().to_string(),
            expires_at: session.expires_at,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;

    use crate::web::test_support::{
        get_with_token, post, post_with_api_key, send, test_router_with_database,
    };

    /// Bootstraps a user over `router` and returns its plaintext API key.
    async fn bootstrap_api_key(router: &Router) -> String {
        let (status, body) = send(router.clone(), post("/api/v1/auth/bootstrap")).await;
        assert_eq!(status, StatusCode::CREATED);
        body["api_key"]
            .as_str()
            .expect("api_key missing")
            .to_string()
    }

    #[tokio::test]
    async fn should_exchange_a_valid_api_key_for_a_session_token() {
        let (router, _db) = test_router_with_database().await;
        let api_key = bootstrap_api_key(&router).await;

        let (status, body) =
            send(router, post_with_api_key("/api/v1/auth/sign-in", &api_key)).await;

        assert_eq!(status, StatusCode::OK);
        let token = body["token"].as_str().expect("token missing");
        assert!(!token.is_empty());
        // `expires_at` is a real RFC 3339 timestamp in the future.
        let expires_at = body["expires_at"].as_str().expect("expires_at missing");
        let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at)
            .expect("expires_at is not a valid RFC 3339 timestamp");
        assert!(expires_at > chrono::Utc::now());

        // The body carries exactly the two documented fields and no other
        // secret values such as the API key.
        let object = body.as_object().expect("body is not a JSON object");
        assert_eq!(object.len(), 2);
        assert!(object.contains_key("token"));
        assert!(object.contains_key("expires_at"));
    }

    #[tokio::test]
    async fn should_issue_a_token_accepted_by_the_authenticate_middleware() {
        let (router, _db) = test_router_with_database().await;
        let api_key = bootstrap_api_key(&router).await;

        let (_, body) = send(
            router.clone(),
            post_with_api_key("/api/v1/auth/sign-in", &api_key),
        )
        .await;
        let token = body["token"].as_str().expect("token missing");

        // A protected route rejects an unknown token with `401`, so reaching the
        // handler with the issued token (here a scaffolded `501`) proves the
        // `authenticate` middleware accepted it end to end.
        let (status, _) = send(router, get_with_token("/api/v1/auth/me", token)).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn should_reject_a_request_without_the_api_key_header() {
        let (router, _db) = test_router_with_database().await;

        let (status, body) = send(router, post("/api/v1/auth/sign-in")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_reject_a_malformed_api_key() {
        let (router, _db) = test_router_with_database().await;

        let (status, body) = send(
            router,
            post_with_api_key("/api/v1/auth/sign-in", "not-a-smista-api-key"),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        // A malformed key is reported uniformly so it never leaks which users exist.
        assert_eq!(body["error"]["code"], "invalid_api_key");
    }

    #[tokio::test]
    async fn should_not_echo_the_api_key_in_the_response() {
        let (router, _db) = test_router_with_database().await;
        let api_key = bootstrap_api_key(&router).await;

        let (_, body) = send(router, post_with_api_key("/api/v1/auth/sign-in", &api_key)).await;

        // The API key never appears anywhere in the response body.
        assert!(!body.to_string().contains(&api_key));
    }
}
