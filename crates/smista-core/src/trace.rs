//! Execution traces recording how a session's tasks were routed and run.
//!
//! A [`Trace`] is the deterministic record of a session's execution: the ordered
//! [`TraceEvent`]s that were emitted, each carrying the routing context of the
//! task it belongs to and a free-form payload. The router produces it, the HTTP
//! API ([`crate::api`]) returns it for the `/trace` and `/why` views, and
//! smista-storage persists the events and assembles this view.
//!
//! Event payloads are typed. Each [`TraceEvent`] carries a [`TraceEventPayload`]:
//! either the decrypted [`Payload`] — a union tagged by event kind over the
//! per-kind shapes — or, for an end-to-end encrypted session, the sealed
//! [`EncryptedPayload`] envelope the client must open to read it.
//!
//! # Examples
//!
//! ```
//! use smista_core::trace::Trace;
//! use uuid::Uuid;
//!
//! let trace = Trace {
//!     session_id: Uuid::nil(),
//!     events: Vec::new(),
//! };
//! assert!(trace.events.is_empty());
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "surrealdb")]
use surrealdb_types::SurrealValue;
use uuid::Uuid;

use crate::api::{ApprovalDecision, EncryptedPayload};
use crate::intent::TaskIntent;
use crate::message::MessageRole;
use crate::model::Provider;
use crate::policy::Classification;
use crate::tool::ToolCallStatus;

/// The kind of a trace event.
///
/// **Provisional**: these variants are a placeholder until the private spec
/// pins the trace taxonomy. Serialized as `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "surrealdb", derive(SurrealValue))]
#[cfg_attr(feature = "surrealdb", surreal(crate = "::surrealdb_types"))]
#[cfg_attr(feature = "surrealdb", surreal(rename_all = "snake_case", untagged))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum TraceEventType {
    /// A message was recorded.
    Message,
    /// The router classified the task's intent.
    Classification,
    /// The router selected a provider/model for a task.
    RoutingDecision,
    /// Context was selected or excluded for a task.
    ContextSelection,
    /// A tool was requested or executed.
    ToolCall,
    /// A confirmation decision was recorded.
    Approval,
    /// Token usage or cost was recorded.
    Cost,
}

/// Payload of a [`TraceEventType::Message`] event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct MessagePayload {
    /// Role of the recorded message.
    pub role: MessageRole,
    /// Provider that served the message.
    pub provider: Provider,
    /// Model that served the message.
    pub model: String,
}

/// Payload of a [`TraceEventType::RoutingDecision`] event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct RoutingDecisionPayload {
    /// Provider that was selected.
    pub provider: Provider,
    /// Model that was selected.
    pub model: String,
    /// Routing rule that matched, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub matched_rule: Option<String>,
    /// Whether a fallback route was used.
    pub fallback_used: bool,
    /// Whether a per-request override was used.
    pub override_used: bool,
    /// Human-readable explanation of the decision.
    pub reason: String,
}

/// Payload of a [`TraceEventType::ContextSelection`] event.
///
/// Records a reference to the selected or excluded context — its path and kind —
/// never the context body itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ContextSelectionPayload {
    /// Path of the referenced context, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub path: Option<String>,
    /// Kind of context (for example `git_diff` or `file`).
    pub kind: String,
    /// Whether the context was included (`true`) or excluded (`false`).
    pub included: bool,
    /// Human-readable explanation of the selection.
    pub reason: String,
}

/// Payload of a [`TraceEventType::ToolCall`] event.
///
/// `arguments`, `result` and `error` are recorded as sanitized references, never
/// raw tool input or output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ToolCallPayload {
    /// Name of the tool that was requested or executed.
    pub tool_name: String,
    /// Lifecycle status of the tool call.
    pub status: ToolCallStatus,
    /// Reference to the tool arguments, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub arguments: Option<String>,
    /// Reference to the tool result, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub result: Option<String>,
    /// Reference to the tool error, if the call failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub error: Option<String>,
}

/// Payload of a [`TraceEventType::Approval`] event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ApprovalPayload {
    /// Type of the entity the decision applies to (for example `tool_call`).
    pub target_type: String,
    /// Identifier of the entity the decision applies to.
    pub target_id: String,
    /// The decision that was recorded.
    pub decision: ApprovalDecision,
    /// Human-readable reason for the decision, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reason: Option<String>,
}

/// Payload of a [`TraceEventType::Cost`] event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct CostPayload {
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Input tokens consumed.
    pub input_tokens: u64,
    /// Output tokens produced.
    pub output_tokens: u64,
    /// Estimated cost as a decimal string, if known. Never a float.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cost: Option<String>,
}

/// The decrypted payload of a [`TraceEvent`], tagged by event kind.
///
/// Serialized internally tagged under `type` (for example
/// `{ "type": "message", "role": "user", … }`), so it deserializes on its own
/// once a sealed payload has been opened. Its variant always matches the
/// event's [`TraceEventType`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum Payload {
    /// A recorded message.
    Message(MessagePayload),
    /// An intent classification result.
    Classification(Classification),
    /// A routing decision.
    RoutingDecision(RoutingDecisionPayload),
    /// A context selection or exclusion.
    ContextSelection(ContextSelectionPayload),
    /// A tool request or execution.
    ToolCall(ToolCallPayload),
    /// A confirmation decision.
    Approval(ApprovalPayload),
    /// Recorded token usage or cost.
    Cost(CostPayload),
}

impl Payload {
    /// Returns the [`TraceEventType`] that matches this payload's variant.
    #[must_use]
    pub fn event_type(&self) -> TraceEventType {
        match self {
            Self::Message(_) => TraceEventType::Message,
            Self::Classification(_) => TraceEventType::Classification,
            Self::RoutingDecision(_) => TraceEventType::RoutingDecision,
            Self::ContextSelection(_) => TraceEventType::ContextSelection,
            Self::ToolCall(_) => TraceEventType::ToolCall,
            Self::Approval(_) => TraceEventType::Approval,
            Self::Cost(_) => TraceEventType::Cost,
        }
    }
}

/// A [`TraceEvent`]'s payload, in clear or sealed.
///
/// A non-encrypted session yields [`Plaintext`](Self::Plaintext) with the typed
/// [`Payload`]. An end-to-end encrypted session yields
/// [`Encrypted`](Self::Encrypted) with the sealed [`EncryptedPayload`] envelope;
/// the router holds no key, so only a client holding the session key can open it
/// into a [`Payload`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum TraceEventPayload {
    /// The payload in clear, for a non-encrypted session.
    Plaintext(Payload),
    /// The payload sealed as an AEAD envelope, for an encrypted session.
    Encrypted(EncryptedPayload),
}

/// A single recorded execution event within a [`Trace`].
///
/// Each event carries the routing context of the task that emitted it
/// (`task_type`, `provider`, `model`, `matched_rule`) and a typed `payload`
/// whose variant matches `event_type`. For an encrypted session the payload is
/// sealed, so `event_type` still names the kind without opening it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct TraceEvent {
    /// Kind of trace event.
    pub event_type: TraceEventType,
    /// Detected intent that drove routing for this event's task.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Description of the routing rule that matched, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub matched_rule: Option<String>,
    /// When the event occurred.
    pub created_at: DateTime<Utc>,
    /// Typed event payload, in clear or sealed.
    pub payload: TraceEventPayload,
}

/// The recorded outcome of routing and running a session's tasks.
///
/// `events` holds the session's ordered execution events, oldest first. Each
/// [`TraceEvent`] is self-describing — it carries its own routing context — so
/// the trace needs no top-level routing fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Trace {
    /// Session the traced tasks belong to.
    pub session_id: Uuid,
    /// Ordered execution events, oldest first.
    pub events: Vec<TraceEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> Payload {
        Payload::RoutingDecision(RoutingDecisionPayload {
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
            matched_rule: Some("rule".to_string()),
            fallback_used: false,
            override_used: false,
            reason: "best for edit".to_string(),
        })
    }

    fn event() -> TraceEvent {
        TraceEvent {
            event_type: TraceEventType::RoutingDecision,
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
            matched_rule: Some("rule".to_string()),
            created_at: Utc::now(),
            payload: TraceEventPayload::Plaintext(payload()),
        }
    }

    fn sample() -> Trace {
        Trace {
            session_id: Uuid::nil(),
            events: vec![event()],
        }
    }

    #[test]
    fn should_serialize_with_snake_case_fields() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["session_id"], Uuid::nil().to_string());
        assert_eq!(value["events"][0]["event_type"], "routing_decision");
        assert_eq!(value["events"][0]["task_type"], "edit");
        // The plaintext payload is tagged by kind under `type`.
        assert_eq!(
            value["events"][0]["payload"]["plaintext"]["type"],
            "routing_decision"
        );
        assert_eq!(
            value["events"][0]["payload"]["plaintext"]["model"],
            "claude-sonnet"
        );
    }

    #[test]
    fn should_tag_payload_by_event_kind() {
        assert_eq!(payload().event_type(), TraceEventType::RoutingDecision);
    }

    #[test]
    fn should_serialize_encrypted_payload_as_envelope() {
        let event = TraceEvent {
            payload: TraceEventPayload::Encrypted(EncryptedPayload {
                version: 1,
                algorithm: "xchacha20poly1305".to_string(),
                key_id: "kf_ab12".to_string(),
                nonce: "bm9uY2U".to_string(),
                ciphertext: "Y2lwaGVydGV4dA".to_string(),
            }),
            ..event()
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["payload"]["encrypted"]["key_id"], "kf_ab12");
        assert!(value["payload"]["encrypted"]["ciphertext"].is_string());
    }

    #[test]
    fn should_omit_absent_matched_rule() {
        let event = TraceEvent {
            matched_rule: None,
            ..event()
        };
        let value = serde_json::to_value(event).unwrap();
        assert!(value.get("matched_rule").is_none());
    }

    #[test]
    fn should_roundtrip_serde() {
        let trace = sample();
        let json = serde_json::to_string(&trace).unwrap();
        assert_eq!(serde_json::from_str::<Trace>(&json).unwrap(), trace);
    }
}
