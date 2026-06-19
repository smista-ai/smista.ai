//! `POST /api/v1/auth/sign-out` — revoke the current session token.
//!
//! Protected endpoint. The [`authenticate`](crate::web) middleware validates the
//! Bearer token and stores it in the request extensions; this handler revokes
//! that token and returns a [`SignOutResponse`] confirming it. Once revoked, the
//! same token is rejected by the middleware with `401 token_revoked`, so a
//! caller must sign in again to obtain a fresh token.

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::SignOutResponse;

use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `POST /api/v1/auth/sign-out`.
pub(crate) async fn sign_out(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<SignOutResponse> {
    state.authenticator.sign_out(&user.token).await?;

    Ok((StatusCode::OK, Json(SignOutResponse { revoked: true })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::web::test_support::{
        authenticated_router, get_with_token, post, post_with_token, send,
    };

    #[tokio::test]
    async fn should_revoke_the_presented_token() {
        let (router, token) = authenticated_router().await;

        let (status, body) = send(router, post_with_token("/api/v1/auth/sign-out", &token)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["revoked"], true);
    }

    #[tokio::test]
    async fn should_reject_reuse_of_a_revoked_token() {
        let (router, token) = authenticated_router().await;

        // Sign out, revoking the token.
        let (status, _) = send(
            router.clone(),
            post_with_token("/api/v1/auth/sign-out", &token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Reusing the now-revoked token on a protected route is rejected with the
        // distinct `token_revoked` code, not a generic `invalid_token`.
        let (status, body) = send(router, get_with_token("/api/v1/auth/me", &token)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "token_revoked");
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(router, post("/api/v1/auth/sign-out")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}
