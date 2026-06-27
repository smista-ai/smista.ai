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
use smista_core::error::{ProviderError, ProviderErrorCategory};
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
    /// Whether the model advertises the streaming capability.
    streaming: bool,
    /// Whether `stream` yields a single transport error instead of events.
    stream_error: bool,
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
            streaming: true,
            stream_error: false,
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

    /// Sets whether the model advertises the streaming capability.
    #[cfg(test)]
    pub(super) fn with_streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Makes `stream` yield a single transport error, for the mid-stream-error path.
    #[cfg(test)]
    pub(super) fn with_stream_error(mut self, stream_error: bool) -> Self {
        self.stream_error = stream_error;
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
                streaming: self.streaming,
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
            stream_error: self.stream_error,
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
    /// Whether `stream` yields a single transport error instead of events.
    stream_error: bool,
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
        if self.stream_error {
            let error = ProviderError {
                category: ProviderErrorCategory::ProviderUnavailable,
                message: "mock stream failure".to_string(),
                provider: self.reference.provider.clone(),
                model: Some(self.reference.model.clone()),
            };
            return Ok(ResponseStream::new(stream::iter(vec![Err(error)])));
        }
        let events = match self
            .script
            .lock()
            .expect("mock script poisoned")
            .pop_front()
        {
            Some(scripted) => render_completion(scripted),
            None => canned_stream(),
        };
        Ok(ResponseStream::new(stream::iter(
            events.into_iter().map(Ok),
        )))
    }
}

/// Renders a scripted completion as the stream a streaming model would produce.
///
/// Content is split into two chunks so a test can observe incremental delivery;
/// each tool call becomes a `tool_call_started` then `tool_call_requested` pair.
fn render_completion(response: CompletionResponse) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for chunk in chunk_in_two(&response.content) {
        events.push(StreamEvent::TextDelta { delta: chunk });
    }
    for call in response.tool_calls {
        events.push(StreamEvent::ToolCallStarted {
            call_id: call.call_id.clone(),
            name: call.name.clone(),
        });
        events.push(StreamEvent::ToolCallRequested {
            call_id: call.call_id,
            name: call.name,
            arguments: call.arguments,
        });
    }
    events.push(StreamEvent::Usage(response.usage));
    events.push(StreamEvent::Done);
    events
}

/// The default stream when nothing is scripted: a reasoning delta and two text
/// deltas that aggregate to [`MOCK_RESPONSE`], plus usage and a terminal marker.
fn canned_stream() -> Vec<StreamEvent> {
    vec![
        StreamEvent::ReasoningDelta {
            delta: "thinking...".to_string(),
        },
        StreamEvent::TextDelta {
            delta: "mock ".to_string(),
        },
        StreamEvent::TextDelta {
            delta: "response".to_string(),
        },
        StreamEvent::Usage(Usage::default()),
        StreamEvent::Done,
    ]
}

/// Splits `text` into two character-balanced chunks; one chunk (or none) when it
/// is too short to split.
fn chunk_in_two(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 {
        return vec![text.to_string()];
    }
    let mid = chars.len() / 2;
    vec![chars[..mid].iter().collect(), chars[mid..].iter().collect()]
}

#[cfg(test)]
mod tests {
    use futures::StreamExt as _;
    use smista_core::model::Provider as ProviderId;
    use smista_providers::auth::Authentication;
    use smista_providers::memory::MemoryScope;

    use super::*;

    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: uuid::Uuid::nil(),
            session_id: uuid::Uuid::nil(),
        }
    }

    async fn resolve(provider: MockProvider) -> Arc<dyn Model> {
        let reference = provider.reference();
        provider
            .resolve(&reference, &Authentication::None, scope(), &[])
            .await
            .expect("resolve")
    }

    #[tokio::test]
    async fn should_stream_canned_response_as_reasoning_and_multiple_text_deltas() {
        let model = resolve(MockProvider::new(
            ProviderId::Ollama,
            "Mock",
            true,
            "mock-local",
        ))
        .await;
        let events: Vec<_> = model
            .stream(CompletionRequest::default())
            .await
            .expect("stream")
            .collect()
            .await;
        let events: Vec<StreamEvent> = events.into_iter().map(|e| e.expect("event")).collect();

        let text_deltas = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::TextDelta { .. }))
            .count();
        assert!(
            text_deltas >= 2,
            "expected >=2 text deltas, got {text_deltas}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, StreamEvent::ReasoningDelta { .. })),
            "expected a reasoning delta"
        );
        assert!(matches!(events.last(), Some(StreamEvent::Done)));
    }

    #[tokio::test]
    async fn should_stream_an_error_when_configured() {
        let model = resolve(
            MockProvider::new(ProviderId::Ollama, "Mock", true, "mock-local")
                .with_stream_error(true),
        )
        .await;
        let first = model
            .stream(CompletionRequest::default())
            .await
            .expect("stream")
            .next()
            .await
            .expect("an item");
        assert!(first.is_err(), "expected a transport error item");
    }
}
