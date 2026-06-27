//! Session request and response bodies for `/sessions`.
//!
//! A session groups the messages and execution history of a piece of work.
//! These types cover its lifecycle: [`SessionSummary`] is the lightweight view
//! returned in lists and on creation, while [`SessionDetail`] is the full view
//! returned when a session is fetched to be resumed, carrying its messages and
//! free-form metadata.
//!
//! `SessionDetail` deliberately omits the `archived` flag: archived sessions
//! are not returned by `GET /sessions/{id}`, so a fetched session is always
//! active.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::CreateSessionRequest;
//!
//! let request = CreateSessionRequest {
//!     title: "Refactor auth".to_string(),
//!     key_id: None,
//! };
//! let json = serde_json::to_string(&request).unwrap();
//! assert_eq!(json, r#"{"title":"Refactor auth"}"#);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::EncryptedPayload;
use crate::message::MessageRole;
use crate::model::Provider;

/// Lightweight view of a session, used in listings and on creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct SessionSummary {
    /// Unique session identifier.
    pub id: Uuid,
    /// Human-readable session title.
    pub title: Option<String>,
    /// Whether the session's content is end-to-end encrypted.
    pub encrypted: bool,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Whether the session is archived.
    pub archived: bool,
}

/// A session message's content, in clear or sealed.
///
/// A non-encrypted session yields [`Plaintext`](Self::Plaintext) with the
/// message text. An end-to-end encrypted session yields
/// [`Encrypted`](Self::Encrypted) with the sealed [`EncryptedPayload`] envelope;
/// the router holds no key, so only a client holding the session key can open it
/// back into the message text. This mirrors
/// [`TraceEventPayload`](crate::trace::TraceEventPayload) for message bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum MessageContent {
    /// The message text in clear, for a non-encrypted session.
    Plaintext(String),
    /// The message sealed as an AEAD envelope, for an encrypted session.
    Encrypted(EncryptedPayload),
}

/// One message in a fetched [`SessionDetail`], in clear or sealed.
///
/// Mirrors [`Message`](crate::message::Message) but carries its `content` as a
/// [`MessageContent`], so an end-to-end encrypted session can return its sealed
/// body without the router ever holding the key. `provider` and `model` name the
/// model that produced an assistant turn and are absent for the other roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct SessionMessageDetail {
    /// The role of the message's author.
    pub role: MessageRole,
    /// The message content, in clear or sealed.
    pub content: MessageContent,
    /// Provider that produced the message, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub provider: Option<Provider>,
    /// Model that produced the message, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub model: Option<String>,
}

/// Full view of a session, returned when it is fetched to be resumed.
///
/// Carries the session's `messages` and free-form `metadata`. It omits
/// `archived`, since archived sessions are not returned on fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct SessionDetail {
    /// Unique session identifier.
    pub id: Uuid,
    /// Human-readable session title.
    pub title: String,
    /// Whether the session's content is end-to-end encrypted.
    pub encrypted: bool,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// The session's conversation history.
    pub messages: Vec<SessionMessageDetail>,
    /// Free-form session metadata.
    #[cfg_attr(feature = "openapi", schema(value_type = Object, nullable = true))]
    pub metadata: serde_json::Value,
}

/// Body of `POST /sessions`. The title is mandatory.
///
/// `key_id` opts the session into end-to-end encryption: a session is encrypted
/// when, and only when, a `key_id` is present, naming the fingerprint of the
/// per-session key the client holds. There is no separate `encrypted` flag, so
/// the two can never disagree. The choice is fixed for the life of the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct CreateSessionRequest {
    /// Title for the new session.
    pub title: String,
    /// Fingerprint of the per-session key; its presence makes the session
    /// end-to-end encrypted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub key_id: Option<String>,
}

/// Response to `POST /sessions`, wrapping the created session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct CreateSessionResponse {
    /// The created session.
    pub session: SessionSummary,
}

/// Response to `GET /sessions/{id}`, wrapping the full session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct GetSessionResponse {
    /// The fetched session.
    pub session: SessionDetail,
}

/// Body of `PUT /sessions/{id}`, updating title and/or archive state.
///
/// Each field is optional; omit a field to leave it unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct UpdateSessionRequest {
    /// New title, if changing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub title: Option<String>,
    /// New archive state, if changing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub archived: Option<bool>,
}

/// Response to `PUT /sessions/{id}`, wrapping the updated session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct UpdateSessionResponse {
    /// The updated session.
    pub session: SessionSummary,
}

/// Response to `DELETE /sessions/{id}`, confirming deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct DeleteSessionResponse {
    /// Whether the session was deleted.
    pub deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp() -> DateTime<Utc> {
        "2026-05-25T09:00:00Z".parse().unwrap()
    }

    fn summary() -> SessionSummary {
        SessionSummary {
            id: Uuid::nil(),
            title: Some("Refactor auth middleware".to_string()),
            encrypted: false,
            created_at: timestamp(),
            updated_at: timestamp(),
            archived: false,
        }
    }

    #[test]
    fn should_serialize_summary_with_snake_case_fields() {
        let value = serde_json::to_value(summary()).unwrap();
        assert_eq!(value["title"], "Refactor auth middleware");
        assert_eq!(value["encrypted"], false);
        assert_eq!(value["created_at"], "2026-05-25T09:00:00Z");
        assert_eq!(value["archived"], false);
    }

    fn envelope() -> EncryptedPayload {
        EncryptedPayload {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    #[test]
    fn should_roundtrip_session_detail() {
        let detail = SessionDetail {
            id: Uuid::nil(),
            title: "Refactor auth middleware".to_string(),
            encrypted: false,
            created_at: timestamp(),
            updated_at: timestamp(),
            messages: vec![
                SessionMessageDetail {
                    role: MessageRole::User,
                    content: MessageContent::Plaintext("Refactor the auth middleware.".to_string()),
                    provider: None,
                    model: None,
                },
                SessionMessageDetail {
                    role: MessageRole::Assistant,
                    content: MessageContent::Plaintext("Here is the plan...".to_string()),
                    provider: Some(Provider::Anthropic),
                    model: Some("claude-sonnet".to_string()),
                },
            ],
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert_eq!(
            serde_json::from_str::<SessionDetail>(&json).unwrap(),
            detail
        );
    }

    #[test]
    fn should_serialize_plaintext_message_content_as_clear_text() {
        let content = MessageContent::Plaintext("hello".to_string());
        assert_eq!(
            serde_json::to_value(&content).unwrap(),
            serde_json::json!({ "plaintext": "hello" })
        );
    }

    #[test]
    fn should_serialize_encrypted_message_content_as_a_sealed_envelope() {
        let content = MessageContent::Encrypted(envelope());
        let value = serde_json::to_value(&content).unwrap();
        assert_eq!(value["encrypted"]["key_id"], "kf_ab12");
        assert_eq!(value["encrypted"]["ciphertext"], "Y2lwaGVydGV4dA");
    }

    #[test]
    fn should_omit_provider_and_model_for_a_non_assistant_message() {
        let message = SessionMessageDetail {
            role: MessageRole::User,
            content: MessageContent::Plaintext("hello".to_string()),
            provider: None,
            model: None,
        };
        let value = serde_json::to_value(&message).unwrap();
        assert!(value.get("provider").is_none());
        assert!(value.get("model").is_none());
    }

    #[test]
    fn should_roundtrip_a_sealed_message() {
        let message = SessionMessageDetail {
            role: MessageRole::Assistant,
            content: MessageContent::Encrypted(envelope()),
            provider: Some(Provider::Anthropic),
            model: Some("claude-sonnet".to_string()),
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(
            serde_json::from_str::<SessionMessageDetail>(&json).unwrap(),
            message
        );
    }

    #[test]
    fn should_omit_unset_update_fields() {
        let update = UpdateSessionRequest::default();
        assert_eq!(
            serde_json::to_value(&update).unwrap(),
            serde_json::json!({})
        );
    }

    #[test]
    fn should_deserialize_partial_update() {
        let update: UpdateSessionRequest = serde_json::from_str(r#"{"archived":true}"#).unwrap();
        assert_eq!(
            update,
            UpdateSessionRequest {
                title: None,
                archived: Some(true),
            }
        );
    }

    #[test]
    fn should_wrap_the_summary_in_an_update_response() {
        let response = UpdateSessionResponse { session: summary() };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["session"]["title"], "Refactor auth middleware");
        assert_eq!(value["session"]["archived"], false);
    }

    #[test]
    fn should_serialize_create_request() {
        let request = CreateSessionRequest {
            title: "Refactor auth".to_string(),
            key_id: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"title":"Refactor auth"}"#
        );
    }

    #[test]
    fn should_default_encrypted_to_false_when_omitted() {
        let request: CreateSessionRequest = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert_eq!(request.key_id, None);
    }

    #[test]
    fn should_serialize_encrypted_create_request_with_key_id() {
        let request = CreateSessionRequest {
            title: "secret".to_string(),
            key_id: Some("kf_ab12".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"title":"secret","key_id":"kf_ab12"}"#
        );
    }

    #[test]
    fn should_serialize_delete_response() {
        assert_eq!(
            serde_json::to_value(DeleteSessionResponse { deleted: true }).unwrap(),
            serde_json::json!({ "deleted": true })
        );
    }
}
