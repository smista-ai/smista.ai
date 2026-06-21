//! `POST /api/v1/sessions/{session_id}/execute` — run a task.
//!
//! Takes an [`ExecuteRequest`](smista_core::api::ExecuteRequest), routes it
//! deterministically, calls the selected model and returns a
//! [`TurnResponse`](smista_core::api::TurnResponse) with the routing
//! explanation — buffered, or streamed as
//! [`TurnEvent`](smista_core::api::TurnEvent) Server-Sent Events when the
//! client asks via the `Accept` header.
//!
//! Scaffolded; the handler is implemented in the execution issue.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/execute`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/execute",
        operation_id = "executeTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ExecuteRequest,
        responses(
            (
                status = 200,
                description = "Turn completed or awaiting input. Buffered as a single `TurnResponse` (`application/json`) or, when the client sends `Accept: text/event-stream`, streamed as Server-Sent Events of `TurnEvent` whose terminal `turn_end` carries the `TurnResponse`.",
                content(
                    (smista_core::api::TurnResponse = "application/json"),
                    (smista_core::api::TurnEvent = "text/event-stream"),
                )
            ),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 422, description = "Routing rejected the request", body = smista_core::api::ApiError),
            (status = 503, description = "Provider credentials missing or fallbacks exhausted", body = smista_core::api::ApiError),
            (status = 502, description = "Provider error", body = smista_core::api::ApiError),
            (status = 504, description = "Provider timed out", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn execute() -> WebError {
    WebError::not_implemented()
}
