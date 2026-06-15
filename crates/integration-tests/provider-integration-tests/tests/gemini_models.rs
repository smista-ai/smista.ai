//! Integration test for the Gemini [`Provider`] driven by a live model.
//!
//! Exercises the model adapter end to end against the real Gemini API: the
//! provider resolves the Gemini 2.5 Flash reference into a [`Model`], the
//! returned handle is asserted to be exactly that model (identity and
//! descriptor), and the same request is then run through both [`Model::complete`]
//! and [`Model::stream`] to prove the full request/response flow works.
//!
//! Resolution is memory-backed, so the test supplies the shared
//! [`InMemoryStorage`] fixture: these models do not need stored memories, only a
//! backend to construct against, and it stays empty throughout. The assertions
//! on model prose are deliberately loose — that some non-empty text came back —
//! because the point is the transport, not the wording.
//!
//! Gemini 2.5 Flash is the lightest offered model, keeping the live call cheap;
//! the suite only asserts transport, not answer quality.
//!
//! Requires `GEMINI_API_KEY`; when it is absent the test panics rather than
//! skipping, matching the sibling `openai_models.rs` suite. It runs only under
//! `just provider_integration_test`.

use std::sync::Arc;

use futures::StreamExt;
use provider_integration_tests::{InMemoryStorage, init_tracing};
use secrecy::SecretString;
use smista_core::model::{ModelParameters, ModelReference, Provider as ProviderId};
use smista_core::stream::StreamEvent;
use smista_providers::api::{CompletionRequest, FinishReason, RequestMessage};
use smista_providers::auth::Authentication;
use smista_providers::memory::MemoryScope;
use smista_providers::model::gemini::GeminiModelArgs;
use smista_providers::provider::Provider;
use smista_providers::provider::gemini::GeminiProvider;
use uuid::Uuid;

/// Model id of the Gemini Flash variant exercised by this test.
const GEMINI_FLASH_MODEL_ID: &str = "gemini-2.5-flash";

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
async fn should_resolve_gemini_2_5_flash_and_run_complete_and_stream() {
    init_tracing();

    // ensure the API key is set
    let Ok(api_key) = std::env::var("GEMINI_API_KEY").map(SecretString::from) else {
        panic!("GEMINI_API_KEY is not set");
    };

    let provider = GeminiProvider::new(GeminiModelArgs {
        preamble: "You are a terse assistant.".to_string(),
        storage: Arc::new(InMemoryStorage::default()),
    });
    let authentication = Authentication::ApiKey(api_key);

    // Step 1: resolving the Gemini 2.5 Flash reference yields the Gemini 2.5
    // Flash model — assert on its stable identity and descriptor, not on a
    // concrete type.
    let reference = ModelReference {
        provider: ProviderId::Gemini,
        model: GEMINI_FLASH_MODEL_ID.to_string(),
    };

    let model = provider
        .resolve(
            &reference,
            &authentication,
            MemoryScope {
                user_id: Uuid::now_v7(),
                session_id: Uuid::now_v7(),
            },
        )
        .await
        .expect("resolving the Gemini 2.5 Flash reference must succeed");

    assert_eq!(
        model.reference(),
        &reference,
        "resolve returned a model with a different identity"
    );
    let descriptor = model.descriptor();
    assert_eq!(descriptor.provider, ProviderId::Gemini);
    assert_eq!(descriptor.model, GEMINI_FLASH_MODEL_ID);

    // Step 2: the blocking completion flow returns non-empty content and a
    // natural stop.
    let response = model
        .complete(ping_request())
        .await
        .expect("complete must succeed against the live API");

    assert!(
        !response.content.trim().is_empty(),
        "complete returned no content"
    );
    assert_eq!(
        response.finish_reason,
        FinishReason::Stop,
        "a one-word reply within the token budget should stop naturally"
    );

    // Step 3: the streaming flow yields text deltas and terminates with `Done`.
    let mut stream = model
        .stream(ping_request())
        .await
        .expect("stream must succeed against the live API");

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
