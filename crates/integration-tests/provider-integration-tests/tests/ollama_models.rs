//! Integration test for the Ollama [`Provider`] driven by a live daemon.
//!
//! Exercises the model adapter end to end against a real Ollama instance: the
//! provider lists the daemon's installed models, the configured reference is
//! resolved into a [`Model`], the returned handle is asserted to be exactly
//! that model (identity and descriptor), and the same request is then run
//! through both [`Model::complete`] and [`Model::stream`] to prove the full
//! request/response flow works.
//!
//! Resolution is memory-backed, so the test supplies the shared
//! [`InMemoryStorage`] fixture: these models do not need stored memories, only a
//! backend to construct against, and it stays empty throughout. The assertions
//! on model prose are deliberately loose — that some non-empty text came back —
//! because the point is the transport, not the wording.
//!
//! Unlike the remote suites, the daemon has no static model catalogue: which
//! models are installed is a property of the running instance. The model under
//! test is therefore named by `OLLAMA_MODEL`, and the daemon is located by
//! `OLLAMA_BASE_URL`. When either is absent the test panics rather than
//! skipping, matching the sibling `anthropic_models.rs` and `openai_models.rs`
//! suites. CI provides both by standing up a daemon with a tiny model; it runs
//! only under `just provider_integration_test`.

use std::sync::Arc;

use futures::StreamExt;
use provider_integration_tests::InMemoryStorage;
use smista_core::model::{ModelParameters, ModelReference, Provider as ProviderId};
use smista_core::stream::StreamEvent;
use smista_providers::api::{CompletionRequest, FinishReason, RequestMessage};
use smista_providers::auth::Authentication;
use smista_providers::model::ollama::{OllamaEndpoint, OllamaModelRuntime};
use smista_providers::provider::Provider;
use smista_providers::provider::ollama::OllamaProvider;

/// Builds the deterministic, single-turn request both flows send.
fn ping_request() -> CompletionRequest {
    CompletionRequest {
        messages: vec![RequestMessage::User {
            content: "Reply with exactly one word: pong".to_string(),
        }],
        parameters: ModelParameters {
            temperature: Some(0.0),
            max_tokens: Some(64),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn should_resolve_installed_model_and_run_complete_and_stream() {
    // The daemon location and the model to exercise both come from the
    // environment; absent either, fail loudly like the remote suites.
    let Ok(base_url) = std::env::var("OLLAMA_BASE_URL") else {
        panic!("OLLAMA_BASE_URL is not set");
    };
    let Ok(model_name) = std::env::var("OLLAMA_MODEL") else {
        panic!("OLLAMA_MODEL is not set");
    };

    let provider = OllamaProvider::new(
        OllamaEndpoint::local(base_url),
        OllamaModelRuntime {
            preamble: "You are a terse assistant.".to_string(),
            storage: Arc::new(InMemoryStorage::default()),
        },
    );
    // A local daemon is keyless.
    let authentication = Authentication::None;

    // Step 1: resolving the configured reference yields that model — assert on
    // its stable identity and descriptor, not on a concrete type. The daemon
    // stamps the display name, so it is not asserted here.
    let reference = ModelReference {
        provider: ProviderId::Ollama,
        model: model_name.clone(),
    };

    let model = provider
        .resolve(&reference, &authentication)
        .await
        .expect("resolving the configured Ollama model must succeed");

    assert_eq!(
        model.reference(),
        &reference,
        "resolve returned a model with a different identity"
    );
    let descriptor = model.descriptor();
    assert_eq!(descriptor.provider, ProviderId::Ollama);
    assert_eq!(descriptor.model, model_name);
    assert!(
        descriptor.local,
        "a model resolved from a local endpoint must be stamped local"
    );

    // Step 2: the blocking completion flow returns non-empty content. Unlike
    // the remote suites, the model under test may be a tiny CI model that runs
    // out the token budget before finishing its reply, so both a natural stop
    // and the length cap count as clean terminations — only a filter stop or a
    // dangling tool call would signal a broken flow.
    let response = model
        .complete(ping_request())
        .await
        .expect("complete must succeed against the live daemon");

    assert!(
        !response.content.trim().is_empty(),
        "complete returned no content"
    );
    assert!(
        matches!(
            response.finish_reason,
            FinishReason::Stop | FinishReason::Length
        ),
        "expected a clean termination, got {:?}",
        response.finish_reason
    );

    // Step 3: the streaming flow yields text deltas and terminates with `Done`.
    let mut stream = model
        .stream(ping_request())
        .await
        .expect("stream must succeed against the live daemon");

    let mut streamed = String::new();
    let mut saw_done = false;
    while let Some(event) = stream.next().await {
        match event.expect("stream yielded an error item") {
            StreamEvent::TextDelta { delta } => streamed.push_str(&delta),
            StreamEvent::Done => saw_done = true,
            // usage, tool calls and the SSE-only error shape are irrelevant here
            _ => {}
        }
    }

    assert!(saw_done, "stream ended without a Done event");
    assert!(
        !streamed.trim().is_empty(),
        "stream produced no text deltas"
    );
}
