//! Session list and resume command handlers.

use smista_sdk::client::Client;
use smista_sdk::core::api::{EncryptedPayload, MessageContent, SessionMessageDetail};
use smista_sdk::core::message::MessageRole;
use uuid::Uuid;

use crate::app::router_client::msg::{ResumedSession, SessionListItem, SessionMessage};
use crate::app::router_client::state::State;
use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Lists sessions for the current workspace and emits [`Msg::SessionsList`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn list_sessions(&self) {
        tracing::debug!("listing sessions available on the router for this user");
        let msg = match self
            .context
            .router_client
            .list_sessions(Some(self.scope()), None)
            .await
        {
            Ok(sessions) => {
                let sessions_list = sessions
                    .sessions
                    .into_iter()
                    .map(|session| SessionListItem {
                        id: session.id,
                        title: session.title,
                        scope: session.scope,
                        updated_at: session.updated_at.to_rfc2822(),
                    })
                    .collect::<Vec<_>>();

                tracing::debug!(
                    "{count} sessions listed successfully",
                    count = sessions_list.len()
                );
                Msg::SessionsList(sessions_list)
            }
            Err(err) => {
                tracing::error!("failed to list sessions: {err}");
                Msg::Error(format!("Failed to list sessions: {err}"))
            }
        };

        self.send_msg(msg).await;
    }

    /// Loads a session transcript and emits [`Msg::ResumedSession`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn resume_session(&mut self, session_id: Uuid) {
        tracing::debug!("clearing current session and resuming session {session_id}");
        if let Err(err) = self.terminate_active_run().await {
            tracing::error!("failed to terminate active run: {err}");
            self.send_msg(Msg::Error(format!("Failed to terminate active run: {err}")))
                .await;
        }

        let msg = match self.context.router_client.get_session(session_id).await {
            Ok(session) => {
                let mut messages = Vec::with_capacity(session.session.messages.len());
                for message in session.session.messages {
                    let Some(message) = self.render_session_message(session_id, message).await
                    else {
                        continue;
                    };
                    messages.push(message);
                }

                tracing::debug!(
                    "resumed session {session_id} successfully with {count} messages",
                    count = messages.len()
                );

                self.session_id = Some(session_id);
                self.approvals.clear();
                self.state = State::Idle;

                Msg::ResumedSession(ResumedSession {
                    id: session.session.id,
                    title: session.session.title,
                    messages,
                })
            }
            Err(err) => {
                tracing::error!("failed to get session {session_id}: {err}");
                Msg::Error(format!("Failed to get session {session_id}: {err}"))
            }
        };

        self.send_msg(msg).await;
    }

    /// Renders one fetched session message for the UI transcript.
    async fn render_session_message(
        &self,
        session_id: Uuid,
        message: SessionMessageDetail,
    ) -> Option<SessionMessage> {
        let role = message_role_label(message.role).to_string();
        let content = self
            .decrypt_message_content_if_needed(session_id, message.content)
            .await?;

        Some(SessionMessage { role, content })
    }

    /// Opens encrypted message content and returns plaintext content unchanged.
    async fn decrypt_message_content_if_needed(
        &self,
        session_id: Uuid,
        content: MessageContent,
    ) -> Option<String> {
        match content {
            MessageContent::Plaintext(plaintext) => Some(plaintext),
            MessageContent::Encrypted(ciphertext) => {
                self.decrypt_session_message_content(session_id, &ciphertext)
                    .await
            }
        }
    }

    /// Decrypts one encrypted session message payload.
    async fn decrypt_session_message_content(
        &self,
        session_id: Uuid,
        ciphertext: &EncryptedPayload,
    ) -> Option<String> {
        match self.context.e2ee_keys.decrypt_payload(ciphertext) {
            Ok(plaintext) => Some(plaintext),
            Err(err) => {
                tracing::error!(
                    "failed to decrypt message content for session {session_id}: {err}"
                );
                self.send_msg(Msg::Error(format!(
                    "Failed to decrypt message content for session {session_id}: {err}"
                )))
                .await;
                None
            }
        }
    }
}

/// Returns the stable UI label for a message role.
#[must_use]
fn message_role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::Assistant => "assistant",
        MessageRole::User => "user",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
    }
}
