//! Conversation messages and the roles their authors play.
//!
//! A [`Message`] is one turn in a conversation: a role, its text content, and —
//! for assistant turns — which model produced it. These types are shared by
//! storage (session history), trace and the execute API, so they stay
//! provider-agnostic and serialization-friendly.
//!
//! # Examples
//!
//! ```
//! use smista_core::message::{Message, MessageRole};
//!
//! let message = Message {
//!     role: MessageRole::User,
//!     content: "Summarize the changelog.".to_string(),
//!     provider: None,
//!     model: None,
//! };
//! assert_eq!(message.role, MessageRole::User);
//! ```

use serde::{Deserialize, Serialize};

use crate::model::Provider;

/// The role an author plays in a conversation.
///
/// Each variant serializes to its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MessageRole {
    /// Instructions that frame the conversation.
    System,
    /// Input from the end user.
    User,
    /// A model's response.
    Assistant,
    /// The result of a tool invocation.
    Tool,
}

/// A single message in a conversation.
///
/// `provider` and `model` identify which model produced an assistant turn; they
/// are `None` for user, system and tool messages, and may be `None` for an
/// assistant turn whose origin is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Message {
    /// The role of the message's author.
    pub role: MessageRole,
    /// The textual content of the message.
    pub content: String,
    /// Provider that produced the message, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<Provider>,
    /// Model that produced the message, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ROLES: [MessageRole; 4] = [
        MessageRole::System,
        MessageRole::User,
        MessageRole::Assistant,
        MessageRole::Tool,
    ];

    #[test]
    fn should_serialize_role_to_lowercase_name() {
        assert_eq!(
            serde_json::to_string(&MessageRole::Assistant).unwrap(),
            "\"assistant\""
        );
    }

    #[test]
    fn should_roundtrip_role_for_every_variant() {
        for role in ALL_ROLES {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(serde_json::from_str::<MessageRole>(&json).unwrap(), role);
        }
    }

    #[test]
    fn should_omit_absent_provider_and_model() {
        let message = Message {
            role: MessageRole::User,
            content: "hello".to_string(),
            provider: None,
            model: None,
        };
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            serde_json::json!({ "role": "user", "content": "hello" })
        );
    }

    #[test]
    fn should_roundtrip_assistant_message_with_origin() {
        let message = Message {
            role: MessageRole::Assistant,
            content: "done".to_string(),
            provider: Some(Provider::Anthropic),
            model: Some("claude-sonnet".to_string()),
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(serde_json::from_str::<Message>(&json).unwrap(), message);
    }
}
