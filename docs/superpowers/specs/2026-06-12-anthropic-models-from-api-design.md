# Anthropic models from the API (#105)

## Problem

The Anthropic provider serves a hardcoded model catalog (`crates/smista-providers/src/model/anthropic/{opus,haiku,sonnet}.rs` plus `catalog()`). Every Anthropic release requires a manual edit, so the list drifts out of date.

Anthropic exposes a model listing endpoint. We will fetch the catalog live and keep only what the endpoint cannot give us (pricing) in code.

## Goal

Replace the static Anthropic catalog with a live fetch from `GET https://api.anthropic.com/v1/models`, cached behind a one hour TTL, mirroring the existing Ollama client-and-cache pattern. Pricing stays in code as a small family table, since the endpoint does not report it.

## Non-goals

- No offline fallback to a static list. The API is the sole source of the model set.
- No per-generation pricing. Pricing is matched at the family level (see Pricing).
- No changes to the completion path, the agent, or `ModelDescriptor`.

## API

`GET https://api.anthropic.com/v1/models`

- Header `x-api-key` carries the API key, taken from the per-request `Authentication`.
- Query `limit=50` per page.
- Paginated: the response carries `has_more` and `last_id`. When `has_more` is true, the next page is fetched with `after_id=<last_id>` until `has_more` is false. The page limit will not be hit in practice, but pagination is implemented for correctness.

Response shape (fields we read):

```json
{
  "data": [
    {
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
}
```

Capability sub-objects we do not read (batch, citations, code_execution, context_management, effort, pdf_input, and the nested thinking/effort detail) are ignored. Unknown fields are tolerated.

## Components

### 1. Shared TTL cache

Promote `crates/smista-providers/src/provider/ollama/cache.rs` to `crates/smista-providers/src/provider/cache.rs` so Anthropic, Ollama, and later Gemini share one `TtlCache`. The implementation is unchanged; only its location and the importing modules move. Ollama's `mod cache;` is removed and its imports point at the shared module.

### 2. Anthropic management client

New module `crates/smista-providers/src/provider/anthropic/client.rs` with a `client/api.rs` submodule, mirroring `provider/ollama/client.rs`.

- Trait `AnthropicClient` with one method: `models(&self, authentication: &Authentication) -> ProviderResult<api::ModelsResponse>`. The trait method fetches a single page; pagination is driven by the provider, OR the trait method returns the full list. Decision: the trait exposes a single `models` call that returns the fully paginated list of `api::Model`, so the provider does not know about cursors and the test double stays trivial. The `HttpAnthropicClient` performs the page loop internally.
- `HttpAnthropicClient` holds a base URL (default `https://api.anthropic.com`) and an `Arc<reqwest::Client>`. It builds each request with the `x-api-key` header from `Authentication::api_key()` and the extra `Authentication::headers()`, matching the Ollama client's request builder.
- Errors normalize through the same `category_from_reqwest` path as Ollama, with `Provider::Anthropic`.
- `client/api.rs` holds serde types: `ModelsResponse { data: Vec<Model>, has_more: bool, last_id: Option<String> }`, `Model { id, display_name: Option<String>, max_input_tokens: u32, max_tokens: Option<u32>, capabilities: Capabilities }`, and `Capabilities` with the three `{ "supported": bool }` sub-objects we read, each defaulting to unsupported when absent.
- `MockAnthropicClient` test double (under `#[cfg(test)]`) returns a canned `Vec<Model>` and counts calls, so cache tests can assert a hot cache serves repeat lookups.

### 3. Capability mapping

The endpoint reports no streaming, tools, system-prompt, or memory flags; these are universal Anthropic features, so they are seeded on, the same way the Ollama adapter seeds its universal features. The rest derive from the reported capabilities:

| `ModelCapabilities` field | Source |
| ------------------------- | ------------------------------------------ |
| `streaming`               | always `true`                              |
| `system_prompt`           | always `true`                              |
| `tools`                   | always `true`                              |
| `memory`                  | always `true` (driven by tool calls)       |
| `json_output`             | `capabilities.structured_outputs.supported`|
| `images`                  | `capabilities.image_input.supported`       |
| `reasoning`               | `capabilities.thinking.supported`          |

Context sizes map directly: `max_context_tokens` from `max_input_tokens`, `max_output_tokens` from `max_tokens`.

### 4. Pricing

New pricing helper in the `model/anthropic` module (`model/anthropic.rs`, since the per-model files are deleted). It matches a model id to a family by lowercase substring and returns the input and output price per million tokens as `rust_decimal::Decimal`:

| Family substring | Input ($/Mtok) | Output ($/Mtok) |
| ---------------- | -------------- | --------------- |
| `opus`           | 5              | 25              |
| `sonnet`         | 3              | 15              |
| `haiku`          | 1              | 5               |
| `fable`          | 10             | 50              |

A model id matching no family leaves both costs `None`. Substrings are checked against the lowercased id; the set is small and non-overlapping among current ids.

Known tradeoff: legacy Opus ids (`claude-opus-4`, `claude-opus-4-1`) match `opus` and are priced at the current Opus rate rather than their historical $15/$75. This is accepted: pricing is deliberately family level, and the legacy models are not part of smista.ai's routing set in practice.

### 5. Provider rewrite

`AnthropicProvider` is restructured to mirror `OllamaProvider`:

- Fields: `client: C` (where `C: AnthropicClient`), `args: AnthropicModelArgs<S>`, and `models: TtlCache<Vec<ModelDescriptor>>`.
- `MODELS_CACHE_TTL = Duration::from_secs(3600)` (one hour).
- `new(args)` builds an `HttpAnthropicClient` with the default base URL and calls `with_client`.
- `with_client(client, args)` is the test seam, constructing a fresh cache.
- A private `fetch_models(authentication)` returns the cached descriptors on a hit, otherwise calls `client.models(authentication)`, normalizes each `api::Model` into a `ModelDescriptor` (capability mapping plus pricing merge, `provider = Anthropic`, `local = false`, `auth = ApiKey`, `default_parameters` empty, `provider_options = None`), caches, and returns.
- `resolve` finds the descriptor whose `reference()` matches and builds `AnthropicModel::new(args, authentication, descriptor)`. The model build still enforces the API key, so a keyless resolve fails with `MissingCredentials` exactly as today.
- `list_models` maps the fetched descriptors to references.

The generic parameter `C` is added to `AnthropicProvider<S>`, making it `AnthropicProvider<C, S>` like `OllamaProvider<C, S>`. The `Debug`, `From`, and constructor impls move accordingly.

### 6. Deletions and test updates

- Delete `model/anthropic/{opus,haiku,sonnet}.rs`, the `opus_4_*`/`sonnet_4_6`/`haiku_4_5` functions, the re-exports, and `catalog()`.
- The model id no longer comes from rig constants (`CLAUDE_OPUS_4_8`, etc.); the agent already takes the id from `descriptor.model`, which now comes from the API, so nothing in the completion path changes.
- Replace the catalog-shape unit test in `model/anthropic.rs` with unit tests for capability mapping and pricing.
- Provider unit tests use `MockAnthropicClient` with a canned model list: identifies as Anthropic, lists models, serves repeat lookups from the cache (one client call for two listings), rejects an unknown model with `ModelNotFound`, and rejects a keyless resolve with `MissingCredentials`.
- Update the integration test `crates/integration-tests/provider-integration-tests/tests/anthropic_models.rs` to the new constructor (still `AnthropicProvider::new(args)`, since `new` keeps the same signature).

## Testing

- `cache.rs`: existing tests move with the file, unchanged.
- `client/api.rs`: deserialize a trimmed real payload, tolerate missing capability sub-objects, round-trip the fields read.
- Capability mapping: structured/image/thinking flags drive json_output/images/reasoning; the four universal flags are always on.
- Pricing: each family maps to its prices; an unknown id yields `None`.
- Provider: the `MockAnthropicClient` behaviours listed above, including cache-hit call counting.
- `just check_code` and `just test_all` pass.

## Docs

Update the providers documentation under `docs/` to state that the Anthropic model list is fetched live from the API and refreshed hourly, and that pricing is maintained in code by family. Register any new page in `docs/SUMMARY.md` (likely an edit to the existing Anthropic provider page, not a new page).
