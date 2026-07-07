//! Clear command handler.

use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Interrupts any active run, clears local session state, and emits [`Msg::Idle`].
    pub(in crate::app::router_client) async fn clear_session(&mut self) {
        tracing::debug!("clearing current session and resetting router client state");
        if let Err(err) = self.interrupt_active_run().await {
            tracing::error!("failed to terminate active run: {err}");
            self.send_msg(Msg::Error(format!("Failed to terminate active run: {err}")))
                .await;
        }

        self.reset_session_state();
        self.send_msg(Msg::Idle).await;
    }
}
