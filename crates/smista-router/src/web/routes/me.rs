//! `GET /api/v1/auth/me` — report the authenticated user.
//!
//! Confirms the session token is valid and returns a
//! [`MeResponse`](smista_core::api::MeResponse) with the caller's user ID.
//!
//! Scaffolded; the handler is implemented in the auth issue.

use crate::web::error::WebError;

/// Handles `GET /api/v1/auth/me`.
pub(crate) async fn me() -> WebError {
    WebError::not_implemented()
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::web::test_support::{get, send, test_router};

    #[tokio::test]
    async fn should_return_not_implemented() {
        let router = test_router().await;
        let (status, body) = send(router, get("/api/v1/auth/me")).await;

        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(body["error"]["code"], "not_implemented");
    }
}
