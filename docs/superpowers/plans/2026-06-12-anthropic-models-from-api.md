# Anthropic models from the API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Anthropic provider's hardcoded model catalog with a live, cached fetch from `GET https://api.anthropic.com/v1/models`, keeping pricing in code as a family table.

**Architecture:** Mirror the existing Ollama client-and-cache pattern. A new `AnthropicClient` management client fetches and paginates the model list; a shared one-hour `TtlCache` holds the normalized descriptors; the provider resolves and lists from that cache. Capabilities map from the API; pricing is matched by model-id family substring.

**Tech Stack:** Rust, `reqwest`, `serde`, `secrecy`, `rust_decimal`, `tokio`, `async-trait` (provider trait only).

---

## Conventions

- Rust: run the `rust-conventions` skill before writing any `.rs` code; `module_name.rs`, no `mod.rs`; `clippy -D warnings`.
- Markdown/docs edits: run `md-conventions` before editing `.md`.
- Build/test only through `just`: `just build_crates`, `just check_code`, `just test_all`.
- Commit per task with Conventional Commits; never add `Co-Authored-By`.

## File Structure

- Move: `crates/smista-providers/src/provider/ollama/cache.rs` to `crates/smista-providers/src/provider/cache.rs` (shared TTL cache).
- Create: `crates/smista-providers/src/provider/anthropic/client.rs` (management client trait + HTTP impl + mock).
- Create: `crates/smista-providers/src/provider/anthropic/client/api.rs` (serde types for `/v1/models`).
- Modify: `crates/smista-providers/src/provider.rs` (declare shared `cache` module).
- Modify: `crates/smista-providers/src/provider/ollama.rs` (drop local `cache`, import shared).
- Modify: `crates/smista-providers/src/provider/anthropic.rs` (rewrite provider to client + cache; declare `client` module).
- Modify: `crates/smista-providers/src/model/anthropic.rs` (delete catalog and per-model modules, add `family_pricing`).
- Delete: `crates/smista-providers/src/model/anthropic/{opus,haiku,sonnet}.rs`.
- Modify: `crates/integration-tests/provider-integration-tests/tests/anthropic_models.rs` (drop `haiku_4_5` dependency).
- Modify: an Anthropic provider page under `docs/`.

---

## Task 1: Promote the TTL cache to a shared module

**Files:**
- Move: `crates/smista-providers/src/provider/ollama/cache.rs` to `crates/smista-providers/src/provider/cache.rs`
- Modify: `crates/smista-providers/src/provider.rs`
- Modify: `crates/smista-providers/src/provider/ollama.rs:3` and `:21`

- [ ] **Step 1: Move the file with git**

Run:
```bash
git mv crates/smista-providers/src/provider/ollama/cache.rs crates/smista-providers/src/provider/cache.rs
```

- [ ] **Step 2: Declare the shared module**

In `crates/smista-providers/src/provider.rs`, add a module declaration alongside the existing `pub mod` lines (a plain `mod` is enough; child provider modules can see it):

```rust
mod cache;
```

- [ ] **Step 3: Drop the local cache module from Ollama**

In `crates/smista-providers/src/provider/ollama.rs`, delete the line:

```rust
mod cache;
```

- [ ] **Step 4: Point Ollama at the shared cache**

In `crates/smista-providers/src/provider/ollama.rs`, change:

```rust
use crate::provider::ollama::cache::{Cached, TtlCache};
```

to:

```rust
use crate::provider::cache::{Cached, TtlCache};
```

- [ ] **Step 5: Build and test**

Run: `just build_crates` then `just test_all`
Expected: PASS. The moved cache tests run from their new location; Ollama provider tests are unchanged.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(providers): promote the TTL cache to a shared provider module"
```

---

## Task 2: Anthropic `/v1/models` response types

**Files:**
- Create: `crates/smista-providers/src/provider/anthropic/client/api.rs`
- Modify: `crates/smista-providers/src/provider/anthropic.rs` (declare the `client` module and its `api` submodule path is created in Task 3; here we only add the file)

This task adds the serde types and their tests. The module is wired up in Task 3, so this task's test is run directly by path until then.

- [ ] **Step 1: Write the types and a failing deserialize test**

Create `crates/smista-providers/src/provider/anthropic/client/api.rs`:

```rust
//! Anthropic API types for the `/v1/models` endpoint.

use serde::{Deserialize, Serialize};

/// One page of the `/v1/models` listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    /// The models on this page.
    pub data: Vec<Model>,
    /// Whether further pages follow this one.
    #[serde(default)]
    pub has_more: bool,
    /// The id of the last model on this page, used as the pagination cursor.
    #[serde(default)]
    pub last_id: Option<String>,
}

/// A single model entry from the `/v1/models` listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// The model identifier, e.g. `claude-opus-4-8`.
    pub id: String,
    /// A human-friendly name, when the API provides one.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Maximum number of input (context) tokens the model accepts.
    pub max_input_tokens: u32,
    /// Maximum number of output tokens the model emits, when bounded.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// The capabilities the model reports.
    #[serde(default)]
    pub capabilities: Capabilities,
}

/// The capability sub-objects the adapter reads from a model entry.
///
/// Each sub-object the API omits defaults to unsupported, so a model that does
/// not report a capability is treated as lacking it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Capabilities {
    /// Whether the model accepts image inputs.
    #[serde(default)]
    pub image_input: Supported,
    /// Whether the model can be constrained to structured (JSON) output.
    #[serde(default)]
    pub structured_outputs: Supported,
    /// Whether the model performs explicit reasoning (thinking).
    #[serde(default)]
    pub thinking: Supported,
}

/// A capability flag of the shape `{ "supported": bool }`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Supported {
    /// Whether the capability is supported. Defaults to `false` when absent.
    #[serde(default)]
    pub supported: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_a_model_entry_with_capabilities() {
        // Trimmed from the issue's `/v1/models` sample: the fields the adapter reads.
        let payload = r#"{
            "data": [
                {
                    "type": "model",
                    "id": "claude-opus-4-8",
                    "display_name": "Claude Opus 4.8",
                    "max_input_tokens": 1000000,
                    "max_tokens": 128000,
                    "capabilities": {
                        "image_input": { "supported": true },
                        "structured_outputs": { "supported": true },
                        "thinking": { "supported": true }
                    }
                }
            ],
            "has_more": false,
            "last_id": "claude-opus-4-8"
        }"#;

        let response: ModelsResponse = serde_json::from_str(payload).expect("parses");

        assert_eq!(response.data.len(), 1);
        let model = &response.data[0];
        assert_eq!(model.id, "claude-opus-4-8");
        assert_eq!(model.display_name.as_deref(), Some("Claude Opus 4.8"));
        assert_eq!(model.max_input_tokens, 1_000_000);
        assert_eq!(model.max_tokens, Some(128_000));
        assert!(model.capabilities.image_input.supported);
        assert!(model.capabilities.structured_outputs.supported);
        assert!(model.capabilities.thinking.supported);
        assert!(!response.has_more);
    }

    #[test]
    fn should_default_missing_capability_sub_objects_to_unsupported() {
        let payload = r#"{
            "data": [
                { "id": "claude-haiku-4-5", "max_input_tokens": 200000, "capabilities": {} }
            ],
            "has_more": false
        }"#;

        let response: ModelsResponse = serde_json::from_str(payload).expect("parses");
        let model = &response.data[0];

        assert_eq!(model.display_name, None);
        assert_eq!(model.max_tokens, None);
        assert!(!model.capabilities.image_input.supported);
        assert!(!model.capabilities.structured_outputs.supported);
        assert!(!model.capabilities.thinking.supported);
        assert_eq!(response.last_id, None);
    }
}
```

- [ ] **Step 2: Wire the module just enough to compile the test**

In `crates/smista-providers/src/provider/anthropic.rs`, add near the top (after the module doc comment):

```rust
pub mod client;
```

Then create the parent `client` module file in Task 3. To run this task's test now, temporarily create `crates/smista-providers/src/provider/anthropic/client.rs` containing only:

```rust
//! Anthropic API client implementation.

pub mod api;
```

Task 3 expands this same file.

- [ ] **Step 3: Run the test**

Run: `cargo test -p smista-providers provider::anthropic::client::api`
Expected: PASS (two tests).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(providers): Anthropic /v1/models response types"
```

---

## Task 3: Anthropic management client (trait, HTTP impl, pagination, mock)

**Files:**
- Modify: `crates/smista-providers/src/provider/anthropic/client.rs`

- [ ] **Step 1: Replace the stub client file with the full client**

Overwrite `crates/smista-providers/src/provider/anthropic/client.rs`:

```rust
//! Anthropic API client implementation.

use std::sync::Arc;

use reqwest::{Method, Request};
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::Provider;

use crate::ProviderResult;
use crate::auth::Authentication;

pub mod api;

/// Default base URL for the Anthropic API.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Number of models requested per `/v1/models` page.
///
/// The catalog is far smaller than one page; pagination exists for correctness,
/// not because the limit is expected to be hit.
const PAGE_LIMIT: u32 = 50;

/// Trait for the Anthropic management API client.
pub trait AnthropicClient: Send + Sync {
    /// Lists every available model, following pagination, authenticating with
    /// the supplied [`Authentication`].
    fn models(
        &self,
        authentication: &Authentication,
    ) -> impl Future<Output = ProviderResult<Vec<api::Model>>> + Send;
}

/// HTTP client implementation of the Anthropic management API client.
///
/// Holds only the connection: credentials are supplied per request as an
/// [`Authentication`] and applied when the request is built.
#[derive(Debug, Clone)]
pub struct HttpAnthropicClient {
    /// Base URL for the Anthropic API.
    base_url: String,
    /// HTTP client for making requests to the Anthropic API.
    client: Arc<reqwest::Client>,
}

impl Default for HttpAnthropicClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpAnthropicClient {
    /// Instantiates a client targeting the public Anthropic API.
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL.to_string())
    }

    /// Instantiates a client targeting `base_url`.
    ///
    /// This is a testing and advanced seam: production code uses
    /// [`HttpAnthropicClient::new`], which targets the public API.
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            client: Arc::new(reqwest::Client::new()),
        }
    }

    fn request(&self, url: String, authentication: &Authentication) -> ProviderResult<Request> {
        let mut req = self
            .client
            .request(Method::GET, format!("{base}{url}", base = self.base_url));

        // Anthropic authenticates the management API with the `x-api-key` header.
        if let Some(api_key) = authentication.api_key() {
            req = req.header("x-api-key", api_key.expose_secret());
        }

        // Extra headers for authentication, useful for custom proxies.
        for (key, value) in authentication.headers() {
            req = req.header(key, value.expose_secret());
        }

        req.build().map_err(normalize_reqwest_error)
    }
}

impl AnthropicClient for HttpAnthropicClient {
    async fn models(&self, authentication: &Authentication) -> ProviderResult<Vec<api::Model>> {
        let mut all = Vec::new();
        let mut after: Option<String> = None;

        loop {
            let url = match &after {
                Some(id) => format!("/v1/models?limit={PAGE_LIMIT}&after_id={id}"),
                None => format!("/v1/models?limit={PAGE_LIMIT}"),
            };

            tracing::debug!("Fetching Anthropic models page: {url}");
            let request = self.request(url, authentication)?;
            let page: api::ModelsResponse = self
                .client
                .execute(request)
                .await
                .map_err(normalize_reqwest_error)?
                .json()
                .await
                .map_err(normalize_reqwest_error)?;

            let has_more = page.has_more;
            let last_id = page.last_id.clone();
            all.extend(page.data);

            // Stop when the API reports no more pages, or when it claims more but
            // gives no cursor to advance with (defensive against a malformed page).
            match (has_more, last_id) {
                (true, Some(id)) => after = Some(id),
                _ => break,
            }
        }

        tracing::debug!("Fetched {} Anthropic model(s)", all.len());
        Ok(all)
    }
}

fn normalize_reqwest_error(error: reqwest::Error) -> ProviderError {
    ProviderError {
        message: error.to_string(),
        category: crate::error::category_from_reqwest(&error),
        provider: Provider::Anthropic,
        model: None,
    }
}

/// A test double that returns a canned model list and counts calls.
///
/// The call counter lets cache tests assert that a hot cache serves repeat
/// lookups without re-querying the API.
#[cfg(test)]
pub struct MockAnthropicClient {
    /// The canned response every [`AnthropicClient::models`] call clones.
    pub models_response: ProviderResult<Vec<api::Model>>,
    /// Number of times [`AnthropicClient::models`] has been invoked.
    pub models_calls: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
impl MockAnthropicClient {
    /// Creates a mock that always returns `models_response`.
    pub fn new(models_response: ProviderResult<Vec<api::Model>>) -> Self {
        Self {
            models_response,
            models_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns how many times [`AnthropicClient::models`] has been called.
    pub fn models_calls(&self) -> usize {
        self.models_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
impl AnthropicClient for MockAnthropicClient {
    async fn models(&self, _authentication: &Authentication) -> ProviderResult<Vec<api::Model>> {
        self.models_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.models_response.clone()
    }
}
```

- [ ] **Step 2: Build the crate**

Run: `cargo build -p smista-providers`
Expected: PASS. (`MockAnthropicClient` is `#[cfg(test)]`, so it builds under test only; it is exercised in Task 5.)

- [ ] **Step 3: Run the api tests again to confirm the module still wires up**

Run: `cargo test -p smista-providers provider::anthropic::client`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(providers): Anthropic management client with model pagination"
```

---

## Task 4: Family pricing in the Anthropic model module

**Files:**
- Modify: `crates/smista-providers/src/model/anthropic.rs`

This task adds `family_pricing` without yet removing the catalog, so the crate keeps compiling. The catalog and per-model modules are removed in Task 5.

- [ ] **Step 1: Add the pricing function and its tests**

In `crates/smista-providers/src/model/anthropic.rs`, add (place the `use` at the top with the other imports, and the function plus tests in the file body):

```rust
use rust_decimal::Decimal;

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
```

- [ ] **Step 2: Run the pricing tests**

Run: `cargo test -p smista-providers model::anthropic::pricing_tests`
Expected: PASS (two tests).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(providers): Anthropic family pricing table"
```

---

## Task 5: Rewrite the provider over the client and cache; remove the static catalog

This task swaps the provider to the client-and-cache model and deletes the hardcoded catalog. It touches three files together so the crate compiles at the end. Write the provider tests first.

**Files:**
- Modify: `crates/smista-providers/src/provider/anthropic.rs`
- Modify: `crates/smista-providers/src/model/anthropic.rs`
- Delete: `crates/smista-providers/src/model/anthropic/{opus,haiku,sonnet}.rs`

- [ ] **Step 1: Remove the per-model modules and the catalog from `model/anthropic.rs`**

Delete the files:
```bash
git rm crates/smista-providers/src/model/anthropic/opus.rs \
       crates/smista-providers/src/model/anthropic/haiku.rs \
       crates/smista-providers/src/model/anthropic/sonnet.rs
```

In `crates/smista-providers/src/model/anthropic.rs`, remove these items:
- the `mod haiku;`, `mod opus;`, `mod sonnet;` declarations;
- the `pub use self::haiku::haiku_4_5;`, `pub use self::opus::{opus_4_6, opus_4_7, opus_4_8};`, `pub use self::sonnet::sonnet_4_6;` re-exports;
- the entire `pub fn catalog() -> Vec<ModelDescriptor> { ... }` function and its doc comment;
- the `should_expose_distinct_model_descriptors` test (it referenced `catalog()`), and the now-unused test imports it relied on (`std::collections::BTreeSet`, and the `Provider` import if only that test used it).

Keep everything else (`family_pricing` from Task 4, `AnthropicModelArgs`, `AnthropicModel`, the remaining tests). If removing the re-exports leaves `ModelDescriptor` unused in this file's imports, drop it from the `use` list to satisfy `-D warnings`.

- [ ] **Step 2: Write the rewritten provider with failing tests**

Overwrite `crates/smista-providers/src/provider/anthropic.rs`:

```rust
//! Anthropic [`Provider`] implementation.

pub mod client;

use std::sync::Arc;
use std::time::Duration;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    Provider as ProviderId,
};

use self::client::{AnthropicClient, HttpAnthropicClient};
use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::anthropic::{self, AnthropicModel, AnthropicModelArgs};
use crate::provider::cache::{Cached, TtlCache};

/// How long the Anthropic models cache stays fresh before the API is queried
/// again.
///
/// The Anthropic catalog changes rarely, so a long, fixed time-to-live keeps
/// repeat lookups off the API while bounding how stale the advertised list can
/// be. One hour.
const MODELS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// A [`Provider`] backed by a single Anthropic account.
///
/// Offers the Claude models the account can see (fetched from `/v1/models` and
/// cached) and resolves a [`ModelReference`] into an executable [`Model`] built
/// from the shared [`AnthropicModelArgs`]. The credential is not held by the
/// provider: it is supplied per request as an [`Authentication`] when listing or
/// resolving, so one long-lived provider can serve many callers and owns its own
/// model cache.
pub struct AnthropicProvider<C, S>
where
    C: AnthropicClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Client used to query the Anthropic management endpoints.
    client: C,
    /// Shared arguments used to construct each resolved [`AnthropicModel`].
    args: AnthropicModelArgs<S>,
    /// Available models cache, refreshed with TTL.
    models: TtlCache<Vec<ModelDescriptor>>,
}

impl<S> AnthropicProvider<HttpAnthropicClient, S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`AnthropicProvider`] targeting the public Anthropic API.
    ///
    /// The provider owns its own models cache, created with a fixed TTL, so a
    /// single long-lived instance keeps repeat lookups off the API.
    pub fn new(args: AnthropicModelArgs<S>) -> Self {
        Self::with_client(HttpAnthropicClient::new(), args)
    }
}

impl<S> From<AnthropicModelArgs<S>> for AnthropicProvider<HttpAnthropicClient, S>
where
    S: MemoryStorage + 'static,
{
    fn from(args: AnthropicModelArgs<S>) -> Self {
        Self::new(args)
    }
}

impl<C, S> AnthropicProvider<C, S>
where
    C: AnthropicClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Constructs a provider with an explicitly supplied management client.
    ///
    /// This is a testing and advanced seam: production code should use
    /// [`AnthropicProvider::new`]. The provider owns a fresh models cache.
    pub fn with_client(client: C, args: AnthropicModelArgs<S>) -> Self {
        Self {
            client,
            args,
            models: TtlCache::new(MODELS_CACHE_TTL),
        }
    }

    /// Fetches the available models, using the cache when it is still fresh.
    async fn fetch_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelDescriptor>> {
        if let Cached::Hit(cached) = self.models.get() {
            tracing::debug!("Anthropic models cache hit: {} model(s)", cached.len());
            return Ok(cached);
        }

        tracing::debug!("Anthropic models cache miss, fetching from API");
        let models = self.client.models(authentication).await?;
        let descriptors: Vec<ModelDescriptor> =
            models.into_iter().map(Self::normalize_model).collect();

        tracing::debug!("Fetched {} model(s) from Anthropic API", descriptors.len());
        self.models.set(descriptors.clone());

        Ok(descriptors)
    }

    /// Turns an API model entry into a [`ModelDescriptor`].
    ///
    /// Streaming, system-prompt, tools, and memory are universal Anthropic
    /// features the listing does not flag per model, so they are seeded on; the
    /// remaining capabilities and the context sizes come from the entry, and the
    /// price comes from the family table.
    fn normalize_model(model: client::api::Model) -> ModelDescriptor {
        let capabilities = ModelCapabilities {
            streaming: true,
            system_prompt: true,
            tools: true,
            memory: true,
            json_output: model.capabilities.structured_outputs.supported,
            images: model.capabilities.image_input.supported,
            reasoning: model.capabilities.thinking.supported,
        };

        let (input_cost, output_cost) = anthropic::family_pricing(&model.id);

        ModelDescriptor {
            provider: ProviderId::Anthropic,
            model: model.id,
            display_name: model.display_name,
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities,
            max_context_tokens: model.max_input_tokens,
            max_output_tokens: model.max_tokens,
            input_cost_per_million_tokens: input_cost,
            output_cost_per_million_tokens: output_cost,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }
}

#[async_trait::async_trait]
impl<C, S> Provider for AnthropicProvider<C, S>
where
    C: AnthropicClient + 'static,
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
    ) -> ProviderResult<Arc<dyn Model>> {
        let descriptor = self
            .fetch_models(authentication)
            .await?
            .into_iter()
            .find(|descriptor| &descriptor.reference() == reference)
            .ok_or_else(|| {
                crate::error::provider_error(
                    ProviderErrorCategory::ModelNotFound,
                    self.id(),
                    Some(reference.model.clone()),
                    format!(
                        "model `{}` not found in Anthropic provider",
                        reference.model
                    ),
                )
            })?;

        Ok(Arc::new(
            AnthropicModel::new(self.args.clone(), authentication, descriptor).await?,
        ))
    }

    async fn list_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>> {
        self.fetch_models(authentication)
            .await
            .map(|models| models.iter().map(ModelDescriptor::reference).collect())
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use smista_core::model::Capability;

    use super::client::MockAnthropicClient;
    use super::client::api::{Capabilities, Model as ApiModel, Supported};
    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; resolution paths under test never
    /// touch it.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    fn api_model(id: &str, image: bool, structured: bool, thinking: bool) -> ApiModel {
        ApiModel {
            id: id.to_string(),
            display_name: Some(id.to_string()),
            max_input_tokens: 1_000_000,
            max_tokens: Some(128_000),
            capabilities: Capabilities {
                image_input: Supported { supported: image },
                structured_outputs: Supported {
                    supported: structured,
                },
                thinking: Supported { supported: thinking },
            },
        }
    }

    fn provider_with(
        models: Vec<ApiModel>,
    ) -> AnthropicProvider<MockAnthropicClient, NoStorage> {
        let client = MockAnthropicClient::new(Ok(models));
        AnthropicProvider::with_client(
            client,
            AnthropicModelArgs {
                preamble: "be helpful".to_string(),
                storage: Arc::new(NoStorage),
            },
        )
    }

    fn authentication() -> Authentication {
        Authentication::ApiKey(SecretString::from("sk-ant-test"))
    }

    #[test]
    fn should_identify_as_anthropic() {
        assert_eq!(provider_with(vec![]).id(), ProviderId::Anthropic);
    }

    #[test]
    fn should_seed_universal_capabilities_and_derive_the_rest() {
        let descriptor =
            AnthropicProvider::<MockAnthropicClient, NoStorage>::normalize_model(api_model(
                "claude-opus-4-8",
                true,
                true,
                true,
            ));

        // Universal Anthropic features are always on.
        assert!(descriptor.capabilities.supports(Capability::Streaming));
        assert!(descriptor.capabilities.supports(Capability::SystemPrompt));
        assert!(descriptor.capabilities.supports(Capability::Tools));
        assert!(descriptor.capabilities.supports(Capability::Memory));
        // Derived from the entry.
        assert!(descriptor.capabilities.supports(Capability::Images));
        assert!(descriptor.capabilities.supports(Capability::JsonOutput));
        assert!(descriptor.capabilities.supports(Capability::Reasoning));
        // Context sizes and price come through.
        assert_eq!(descriptor.max_context_tokens, 1_000_000);
        assert_eq!(descriptor.max_output_tokens, Some(128_000));
        assert_eq!(
            descriptor.input_cost_per_million_tokens,
            Some(rust_decimal::Decimal::new(5, 0))
        );
    }

    #[test]
    fn should_not_set_derived_capabilities_when_unreported() {
        let descriptor =
            AnthropicProvider::<MockAnthropicClient, NoStorage>::normalize_model(api_model(
                "claude-opus-4-8",
                false,
                false,
                false,
            ));

        assert!(!descriptor.capabilities.supports(Capability::Images));
        assert!(!descriptor.capabilities.supports(Capability::JsonOutput));
        assert!(!descriptor.capabilities.supports(Capability::Reasoning));
    }

    #[tokio::test]
    async fn should_list_models_from_the_api() {
        let provider = provider_with(vec![
            api_model("claude-opus-4-8", true, true, true),
            api_model("claude-haiku-4-5-20251001", true, true, true),
        ]);

        let listed = provider
            .list_models(&authentication())
            .await
            .expect("listing cannot fail");

        let ids: Vec<&str> = listed.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(
            ids,
            vec!["claude-opus-4-8", "claude-haiku-4-5-20251001"]
        );
    }

    #[tokio::test]
    async fn should_serve_repeat_lookups_from_the_cache() {
        let provider = provider_with(vec![api_model("claude-opus-4-8", true, true, true)]);

        provider
            .list_models(&authentication())
            .await
            .expect("first listing");
        provider
            .list_models(&authentication())
            .await
            .expect("second listing");

        // The API is queried once; the hot cache serves the second lookup.
        assert_eq!(provider.client.models_calls(), 1);
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let provider = provider_with(vec![api_model("claude-opus-4-8", true, true, true)]);

        let unknown = ModelReference {
            provider: ProviderId::Anthropic,
            model: "claude-does-not-exist".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider.resolve(&unknown, &authentication()).await else {
            panic!("an unoffered model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::Anthropic);
        assert_eq!(error.model.as_deref(), Some("claude-does-not-exist"));
    }

    #[tokio::test]
    async fn should_reject_resolution_without_an_api_key() {
        // An offered model still cannot be built without a credential: the model
        // build requires an API key, so a keyless authentication is rejected.
        let provider = provider_with(vec![api_model("claude-opus-4-8", true, true, true)]);
        let reference = ModelReference {
            provider: ProviderId::Anthropic,
            model: "claude-opus-4-8".to_string(),
        };

        let Err(error) = provider.resolve(&reference, &Authentication::None).await else {
            panic!("a model must not resolve without an API key");
        };

        assert_eq!(error.category, ProviderErrorCategory::MissingCredentials);
        assert_eq!(error.provider, ProviderId::Anthropic);
    }
}
```

- [ ] **Step 3: Run the provider and model tests, expect them to pass**

Run: `cargo test -p smista-providers anthropic`
Expected: PASS. All Anthropic client, model (pricing), and provider tests pass.

- [ ] **Step 4: Full crate check**

Run: `just check_code`
Expected: PASS (fmt, clippy `-D warnings`). Fix any unused-import or formatting findings the rewrite introduced (for example, a leftover `ModelDescriptor` import in `model/anthropic.rs`).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(providers): fetch Anthropic models from the API with a cached catalog"
```

---

## Task 6: Update the integration test and the docs

**Files:**
- Modify: `crates/integration-tests/provider-integration-tests/tests/anthropic_models.rs`
- Modify: an Anthropic provider page under `docs/`

- [ ] **Step 1: Drop the `haiku_4_5` dependency in the integration test**

In `crates/integration-tests/provider-integration-tests/tests/anthropic_models.rs`:

Change the model import line:
```rust
use smista_providers::model::anthropic::{AnthropicModelArgs, haiku_4_5};
```
to:
```rust
use smista_providers::model::anthropic::AnthropicModelArgs;
```

And add `ModelReference` to the existing `smista_core::model` import, so:
```rust
use smista_core::model::{ModelParameters, Provider as ProviderId};
```
becomes:
```rust
use smista_core::model::{ModelParameters, ModelReference, Provider as ProviderId};
```

Add a constant near the top of the file (after the imports):
```rust
/// The Haiku model the live test resolves and drives.
const HAIKU_MODEL: &str = "claude-haiku-4-5-20251001";
```

Replace the reference construction:
```rust
let reference = haiku_4_5().reference();
```
with:
```rust
let reference = ModelReference {
    provider: ProviderId::Anthropic,
    model: HAIKU_MODEL.to_string(),
};
```

Replace the descriptor-model assertion:
```rust
assert_eq!(descriptor.model, haiku_4_5().model);
```
with:
```rust
assert_eq!(descriptor.model, HAIKU_MODEL);
```

The `display_name` assertion (`Some("Claude Haiku 4.5")`) stays: it now comes from the API, which reports the same name.

- [ ] **Step 2: Type-check the integration test target**

Run: `cargo test -p provider-integration-tests --no-run`
Expected: PASS (compiles; the live test itself needs `ANTHROPIC_API_KEY` and is not run here).

- [ ] **Step 3: Update the docs**

Run the `md-conventions` skill first. Find the Anthropic provider documentation page (search `docs/` for the Anthropic provider section):
```bash
grep -rl "Anthropic" docs/
```

Edit the relevant page so it states, in task-oriented user language:
- The Anthropic model list is fetched live from the Anthropic API, so new Claude models appear automatically without an app update.
- The list is refreshed about once an hour.
- Per-token pricing is maintained in smista.ai by model family (Opus, Sonnet, Haiku, Fable), because the Anthropic listing does not report price.

If the page is new, register it in `docs/SUMMARY.md`. If it is an existing page, no `SUMMARY.md` change is needed.

- [ ] **Step 4: Build the docs**

Run: `mdbook build docs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "docs(providers): document live Anthropic model fetching"
```

---

## Final verification

- [ ] Run `just check_code` — Expected: PASS.
- [ ] Run `just test_all` — Expected: PASS.
- [ ] Confirm no references to `catalog`, `opus_4_`, `sonnet_4_6`, or `haiku_4_5` remain in non-test source:
  ```bash
  grep -rn "anthropic::catalog\|opus_4_\|sonnet_4_6\|haiku_4_5" crates/ --include='*.rs'
  ```
  Expected: no matches outside of deleted history.
