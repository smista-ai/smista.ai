//! `GET /api/v1/auth/me` — report the authenticated user.
//!
//! Protected endpoint. The [`authenticate`](crate::web) middleware validates the
//! Bearer token and stores the resolved user in the request extensions; this
//! handler reads that user and returns a [`MeResponse`] with the caller's user
//! ID. It lists no sessions and exposes no secret values.

use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::MeResponse;

use crate::web::AuthenticatedUser;
use crate::web::routes::ApiResult;

/// Handles `GET /api/v1/auth/me`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/auth/me",
        operation_id = "getCurrentUser",
        tag = "auth",
        security(("bearer" = [])),
        responses(
            (status = 200, description = "The authenticated user", body = smista_core::api::MeResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn me(Extension(user): Extension<AuthenticatedUser>) -> ApiResult<MeResponse> {
    Ok((
        StatusCode::OK,
        Json(MeResponse {
            user_id: user.user_id.to_string(),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use uuid::Uuid;

    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_user, get, get_with_token, send,
    };

    #[tokio::test]
    async fn should_return_the_authenticated_user_id() {
        let (router, token, user_id) = authenticated_router_with_user().await;

        let (status, body) = send(router, get_with_token("/api/v1/auth/me", &token)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user_id"], user_id.to_string());
    }

    #[tokio::test]
    async fn should_not_expose_sessions_or_secrets() {
        let (router, token, _user_id) = authenticated_router_with_user().await;

        let (status, body) = send(router, get_with_token("/api/v1/auth/me", &token)).await;

        assert_eq!(status, StatusCode::OK);
        // The response carries the user id and nothing else: no session list and
        // no secret values such as the api key or the token.
        let object = body
            .as_object()
            .expect("response body was not a JSON object");
        assert_eq!(object.keys().collect::<Vec<_>>(), vec!["user_id"]);
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(router, get("/api/v1/auth/me")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_reject_an_invalid_token() {
        let (router, _token) = authenticated_router().await;

        // A well-formed but unknown token never matches a stored session.
        let bogus = Uuid::new_v4().to_string();
        let (status, body) = send(router, get_with_token("/api/v1/auth/me", &bogus)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token");
    }
}
