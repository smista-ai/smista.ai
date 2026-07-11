//! Execute command handler.

use std::collections::HashSet;
use std::path::PathBuf;

use smista_sdk::client::Client as _;
use smista_sdk::core::model::ModelReference;
use smista_sdk::core::policy::{Confidence, IntentSource};

use crate::app::router_client::msg::{PreviewPermissionSummary, PreviewSummary};
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
                let classification_source = match response.classification.source {
                    IntentSource::Explicit => "explicit",
                    IntentSource::Inferred => "inferred",
                }
                .to_owned();
                let classification_confidence =
                    response.classification.confidence.map(|confidence| {
                        match confidence {
                            Confidence::Low => "low",
                            Confidence::Medium => "medium",
                            Confidence::High => "high",
                        }
                        .to_owned()
                    });
                tracing::debug!(
                    task_type = %response.routing.intent,
                    provider = %response.routing.provider,
                    model = %response.routing.model,
                    "preview accepted, sending preview results to UI"
                );

                self.send_msg(Msg::Preview(PreviewSummary {
                    routing: response.routing,
                    classification_source,
                    classification_reason: response.classification.reason,
                    classification_confidence,
                    included_context: response.included_context,
                    excluded_context: response.excluded_context,
                    estimated_cost_min: response.estimated_cost.min.to_string(),
                    estimated_cost_max: response.estimated_cost.max.to_string(),
                    estimated_cost_currency: response.estimated_cost.currency,
                    required_permissions: response
                        .required_permissions
                        .into_iter()
                        .map(|permission| PreviewPermissionSummary {
                            permission: permission.permission,
                            mode: permission.mode.to_string(),
                        })
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
