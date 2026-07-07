//! Execute command handler.

use std::collections::HashSet;
use std::path::PathBuf;

use smista_sdk::client::Client as _;
use smista_sdk::core::model::ModelReference;

use crate::app::router_client::msg::PreviewSummary;
use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Handles the execution of a `preview` command.
    ///
    /// On success, sends a [`Msg::Preview`] to the UI with the preview results.
    /// On failure, sends a [`Msg::Error`] to the UI with the error message.
    pub(in crate::app::router_client) async fn preview(
        &mut self,
        prompt: String,
        files: HashSet<PathBuf>,
        plan: bool,
        explicit_model: Option<ModelReference>,
    ) {
        tracing::debug!(
            explicit_model = explicit_model.as_ref().map(ToString::to_string),
            files = ?files.iter().collect::<Vec<_>>(),
            plan,
            prompt.bytes = prompt.len(),
            "previewing prompt",
        );
        let session_id = match self.session_id_or_new(&prompt).await {
            Ok(session_id) => session_id,
            Err(err) => {
                tracing::error!("failed to initialize session: {err}");
                self.send_msg(Msg::Error(format!(
                    "Failed to initialize a new session: {err}"
                )))
                .await;

                return;
            }
        };

        let execute_request = self
            .build_execute_request(prompt, files, plan, explicit_model)
            .await;

        // send Thinking state
        self.send_msg(Msg::Thinking).await;

        // send preview request to router
        match self
            .context
            .router_client
            .preview(session_id, execute_request)
            .await
        {
            Ok(response) => {
                let task_type = response.task_type.to_string();
                let provider = response.provider.to_string();
                let model = response.model.to_string();
                tracing::debug!(
                    task_type,
                    provider,
                    model,
                    "preview accepted, sending preview results to UI"
                );

                self.send_msg(Msg::Preview(PreviewSummary {
                    task_type,
                    provider,
                    model,
                    required_permissions: response
                        .required_permissions
                        .into_iter()
                        .map(|p| p.permission)
                        .collect(),
                }))
                .await;
            }
            Err(err) => {
                tracing::error!("failed to preview prompt: {err}");
                self.send_msg(Msg::Error(format!(
                    "Failed to preview prompt through router: {err}"
                )))
                .await;
            }
        };

        // reset state to Idle after preview
        self.send_msg(Msg::Idle).await;
    }
}
