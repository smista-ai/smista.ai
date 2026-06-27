//! Server-Sent Events rendering shared by the turn-driving endpoints.
//!
//! The execute and continue endpoints answer with a buffered
//! [`TurnResponse`](smista_core::api::TurnResponse) by default, or a
//! `text/event-stream` of [`TurnEvent`](smista_core::api::TurnEvent)s when the
//! client asks via `Accept`. A streamed turn is driven from the model's live
//! output: text and reasoning deltas arrive as the model produces them, and the
//! terminal events — usage, tool-call activity and the closing `turn_end` — are
//! appended by the web layer from the finished response. This module holds the
//! negotiation, the terminal-event mapping and the streaming framing the two
//! handlers share.

use std::future::Future;

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use futures::stream;
use smista_core::api::{ApiErrorResponse, TurnEvent, TurnOutcome, TurnResponse};
use tokio::sync::mpsc::{self, UnboundedReceiver};

use crate::orchestrator::{OrchestratorError, TurnSink};

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

/// Spawns a turn and streams its events as a `text/event-stream` response.
///
/// `run` receives the live [`TurnSink`] and produces the turn future — an
/// `execute_streaming` or `advance_streaming` call. It runs on a spawned task so
/// the body can flush text and reasoning deltas as the model produces them; once
/// the turn finishes its terminal events (or, on failure, an error `turn_end`)
/// are appended and the body closes. Both turn-driving endpoints share this.
pub(crate) fn stream_turn<F, Fut>(run: F) -> Response
where
    F: FnOnce(TurnSink) -> Fut + Send + 'static,
    Fut: Future<Output = Result<TurnResponse, OrchestratorError>> + Send,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let sink = TurnSink::new(tx);
    tokio::spawn(async move {
        match run(sink.clone()).await {
            Ok(response) => {
                for event in terminal_events(response) {
                    sink.emit(event);
                }
            }
            Err(error) => sink.emit(error_turn_end(error)),
        }
    });
    sse_stream_response(rx)
}

/// The terminal events that close a streamed turn, from the finished response.
///
/// Text and reasoning were already streamed live through the sink, so this stage
/// never carries a `text_delta`. A completed turn reports its usage; a tool pause
/// replays each requested call as its `tool_call_started`/`tool_call_requested`
/// pair; every outcome ends with the terminal `turn_end` carrying the full
/// response, so the client reads the same `status` the buffered reply would.
pub(crate) fn terminal_events(response: TurnResponse) -> Vec<TurnEvent> {
    let mut events = Vec::new();
    match &response.outcome {
        TurnOutcome::Completed(turn) => events.push(TurnEvent::Usage(turn.usage.clone())),
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

/// Builds the terminal `turn_end` for a turn that failed after streaming began.
///
/// Pre-flight failures still return ordinary HTTP error codes; once the stream
/// has started the status line is fixed, so an error rides the terminal event as
/// a [`TurnOutcome::Error`], honouring "exactly one terminal event names the
/// outcome".
pub(crate) fn error_turn_end(error: OrchestratorError) -> TurnEvent {
    let api = ApiErrorResponse::from_code(error.api_code(), error.to_string());
    TurnEvent::TurnEnd(Box::new(TurnResponse {
        outcome: TurnOutcome::Error {
            error: api.body.error,
        },
        allowed_continuations: Vec::new(),
    }))
}

/// Renders a channel of [`TurnEvent`]s as a live `text/event-stream` body.
///
/// Each event becomes one `data:` record terminated by a blank line — the
/// Server-Sent Events framing the client parses — flushed as it arrives. The body
/// ends when every sender is dropped. A serialization failure (which the
/// documented turn types never produce) is logged and rendered as an empty frame.
pub(crate) fn sse_stream_response(rx: UnboundedReceiver<TurnEvent>) -> Response {
    let body = Body::from_stream(stream::unfold(rx, |mut rx| async move {
        let event = rx.recv().await?;
        let frame = match serde_json::to_string(&event) {
            Ok(json) => format!("data: {json}\n\n"),
            Err(error) => {
                tracing::error!(%error, "failed to serialize a turn event for streaming");
                String::new()
            }
        };
        Some((Ok::<_, std::convert::Infallible>(frame), rx))
    }));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(body)
        .expect("failed to build the streaming response")
}
