use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use smista_sdk::core::api::{ApiError, ApiErrorBody, ApiErrorCode, TurnEvent};
use tokio::sync::Notify;

/// HTTP status returned by a successful read or update, per the router.
const OK: u16 = 200;

/// A response served by the mock router.
#[derive(Debug, Clone)]
pub struct ResponseTemplate {
    /// HTTP status code.
    pub(crate) status: StatusCode,
    /// Content type header value.
    pub(crate) content_type: &'static str,
    /// Raw response bytes.
    pub(crate) body: Vec<u8>,
    /// Optional synchronization gate blocking this response.
    pub(crate) gate: Option<ResponseGate>,
}

impl ResponseTemplate {
    /// Creates an empty response with `status`.
    #[must_use]
    pub fn new(status: u16) -> Self {
        Self {
            status: StatusCode::from_u16(status).expect("the response status code is valid"),
            content_type: "application/octet-stream",
            body: Vec::new(),
            gate: None,
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

    /// Blocks this response until `gate` is opened.
    #[must_use]
    pub fn set_gate(mut self, gate: ResponseGate) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Converts this template into an HTTP response.
    pub(crate) async fn into_response(self) -> Response {
        if let Some(gate) = self.gate {
            gate.wait().await;
        }

        (
            self.status,
            [("content-type", self.content_type)],
            self.body,
        )
            .into_response()
    }
}

/// Synchronizes a mock response with a test driver.
#[derive(Debug, Clone)]
pub struct ResponseGate {
    inner: Arc<ResponseGateInner>,
}

#[derive(Debug)]
struct ResponseGateInner {
    entered: AtomicBool,
    entered_notify: Notify,
    opened: AtomicBool,
    opened_notify: Notify,
}

impl ResponseGate {
    /// Creates a closed response gate.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ResponseGateInner {
                entered: AtomicBool::new(false),
                entered_notify: Notify::new(),
                opened: AtomicBool::new(false),
                opened_notify: Notify::new(),
            }),
        }
    }

    /// Waits until a mock request reaches the gated response.
    pub async fn wait_until_blocked(&self) {
        let notified = self.inner.entered_notify.notified();
        if !self.inner.entered.load(Ordering::Acquire) {
            notified.await;
        }
    }

    /// Opens the gate and releases the blocked response.
    pub fn open(&self) {
        self.inner.opened.store(true, Ordering::Release);
        self.inner.opened_notify.notify_one();
    }

    async fn wait(&self) {
        self.inner.entered.store(true, Ordering::Release);
        self.inner.entered_notify.notify_one();

        let notified = self.inner.opened_notify.notified();
        if !self.inner.opened.load(Ordering::Acquire) {
            notified.await;
        }
    }
}

impl Default for ResponseGate {
    fn default() -> Self {
        Self::new()
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
