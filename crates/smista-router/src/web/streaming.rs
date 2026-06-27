//! Server-Sent Events rendering shared by the turn-driving endpoints.
//!
//! The execute and continue endpoints answer with the same wire format: a
//! buffered [`TurnResponse`](smista_core::api::TurnResponse) by default, or a
//! `text/event-stream` of [`TurnEvent`](smista_core::api::TurnEvent)s when the
//! client asks via `Accept`. Both buffer the turn rather than streaming it token
//! by token, so a streamed request is answered by replaying the finished outcome
//! as the same event vocabulary a live stream uses, terminated by the `turn_end`
//! event that carries the full response. This module holds the negotiation,
//! replay and framing that the two handlers share.

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use smista_core::api::{ApiErrorCode, TurnEvent, TurnOutcome, TurnResponse};

use crate::web::error::WebError;

/// Whether the client asked for the Server-Sent Events stream.
///
/// Returns `true` only when one of the `Accept` header's media ranges is exactly
/// `text/event-stream`; any media parameters (for example a `q` weight) are
/// ignored. An absent, unparsable, `application/json` or `*/*` `Accept` yields
/// `false`, so the buffered JSON body is the default.
pub(crate) fn wants_event_stream(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| {
            accept
                .split(',')
                .map(|media| media.split(';').next().unwrap_or("").trim())
                .any(|media| media == "text/event-stream")
        })
}

/// Replays a buffered [`TurnResponse`] as the short event sequence a streaming
/// turn would have produced.
///
/// A completed turn replays its assistant text and usage; a tool pause replays
/// each requested call as its `tool_call_started`/`tool_call_requested` pair; any
/// other outcome carries no intermediate events. Every outcome ends with the
/// terminal `turn_end` event holding the full response, so the client reads the
/// same `status` it would from the buffered reply.
pub(crate) fn replay_events(response: TurnResponse) -> Vec<TurnEvent> {
    let mut events = Vec::new();
    match &response.outcome {
        TurnOutcome::Completed(turn) => {
            if !turn.message.content.is_empty() {
                events.push(TurnEvent::TextDelta {
                    delta: turn.message.content.clone(),
                });
            }
            events.push(TurnEvent::Usage(turn.usage.clone()));
        }
        TurnOutcome::AwaitingTool { tool_requests, .. } => {
            events.extend(tool_requests.iter().flat_map(|request| {
                [
                    TurnEvent::ToolCallStarted {
                        call_id: request.call_id.clone(),
                        name: request.name.clone(),
                    },
                    TurnEvent::ToolCallRequested {
                        call_id: request.call_id.clone(),
                        name: request.name.clone(),
                        arguments: request.arguments.clone(),
                        requires_approval: request.requires_approval,
                    },
                ]
            }));
        }
        _ => {}
    }
    events.push(TurnEvent::TurnEnd(Box::new(response)));
    events
}

/// Renders a sequence of [`TurnEvent`]s as a `text/event-stream` response.
///
/// Each event becomes one `data:` record terminated by a blank line, the
/// Server-Sent Events framing the client parses. A serialization failure (which
/// the documented turn types never produce) is reported as an internal error.
pub(crate) fn sse_response(events: &[TurnEvent]) -> Response {
    let mut body = String::new();
    for event in events {
        let payload = match serde_json::to_string(event) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!(%error, "failed to serialize a turn event for streaming");
                return WebError::from_code(
                    ApiErrorCode::InternalError,
                    "An internal error occurred.",
                )
                .into_response();
            }
        };
        body.push_str("data: ");
        body.push_str(&payload);
        body.push_str("\n\n");
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
        .into_response()
}
