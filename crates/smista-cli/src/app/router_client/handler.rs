//! Command handlers for router-client commands.

mod catalog;
mod clear;
mod continuation;
mod execute;
mod preview;
mod session;
mod status;
mod stream;
mod trace;
mod usage;

use smista_sdk::client::Client;
use smista_sdk::core::api::ContinueRequest;

use super::{Msg, RouterClient, State};

impl RouterClient {
    /// Resets all state that is scoped to the current session.
    pub(in crate::app::router_client) fn reset_session_state(&mut self) {
        self.session = None;
        self.approvals.clear();
        self.pending_seals.clear();
        self.pending_tool_prompts.clear();
        self.pending_tool_requests.clear();
        self.pending_tool_results.clear();
        self.state = State::Idle;
    }

    /// Terminates any active run before resetting local session state.
    ///
    /// # Errors
    ///
    /// Returns an error if the router rejects or fails the interrupt request.
    pub(in crate::app::router_client) async fn terminate_active_run(
        &mut self,
    ) -> anyhow::Result<()> {
        tracing::debug!("terminating active run and clearing current session");
        self.interrupt_active_run().await?;
        tracing::debug!("active run interrupted; clearing current session");
        self.reset_session_state();

        Ok(())
    }

    /// Interrupts any active run by sending a break continuation to the router.
    ///
    /// If there is no active run or no current session id, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error if sending the break continuation fails.
    pub(in crate::app::router_client) async fn interrupt_active_run(
        &mut self,
    ) -> anyhow::Result<()> {
        if self.state == State::Idle {
            tracing::warn!("tried to interrupt active run, but no active run is present");
            return Ok(());
        }
        let Some(id) = self.session_id() else {
            tracing::warn!("tried to interrupt active run, but no session id is present");
            return Ok(());
        };
        tracing::debug!("interrupting active run");

        self.context
            .router_client
            .continue_run(id, ContinueRequest::Break)
            .await
            .map_err(anyhow::Error::from)
            .map(|_| ())
    }

    /// Sends a router-client message to the UI.
    ///
    /// If the receiver is gone, the application exit token is cancelled so the
    /// worker topology shuts down together.
    pub(in crate::app::router_client) async fn send_msg(&self, msg: Msg) {
        if let Err(err) = self.msg_tx.send(msg).await {
            tracing::error!("failed to send message: {err}");
            self.context.exit.cancel();
        }
    }
}
