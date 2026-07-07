//! Session usage command handler.

use smista_sdk::client::Client;

use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Gets usage for the current session and emits [`Msg::Usage`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn get_usage(&self) {
        tracing::debug!("getting usage statistics for this session");
        let Some(session_id) = self.session_id() else {
            tracing::warn!("no active session, cannot get usage statistics");
            self.send_msg(Msg::Error(
                "No active session, cannot get usage statistics".to_string(),
            ))
            .await;
            return;
        };

        let msg = match self.context.router_client.session_usage(session_id).await {
            Ok(usage) => {
                tracing::debug!("usage statistics retrieved successfully for session {session_id}");
                Msg::Usage(usage)
            }
            Err(err) => {
                tracing::error!("failed to get usage statistics for session {session_id}: {err}");
                Msg::Error(format!(
                    "Failed to get usage statistics for session {session_id}: {err}"
                ))
            }
        };

        self.send_msg(msg).await;
    }
}
