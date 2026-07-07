//! Execution trace command handler.

use smista_sdk::client::Client;
use smista_sdk::core::api::EncryptedPayload;
use smista_sdk::core::trace::{Payload, TraceEventPayload, TraceEventType};
use uuid::Uuid;

use crate::app::router_client::msg::{TraceEvent, TraceSummary};
use crate::app::router_client::{Msg, RouterClient};

/// Number of trace events requested per page.
const TRACE_LIMIT: u64 = 100;

impl RouterClient {
    /// Gets the current session trace and emits [`Msg::Trace`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn get_traces(&self) {
        let Some(session_id) = self.session_id() else {
            tracing::warn!("no active session, cannot get execution trace");
            self.send_msg(Msg::Error(
                "No active session, cannot get execution trace".to_string(),
            ))
            .await;
            return;
        };

        let mut events = Vec::new();
        let mut offset = 0;
        loop {
            let traces = match self
                .context
                .router_client
                .get_session_traces(session_id, Some(TRACE_LIMIT), Some(offset))
                .await
            {
                Ok(traces) => traces,
                Err(err) => {
                    tracing::error!(
                        "failed to get execution trace for session {session_id}: {err}"
                    );
                    self.send_msg(Msg::Error(format!(
                        "Failed to get execution trace for session {session_id}: {err}"
                    )))
                    .await;
                    return;
                }
            };

            let page_len = traces.trace.events.len();
            if page_len == 0 {
                tracing::debug!(
                    "no more trace events for session {session_id}, stopping trace retrieval"
                );
                break;
            }

            for event in traces.trace.events {
                let Some(payload) = self
                    .render_trace_event_payload(session_id, event.payload)
                    .await
                else {
                    continue;
                };

                let event_type = trace_event_type_label(event.event_type);
                let task_type = event.task_type.as_str();
                tracing::debug!(
                    "got trace event for session {session_id}: {event_type} - {task_type}"
                );

                events.push(TraceEvent {
                    event_type,
                    task_type,
                    provider: event.provider.to_string(),
                    model: event.model,
                    matched_rule: event.matched_rule,
                    created_at: event.created_at.to_rfc2822(),
                    payload,
                });
            }

            offset += page_len as u64;
            if page_len < TRACE_LIMIT as usize {
                break;
            }
        }

        self.send_msg(Msg::Trace(TraceSummary { events })).await;
    }

    /// Opens and renders a trace payload for display.
    async fn render_trace_event_payload(
        &self,
        session_id: Uuid,
        payload: TraceEventPayload,
    ) -> Option<String> {
        match payload {
            TraceEventPayload::Plaintext(plaintext) => Some(render_trace_payload(&plaintext)),
            TraceEventPayload::Encrypted(ciphertext) => {
                self.render_encrypted_trace_payload(session_id, &ciphertext)
                    .await
            }
        }
    }

    /// Decrypts, deserializes, and renders an encrypted trace payload.
    async fn render_encrypted_trace_payload(
        &self,
        session_id: Uuid,
        ciphertext: &EncryptedPayload,
    ) -> Option<String> {
        let plaintext = match self.context.e2ee_keys.decrypt_payload(ciphertext) {
            Ok(plaintext) => plaintext,
            Err(err) => {
                tracing::error!(
                    "failed to decrypt trace event payload for session {session_id}: {err}"
                );
                self.send_msg(Msg::Error(format!(
                    "Failed to decrypt trace event payload for session {session_id}: {err}"
                )))
                .await;
                return None;
            }
        };

        let payload = match serde_json::from_str::<Payload>(&plaintext) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::error!(
                    "failed to parse trace event payload for session {session_id}: {err}"
                );
                self.send_msg(Msg::Error(format!(
                    "Failed to parse trace event payload for session {session_id}: {err}"
                )))
                .await;
                return None;
            }
        };

        Some(render_trace_payload(&payload))
    }
}

/// Renders a typed trace payload for the UI.
#[must_use]
pub(in crate::app::router_client) fn render_trace_payload(payload: &Payload) -> String {
    serde_json::to_string(payload).unwrap_or_else(|err| {
        tracing::error!("failed to render trace event payload: {err}");
        format!("{payload:?}")
    })
}

/// Returns the stable UI label for a trace event type.
#[must_use]
fn trace_event_type_label(event_type: TraceEventType) -> &'static str {
    match event_type {
        TraceEventType::Message => "message",
        TraceEventType::Classification => "classification",
        TraceEventType::RoutingDecision => "routing_decision",
        TraceEventType::ContextSelection => "context_selection",
        TraceEventType::ToolCall => "tool_call",
        TraceEventType::Approval => "approval",
        TraceEventType::Cost => "cost",
    }
}
