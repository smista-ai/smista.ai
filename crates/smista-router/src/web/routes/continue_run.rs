//! `POST /api/v1/sessions/{session_id}/continue` — advance an in-flight run.
//!
//! Takes a [`ContinueRequest`](smista_core::api::ContinueRequest) — tool
//! results, approval decisions, queued user input or an interrupt — and returns
//! the next [`TurnResponse`](smista_core::api::TurnResponse), buffered or
//! streamed by the `Accept` header. It replaces the standalone approval
//! endpoint.
//!
//! Scaffolded; the handler is implemented in the execution flow.

use crate::web::error::WebError;

/// Handles `POST /api/v1/sessions/{session_id}/continue`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/continue",
        operation_id = "continueTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ContinueRequest,
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
pub(crate) async fn continue_run() -> WebError {
    WebError::not_implemented()
}
