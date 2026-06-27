//! Live model invocation for a resolved turn.
//!
//! The resolver picks the model and its fallback chain; this module turns that
//! decision into an actual provider call. It resolves the chosen model through
//! its provider, races the call against the turn's cancellation token so a
//! superseding request can abort it, and walks the fallback chain when a
//! transient provider error makes the next model worth trying.
//!
//! Authentication is built per provider from the request credentials and never
//! logged, and request content is never traced.

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt as _;
use secrecy::SecretString;
use smista_core::api::TurnEvent;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::{ModelReference, Provider as ProviderId};
use smista_core::stream::StreamEvent;
use smista_core::usage::Usage;
use smista_providers::api::{
    CompletionRequest, CompletionResponse, FinishReason, ResponseStream, ToolCall,
};
use smista_providers::auth::Authentication;
use smista_providers::memory::MemoryScope;
use smista_providers::model::Model;
use tokio_util::sync::CancellationToken;

use crate::orchestrator::error::OrchestratorError;
use crate::orchestrator::stream::TurnSink;
use crate::router::Router;
use crate::router::resolver::ResolvedTurn;

/// Invokes the resolved model, streaming live events when possible.
///
/// With a sink and a streaming-capable model, drives the live stream and
/// forwards text and reasoning deltas as they arrive, aggregating them into the
/// same [`CompletionResponse`] the buffered path returns. Without a streaming
/// model it calls the buffered path; when a sink is present (a streaming request
/// routed to a non-streaming model) it forwards the whole reply as a single text
/// delta, so the client still sees the streamed event shape.
///
/// # Errors
///
/// Surfaces the same failures as [`invoke_complete`]/[`invoke_stream`], plus a
/// transport error raised while consuming the live stream.
pub(crate) async fn invoke(
    router: &Router,
    resolved: &ResolvedTurn,
    credentials: &HashMap<ProviderId, SecretString>,
    scope: MemoryScope,
    request: CompletionRequest,
    cancel: &CancellationToken,
    sink: Option<&TurnSink>,
) -> Result<CompletionResponse, OrchestratorError> {
    match sink {
        Some(sink) if resolved.model.capabilities.streaming => {
            tracing::debug!(
                provider = %resolved.routing.provider,
                model = %resolved.routing.model,
                "model streams; driving the turn from live output"
            );
            let stream =
                invoke_stream(router, resolved, credentials, scope, request, cancel).await?;
            aggregate_stream(stream, cancel, sink).await
        }
        Some(sink) => {
            tracing::debug!(
                provider = %resolved.routing.provider,
                model = %resolved.routing.model,
                "model cannot stream; buffering then replaying as one delta"
            );
            let response =
                invoke_complete(router, resolved, credentials, scope, request, cancel).await?;
            if !response.content.is_empty() {
                sink.emit(TurnEvent::TextDelta {
                    delta: response.content.clone(),
                });
            }
            Ok(response)
        }
        None => invoke_complete(router, resolved, credentials, scope, request, cancel).await,
    }
}

/// Consumes a live model stream, forwarding deltas to the sink and aggregating
/// the final completion.
///
/// Text and reasoning deltas are forwarded as they arrive; tool calls and usage
/// are collected for the aggregated [`CompletionResponse`] that the rest of the
/// turn loop consumes unchanged. Tool-call activity is not forwarded here — a
/// tool pause stays a discrete turn boundary carried by the terminal event. The
/// consume loop is raced against `cancel` so a superseding request aborts it.
async fn aggregate_stream(
    mut stream: ResponseStream,
    cancel: &CancellationToken,
    sink: &TurnSink,
) -> Result<CompletionResponse, OrchestratorError> {
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let mut usage = Usage::default();
    let mut deltas = 0_usize;
    loop {
        let item = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                tracing::debug!("turn superseded mid-stream; aborting");
                return Err(OrchestratorError::Superseded);
            }
            item = stream.next() => item,
        };
        let Some(item) = item else { break };
        match item {
            Ok(StreamEvent::TextDelta { delta }) => {
                content.push_str(&delta);
                deltas += 1;
                sink.emit(TurnEvent::TextDelta { delta });
            }
            Ok(StreamEvent::ReasoningDelta { delta }) => {
                sink.emit(TurnEvent::ReasoningDelta { delta });
            }
            Ok(StreamEvent::ToolCallRequested {
                call_id,
                name,
                arguments,
            }) => {
                tool_calls.push(ToolCall {
                    call_id,
                    name,
                    arguments,
                });
            }
            Ok(StreamEvent::Usage(reported)) => usage = reported,
            Ok(
                StreamEvent::ToolCallStarted { .. }
                | StreamEvent::ApprovalRequired { .. }
                | StreamEvent::ToolResult { .. },
            ) => {}
            Ok(StreamEvent::Error { code, message }) => {
                // Providers surface transport failures as `Err`; the in-band
                // `Error` event is the web wire shape and is not expected here.
                tracing::warn!(%code, "model stream produced an unexpected in-band error");
                return Err(OrchestratorError::Internal(message));
            }
            Ok(StreamEvent::Done) => break,
            Err(error) => return Err(OrchestratorError::Provider(error)),
        }
    }
    let finish_reason = if tool_calls.is_empty() {
        FinishReason::Stop
    } else {
        FinishReason::ToolCalls
    };
    tracing::debug!(
        deltas,
        tool_calls = tool_calls.len(),
        "aggregated the live model stream"
    );
    Ok(CompletionResponse {
        content,
        tool_calls,
        usage,
        finish_reason,
    })
}

/// Invokes the resolved model for a buffered completion, with fallback.
///
/// Resolves the turn's primary model and calls it; on a fallback-eligible
/// provider error it advances down [`ResolvedTurn::fallbacks`] and retries.
/// The call is raced against `cancel`: a triggered token aborts the in-flight
/// call and surfaces [`OrchestratorError::Superseded`]. When every model in the
/// chain fails with a retryable error, [`OrchestratorError::FallbackExhausted`]
/// is returned; a non-retryable error is surfaced as-is.
pub(crate) async fn invoke_complete(
    router: &Router,
    resolved: &ResolvedTurn,
    credentials: &HashMap<ProviderId, SecretString>,
    scope: MemoryScope,
    request: CompletionRequest,
    cancel: &CancellationToken,
) -> Result<CompletionResponse, OrchestratorError> {
    let chain = candidate_chain(resolved);
    let total = chain.len();
    for (index, reference) in chain.into_iter().enumerate() {
        let has_more = index + 1 < total;
        if cancel.is_cancelled() {
            tracing::debug!("turn cancelled before invoking the model; reporting superseded");
            return Err(OrchestratorError::Superseded);
        }
        let model = match resolve_model(router, &reference, credentials, scope).await {
            Ok(model) => model,
            Err(error) => match classify_failure(error, has_more, &reference) {
                Outcome::Retry => continue,
                Outcome::Fail(error) => return Err(error),
            },
        };
        tracing::debug!(
            provider = %reference.provider,
            model = %reference.model,
            "invoking the resolved model for a buffered completion"
        );
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                tracing::debug!("turn superseded mid-invocation; aborting the call");
                return Err(OrchestratorError::Superseded);
            }
            result = model.complete(request.clone()) => result,
        };
        match outcome {
            Ok(response) => return Ok(response),
            Err(error) => match classify_failure(error, has_more, &reference) {
                Outcome::Retry => continue,
                Outcome::Fail(error) => return Err(error),
            },
        }
    }
    tracing::warn!("the primary route and every fallback were exhausted");
    Err(OrchestratorError::FallbackExhausted)
}

/// Invokes the resolved model for a streaming completion, with fallback.
///
/// Behaves like [`invoke_complete`] but returns the live [`ResponseStream`] the
/// caller drives. The race against `cancel` covers opening the stream; the
/// caller is responsible for honoring cancellation while consuming it.
pub(crate) async fn invoke_stream(
    router: &Router,
    resolved: &ResolvedTurn,
    credentials: &HashMap<ProviderId, SecretString>,
    scope: MemoryScope,
    request: CompletionRequest,
    cancel: &CancellationToken,
) -> Result<ResponseStream, OrchestratorError> {
    let chain = candidate_chain(resolved);
    let total = chain.len();
    for (index, reference) in chain.into_iter().enumerate() {
        let has_more = index + 1 < total;
        if cancel.is_cancelled() {
            tracing::debug!("turn cancelled before invoking the model; reporting superseded");
            return Err(OrchestratorError::Superseded);
        }
        let model = match resolve_model(router, &reference, credentials, scope).await {
            Ok(model) => model,
            Err(error) => match classify_failure(error, has_more, &reference) {
                Outcome::Retry => continue,
                Outcome::Fail(error) => return Err(error),
            },
        };
        tracing::debug!(
            provider = %reference.provider,
            model = %reference.model,
            "invoking the resolved model for a streaming completion"
        );
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                tracing::debug!("turn superseded before the stream opened; aborting");
                return Err(OrchestratorError::Superseded);
            }
            result = model.stream(request.clone()) => result,
        };
        match outcome {
            Ok(stream) => return Ok(stream),
            Err(error) => match classify_failure(error, has_more, &reference) {
                Outcome::Retry => continue,
                Outcome::Fail(error) => return Err(error),
            },
        }
    }
    tracing::warn!("the primary route and every fallback were exhausted");
    Err(OrchestratorError::FallbackExhausted)
}

/// The chosen model followed by its remaining fallbacks, in invocation order.
fn candidate_chain(resolved: &ResolvedTurn) -> Vec<ModelReference> {
    let primary = ModelReference {
        provider: resolved.routing.provider.clone(),
        model: resolved.routing.model.clone(),
    };
    std::iter::once(primary)
        .chain(resolved.fallbacks.iter().cloned())
        .collect()
}

/// Resolves `reference` through its provider into a callable model.
///
/// Returns a [`ProviderErrorCategory::ModelNotFound`] error when the router has
/// no provider for the reference, so the caller treats it like any other
/// provider failure.
async fn resolve_model(
    router: &Router,
    reference: &ModelReference,
    credentials: &HashMap<ProviderId, SecretString>,
    scope: MemoryScope,
) -> Result<Arc<dyn Model>, ProviderError> {
    let Some(provider) = router.provider(&reference.provider) else {
        return Err(ProviderError {
            category: ProviderErrorCategory::ModelNotFound,
            message: "no provider configured for the resolved model".to_string(),
            provider: reference.provider.clone(),
            model: Some(reference.model.clone()),
        });
    };
    let authentication = authentication(&reference.provider, credentials);
    provider
        .resolve(reference, &authentication, scope, &[])
        .await
}

/// Decides whether a provider failure should fall through to the next model.
enum Outcome {
    /// Try the next model in the chain.
    Retry,
    /// Stop and surface this error.
    Fail(OrchestratorError),
}

/// Classifies a provider failure into [`Outcome::Retry`] or [`Outcome::Fail`].
///
/// A fallback-eligible category (transient or capacity-related) with another
/// model left to try is retried; otherwise the error is surfaced, so a
/// deterministic failure (a bad request, wrong credentials) is not masked by
/// pointlessly retrying every fallback.
fn classify_failure(error: ProviderError, has_more: bool, reference: &ModelReference) -> Outcome {
    if error.category.is_fallback_eligible() && has_more {
        tracing::warn!(
            provider = %reference.provider,
            model = %reference.model,
            category = %error.category,
            "provider call failed with a retryable error; trying the next fallback"
        );
        Outcome::Retry
    } else {
        Outcome::Fail(OrchestratorError::Provider(error))
    }
}

/// Builds the [`Authentication`] for `provider` from the request credentials.
///
/// A configured secret becomes an API-key authentication; an absent one means
/// the provider is called without credentials. The secret is never logged.
fn authentication(
    provider: &ProviderId,
    credentials: &HashMap<ProviderId, SecretString>,
) -> Authentication {
    credentials
        .get(provider)
        .map(|secret| Authentication::ApiKey(secret.clone()))
        .unwrap_or(Authentication::None)
}

#[cfg(test)]
mod tests {
    use smista_core::api::TurnEvent;
    use smista_core::intent::TaskIntent;
    use smista_core::model::{
        ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters,
    };
    use smista_core::policy::{Classification, IntentSource, ToolsConfig};
    use tokio::sync::mpsc;

    use super::*;
    use crate::orchestrator::stream::TurnSink;
    use crate::router::resolver::RoutingDecision;
    use crate::router::resolver::context::{ContextOutcome, ResolvedContext};

    /// Drains an unbounded receiver into a vec after the sender has dropped.
    fn drain(mut rx: mpsc::UnboundedReceiver<TurnEvent>) -> Vec<TurnEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Builds a [`ResolvedTurn`] routed to the local mock model, no fallbacks.
    fn resolved_turn_for_mock() -> ResolvedTurn {
        let reference: ModelReference = "ollama/mock-local".parse().expect("valid reference");
        ResolvedTurn {
            classification: Classification {
                intent: TaskIntent::Chat,
                source: IntentSource::Inferred,
                reason: "test".to_string(),
                matched_rule: None,
                confidence: None,
            },
            routing: RoutingDecision {
                intent: TaskIntent::Chat,
                provider: reference.provider.clone(),
                model: reference.model.clone(),
                matched_rule: None,
                fallback_used: false,
                override_used: false,
                reason: "test".to_string(),
            },
            model: ModelDescriptor {
                provider: reference.provider.clone(),
                model: reference.model.clone(),
                display_name: None,
                local: true,
                auth: ModelAuthRequirement::None,
                capabilities: ModelCapabilities {
                    streaming: true,
                    ..Default::default()
                },
                max_context_tokens: 8_192,
                max_output_tokens: Some(4_096),
                input_cost_per_million_tokens: None,
                output_cost_per_million_tokens: None,
                default_parameters: ModelParameters::default(),
                provider_options: None,
            },
            fallbacks: Vec::new(),
            context: ResolvedContext {
                included: Vec::new(),
                outcome: ContextOutcome::default(),
                references: Vec::new(),
            },
            required_permissions: ToolsConfig::default(),
        }
    }

    fn credentials() -> HashMap<ProviderId, SecretString> {
        HashMap::new()
    }

    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: uuid::Uuid::nil(),
            session_id: uuid::Uuid::nil(),
        }
    }

    #[tokio::test]
    async fn should_invoke_mock_model_and_return_completion() {
        let router = Router::mock();
        let resolved = resolved_turn_for_mock();
        let response = invoke_complete(
            &router,
            &resolved,
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &CancellationToken::new(),
        )
        .await
        .expect("completion");
        assert!(!response.content.is_empty() || !response.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn should_report_superseded_when_cancelled() {
        let router = Router::mock();
        let token = CancellationToken::new();
        token.cancel();
        let error = invoke_complete(
            &router,
            &resolved_turn_for_mock(),
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &token,
        )
        .await
        .expect_err("superseded");
        assert!(matches!(error, OrchestratorError::Superseded));
    }

    #[tokio::test]
    async fn should_open_stream_for_mock_model() {
        let router = Router::mock();
        let stream = invoke_stream(
            &router,
            &resolved_turn_for_mock(),
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &CancellationToken::new(),
        )
        .await;
        assert!(stream.is_ok());
    }

    #[tokio::test]
    async fn should_stream_text_deltas_to_the_sink() {
        let router = Router::mock();
        let (tx, rx) = mpsc::unbounded_channel();
        let sink = TurnSink::new(tx);
        let response = invoke(
            &router,
            &resolved_turn_for_mock(),
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &CancellationToken::new(),
            Some(&sink),
        )
        .await
        .expect("completion");

        drop(sink);
        let events = drain(rx);
        let text_deltas = events
            .iter()
            .filter(|e| matches!(e, TurnEvent::TextDelta { .. }))
            .count();
        assert!(
            text_deltas >= 2,
            "expected incremental text, got {text_deltas}"
        );
        // The aggregated content equals what the buffered call would return.
        assert_eq!(response.content, "mock response");
    }

    #[tokio::test]
    async fn should_forward_reasoning_deltas_without_folding_them_into_content() {
        let router = Router::mock();
        let (tx, rx) = mpsc::unbounded_channel();
        let sink = TurnSink::new(tx);
        let response = invoke(
            &router,
            &resolved_turn_for_mock(),
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &CancellationToken::new(),
            Some(&sink),
        )
        .await
        .expect("completion");

        drop(sink);
        let events = drain(rx);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::ReasoningDelta { .. })),
            "expected a reasoning delta"
        );
        assert!(
            !response.content.contains("thinking"),
            "reasoning leaked into content"
        );
    }

    #[tokio::test]
    async fn should_replay_a_non_streaming_model_as_one_delta() {
        // A non-streaming model under a sink still emits its whole reply as one
        // text delta, so the client sees the same shape.
        let router = Router::mock();
        let mut resolved = resolved_turn_for_mock();
        resolved.model.capabilities.streaming = false;
        let (tx, rx) = mpsc::unbounded_channel();
        let sink = TurnSink::new(tx);
        let response = invoke(
            &router,
            &resolved,
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &CancellationToken::new(),
            Some(&sink),
        )
        .await
        .expect("completion");

        drop(sink);
        let events = drain(rx);
        let text: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TurnEvent::TextDelta { .. }))
            .collect();
        assert_eq!(text.len(), 1, "non-streaming model should emit one delta");
        assert_eq!(response.content, "mock response");
    }

    #[tokio::test]
    async fn should_surface_a_stream_transport_error() {
        let router = Router::mock_stream_error();
        let (tx, _rx) = mpsc::unbounded_channel();
        let sink = TurnSink::new(tx);
        let error = invoke(
            &router,
            &resolved_turn_for_mock(),
            &credentials(),
            scope(),
            CompletionRequest::default(),
            &CancellationToken::new(),
            Some(&sink),
        )
        .await
        .expect_err("stream error");
        assert!(matches!(error, OrchestratorError::Provider(_)));
    }
}
