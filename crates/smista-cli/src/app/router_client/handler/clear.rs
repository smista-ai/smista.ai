//! Clear command handler.

use smista_sdk::client::Client as _;

use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Interrupts any active run, clears local session state, and reports completion.
    pub(in crate::app::router_client) async fn clear_session(&mut self) {
        tracing::debug!("clearing current session and resetting router client state");
        if let Err(err) = self.interrupt_active_run().await {
            tracing::error!("failed to terminate active run: {err}");
            self.send_msg(Msg::Error(format!("Failed to terminate active run: {err}")))
                .await;
        }

        let session_id = self.session_id();
        let usage = if let Some(session_id) = session_id {
            match self.context.router_client.session_usage(session_id).await {
                Ok(usage) => {
                    tracing::debug!(
                        "usage statistics retrieved successfully for session {session_id}"
                    );
                    Some(usage)
                }
                Err(err) => {
                    tracing::error!(
                        "failed to get usage statistics for session {session_id}: {err}"
                    );
                    None
                }
            }
        } else {
            None
        };

        self.reset_session_state();
        self.send_msg(Msg::Idle).await;
        self.send_msg(Msg::SessionClosed { session_id, usage })
            .await;
    }
}
