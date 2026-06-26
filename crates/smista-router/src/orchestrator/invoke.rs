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
#![allow(
    dead_code,
    reason = "wired into the orchestrator run loop in a later task"
)]

use std::collections::HashMap;
use std::sync::Arc;

use secrecy::SecretString;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::{ModelReference, Provider as ProviderId};
use smista_providers::api::{CompletionRequest, CompletionResponse, ResponseStream};
use smista_providers::auth::Authentication;
use smista_providers::memory::MemoryScope;
use smista_providers::model::Model;
use tokio_util::sync::CancellationToken;

use crate::orchestrator::error::OrchestratorError;
use crate::router::Router;
use crate::router::resolver::ResolvedTurn;

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
    use smista_core::intent::TaskIntent;
    use smista_core::model::{
        ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters,
    };
    use smista_core::policy::{Classification, IntentSource};

    use super::*;
    use crate::router::resolver::RoutingDecision;
    use crate::router::resolver::context::{ContextOutcome, ResolvedContext};

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
                capabilities: ModelCapabilities::default(),
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
}
