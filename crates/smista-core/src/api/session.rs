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
//!     encrypted: false,
//!     key_id: None,
//! };
//! let json = serde_json::to_string(&request).unwrap();
//! assert_eq!(json, r#"{"title":"Refactor auth","encrypted":false}"#);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::message::Message;

/// Lightweight view of a session, used in listings and on creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SessionSummary {
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
    /// Whether the session is archived.
    pub archived: bool,
}

/// Full view of a session, returned when it is fetched to be resumed.
///
/// Carries the session's `messages` and free-form `metadata`. It omits
/// `archived`, since archived sessions are not returned on fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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
    pub messages: Vec<Message>,
    /// Free-form session metadata.
    pub metadata: serde_json::Value,
}

/// Body of `POST /sessions`. The title is mandatory.
///
/// `encrypted` opts the session into end-to-end encryption and defaults to
/// `false` when omitted. When it is `true`, `key_id` is required and names the
/// fingerprint of the per-session key the client holds; when it is `false`,
/// `key_id` must be absent. The pairing is validated by the router, not by this
/// type. The choice is fixed for the life of the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct CreateSessionRequest {
    /// Title for the new session.
    pub title: String,
    /// Whether the new session is end-to-end encrypted.
    #[serde(default)]
    pub encrypted: bool,
    /// Fingerprint of the per-session key, required when `encrypted` is `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub key_id: Option<String>,
}

/// Response to `POST /sessions`, wrapping the created session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct CreateSessionResponse {
    /// The created session.
    pub session: SessionSummary,
}

/// Response to `GET /sessions/{id}`, wrapping the full session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct GetSessionResponse {
    /// The fetched session.
    pub session: SessionDetail,
}

/// Body of `PUT /sessions/{id}`, updating title and/or archive state.
///
/// Each field is optional; omit a field to leave it unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct UpdateSessionRequest {
    /// New title, if changing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// New archive state, if changing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub archived: Option<bool>,
}

/// Response to `DELETE /sessions/{id}`, confirming deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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
            title: "Refactor auth middleware".to_string(),
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

    #[test]
    fn should_roundtrip_session_detail() {
        let detail = SessionDetail {
            id: Uuid::nil(),
            title: "Refactor auth middleware".to_string(),
            encrypted: false,
            created_at: timestamp(),
            updated_at: timestamp(),
            messages: Vec::new(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert_eq!(
            serde_json::from_str::<SessionDetail>(&json).unwrap(),
            detail
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
    fn should_serialize_create_request() {
        let request = CreateSessionRequest {
            title: "Refactor auth".to_string(),
            encrypted: false,
            key_id: None,
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"title":"Refactor auth","encrypted":false}"#
        );
    }

    #[test]
    fn should_default_encrypted_to_false_when_omitted() {
        let request: CreateSessionRequest = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert!(!request.encrypted);
        assert_eq!(request.key_id, None);
    }

    #[test]
    fn should_serialize_encrypted_create_request_with_key_id() {
        let request = CreateSessionRequest {
            title: "secret".to_string(),
            encrypted: true,
            key_id: Some("kf_ab12".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"title":"secret","encrypted":true,"key_id":"kf_ab12"}"#
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
