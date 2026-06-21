//! `POST /api/v1/sessions/{session_id}/stream` — run a task, streaming events.
//!
//! Takes the same body as `/execute` and responds with a Server-Sent Events
//! stream of [`TurnEvent`](smista_core::api::TurnEvent) values.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/stream`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/stream",
        operation_id = "streamTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ExecuteRequest,
        responses(
            (
                status = 200,
                description = "Server-Sent Events stream. Each `data:` line is one TurnEvent (internally tagged by `type`, snake_case). Exactly one terminal `turn_end` event carries the TurnResponse payload.",
                content_type = "text/event-stream",
                body = smista_core::api::TurnEvent
            ),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 422, description = "Routing rejected the request", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn stream() -> WebError {
    WebError::not_implemented()
}
