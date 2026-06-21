//! A mock [`Provider`] and [`Model`] for router tests.
//!
//! [`MockProvider`] resolves a single canned [`MockModel`] whose completion and
//! stream are fixed, so a test can exercise router wiring without a network
//! call or an API key. [`Router::mock`](super::Router::mock) installs one local
//! and one remote instance.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    Provider as ProviderId, ProviderDescriptor,
};
use smista_core::stream::StreamEvent;
use smista_core::usage::Usage;
use smista_providers::ProviderResult;
use smista_providers::api::{CompletionRequest, CompletionResponse, FinishReason, ResponseStream};
use smista_providers::auth::Authentication;
use smista_providers::memory::MemoryScope;
use smista_providers::model::Model;
use smista_providers::provider::Provider;

/// The text every [`MockModel`] returns, in both completion and stream form.
const MOCK_RESPONSE: &str = "mock response";

/// A provider that advertises and resolves a single canned model.
#[derive(Debug)]
pub(super) struct MockProvider {
    /// The provider identity this mock answers as.
    id: ProviderId,
    /// The display name reported in the descriptor.
    display_name: String,
    /// Whether this provider (and its model) is treated as local.
    local: bool,
    /// The name of the single model the provider offers.
    model: String,
}

impl MockProvider {
    /// Creates a mock provider answering as `id`, offering one `model`.
    pub(super) fn new(
        id: ProviderId,
        display_name: impl Into<String>,
        local: bool,
        model: impl Into<String>,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            local,
            model: model.into(),
        }
    }

    /// Builds the descriptor for `reference`, stamped with this mock's facts.
    fn descriptor_for(&self, reference: &ModelReference) -> ModelDescriptor {
        ModelDescriptor {
            provider: reference.provider.clone(),
            model: reference.model.clone(),
            display_name: None,
            local: self.local,
            auth: if self.local {
                ModelAuthRequirement::None
            } else {
                ModelAuthRequirement::ApiKey
            },
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                json_output: true,
                system_prompt: true,
                images: false,
                reasoning: false,
                memory: true,
            },
            max_context_tokens: 8_192,
            max_output_tokens: Some(4_096),
            input_cost_per_million_tokens: None,
            output_cost_per_million_tokens: None,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }

    /// The reference of the single model this provider offers.
    fn reference(&self) -> ModelReference {
        ModelReference {
            provider: self.id.clone(),
            model: self.model.clone(),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            local: self.local,
        }
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        _authentication: &Authentication,
        _scope: MemoryScope,
    ) -> ProviderResult<Arc<dyn Model>> {
        Ok(Arc::new(MockModel {
            descriptor: self.descriptor_for(reference),
            reference: reference.clone(),
        }))
    }

    async fn list_models(
        &self,
        _authentication: &Authentication,
    ) -> ProviderResult<HashMap<ModelReference, ModelDescriptor>> {
        Ok(vec![self.reference()]
            .into_iter()
            .map(|r| (r.clone(), self.descriptor_for(&r)))
            .collect())
    }
}

/// A model that returns a fixed completion and a fixed two-event stream.
struct MockModel {
    /// The resolved model's identity.
    reference: ModelReference,
    /// The resolved model's facts.
    descriptor: ModelDescriptor,
}

#[async_trait]
impl Model for MockModel {
    fn reference(&self) -> &ModelReference {
        &self.reference
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    async fn complete(&self, _request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        Ok(CompletionResponse {
            content: MOCK_RESPONSE.to_string(),
            tool_calls: Vec::new(),
            usage: Usage::default(),
            finish_reason: FinishReason::Stop,
        })
    }

    async fn stream(&self, _request: CompletionRequest) -> ProviderResult<ResponseStream> {
        let events = stream::iter(vec![
            Ok(StreamEvent::TextDelta {
                delta: MOCK_RESPONSE.to_string(),
            }),
            Ok(StreamEvent::Done),
        ]);
        Ok(ResponseStream::new(events))
    }
}
