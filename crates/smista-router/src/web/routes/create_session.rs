//! `POST /api/v1/sessions` — create a session.
//!
//! Takes a [`CreateSessionRequest`](smista_core::api::CreateSessionRequest) and
//! returns `201` with a
//! [`CreateSessionResponse`](smista_core::api::CreateSessionResponse).
//!
//! Scaffolded; the handler is implemented in the sessions issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions",
        operation_id = "createSession",
        tag = "sessions",
        security(("bearer" = [])),
        request_body = smista_core::api::CreateSessionRequest,
        responses(
            (status = 201, description = "Session created", body = smista_core::api::CreateSessionResponse),
            (status = 400, description = "Invalid request", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn create_session() -> WebError {
    WebError::not_implemented()
}
