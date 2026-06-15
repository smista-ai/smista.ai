//! Anthropic models

use std::sync::Arc;

use rig_core::providers::anthropic::client::Client as AnthropicClient;
use rust_decimal::Decimal;
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::{ModelDescriptor, ModelReference, Provider};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::auth::Authentication;
use crate::memory::{MemoryScope, MemoryStorage};
use crate::model::Model;

/// Returns the input and output price per million tokens for `model_id`.
///
/// Anthropic prices per model family, and the `/v1/models` listing does not
/// report price, so the family is inferred from the id and the prices are kept
/// here. A model whose id matches no known family is left unpriced (`None`).
///
/// Family granularity is deliberate: legacy Opus ids (`claude-opus-4`,
/// `claude-opus-4-1`) are priced at the current Opus rate rather than their
/// historical rate, which is acceptable because they are not part of the
/// routing set in practice.
pub fn family_pricing(model_id: &str) -> (Option<Decimal>, Option<Decimal>) {
    let id = model_id.to_ascii_lowercase();
    let (input, output) = if id.contains("opus") {
        (5, 25)
    } else if id.contains("sonnet") {
        (3, 15)
    } else if id.contains("haiku") {
        (1, 5)
    } else if id.contains("fable") {
        (10, 50)
    } else {
        return (None, None);
    };
    (Some(Decimal::new(input, 0)), Some(Decimal::new(output, 0)))
}

/// Arguments for creating a new Anthropic model.
///
/// Carries the base system prompt and memory backend every Anthropic model needs
/// to construct its underlying agent. The credential is not held here: it is
/// supplied per request as an [`Authentication`]
/// when the model is resolved.
pub struct AnthropicModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    /// The base system prompt; the model appends its memory preamble to this.
    pub preamble: String,
    /// The memory backend the model reads to build its preamble and writes
    /// through for the memory tool.
    pub storage: Arc<S>,
}

// `#[derive(Debug)]` would add a spurious `S: Debug` bound; render the storage
// type name by hand instead.
impl<S> std::fmt::Debug for AnthropicModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicModelArgs")
            .field("preamble", &self.preamble)
            .field(
                "storage",
                &format_args!("Arc<{}>", std::any::type_name::<S>()),
            )
            .finish()
    }
}

// `#[derive(Clone)]` would add a spurious `S: Clone` bound; `Arc<S>` is `Clone`
// for any `S`, so implement it by hand without the bound.
impl<S> Clone for AnthropicModelArgs<S>
where
    S: MemoryStorage + 'static,
{
    fn clone(&self) -> Self {
        Self {
            preamble: self.preamble.clone(),
            storage: Arc::clone(&self.storage),
        }
    }
}

/// An Anthropic model.
///
/// One shared adapter for every Claude model: its facts come from the
/// [`ModelDescriptor`] it is constructed with (sourced from the provider's live
/// API listing), and its [`reference`](Model::reference) is derived from that
/// descriptor so the two can never disagree. The credential is supplied per
/// request as an [`Authentication`] when the model is resolved.
pub struct AnthropicModel {
    agent: Agent<AnthropicClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl AnthropicModel {
    /// Creates a new Anthropic model from the given arguments and descriptor,
    /// authenticating with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        AnthropicModelArgs { preamble, storage }: AnthropicModelArgs<S>,
        authentication: &Authentication,
        descriptor: ModelDescriptor,
        scope: MemoryScope,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        let reference = descriptor.reference();
        tracing::debug!("Creating Anthropic {} model", reference.model);
        let api_key = authentication.require_api_key(Provider::Anthropic, &reference.model)?;
        let client = AnthropicClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Anthropic,
                Some(reference.model.clone()),
                "Failed to create Anthropic client",
            )
        })?;

        tracing::debug!("Creating agent for Anthropic {} model", reference.model);
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
            scope,
        })
        .await?;

        tracing::debug!("Successfully created Anthropic {} model", reference.model);
        Ok(Self {
            agent,
            descriptor,
            reference,
        })
    }
}

#[async_trait::async_trait]
impl Model for AnthropicModel {
    fn reference(&self) -> &ModelReference {
        &self.reference
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        self.agent.complete(request).await
    }

    async fn stream(&self, request: CompletionRequest) -> ProviderResult<ResponseStream> {
        self.agent.stream(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; only its type identity matters here.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    #[test]
    fn should_render_storage_type_in_debug_without_an_s_debug_bound() {
        let args = AnthropicModelArgs {
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        };

        let rendered = format!("{args:?}");
        assert!(rendered.contains("AnthropicModelArgs"));
        assert!(rendered.contains("be helpful"));
    }

    #[test]
    fn should_clone_args_without_requiring_storage_clone() {
        let args = AnthropicModelArgs {
            preamble: "preamble".to_string(),
            storage: Arc::new(NoStorage),
        };

        let cloned = args.clone();
        assert_eq!(cloned.preamble, "preamble");
        // The clone shares the same backend rather than duplicating it.
        assert!(Arc::ptr_eq(&args.storage, &cloned.storage));
    }
}

#[cfg(test)]
mod pricing_tests {
    use super::*;

    #[test]
    fn should_price_each_known_family() {
        let cases = [
            ("claude-opus-4-8", 5, 25),
            ("claude-sonnet-4-6", 3, 15),
            ("claude-haiku-4-5-20251001", 1, 5),
            ("claude-fable-5", 10, 50),
        ];

        for (id, input, output) in cases {
            let (got_in, got_out) = family_pricing(id);
            assert_eq!(got_in, Some(Decimal::new(input, 0)), "input price for {id}");
            assert_eq!(
                got_out,
                Some(Decimal::new(output, 0)),
                "output price for {id}"
            );
        }
    }

    #[test]
    fn should_leave_an_unknown_family_unpriced() {
        let (input, output) = family_pricing("some-future-model");
        assert_eq!(input, None);
        assert_eq!(output, None);
    }
}
