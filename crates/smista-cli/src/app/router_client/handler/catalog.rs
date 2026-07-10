//! Provider and model catalog command handlers.

use smista_sdk::client::Client;

use crate::app::router_client::msg::{Model, Provider};
use crate::app::router_client::{Msg, RouterClient};

impl RouterClient {
    /// Lists available models and emits [`Msg::ModelsList`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn list_models(&self) {
        tracing::debug!("listing models available on the router for this user");
        let msg = match self.context.router_client.list_models().await {
            Ok(models) => {
                let models = models
                    .models
                    .into_iter()
                    .map(|model| Model {
                        reference: model.reference(),
                        provider: model.provider.to_string(),
                        display_name: model.display_name.unwrap_or_else(|| model.model.clone()),
                        id: model.model,
                        max_context_tokens: model.max_context_tokens,
                        max_output_tokens: model.max_output_tokens,
                        input_cost_per_million_tokens: model.input_cost_per_million_tokens,
                        output_cost_per_million_tokens: model.output_cost_per_million_tokens,
                    })
                    .collect::<Vec<_>>();

                tracing::debug!("{count} models listed successfully", count = models.len());
                Msg::ModelsList(models)
            }
            Err(err) => {
                tracing::error!("failed to list models: {err}");
                Msg::Error(format!("Failed to list models: {err}"))
            }
        };

        self.send_msg(msg).await;
    }

    /// Lists available providers and emits [`Msg::ProvidersList`] or [`Msg::Error`].
    pub(in crate::app::router_client) async fn list_providers(&self) {
        tracing::debug!("listing providers available on the router for this user");
        let msg = match self.context.router_client.list_providers().await {
            Ok(providers) => {
                let providers = providers
                    .providers
                    .into_iter()
                    .map(|provider| Provider {
                        name: provider.display_name,
                        local: provider.local,
                    })
                    .collect::<Vec<_>>();

                tracing::debug!(
                    "{count} providers listed successfully",
                    count = providers.len()
                );
                Msg::ProvidersList(providers)
            }
            Err(err) => {
                tracing::error!("failed to list providers: {err}");
                Msg::Error(format!("Failed to list providers: {err}"))
            }
        };

        self.send_msg(msg).await;
    }
}
