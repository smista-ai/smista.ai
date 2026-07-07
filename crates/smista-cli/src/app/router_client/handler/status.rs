//! Router status command handler.

use smista_sdk::client::Client;

use crate::app::router_client::msg::RouterStatus;
use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Gets router health and emits [`Msg::RouterStatus`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn get_router_status(&self) {
        tracing::debug!("getting router health status");
        let msg = match self.context.router_client.status().await {
            Ok(status) => {
                tracing::debug!("router health status retrieved successfully: {status:?}");
                Msg::RouterStatus(RouterStatus {
                    status: status.status,
                    version: status.version,
                })
            }
            Err(err) => {
                tracing::error!("failed to get router health status: {err}");
                Msg::Error(format!("Failed to get router health status: {err}"))
            }
        };

        self.send_msg(msg).await;
    }
}
