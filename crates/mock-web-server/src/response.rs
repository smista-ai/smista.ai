use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use smista_sdk::core::api::{ApiError, ApiErrorBody, ApiErrorCode, TurnEvent};

/// HTTP status returned by a successful read or update, per the router.
const OK: u16 = 200;

/// A response served by the mock router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseTemplate {
    /// HTTP status code.
    pub(crate) status: StatusCode,
    /// Content type header value.
    pub(crate) content_type: &'static str,
    /// Raw response bytes.
    pub(crate) body: Vec<u8>,
}

impl ResponseTemplate {
    /// Creates an empty response with `status`.
    #[must_use]
    pub fn new(status: u16) -> Self {
        Self {
            status: StatusCode::from_u16(status).expect("the response status code is valid"),
            content_type: "application/octet-stream",
            body: Vec::new(),
        }
    }

    /// Sets a JSON body.
    #[must_use]
    pub fn set_body_json<T>(mut self, body: T) -> Self
    where
        T: Serialize,
    {
        self.content_type = "application/json";
        self.body = serde_json::to_vec(&body).expect("the canned JSON body serializes");
        self
    }

    /// Sets a UTF-8 text body.
    #[must_use]
    pub fn set_body_string(mut self, body: impl Into<String>) -> Self {
        self.content_type = "text/plain; charset=utf-8";
        self.body = body.into().into_bytes();
        self
    }

    /// Sets a raw body and content type.
    #[must_use]
    pub fn set_body_raw(mut self, body: Vec<u8>, content_type: &'static str) -> Self {
        self.content_type = content_type;
        self.body = body;
        self
    }

    /// Converts this template into an HTTP response.
    pub(crate) fn into_response(self) -> Response {
        (
            self.status,
            [("content-type", self.content_type)],
            self.body,
        )
            .into_response()
    }
}

/// Builds an error response carrying the router's structured [`ApiError`] body.
#[must_use]
pub fn api_error(code: ApiErrorCode, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(code.status().as_u16()).set_body_json(ApiError {
        error: ApiErrorBody {
            code: code.as_str().to_owned(),
            message: message.to_owned(),
            details: None,
        },
    })
}

/// Builds a `text/event-stream` response replaying `events`.
#[must_use]
pub fn sse(events: &[TurnEvent]) -> ResponseTemplate {
    let body: String = events
        .iter()
        .map(|event| {
            let json = serde_json::to_string(event).expect("a turn event serializes");
            format!("data: {json}\n\n")
        })
        .collect();
    ResponseTemplate::new(OK).set_body_raw(body.into_bytes(), "text/event-stream")
}
