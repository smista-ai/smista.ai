//! A mock [`Provider`] and [`Model`] for router tests.
//!
//! [`MockProvider`] resolves a single canned [`MockModel`]. By default its
//! completion and stream are fixed, so a test can exercise router wiring without
//! a network call or an API key; a [scripted](MockProvider::with_script) provider
//! instead returns a queue of pre-built completions in order, so a test can drive
//! the orchestrator through tool requests and follow-up turns. The script is
//! shared across every model the provider resolves, so it advances across the
//! re-invocations of a single turn. [`Router::mock`](super::Router::mock)
//! installs one local and one remote instance.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

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

/// A queue of completions a scripted mock returns in order, shared across the
/// models the provider resolves so it advances across a turn's re-invocations.
type Script = Arc<Mutex<VecDeque<CompletionResponse>>>;

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
    /// Pre-built completions returned in order; empty means the fixed response.
    script: Script,
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
            script: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Scripts the provider to return `responses` in order across invocations.
    ///
    /// Once the queue drains, the model falls back to the fixed response, so a
    /// test only scripts the turns it cares about.
    #[cfg(test)]
    pub(super) fn with_script(mut self, responses: Vec<CompletionResponse>) -> Self {
        self.script = Arc::new(Mutex::new(VecDeque::from(responses)));
        self
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
        _preamble_segments: &[String],
    ) -> ProviderResult<Arc<dyn Model>> {
        Ok(Arc::new(MockModel {
            descriptor: self.descriptor_for(reference),
            reference: reference.clone(),
            script: self.script.clone(),
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

/// A model that returns scripted completions, falling back to a fixed one, and
/// a fixed two-event stream.
struct MockModel {
    /// The resolved model's identity.
    reference: ModelReference,
    /// The resolved model's facts.
    descriptor: ModelDescriptor,
    /// The shared script; each completion pops the next entry, if any.
    script: Script,
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
        if let Some(scripted) = self
            .script
            .lock()
            .expect("mock script poisoned")
            .pop_front()
        {
            return Ok(scripted);
        }
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
