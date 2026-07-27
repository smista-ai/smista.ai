# Changelog

## 0.1.0-alpha.1

Released on 2026-07-27

### ⚠ Breaking Changes

- **storage:** remove session deleted_at, delete is a hard delete
  > remove session deleted_at, delete is a hard delete
- **api:** return only available providers and full model descriptors
  > return only available providers and full model descriptors
- **providers:** expose provider locality via ProviderDescriptor
  > the `Provider` trait now requires `descriptor()`,
  > `ProviderDescriptor` has a new required `local` field, and
- **storage:** add end-to-end encryption substrate for session content
  > the session table gains required encrypted and key_id fields,
  > the six session-scoped _content fields change from string to a SecretContent
  > object, and CreateSessionRequest/SessionSummary/SessionDetail gain an encrypted
  > field. Stored data and API bodies from before this change no longer match.
- **providers:** scope memory operations per call
  > MemoryStorage trait methods, Provider::resolve, AgentArgs
  > and every provider model constructor now take a MemoryScope argument.
- **router:** define the client and router execution and streaming protocol (#182)
  > ExecuteResponse, ExecutionStatus, ExecuteContext,
  > SubmitApprovalRequest and SubmitApprovalResponse are removed; ExecuteRequest
  > carries `attachments` instead of `context`; the approvals endpoint is replaced
  > by POST /sessions/{id}/continue.
- **core:** split invoked and available skills, drop inferred relevance
  > Skill loses 'description' and renames 'instructions' to
  > 'content'. Attachments.skills is replaced by invoked_skills and available_skills.
  > The OpenAPI schema and the generated TypeScript bindings change accordingly.
- **core:** remove skill from routing rules
  > routing rules no longer accept a 'skill' condition, and the
  > RoutingRule 'skill' field and the skill specificity rungs are removed. The
  > OpenAPI schema and the generated TypeScript bindings change accordingly.
- **providers:** let the model-building call take extra preamble segments (#203)
  > Provider::resolve now takes a preamble_segments: &[String]
  > argument; existing implementors and callers must pass it (an empty slice
  > preserves today's behaviour).
- add plan approval kind and plan/diff status-transition writes (#207)
  > the Database storage trait gains the required methods
  > set_plan_status and set_diff_status, and the ApprovalKind enum (in
  > smista-core and smista-storage) gains a Plan variant; external trait
  > implementations and exhaustive matches must be updated.
- **router:** add session interface and write-once content recording (#213)
  > the storage Database trait no longer exposes
  > seal_content; encrypted content is sealed before its single append, not
  > rewritten in place afterwards.
- **router:** add model selection with capability validation and fallback (#214)
  > ExecuteRequest no longer carries the `providers` field (and
  > the ProviderCredentialInfo / ProviderModelInfo types are removed) nor the
  > `input_ciphertext` field. Clients must stop sending them; the router
  > determines provider availability from its model catalog and the provider
  > credential request headers.
- run the router in-process with `smista start` and `smista stop` (#217)
  > smista-router is no longer a binary. There is no `smista-router`
  > executable; start a local router with `smista start`. The crate is now a library
  > whose entry point is `smista_router::run`.
- design the run state machine and reshape the resumable protocol (#218)
  > ContinueRequest is now a tagged { type, data } enum instead of
  > an optional-field bundle; TurnResponse gains an idle status, to_encrypt and
  > to_decrypt maps keyed by ContentRef, and allowed_continuations; SealedRecord
  > and PlainRecord are removed; the session_run_state RunPhase variants and row
  > shape changed.
- **router:** add the run execution orchestrator (#148) (#219)
  > the run context is now sealable via content
  > references; ExecuteRequest and the run-context API types changed shape.
- **router:** add OpenTelemetry trace export for the local router (#222)
  > `smista_router::run` now takes a validated `RouterConfig`
  > in `RouterArgs.config` instead of an `Option<PathBuf>`; the caller loads and
  > validates the configuration. `RouterError` no longer has `ConfigInvalid` or
  > `ConfigNotFound` variants, and `smista_router::config` is now public.
- **router:** implement POST /sessions to create a session (#153) (#223)
  > the `encrypted` field is removed from
  > CreateSessionRequest; a session is encrypted when, and only when, a
  > `key_id` is supplied. SessionSummary.title is now nullable.
- **router:** implement GET /sessions/{id} to fetch a session (#154) (#224)
  > SessionDetail.messages now holds SessionMessageDetail with a tagged MessageContent body instead of Message with a plain string content. ApiErrorCode::InvalidSessionId now maps to 400 (was 422) and SessionNotFound to 404 (was 401).
- **router:** implement DELETE /sessions/{id} to delete a session (#156) (#225)
  > DELETE /api/v1/sessions/{session_id} now responds 200 OK
  > with a { "deleted": true } body instead of 204 No Content.
- **router:** implement POST /sessions/{id}/continue and drop the /stream endpoint (#229)
  > the POST /api/v1/sessions/{session_id}/stream endpoint is
  > removed. Clients that want a streamed turn send Accept: text/event-stream to
  > /execute or /continue instead.
- **sessions:** add scope field and search by scope and title (#279)
  > `Session::new` takes a new `scope` argument before `key_id`,
  > and the router-client `list_sessions` now takes `scope` and `title` filter
  > arguments.
- **cli:** route execute and preview through router (#306)
  > SessionDetail responses and generated SDK bindings now expose key_id instead of the encrypted boolean.

* feat(cli): render validated config show output

* feat(cli)!: route execute and preview through router

Implement router-backed execute and preview handling in the CLI, including request assembly, streamed turn events, local tool execution, and safer logging.

- **preview:** mirror execute routing context (#324)
  > PreviewResponse replaces task_type, provider, model, and matched_rule with a nested routing field.

* fix(run): preserve graceful router shutdown status

### Added

- scaffold workspace, crates, SDK, docs and CI (#1)
  > Bootstrap the smista.ai project:
  >
  > - Cargo workspace with seven crates under crates/ (smista-core,
  >   smista-storage, smista-providers, smista-trace, smista-router,
  >   smista-web, smista-cli), shared workspace deps and rust toolchain config.
  > - TypeScript SDK package (@smista-ai/sdk) with Biome + tsc setup.
  > - mdBook documentation under docs/ (intro, get-started, architecture,
  >   imported specification) served via GitHub Pages.
  > - Repo files: MIT LICENSE, README with badges and logo, CLAUDE.md,
  >   CONTRIBUTING.md.
  > - GitHub Actions: Rust build/fmt/clippy/test, SDK build+lint, docs Pages
  >   deploy, and zizmor security scanning. All actions pinned by SHA;
  >   zizmor reports no findings.
- **core:** add core domain types
  > Implements the shared smista-core vocabulary consumed by every other crate:
  > task intents, model/provider references, model descriptor with capability
  > and context-window checks, authentication requirements, generation
  > parameters, messages, usage/cost metadata and stream events. All types are
  > provider-agnostic and serialization-friendly.
- **smista-router-client:** scaffold smista-router-client
- **config:** add configuration schema and layered merge across core, cli, and router
  > Split configuration by concern per the spec: shared policy/exchange types,
  > secret references and path primitives in smista-core; the config.toml model,
  > layered merge and secret resolution in smista-cli; the router.toml runtime
  > config in smista-router. No type duplication.
  >
  > - core: PermissionMode, ToolsConfig, PrivacyPolicy (remote/local), routing
  >   rules + default route, ClassificationConfig, SecretRef, paths primitives.
  > - cli: Config aggregate, ProviderConfig/ModelConfig, layer-aware merge where
  >   preference layers may only tighten safety, secret resolution via secrecy.
  > - router: RouterConfig tree under [router] with spec-matching defaults.
  > - Example config.toml/router.toml fixtures with round-trip tests; docs updated.
- **config:** resolve provider secrets via ${secret:NAME} references
  > Align SecretRef with the spec's secret-resolution model. A secret is
  > referenced inline with the ${secret:NAME} interpolation form (SecretRef::parse),
  > never as an api_key_ref field. ProviderConfig gains api_key: Option<String>,
  > holding a ${secret:NAME} reference or, where allowed, a literal.
  >
  > SecretResolver now resolves env-first, then a dotenv-style .smista/secrets
  > file with the project file overriding the global ~/.smista/secrets. Add the
  > home_smista_dir() core primitive and global_secrets_file() to anchor the
  > global secrets file. OS keychain and credential helpers are deferred (#50).
  >
  > Update the config fixture, cli configuration docs, and tests.
- **config:** drop api_key_env, unify provider credential on api_key
  > A ${secret:NAME} reference already resolves the env var named NAME first, so a

>> separate api_key_env field is redundant. ProviderConfig now carries only
>> api_key; the conventional env-var case is api_key = "${secret:OPENAI_API_KEY}".
>
> Update the config fixture, cli configuration docs, and tests.

- **policy:** add effort parameter to routing rules
  > Introduce an Effort enum (low/medium/high/xhigh, default medium) in
  > smista-core with serde lowercase representation, Display/FromStr and
  > tests. Add an `effort` field to RoutingRule, defaulting to medium when
  > unset. Update fixtures and docs.
- **core:** add api request/response types
  > Add the smista-core `api` module: the JSON wire contract for the router
  > HTTP API (`/api/v1/...`), as serialization-first request/response DTOs for
  > auth, sessions, execute, stream, preview, approvals, traces, providers/models
  > and usage, plus the structured error body. Submodules are re-exported from
  > `api.rs`.
  >
  > Types reuse the existing domain vocabulary where the wire shape matches
  > (`TaskIntent`, `ModelReference`, `Provider`, `PermissionMode`, `Message`,
  > `Usage`, `ProviderDescriptor`, `StreamEvent`). The execute `policy` block is a
  > dedicated API snapshot rather than the CLI config schema, and `/llm/models`
  > uses a flat `ModelInfo` distinct from the internal `ModelDescriptor`.
  >
  > Add supporting core modules `trace` (`Trace`) and `skill` (`Skill`), and wire
  > in `chrono`/`uuid` for typed timestamps and session ids.
- **config:** validate cli and router configuration with actionable errors
  > Add deterministic, single-pass configuration validation to smista-cli and
  > smista-router. Each crate owns its own report types (Severity, ValidationCode,
  > ValidationError, ValidationReport) — validation is local, never on the API.
  > Findings are collected together (no fail-fast) as errors (block) or warnings
  > (advisory), each with a field-path location and a fix hint; secret values are
  > never echoed into messages.
  >
  > CLI / policy (config.toml): unknown provider/model references, invalid globs
  > (globset), duplicate rule names, missing default route, self/duplicate
  > fallbacks (rules and default route), rule ambiguity (equal priority and
  > specificity), inline provider secrets, and layer-provenance checks (unsafe
  > override and permission widening across config layers).
  >
  > Router (router.toml): bind host/port, public-bind-in-embedded warning, storage
  > path/url by mode, local bootstrap in remote mode, zero/excessive timeouts and
  > zero size limits, and unrestricted CORS.
  >
  > Document every enforced rule in docs/configuration/validation.md. Add globset
  > as a workspace dependency.
- **core:** add routing policy evaluation layer
  > Add the deterministic, LLM-free evaluation behaviour for smista-core policy
  > types:
  >
  > - RoutingRule gains requires_capabilities, required_permissions and a
  >   cost_limit (rust_decimal::Decimal, serialized as a string)
  > - Specificity ladder, RoutingContext and RoutingRule::matches/specificity/
  >   precedence_cmp for deterministic rule selection
  > - ToolsConfig::narrow, which tightens tool permissions and rejects loosening
  >   with PolicyError::PermissionExpansion
  > - PrivacyPolicy::is_restricted_for_remote (fail-closed) backed by a shared
  >   globset helper
  > - Classification result type (Classification/IntentSource/Confidence)
  >
  > Document every CLI and router config field with per-section tables in
  > docs/configuration.
- **core:** add error types and API error mapping
  > Rename SmistaError to CoreError and add domain sub-errors for the
  > router's runtime boundary: AuthError, RoutingError, ProviderError
  > (with ProviderErrorCategory + is_fallback_eligible) and an opaque
  > Internal variant. Config, validation and storage errors stay in
  > their owning crates and never cross the HTTP boundary.
  >
  > Add ApiErrorResponse pairing an ApiError with an http::StatusCode,
  > and a From<CoreError> impl that is the single source of truth for
  > mapping each variant to a stable code, message and HTTP status,
  > with redacted details. Closes #6 except for the ts-rs annotations,
  > which are tracked in #7.
- **sdk:** generate TypeScript bindings from Rust types
  > Derive `ts_rs::TS` on the shared domain and HTTP API types in
  > smista-core and export the generated `.ts` definitions to
  > sdk/src/bindings, so the SDK consumes a single source of truth for the
  > wire format instead of hand-written types.
  >
  > - add ts-rs (with chrono/serde-json/uuid impls) to smista-core
  > - annotate API and domain types with `#[ts(export)]`, marking
  >   skip_serializing_if fields `#[ts(optional)]`
  > - switch monetary fields (Usage, CostRange) from f64 to rust_decimal
  >   Decimal, serialized and typed as decimal strings to preserve precision
  > - map ModelReference to a TS `string` to match its serde form
  > - configure export via .cargo/config.toml (TS_RS_EXPORT_DIR, js import
  >   extension, large-int as number)
  > - add `build_sdk` regeneration step and run it in CI, then verify the
  >   checked-in bindings are up to date
  > - exclude generated bindings from Biome
- **sdk:** scaffold smista-sdk facade crate
  > Add `smista-sdk`, the Rust consumer facade that re-exports `smista-core`
  > as `smista_sdk::core`. The router client will be added later as
  > `smista_sdk::client`; `smista-core` stays a leaf dependency.
  >
  > - new crate `smista-sdk` (`pub use smista_core as core;`) with docs and doctest
  > - register it in the workspace members and publish order (after
  >   router-client, before cli)
  > - swap `smista-cli` onto the facade (`smista_core::` -> `smista_sdk::core::`)
  > - document the crate (CLAUDE.md table, docs/sdk/rust-sdk.md, SUMMARY)
  >
  > Also source the CI Rust toolchain from rust-toolchain.toml via yq instead
  > of hardcoded versions, so the channel has a single source of truth.
- **cli:** add skill discovery and SKILL.md reader
  > Discover skills from `<cwd>/.agents/skills` (project) and `~/.agents/skills`
  > (global), with project precedence. The directory name is the skill identity;
  > the SKILL.md front matter (`name`, `description`) is metadata and the Markdown
  > body holds the behavioural instructions.
  >
  > Discovery follows progressive disclosure: only `description` plus the path are
  > held in memory; the body is read on demand by `SkillStore::load`, which builds
  > `smista_core::skill::Skill`. A name found in the project location is never
  > overwritten by a global one, and the overridden descriptor is not even read.
  >
  > Directories without a SKILL.md, missing descriptions, name/dir mismatches and
  > invalid front matter are surfaced as `SkillWarning`s rather than failing.
  > Processing emits structured `tracing` events. Parsing uses `yaml_serde`.
  >
  > Adds path primitives `home_agents_dir`/`project_agents_dir` to smista-core and
  > `global_skills_dir` to the CLI; documents skills under docs/usage/skills.md.
- **tracing:** instrument core, cli and router flows
  > Add tracing events across smista-core, smista-cli and smista-router so
  > errors, warnings and flow checkpoints are observable. Follows the
  > M-LOG-STRUCTURED convention: dotted structured fields with {{field}}
  > message templates, trace for fine steps, debug for checkpoints, warn for
  > recoverable findings and error for returned errors.
  >
  > Add the tracing dependency to smista-core and smista-router, and a
  > tracing-subscriber init in both binary entry points. Secret values are
  > never logged; only key names, paths and counts.
- **core:** add memory model capability
  > Add a `memory` capability so the router can gate memory orchestration on
  > models that can drive the memory tool — via tool calling, or via a
  > constrained-output equivalent such as OpenAI structured outputs.
  >
  > - core: `ModelCapabilities.memory` + `Capability::Memory` ("memory"),
  >   wired into `supports()`, `as_str()` and `RoutingRequirements`.
  > - cli: `ModelConfig.supports_memory` config flag + fixtures.
  > - docs: document the flag in the CLI config reference and add a memory
  >   page under docs/technical describing how user and context memory work.
- **cli:** add skill reference and capability requirement validation
  > Add the two config-validation checks deferred from #4, now that their
  > dependencies exist:
  >
  > - Unknown skill: a routing rule whose `skill` does not resolve against the
  >   discovered SkillStore is reported as an error.
  > - Unsupported capability: a rule whose `requires_capabilities` demands a
  >   capability its `model` or any fallback does not declare is reported as an
  >   error.
  >
  > `validate()` now takes the `SkillStore` as the source of truth for skill
  > names. Document both checks in docs/configuration/validation.md.
- **config:** source model facts from providers, not config (#74)
  > Model/provider facts (capabilities, context window, auth, local, costs)
  > are no longer hand-declared in configuration. They are intrinsic model
  > facts the provider knows, exposed through the provider `Model` trait at
  > runtime (#11). Configuration keeps only what the user genuinely owns:
  > credentials and routing policy.
- **storage:** add storage traits and domain entities (#8)
  > Define the storage domain entity types and the async `Database` trait;
  > application code depends on the trait, never on SurrealDB directly.
  >
  > - Entities live in `smista-storage::entity`: metadata-only tables and
  >   content-bearing tables paired 1:1 by a shared UUIDv7 record id, ready
  >   for future at-rest encryption of the `_content` payloads.
  > - `Database` covers users, tokens, sessions, append ops, session-state
  >   and trace reads, and user/context memory. Every user-scoped op takes
  >   the authenticated `user_id`.
  > - Trace events are a normal storage entity (`trace_event`), with the
  >   append/get ops on `Database`; the assembled `Trace` view stays in
  >   `smista-core`. The standalone `smista-trace` crate is removed.
  > - Core enums derive `SurrealValue` behind an optional `surrealdb`
  >   feature, reused by storage rather than duplicated.
  > - Document the schema in `docs/technical/schema.md`.
- **router:** support SurrealDB auth credentials in storage config
  > Add optional username and password to [router.storage] for
  > authenticating against a remote SurrealDB server. The password is held
  > as a secrecy::SecretString: it is never logged and is skipped on
  > serialization, so it is never written back out to a config file.
  >
  > StorageConfig keeps PartialEq/Eq via hand-written impls because
  > SecretString does not implement comparison. Enable the secrecy "serde"
  > feature at the workspace so SecretString can be deserialized from TOML.
  > Document both keys in the router configuration page.
- **storage:** SurrealDB connection and schema initialization
  > Implement the SurrealDB connection layer behind the storage boundary,
  > covering embedded (SurrealKV), in-memory and remote (HTTP/WebSocket)
  > backends through a single `Surreal<Any>` handle.
  >
  > - split the backend selection and connection options into submodules
  >   (`backend`, `options`); embedded connect creates its data directory and
  >   remote connect optionally signs in with namespace credentials
  > - apply an idempotent schema migration on connect: SCHEMALESS tables plus
  >   unique indexes for secret hashes and lookup indexes for ownership and
  >   keyed-memory queries
  > - document cross-user access as absent (`None` / `NotFound`) in the
  >   `Database` trait now that `Unauthorized` is gone
  > - add the `integration-tests` crate (publish = false) for the
  >   container-backed remote tests; `just test` excludes it, `just test_all`
  >   and `just integration_test` run it
  > - require schema changes to stay in sync with docs/technical/schema.md
- **smista-storage:** add SCHEMAFULL to database schema
- **storage:** implement SurrealDB Database trait with ownership enforcement
  > Implement all remaining Database methods on SurrealDatabase: token
  > validation/revocation, session CRUD with an explicit ownership cascade on
  > delete, paired base+content appends via SurrealDB transactions, session-state
  > and latest-trace reads, and user/context memory with keyed upsert. Cross-user
  > access is rejected (read -> None, write -> NotFound).
  >
  > Rework trace into a typed read model: move TraceEventType into smista-core,
  > add a read-model TraceEvent { event_type, task_type, provider, model,
  > matched_rule, created_at, payload }, and slim Trace to { session_id, events }.
  > Storage keeps its trace_event/_content rows and maps them on read.
  >
  > Fix a latent SurrealValue enum bug: the derive's attribute parser aborts on an
  > unrecognized `crate` meta, so rename_all/untagged were dropped and fieldless
  > enums serialized as tagged objects instead of strings. Split the surreal attr
  > and add untagged so role/provider/task_type/decision/status/event_type persist
  > as plain strings, matching the SCHEMAFULL string columns.
  >
  > Add serde_json dependency, document the trace endpoint body and per-event
  > payload schema, and regenerate the TypeScript bindings.
- 💥 **storage:** remove session deleted_at, delete is a hard delete
  > A session delete is a physical cascade purge, not a soft delete, so the
  > `deleted_at` column was always null and never read. Drop it from the
  > session entity, the SCHEMAFULL migration and the schema reference. Hard
  > deletion is also what GDPR erasure requires.
- **storage:** enforce write ownership on the redundant user column
  > Session-scoped writes trusted the caller-supplied `user` field. A caller
  > owning a session could append a row whose `user` named someone else; since
  > every ownership-scoped read filters on that column, the row became an
  > unreachable orphan. Add `assert_write_owned`, which rejects a write whose
  > `user` disagrees with the authenticated caller before the existing session
  > check, and route every append plus `record_context_memory` through it.
- **router:** add storage retention and cleanup jobs
  > Add a RetentionService that periodically purges expired auth tokens,
  > old and archived sessions, and old trace events from storage, driven by
  > the new [retention] config section. Wire the router binary to build
  > storage, run the retention task and shut down cleanly on SIGINT/SIGTERM,
  > with configurable logging.
  >
  > Add the SurrealDB cleanup operations (delete_expired_tokens,
  > purge_old_sessions, purge_archived_sessions, purge_traces) behind the
  > Database trait, with bulk ownership cascades.
  >
  > Cover the new router code with tests: retention purge behaviour and
  > shutdown, storage builder, CLI args, logging and validation rendering.
- **providers:** model trait, request/response vocabulary
  > Define the provider-agnostic request/response types in
  > smista-providers `api` module: `CompletionRequest`,
  > `CompletionResponse`, `RequestMessage`, `ToolDefinition`, `ToolCall`,
  > `ToolChoice`, `FinishReason`, and the `ResponseStream` newtype over a
  > boxed `futures::Stream`. Reuse core `ModelParameters`, `Usage`,
  > `StreamEvent` and `ProviderError` rather than redefining them.
  >
  > Flesh out the `Model` trait: require `Send + Sync` for object-safe
  > `Arc<dyn Model>` sharing, and return `CompletionResponse` (not
  > `CompletionRequest`) from `complete`.
- **providers:** document Provider trait, add ModelNotFound category
  > Add full rustdoc to the Provider trait (module + per-method docs with
  >
  > # Errors sections) and export it from the crate root.
  >
  > Introduce ProviderErrorCategory::ModelNotFound for the resolve() path
  > when a provider does not offer the referenced model, deriving
  > thiserror::Error so the enum carries per-variant Display messages.
  > Map it to HTTP 404 model_not_found in the API error layer and document
  > the code in the HTTP API reference.
- **providers:** error normalization & fallback eligibility
  > Add the shared mapping layer that collapses rig, reqwest and serde_json
  > failures onto core's ProviderError / ProviderErrorCategory, so the
  > category drives fallback eligibility (decided in core) consistently
  > across adapters.
  >
  > - core: impl From<http::StatusCode> for ProviderErrorCategory, the single
  >   source of truth for the status -> category axis (401/403/429/5xx).
  > - providers: category_from_{reqwest,serde,completion,prompt,structured}
  >   classifiers, delegating to core's From<StatusCode> when a status is
  >   present and recovering timeouts/connection failures from reqwest.
  > - providers: provider_error(..) builder constructing a ProviderError with
  >   provider, model and a redacted message that never leaks credentials.
  > - Message heuristics recover categories no HTTP status expresses
  >   (context length, model not found, rate limit, quota -> authentication).
  >
  > Adapters only report failures with the right category; they never
  > re-implement is_fallback_eligible. Per-adapter wiring is out of scope.
- **providers:** memory tool for context and user memory
  > Add the model-facing `memory` tool and the `MemoryStorage` seam behind it,
  > the first consumer of tool-calling for both memory scopes:
  >
  > - `MemoryStorage` trait: backend-agnostic storage the tool writes through and
  >   the caller reads from to build the agent preamble. Scoped per user/session,
  >   associated error type so the crate takes no storage dependency; the router
  >   backs it with its database.
  > - `MemoryTool`: `record` and `forget` ops over `user` (durable) and `session`
  >   (working) scopes, addressed by `key`. Forget resolves a key to its backend
  >   handle via `get_*_memory_by_key`, so the model never deals with opaque ids.
  >   Recall is omitted: stored memories are injected into the preamble up front.
  > - `build_preamble`: renders loaded memories into a framed, scope-grouped block
  >   appended to the agent system prompt.
  > - `ProviderErrorCategory::Storage` for storage failures surfaced through the
  >   provider adapter, mapped to 502 in the API error response.
- **providers:** anthropic provider, models and rig-backed agent
  > Add the Anthropic provider adapter, its Claude model definitions and the
  > shared rig-backed agent that maps the internal request/response vocabulary
  > onto rig and runs the memory-tool loop for completions.
  >
  > - AnthropicProvider resolves Haiku 4.5, Sonnet 4.6 and Opus 4.6/4.7/4.8
  >   references into executable models and lists what it offers; unknown
  >   references yield ModelNotFound.
  > - Each model carries its descriptor facts (capabilities, limits, costs,
  >   auth) read from provider knowledge rather than configuration.
  > - The Agent maps messages, tools, tool choice and parameters onto rig,
  >   executes agent-internal memory tool calls in complete(), and surfaces
  >   stream events; finish reasons are derived from the raw provider payload.
  > - AnthropicModelArgs and AnthropicProvider implement Debug with the API
  >   key redacted.
  > - Unit tests cover provider resolution/listing and the model references;
  >   a new API-key-gated integration test resolves Haiku and exercises both
  >   complete and stream against the live API. The in-memory storage fixture
  >   is shared from the test crate's lib.
- **providers:** openai provider, gpt models and integration test
  > Add the OpenAI provider adapter and its GPT model definitions, reusing the
  > shared rig-backed agent already used by the Anthropic adapter.
  >
  > - OpenAIProvider resolves GPT-5.4, GPT-5.4-mini and GPT-5.5 references into
  >   executable models and lists what it offers; unknown references yield
  >   ModelNotFound.
  > - Each model carries its descriptor facts (capabilities, limits, costs, auth)
  >   read from provider knowledge rather than configuration.
  > - OpenAIModelArgs and OpenAIProvider implement Debug with the API key redacted.
  > - Unit tests cover provider resolution/listing and the model references; a new
  >   OPENAI_API_KEY-gated integration test resolves GPT-5.4-mini and exercises
  >   both complete and stream against the live API.
  > - The provider integration workflow now passes OPENAI_API_KEY alongside
  >   ANTHROPIC_API_KEY.
- **providers:** ollama provider, dynamic models and integration test
  > Add an Ollama provider that discovers the daemon's installed models at
  > runtime and resolves them into executable models, with a single
  > `OllamaEndpoint` as the source of truth for locality and credentials so a
  > local-labelled model can never route completions to the cloud. Connection
  > facts are cached behind a small TTL cache shared across instances.
  >
  > Cover it with an API-key-gated provider integration test that resolves a
  > configured model and exercises both completion and streaming against a
  > live daemon, named by `OLLAMA_BASE_URL` and `OLLAMA_MODEL`. CI stands up a
  > local daemon with a tiny model so the suite runs without a paid cloud key.
- **core:** OpenAI-compatible providers as named instances (#81)
  > Introduce the identity and configuration for generic OpenAI-compatible
  > endpoints (vLLM, LM Studio, gateways, ...) as named instances, addressed
  > as `openai-compat:<name>/<model>`. This is part 1 of #81; the adapter and
  > `Provider` implementation in smista-providers (#82) are not included.
- **providers:** OpenAI-compatible adapter and provider (#82)
  > Add the smista-providers half of issue #81: an adapter that serves any
  > endpoint speaking OpenAI's chat-completions wire format (vLLM, LM Studio,
  > a gateway, ...) as a named instance.
  >
  > - OpenAICompatEndpoint: base URL plus optional bearer credential, the
  >   single source of truth for an instance's connection. Keyless endpoints
  >   send a placeholder token because rig's client always emits the header.
  > - OpenAICompatRuntime: shared preamble and memory backend.
  > - OpenAICompatModel: derives its reference from the descriptor so the two
  >   cannot disagree.
  > - OpenAICompatProvider: returns its own openai-compat:<name> identity from
  >   id(), never bare openai, so several instances route and catalog
  >   independently of each other and of OpenAI.
  >
  > Mirrors the Ollama adapter's endpoint/runtime split. Credentials are
  > redacted from Debug, guarded by leak tests.
- **providers:** streaming events and usage/cost metadata extraction
  > Map provider streams onto the full StreamEvent vocabulary and extract
  > usage and cost metadata from provider responses.
  >
  > Core gains two stream events: reasoning_delta for models that stream
  > their thinking, and tool_call_started to announce a forming tool call
  > as soon as its name is known, ahead of the complete tool_call_requested.
  > The ts-rs bindings are regenerated accordingly.
  >
  > The shared agent now takes the whole model descriptor instead of a bare
  > model name and provider, making it the single source for identity,
  > pricing and the streaming capability. Reported token usage is priced
  > from the descriptor's per-million-token costs into actual_cost; the
  > estimated cost stays with the router, which owns the token estimator.
  > Local Ollama models declare zero prices so they report a zero cost,
  > while remote Ollama endpoints keep their pricing unreported.
  >
  > Models without the streaming capability now fall back to a buffered
  > completion replayed as a short stream, so every model answers on the
  > streaming interface. Agent logs always carry the model provider and
  > name as structured fields and report request and response milestones
  > without leaking message content, tool arguments or tool output.
- **providers:** Gemini adapter and provider (#100)
  > Add a first-party Google Gemini provider that mirrors the OpenAI/Anthropic
  > provider and model split. Adds `Provider::Gemini` to smista-core and a static
  > catalog of the active Gemini text models: gemini-2.5-pro, gemini-2.5-flash,
  > gemini-3.1-pro-preview and gemini-3.5-flash. Models complete and stream through
  > `rig_core::providers::gemini`, and the provider resolves references and lists
  > models, rejecting unknown or retired ids with ModelNotFound.
  >
  > Descriptor facts (context window, output limit, default Standard text-tier
  > costs, capabilities) live in ModelDescriptor, not configuration. Documents the
  > new provider in the CLI and HTTP API docs and adds an API-key-gated integration
  > test plus the GEMINI_API_KEY secret to the provider integration workflow.
- **providers:** fetch Anthropic models from the API
  > Replace the hardcoded Anthropic catalog with a live, cached fetch from
  > GET /v1/models. A new management client paginates the listing and the
  > provider normalizes each entry into a model descriptor, caching the set
  > for one hour. The API does not report pricing, so per-token costs are
  > kept in code and matched by model family.
  >
  > - Promote the TTL cache to a shared provider module, reused by Ollama.
  > - Add the Anthropic management client, response types, pagination, and the
  >   required anthropic-version header.
  > - Add family pricing for Opus, Sonnet, Haiku, and Fable; an unrecognised
  >   family is left unpriced.
  > - Rewrite AnthropicProvider over the client and cache and delete the
  >   static catalog and per-model files.
  > - Add a DEBUG tracing subscriber to the provider integration tests.
  > - Document live model fetching and family pricing.
- **providers:** fetch Gemini models from the API
  > Replace the hardcoded Gemini catalog with a live, cached fetch from
  > GET /v1beta/models. A new management client paginates the listing and the
  > provider normalizes each entry into a model descriptor, caching the set
  > for one hour. The listing returns more than chat models, so it is filtered
  > down to the chat models smista.ai routes to. The API does not report
  > pricing, so per-token costs are kept in code as a hand-maintained census.
  >
  > - Add the Gemini management client, response types, pagination, and the
  >   required x-goog-api-key header.
  > - Filter the listing to chat models: keep entries that support
  >   generateContent and whose id carries no specialised-modality marker,
  >   dropping embedding, image, audio, tts and similar models.
  > - Add a per-model price census for the 2.5 and 3.x families, sourced from
  >   Google's published pricing where available; an unrecognised id is left
  >   unpriced.
  > - Rewrite GeminiProvider over the client and cache and delete the static
  >   catalog and per-model files.
  > - Document live model fetching, chat-model filtering and pricing.
- 💥 **api:** return only available providers and full model descriptors
  > Two changes to the LLM listing endpoints:
  >
  > - `GET /llm/providers` now lists only providers that are available
  >   (configured with usable credentials or a base URL). `ProviderDescriptor`
  >   drops its `configured` flag; unavailable providers are omitted entirely.
  > - `GET /llm/models` now returns a full `ModelDescriptor` per model instead of
  >   the flat `ModelInfo`, exposing auth, costs, output limits and default
  >   parameters. `ModelInfo` is removed.
  >
  > `ModelDescriptor`, `ModelParameters` and `ModelAuthRequirement` now derive
  > ts-rs bindings; the SDK bindings are regenerated and the HTTP API docs updated.
- 💥 **providers:** expose provider locality via ProviderDescriptor
  > Add `Provider::descriptor()` returning a `ProviderDescriptor` that now
  > carries a `local` flag: the provider-level source of truth for routing
  > locality. Ollama derives it from its endpoint (the same source that
  > stamps each model's `local`), cloud providers report false, and
  > OpenAI-compatible instances take it from configuration and reject any
  > configured model whose locality disagrees.
  >
  > Add `ProviderErrorCategory::InvalidConfiguration` (mapped to
  > `500 invalid_provider_configuration`) for that rejection, and document
  > the provider `local` flag in the HTTP API reference.
- **router:** add local and display_name to provider config
  > Add two optional fields to RouterProviderConfig:
  >
  > - local: whether the provider runs locally (default false)
  > - display_name: human-readable provider name (optional)
  >
  > These are surfaced to consumers and will later back the
  > smista-providers Provider. Update docs/configuration/router.md.
- **api:** report unavailable providers when listing models
  > GET /llm/models silently dropped providers it could not list, so a caller
  > could not tell an incomplete result from an empty one or see why a provider
  > was missing.
  >
  > Add an `unavailable` array to `ListModelsResponse`, each entry naming the
  > provider, a machine-readable `reason` (reusing `ProviderErrorCategory`) and
  > an optional redacted message. The field defaults to empty, so existing
  > payloads keep deserializing. `ProviderErrorCategory` now derives `ts_rs::TS`
  > so it exports to the TypeScript bindings.
  >
  > Update the HTTP API docs and regenerate the SDK bindings.
- 💥 **storage:** add end-to-end encryption substrate for session content
  > Add a per-session encrypted flag and key_id, and let every session-scoped
  > content field hold either plaintext or an opaque ciphertext envelope through a
  > new SecretContent type. The router stores and returns content without ever
  > holding a key. Covers the six session-scoped _content tables; user_memory_content
  > stays plaintext for now since it is user-scoped.
  >
  > Reflect the new shape in the SurrealDB migration, the schema reference, the HTTP
  > API session bodies and the core CreateSessionRequest/SessionSummary/SessionDetail
  > types, and document the design under docs/technical/e2e.md.
- **web:** scaffold axum server with status endpoint, logging and error format
  > Bring up the smista-router HTTP host. Add a WebServer service that binds
  > the listener and serves until the shared cancellation token is triggered,
  > started and joined from main.rs alongside the retention service.
  >
  > Mount every documented endpoint under /api/v1 as a module-per-request,
  > each a 501 not_implemented stub to be filled by its owning issue, plus the
  > public GET /status health check returning the crate version. Add request
  > logging middleware that keeps query strings and headers out of the logs,
  > and a guard that rejects credentials passed as query parameters. Render
  > domain errors as structured JSON via a WebError newtype over the core
  > ApiErrorResponse mapping. Add a shared web test helper reused per route.
- **router:** add per-client HTTP rate limiting via tower_governor
  > Add a [router.rate_limit] config section and a tower_governor middleware
  > layer on the web server. Requests over the limit get 429 Too Many Requests.
  >
  > The limiter keys buckets per client: by default the connection's source
  > address, or by proxy headers (X-Forwarded-For/X-Real-IP/Forwarded) when
  > trust_proxy_headers is set. The header path is opt-in because, without a
  > trusted proxy overwriting the header, a client could forge a fresh address
  > per request and bypass the limit.
  >
  > Enabled by default with limits sized for local use (about 100 req/s per
  > client, burst of 200). Startup validation rejects a zero period or burst
  > while rate limiting is enabled.
- 💥 **providers:** scope memory operations per call
  > The MemoryStorage backend is now a long-lived, shared handle rather than
  > one created per session. Every trait method takes a MemoryScope (user and
  > session ids), which is threaded from Provider::resolve through the model
  > constructors into AgentArgs, the Agent and the MemoryTool. The former
  > MemoryScope enum (user vs session store) is renamed MemoryStore to free the
  > name for the new owner type.
- **storage:** read memory rows with their content
  > Add list_user_memory_with_content and list_context_memory_with_content to
  > the Database trait and SurrealDatabase. Each pairs a memory row with its
  > _content row by shared record key, ordered most recently updated first,
  > mirroring how get_session_trace_events assembles content. Encrypted context
  > memory is returned sealed, since storage holds no key.
- **storage:** build and read memory entities by uuid
  > Add Uuid-based constructors and a uuid() accessor to the user and context
  > memory entities, plus a shared record-id-to-uuid extractor. This lets
  > application code mint and identify memories with plain Uuids, keeping callers
  > of the Database trait free of SurrealDB record id types.
- **config:** declare openai-compat model facts and warn on ignored base_url
  > A generic openai-compat:NAME endpoint publishes no catalog, so each model's
  > facts now live in config: RouterProviderConfig.models becomes a list of
  > RouterModelConfig (name and context window required; capabilities, auth and
  > costs optional with OpenAI-compatible defaults), with a to_descriptor helper.
  >
  > Validation gains a non-blocking warning when a built-in API provider (OpenAI,
  > Anthropic, Gemini) is given a base_url, which those fixed-endpoint clients
  > ignore.
- **router:** construct the routing service from configuration
- **web:** extract provider credential headers per request
  > Add an axum middleware that lifts each
  > X-Smista-Provider-<provider>-Api-Key header into a per-request
  > RequestCredentials map kept in the request extensions, so a handler can
  > use the caller's own provider key for that single request. Keys are held
  > in SecretString and never logged, traced or persisted. Split the web
  > middleware into one module per layer behind a re-exporting middleware.rs.
- **auth:** add API key generation and user bootstrap (#178)
  > Issue a versioned API key (sk-smista-api01-<user-id>-<secret>) with a CSPRNG
  > secret and the owner's id embedded, so the authentication layer recovers the
  > user from the key alone. Hash secrets with Argon2 and bootstrap a user by
  > persisting only the hash, never the raw key.
  >
  > Drop the get_user_by_api_key_hash storage method: a salted hash cannot be
  > queried by equality, so callers parse the user id from the key, load the user
  > by id and verify the key against the stored hash instead.
- **auth:** session tokens, request authentication and user scoping (#179)
  > Issue the short-lived session token on sign-in, hash it with sha-crypt
  > SHA-512 (Argon2 stays for API keys), and store only the salted hash. Load
  > tokens by their embedded id and verify against the stored hash instead of a
  > hash equality lookup, then resolve and return the owning user so requests can
  > be scoped to it. Support revocation via sign-out, and source the token TTL
  > from router.auth.token_ttl_seconds.
  >
  > Add a SecretHasher enum to dispatch between Argon2 and sha-crypt, generate
  > lowercase-alphanumeric token secrets, document router authentication, and fix
  > two token tests that asserted against the wrong token id.
- **router:** return `UserSession` with `user_id` and `expires_at` on `sign_in`
  > we need to provide on sign-in response the token expiration as well, so just returning the session token was not enough
- **web:** authenticate protected routes with bearer tokens
  > Wire the bearer authentication middleware into the router, gating the
  > protected /api/v1 endpoints while leaving bootstrap and sign-in public.
  > The guard is attached with route_layer so it runs only on matched
  > protected routes, validates the Authorization bearer token and stores
  > the resolved user id in the request extensions.
- 💥 **router:** define the client and router execution and streaming protocol (#182)
  > Pin down how the client and router talk while a task runs. Add the execution
  > protocol, task classification and routing technical references, and correct the
  > execute, stream and preview sections of the HTTP API reference to match.
  >
  > Restructure the request and response types: the client sends only local
  > attachments (files, instructions and skills) while the router owns history,
  > memory and context. Replace the single-shot response with a status-tagged
  > TurnResponse (completed, awaiting_tool, awaiting_approval, error) and add the
  > ContinueRequest bundle for advancing a run. Add the classification policy block
  > and surface the classification result. Wire the continue endpoint and drop the
  > standalone approval endpoint, folding tool approvals into the tool result.
  >
  > Regenerate the TypeScript SDK bindings.
- **web:** implement public POST /auth/bootstrap (#184)
  > Implement the bootstrap endpoint: it creates a user and returns 201 with the user id and a one-time API key, persisted only as an argon2 hash and shown once.
  >
  > Expose the wire error codes as a typed `ApiErrorCode` enum in smista-core, mirroring docs/api/http-api.md, and route every router and core error through it so a code and its HTTP status can never drift. Reconcile the router's ad-hoc codes: drop `missing_token` for the documented `missing_credentials`, give the query-credential guard its own `credentials_in_query` (400), and add `not_implemented` (501) and `invalid_api_key` (401). Move `StatusResponse` into smista-core so the status endpoint shares the typed contract.
- **web:** implement public POST /auth/sign-in (#185)
  > Exchange an API key, presented in the X-Smista-Api-Key header, for a
  > short-lived session token and its expiry. The API key already embeds the
  > user id, so no request body is needed; drop the unused SignInRequest type
  > and its TypeScript binding.
  >
  > A missing header returns 401 missing_credentials and a malformed, unknown
  > or non-matching key returns 401 invalid_api_key, reported uniformly so it
  > never reveals which users exist. The API key is never logged or echoed
  > back, and query-parameter credentials are already rejected upstream.
  >
  > Rename UserSession to SessionToken and drop its unused user_id field.
- **web:** implement POST /auth/sign-out (#186)
  > Wire the sign-out handler to revoke the presented session token and confirm
  > with { "revoked": true }.
  >
  > Teach the authenticator to tell a still-known token apart from an unknown one:
  > load the token regardless of state through the new Database::get_token, verify
  > its secret against the stored hash first, then report token_revoked or
  > token_expired. The state is disclosed only to a caller holding the genuine
  > secret, so a forged or unknown token still fails with invalid_token.
- **web:** implement GET /auth/me (#152) (#187)
  > Return the authenticated user's id from the request extensions instead of
  > the scaffolded placeholder. The response carries only the user id, lists no
  > sessions and exposes no secrets.
- **web:** implement GET /llm/providers (#163) (#191)
  > List the providers currently available to route to, each with its id,
  > display name and local flag. Only available providers are registered on
  > the router, so every entry returned is ready to use. Split router
  > construction into router/build.rs and expose Router::list_providers.
- **web:** implement GET /llm/models (#164) (#192)
  > Fetch models from every configured provider in parallel under a
  > per-provider deadline, passing through the caller's
  > X-Smista-Provider-<Provider>-Api-Key credentials. List the available
  > models as full descriptors and report providers that could not be
  > listed under `unavailable`. The deadline defaults to 10s and is
  > tunable per request via the X-Smista-Timeout-Ms header, clamped to 60s.
  >
  > Move model discovery into router/fetch_models.rs. Also require
  > `just test` and `just check_code` to pass before committing.
- **web:** add OpenAPI schema for the router HTTP API (#168) (#193)
  > Generate a machine-readable OpenAPI 3.1 schema for the router HTTP API
  > from the Rust types, published with the docs site at
  > docs/api/openapi.json and kept faithful to docs/api/http-api.md by a
  > drift gate run in CI.
  >
  > The schema is derived behind an optional `openapi` cargo feature
  > (utoipa), so nothing OpenAPI-related compiles in the default build. A
  > feature-gated test serializes the document; `just gen_openapi`
  > regenerates it and `just check_openapi` (in check_code and a dedicated
  > CI job) fails on drift.
  >
  > Squashed commits:
  >
  > - build(core): add optional utoipa behind openapi feature
  > - feat(core): derive ToSchema on api wire types under openapi feature
  > - feat(core): derive ToSchema on nested types reachable from the api
  > - fix(core): use public utoipa PartialSchema for Provider and ModelReference
  > - build(router): add optional utoipa behind openapi feature
  > - feat(router): scaffold OpenAPI document with status and me operations
  > - feat(router): annotate auth, session and llm operations for OpenAPI
  > - feat(router): annotate execution, trace and usage operations for OpenAPI
  > - feat(router): generate assets/openapi.json from the OpenAPI document
  > - build: add gen_openapi and check_openapi recipes with drift gate
  > - fix(router): publish openapi schema under docs/api and document it for consumers
  > - style(just): fix openapi.just indentation and stale path comment
  > - ci: check the OpenAPI schema is up to date
- **router:** add trace recording and retrieval via Tracer (#196)
  > Add the session-scoped Tracer that the routing stages call to record what
  > they decided and the trace endpoint calls to read events back: one record_*
  > method per trace event type plus a paginated traces() retrieval.
  >
  > Type the trace payloads end to end. TraceEvent.payload becomes a
  > TraceEventPayload that is either the decrypted, kind-tagged Payload or, for an
  > encrypted session, the sealed EncryptedPayload envelope, so an encrypted trace
  > keeps its ciphertext instead of reading back empty. get_session_trace_events
  > takes a Pagination window and the two scaffolded trace endpoints collapse into
  > a single paginated GET /sessions/{id}/traces.
  >
  > Add the Classification trace event type and move ToolCallStatus into
  > smista-core so the typed payloads can serialize. Regenerate the OpenAPI schema
  > and TypeScript bindings, and update the storage schema and HTTP API docs.
- **storage:** persist the run state machine in a session_run_state table (#198)
  > The router executes per request and keeps no resident worker, so a run that
  > pauses between protocol turns (waiting for a tool, an approval, or for the
  > client to open or seal content) must survive in storage. Add a single
  > metadata-only row per session that records the phase the in-flight run is in.
  >
  > - Add the `RunState` entity and the `RunPhase` enum (Idle, Running,
  >   AwaitingTool, AwaitingApproval, AwaitingDecrypt, AwaitingEncrypt). Each
  >   Awaiting variant carries only references to rows already in storage, never
  >   raw content, so the row never needs sealing.
  > - Key the row by the session id, so it is 1:1 with the session and a write
  >   overwrites it in place.
  > - Define the `session_run_state` table in the schema migration with a unique
  >   index on `session`.
  > - Add `set_run_state` (upsert) and `get_run_state` (None means idle) to the
  >   `Database` trait and the SurrealDB implementation.
  > - Clear the row through the existing session delete and retention purge
  >   cascades.
  > - Document the table in the schema reference.
  >
  > Also bump rig-core to 0.39 and adapt the provider streaming usage mapping to
  > its non-optional token usage API.
- **router:** add deterministic task normalizer (#199)
  > Add the resolver's first stage, router::resolver::normalizer. The
  > TaskNormalizer turns one turn's prompt, command, workspace, skills and
  > classification config into a NormalizedTask: the canonical Classification
  > (intent + provenance), the relevant skills, and the touched files the
  > routing policy matches on. Classification is purely deterministic and never
  > calls a model: explicit command wins, otherwise rules are tried by ascending
  > priority (config order breaking ties) and the first match wins, falling back
  > to the default intent.
  >
  > Keyword matching is token-based and typo-tolerant via OSA edit distance with
  > static length-bucketed caps; a fuzzy-only match caps confidence at medium, and
  > a substring relationship is treated as a different word so 'preview' does not
  > match 'review'. Skill relevance is name-token based so hyphenated names match.
  >
  > Move routing-rule path matching to the router as NormalizedTask::matches and
  > delete the unused, never-constructed RoutingContext from smista-core, keeping
  > policy evaluation in the router per the project invariant. The stage stays
  > unwired; the routing loop (#141) and orchestrator (#148) consume it later.
- 💥 **core:** split invoked and available skills, drop inferred relevance
  > Skills now follow the standard SKILL.md convention through two channels carried
  > in the request. invoked_skills are the skills the user explicitly invoked:
  > authoritative, matched by routing rules, and added to the model preamble.
  > available_skills are offered for the serving model to activate by reading them,
  > and never influence routing. Both reuse the Skill type, now { name, content },
  > where content is the SKILL.md body.
  >
  > The resolver no longer guesses which skills are relevant from the prompt. The
  > normalizer carries the invoked skills through verbatim, and the name-token
  > matching added in #140 is removed. A routing rule's skill condition still
  > matches the invoked set by exact name.
  >
  > Preamble injection and model-driven activation depend on the execution
  > orchestrator and are tracked separately.
- 💥 **providers:** let the model-building call take extra preamble segments (#203)
  > Resolution now takes a list of opaque preamble segments and appends them
  > to the built model's preamble after the base text and the memory,
  > preserving order. Each segment is kept as its own block. The providers
  > layer never inspects the segments and never stores them, so they apply
  > only to the model built for that turn. Empty input leaves the preamble at
  > base plus memory. Threaded through the Provider trait, all five provider
  > implementations and the shared agent build path.
- **router:** add policy matcher and routing evaluation (#204)
  > Evaluate the user's routing rules against the normalized task and pick
  > exactly one route deterministically, without an LLM. An explicit model
  > override wins outright; otherwise rules are tried in ascending priority,
  > ties broken by descending specificity then configuration order, and the
  > first matching rule wins; otherwise the default route is used, or a
  > no-route error when none is configured.
  >
  > The matcher emits a borrowed RouteMatch naming the chosen model, its
  > fallback chain and the decision reason, leaving availability, capability
  > and locality to model selection (#142).
- 💥 add plan approval kind and plan/diff status-transition writes (#207)
  > Close the wiring gaps that left the plan and diff lifecycles unreachable.
  > Storage gains set_plan_status and set_diff_status to move a stored plan or
  > diff out of its initial status, stamping approved_at and applied_at only on
  > the matching status. The core and storage ApprovalKind enums gain a Plan
  > variant so a plan accept or reject can be raised as a standalone approval
  > with no tool to run. Regenerate the OpenAPI schema and the TypeScript
  > bindings, and document the new approval kind.
- **router:** add deterministic context selection stage (#211)
  > Build a candidate set from recalled history, memory and the request's attachments and workspace, mark each candidate restricted-for-remote and required-or-discardable with a token-size estimate, then trim it to the chosen model's effective budget without ever dropping required context.
  >
  > Authoritative content (instruction documents and invoked skills) is required; history ranks by recency and path affinity; duplicates collapse to one candidate; and the budget reserves room for the model's reply. Add a deterministic token estimator, a storage read for session history, and tracing across the resolver stages.
- 💥 **router:** add session interface and write-once content recording (#213)
  > Add Sessions and UserSession, the router's handles over the storage
  > layer for a user's sessions and for one session, so endpoints and the
  > orchestrator never query session rows directly (#212).
  >
  > Rework the tracer to the same write-once model: SerializedPayload
  > constructors yield the storable JSON, and the record_* methods take the
  > final SecretContent (plain or sealed). A metadata row and its paired
  > content are always written together, once; content is never re-encrypted
  > in place after the write. Drop the tracer's encrypt_trace_event and the
  > storage seal_content primitive that enabled the after-write reseal.
- 💥 **router:** add model selection with capability validation and fallback (#214)
  > Resolve a matched route to a usable model: walk the primary model and its
  > fallback chain in order, checking each candidate against the router's model
  > catalog and the provider credentials supplied through request headers, the
  > task's capability and context-window requirements (unioned with the rule's
  > capability gate), and the privacy locality floor. Restricted context
  > forecloses remote models over the rules and over an explicit override; a
  > remote override on restricted content is refused. Every decision and skip
  > reason is traced.
  >
  > Drop the client-declared provider list and the prompt ciphertext from the
  > execute request: the router owns the model catalog and reads provider
  > credentials from the X-Smista-Provider-<Provider>-Api-Key headers, so it
  > derives model availability itself rather than trusting the client.
- **router:** add the resolver wiring the deterministic routing stages (#216)
- 💥 run the router in-process with `smista start` and `smista stop` (#217)
  > smista.ai now ships as a single binary. smista-router becomes a library that
  > exposes `run(RouterArgs)`, and the `smista` CLI embeds it: `start` launches a
  > local router (daemonized by default, `--foreground` to stay attached) and `stop`
  > signals the recorded process to shut down gracefully.
  >
  > The CLI owns the process lifecycle the router knows nothing about: it writes a
  > pidfile around the running router, refuses to start a second one on top of a
  > live pidfile, and defaults the pidfile to a per-user runtime directory
  > (overridable with `--pidfile` or `SMISTA_ROUTER_PIDFILE`). The router cancels
  > its shutdown token if the web server fails to start, so no service is left
  > running alone.
  >
  > Also fixes the CLI runtime to enable the multi-threaded scheduler.
- 💥 design the run state machine and reshape the resumable protocol (#218)
  > Add the run-state-machine design (docs/technical/run-state-machine.md, with
  > router and client diagrams) and implement the core protocol and storage types
  > it settles. Every paused run records what it waits for and where it resumes;
  > the processing lock is orthogonal to the durable checkpoint, so a crash
  > mid-turn loses nothing.
  >
  > - core: ContinueRequest becomes a tagged { type, data } message per step;
  >   TurnResponse folds encryption onto data responses (to_encrypt), gains
  >   allowed_continuations and a terminal idle; crypto maps are keyed by a typed
  >   ContentRef ("kind:id") so the router dispatches each sealed payload to the
  >   right store; SealedRecord and PlainRecord are dropped.
  > - storage: session_run_state reworks RunPhase with a resume target per wait,
  >   multi-record decrypt/encrypt, a to_seal rider, and an orthogonal active lock
  >   (turn + ActiveTurn); the schema migration and reference stay in sync.
  > - regenerate the OpenAPI schema and TypeScript bindings; align the execution
  >   protocol, e2e and HTTP API docs.
- 💥 **router:** add the run execution orchestrator (#148) (#219)
  > Build the smista-router execution orchestrator: the per-turn run loop
  > that drives an AI workflow against the persisted run state machine,
  > with end-to-end encryption where the router stays blind at rest.
  >
  > Routing stays deterministic and never depends on an LLM. The
  > orchestrator resolves each turn, invokes the selected model with
  > fallback and cancellation, mediates the model's tool calls against the
  > session tool permissions, and pauses for the client to run tools,
  > answer approvals, or open and seal content.
  >
  > Run loop and admission:
  >
  > - in-flight run registry enforcing one turn per run, superseding the
  >   previous turn when a new one is admitted
  > - turn loop that completes a plaintext single turn end to end, pauses
  >   for client tools, advances from tool results and folded approvals,
  >   and supports break and inject with interrupted partials
  > - plan mode and standalone approval handling
  > - per-turn usage pricing from model rates, trace emission, and
  >   orchestrator error mapping to API codes
  >
  > Context and persistence:
  >
  > - the run request context is split into a sealable bundle persisted as
  >   session_run_input, stored in clear because run state is short-lived
  >   and cleared when the run ends
  > - built-in tool catalog offered to the model
  > - provider messages assembled from resolved context; history and memory
  >   recalled, gating on decrypt
  >
  > End-to-end encryption (write-nothing-until-sealed):
  >
  > - the run context is sealable via content references
  > - each content row is sealed by its real secret field (the tool-call
  >   result, not its arguments)
  > - the router stores nothing for an authored row up front: it carries the
  >   row's non-secret metadata in run state and writes metadata and sealed
  >   content together when the ciphertext returns on the next continuation
  > - decrypt is its own pause; encrypt rides the data response
  > - session memory left in clear by the memory tool is folded into the
  >   final seal when an encrypted run finishes
  >
  > Tested with a transition-matrix harness that drives every pause to a
  > terminal outcome under both plaintext and encrypted sessions.
- 💥 **router:** add OpenTelemetry trace export for the local router (#222)
  > The router can now export its tracing spans to an OTLP collector, layered
  > on top of the existing logging and disabled by default. The CLI owns the
  > configuration lifecycle and observability for a local `smista start`: it
  > resolves, loads, validates `router.toml`, folds the `--otel*` overrides in
  > (command line wins over the file), and installs an OpenTelemetry export
  > layer when enabled before handing the validated configuration to the
  > router. Only span and trace metadata is exported; secrets are never sent.
- 💥 **router:** implement POST /sessions to create a session (#153) (#223)
  > Wire the create-session handler over the per-user Sessions store, scoping
  > the new session to the authenticated user and deriving end-to-end
  > encryption solely from the presence of a key_id. Drop the redundant
  > encrypted request flag, which could disagree with key_id, and let the
  > session title be null.
- 💥 **router:** implement GET /sessions/{id} to fetch a session (#154) (#224)
  > Return the full session with its ordered messages and free-form metadata.
  > Each message carries its content as a tagged MessageContent (plaintext or
  > sealed envelope) so an end-to-end encrypted session is returned without the
  > router ever holding a key. An unknown, non-owned or archived session all
  > respond 404 so a session's existence stays private; a malformed session id
  > responds 400.
- 💥 **router:** implement DELETE /sessions/{id} to delete a session (#156) (#225)
  > Wire the delete-session handler: an authenticated owner removes a session
  > and the delete cascades to its context memory, returning
  > `{ "deleted": true }` with `200 OK`. A session owned by another user is
  > treated as absent and yields `404`, never disclosing its existence.
- **router:** implement PUT /sessions/{id} to update a session (#155) (#226)
  > Apply a partial UpdateSessionRequest: title and archived each change only when present, so an omitted field keeps its current value, and archived false restores an archived session. Every successful update refreshes updated_at and returns the updated session summary wrapped in UpdateSessionResponse. A non-owned or unknown session responds 404 so existence stays private; a malformed session id responds 400.
- **router:** implement GET /sessions to list a user's sessions (#166) (#227)
  > List the caller's sessions, archived ones included, newest first, each as
  > a SessionSummary wrapped in a ListSessionsResponse. Add the optional
  > key_id to SessionSummary so an encrypted session reports its key
  > fingerprint. Rework the OpenAPI drift check to render the schema to a
  > scratch file via OPENAPI_OUT and diff it, so it no longer depends on git
  > working-tree state.
- **router:** implement POST /sessions/{id}/execute to run a task (#228)
  > Drive the request through the orchestrator and return the turn. The reply
  > is buffered as a single JSON TurnResponse by default, or streamed as
  > Server-Sent Events of TurnEvent when the client sends
  > Accept: text/event-stream; the buffered outcome is replayed as a short
  > stream ending in the terminal turn_end event.
- 💥 **router:** implement POST /sessions/{id}/continue and drop the /stream endpoint (#229)
  > Wire the continue endpoint to the orchestrator: it advances an in-flight
  > run with a tagged ContinueRequest (tool results, approval decisions,
  > decrypted or sealed records, queued input or break) and returns the next
  > turn, buffered or streamed by the Accept header, the same shape execute
  > returns.
  >
  > Extract the shared server-sent-events helpers (content negotiation, replay
  > and framing) into web::streaming so execute and continue render streams the
  > same way.
  >
  > Remove the standalone /stream endpoint: streaming now lives on execute and
  > continue through Accept: text/event-stream, so the separate route is
  > redundant. Update the HTTP API and execution-protocol docs and regenerate
  > the OpenAPI schema accordingly.
- **router:** stream turns from the model (#158) (#230)
  > Drive a streamed turn from the model's live output instead of buffering
  > the whole turn and replaying it. A client that asks for a stream now sees
  > text and reasoning tokens as the model produces them, with the same
  > latency benefit real streaming provides.
  >
  > - Thread an optional `TurnSink` (an mpsc sender of `TurnEvent`) through the
  >   turn loop. When set and the model can stream, `invoke` drives the live
  >   `invoke_stream`, forwarding `text_delta`/`reasoning_delta` as they arrive
  >   and aggregating the same `CompletionResponse` the buffered path returns,
  >   so mediation, persistence and branching are unchanged.
  > - A non-streaming model under a streaming request is replayed as one
  >   `text_delta`, so the client only ever handles one event shape.
  > - Tool pauses and approvals stay discrete turn boundaries carried by the
  >   terminal event; usage and `turn_end` are appended from the finished
  >   response. Every stream ends with exactly one terminal event naming the
  >   outcome; a mid-stream failure rides a terminal `turn_end` with status
  >   `error`.
  > - The web layer drives the turn on a spawned task and streams the channel
  >   receiver as a live `text/event-stream` body via a shared `stream_turn`
  >   helper, on both `/execute` and `/continue`, selected by the `Accept`
  >   header.
  > - Wrap each turn in a `tracing` span carrying the session and run id, so
  >   every event the turn loop emits is attributable; `skip_all` keeps the
  >   request and credentials out of the span fields.
  > - Remove the remaining references to the former `/stream` endpoint; the
  >   docs describe streaming via the `Accept` header.
- **router:** implement POST /sessions/{id}/preview (#231)
  > Finish the preview handler: it routes the request through a new
  > Orchestrator::preview that opens the session, recalls plaintext context
  > and runs the same deterministic resolve /execute does, then maps the
  > ResolvedTurn onto a PreviewResponse. It acquires no run lock, persists
  > nothing and never invokes the model, so no provider request is made and
  > no tokens are spent.
  >
  > Review the resolver and orchestrator error codes: ResolverError now owns
  > its api_code mapping (override -> override_not_allowed, unservable route
  > -> fallback_exhausted, no rule -> no_route, context -> context_window_
  > exceeded) and the orchestrator delegates to it instead of collapsing
  > every model-selection failure onto missing_capability. Reflect the 403
  > override case in the execute, continue and preview OpenAPI responses and
  > regenerate the schema.
- **router:** implement GET /sessions/{id}/traces (#161) (#232)
  > Wire the trace endpoint to the Tracer: parse the session id, paginate
  > with limit/offset, and return the session's trace. An archived session,
  > one owned by another user, or an unknown id are all reported alike as
  > 404 so existence stays private.
- **router:** implement GET /sessions/{id}/usage (#165) (#233)
  > Aggregate a session's token and cost usage from its cost trace events in
  > a single query: the session total plus per-model and per-task-type
  > breakdowns. Only the owner reads it; an unknown, archived or another
  > user's session answers 404 alike so existence stays private.
  >
  > Token counts a provider never reported are omitted rather than guessed,
  > and a sealed cost event of an encrypted session is counted by request and
  > grouped by its plaintext metadata while its tokens and cost stay hidden.
  > Costs are decimal strings priced in USD.
- **router:** wire traces & diffs into the run loop, drop dead-code suppressions (#234)
  > Finalize the orchestrator's persistence and observability paths and remove
  > every dead-code/lint suppression from smista-router and smista-providers.
  >
  > Trace events
  >
  > - Wire all seven trace events through the orchestrator turn and continuation
  >   flows (resolution, message, cost, tool request/result, approval); the
  >   subsystem was previously unwired.
  > - Persist routing decision and context references every turn at resolve time so
  >   a continuation can recover its trace context.
  > - Seal traces for encrypted runs via reseal-at-finalize: written clear during
  >   the run, folded into to_encrypt at the completed-encrypted finalize and
  >   rewritten sealed on the sealed continuation.
  > - Add list_session_trace_content storage method and TraceEvent::uuid.
  >
  > Session diffs
  >
  > - Render unified diffs from edit_file/write_file tool arguments and persist a
  >   proposed SessionDiff at request time, transitioning to applied/rejected from
  >   the tool result; sealed for encrypted runs.
  >
  > Cleanup
  >
  > - Remove duplicate UserSession context-memory facade; cfg(test)-gate dead
  >   session/registry helpers.
  > - Drop too_many_arguments allows by deriving user_id/session_id from session in
  >   write_sealed_message/write_sealed_tool_call.
  > - providers/error: remove dead category_from_prompt/category_from_structured
  >   (the adapters drive the low-level Completion API) and the module-wide mask.
  > - web/openapi: scope the OpenAPI document to cfg(all(test, feature = "openapi")),
  >   the only configuration that instantiates it, dropping its dead-code expects.
  >
  > Zero allow/expect(dead_code) suppressions remain outside test code in the
  > touched crates.
- **router-client:** async Client trait, credential, error and config types (#237)
  > - feat(router-client): add async Client trait, credential, error and config types
  >
  > Define the backend-agnostic contract for the smista-router HTTP API: the
  > `Client` trait with one method per endpoint (status, auth, sessions, execute
  > and continue plus their streaming variants, preview, traces, providers, models
  > and usage), returning native `impl Future<Output = Result<T>> + Send` with no
  > HTTP-library dependency. Auth and provider credentials are held by the concrete
  > client, not passed per call.
  >
  > Add the supporting types: `RouterClientError`/`Result`, `RouterClientConfig`,
  > and `ProviderCredentials`, re-exporting `ApiKey`/`SessionToken` from the shared
  > `smista-core::credential` module, which single-sources the on-the-wire format
  > and the `X-Smista-Api-Key` header name now referenced through
  > `ApiKey::header_name()`.
  >
  > Concrete HTTP clients and a test mock server are separate tasks.
- **router-client:** mock router web server for client tests (#238)
  > Add a wiremock-backed MockRouter under cfg(test) that serves canned,
  > schema-correct responses for every router endpoint, so HTTP-backed
  > Client implementations can be driven against a faithful stand-in.
  > Defaults cover all routes with the router's status codes; a builder
  > overrides any endpoint, with api_error and sse helpers for error and
  > streamed-turn replies.
- **router-client:** add reqwest router client (#239)
- **router-client:** add ureq blocking router client (#190) (#240)
  > Add UreqClient, a synchronous ureq-backed Client gated behind the new
  > ureq feature, alongside the existing async ReqwestClient. Its Client
  > methods are async to match the trait but block internally with no .await,
  > so any executor drives them without a tokio reactor; the agent is
  > configured to surface non-success statuses as responses and SSE streams
  > are parsed by a blocking iterator wrapped as a Stream.
  >
  > Document both backends as a feature-flag table in the router-client and
  > sdk crate docs, surface UreqClient through smista-sdk behind the
  > ureq-client feature, and add an SDK guide section for the runtime-free
  > client.
- **router-client:** add isahc runtime-agnostic async router client (#241)
  > Implement IsahcClient, a fully async Client backed by isahc. Unlike the
  > reqwest backend it needs no tokio reactor: isahc drives I/O on its own
  > agent thread, so its Send futures resolve on any executor. Wire the
  > isahc / isahc-client features into the crate, the SDK facade, and the
  > just clippy/doc/test/coverage recipes, and document them in the feature
  > tables.
- **sdk:** add TypeScript router client over fetch (#242)
  > - chore(sdk): add node types and rename router client interface to ISmistaClient
  > - feat(sdk): add TypeScript router client over fetch
  >
  > SmistaClient implements the ISmistaClient contract using the platform
  > fetch, covering every router endpoint: auth, sessions, execution,
  > streaming, preview, traces, usage, providers, models and status.
  >
  > The client holds the API key and session token internally: bootstrap
  > stores the key, signIn exchanges it for a token, signOut clears it, and
  > an authenticated call without a token fails before any request. Provider
  > keys travel as headers only on the model-calling methods, never in a
  > query, log or error. Streaming parses server-sent events into typed turn
  > events, ending on the terminal turn_end. Errors surface as a SmistaError
  > carrying the kind, and api errors pair the status with the router code.
  >
  > Request and response types reuse the generated bindings; no hand-written
  > or OpenAPI-generated duplicates are added.
- 💥 **sessions:** add scope field and search by scope and title (#279)
  > Sessions now carry an opaque `scope` grouping key. The router stores and
  > matches it verbatim; the CLI sets it from the working directory so sessions
  > can be listed per project, while another client may scope by repository or
  > workspace id. It is kept in clear even for an encrypted session so listings
  > can filter on it.
  >
  > A new `search_sessions` storage function and a `GET /api/v1/sessions`
  > `?scope=&title=` filter list sessions by scope (exact match) and title
  > (case-insensitive substring); both filters are optional and combine.
  >
  > Also bumps ammonia 4.1.2 -> 4.1.3 to clear RUSTSEC advisory pulled in
  > transitively via surrealdb.
- **router:** validate configuration before starting the router (#281)
  > - fix(tracing): stop emitting literal {field} braces in log messages
  >
  > Structured fields were also re-interpolated in the message text with
  > double braces, e.g. "loading config {{config.path}}". In a format string
  > {{ and }} are escaped literal braces, so these rendered the literal text
  > {config.path} instead of the value and duplicated the structured field.
  > Drop the broken interpolation and keep the value as a structured field
  > with a constant message.
  >
  > - feat(config): default storage to global db dir and add memory mode
  >
  > The default StorageConfig used an empty path, which the storage validator
  > rejected as a missing embedded path even though build_storage already
  > falls back to the global db directory. Default the path to that same
  > global db directory so the default configuration validates and runs
  > without extra setup.
  >
  > Also add a Memory storage mode so the embedded backend can run fully
  > in-memory, and tests asserting the default router and CLI configurations
  > validate clean.
  >
  > - feat(router): validate configuration before starting the router
  >
  > Load and validate the router configuration before launching, both when
  > running in the foreground and when daemonizing, so an invalid config fails
  > fast with a readable report instead of spawning a broken background
  > process. Extract the load-and-validate step into a helper shared by both
  > paths. Default the router log filter to `off` so the CLI stays quiet
  > unless a filter is requested.
- **cli:** add --version flag (#282)
  > Print the CLI version via clap's built-in version flag, and document full
  > smista-cli shell usage in the CLI commands page.
- **cli:** add credential storage backends (#283)
- **cli:** add provider credential management (#284)
- **cli:** add router API key command (#285)
- **cli:** add E2EE session key storage (#286)
- **cli:** add config init command (#288)
- **mock-web-server:** add shared router API mock (#291)
- **cli:** add config inspection commands (#292)
- **cli:** auto-start local router for main command (#294)
- **cli:** add router login command (#296)
- **cli:** add `apikey new` command (#297)
  > `apikey new` command (alias `apikey generate`) calls `bootstrap` on the router and prints to stdout the generated apikey. This command is good to combine with `apikey set` to set an api key previously generated.
- **cli:** scaffold interactive client workers (#298)
- **cli:** add input listener worker (#299)
  > - feat(cli): add input listener worker
  >
  > Read terminal input as CLI events, keep TUI tests on an in-memory backend, and avoid logging typed or pasted content.
- **cli:** scaffold router client protocol (#300)
- **cli:** remember approved command actions (#301)
- **cli:** handle all "stateless" `Cmd`s in router client (#304)
  > add handlers for all the commands that don't go through the execution state (e.g. `Clear` or `ResumeSession`). Executions commands will be implmeneted in #303.
- 💥 **cli:** route execute and preview through router (#306)
  > - feat(api)!: expose session detail key ids
  >
  > Add the CLI encrypt_sessions preference, default new CLI sessions to encrypted, and keep router client session state as SessionInfo with key metadata.
- **router-client:** implement continuation handling (#307)
- **cli:** add TUI state model (#309)
- **tui:** add interactive console view (#310)
- **cli:** handle tui input states (#312)
  > Add keyboard handling for console prompts, list views, approvals, execution turns, and navigation keys.
- **cli:** add resume session selection (#313)
- **cli:** `Interrupt`, if there is not thinking path, cleans the console instead of terminating, if the prompt is not empty (#315)
- **cli:** add skills list view (#316)
  > Add a /skills command that opens a TUI list of discovered skills using the canonical skill names and descriptions.
- **cli:** add model and provider slash commands (#318)
- **cli:** add TUI prompt history navigation (#319)
- **providers:** add `gpt-5.6` models to openai catalog (#320)
- **cli:** autocomplete file mentions in TUI (#321)
- **cli:** add `/status` command (#322)
- **cli:** add routing preview command (#323)
- **cli:** add clear command (#328)
  > Clear terminal history and end the active session before the next prompt. Show final usage and a resume command when available.
- **cli:** add `/chat` and `/plan` commands (#329)
  > use `plan` command to enter plan mode; use `chat` to leave plan mode
- **cli:** add interactive log command (#330)
- **cli:** add interactive trace command (#332)
  > Show session trace events and retained logs in transcript history instead of separate list views.
- **cli:** add `/usage` command (#333)
- **cli:** add interactive help command (#334)
- **dist:** add install scripts and installation docs (#339)
  > Add install.sh and install.ps1, served at smista.ai/install, which
  > install the latest GitHub release (or a requested version) and prefer
  > Homebrew when available. Lint them via the check_install_scripts recipe
  > in CI, and document installation in the READMEs and the get-started
  > guide.

### Changed

- **api:** report model capabilities as a list
  > Replace the flat supports_streaming/supports_tools/supports_json_output
  > fields on ModelInfo with a single capabilities: Vec<Capability>, reusing
  > the existing Capability enum. A capability absent from the list is not
  > supported; this also drops the supports_json_output Option tri-state.
  >
  > Add ModelCapabilities::supported() as the type-safe bridge from the
  > internal bool struct to the public list, and derive ts-rs on Capability.
  > The internal ModelCapabilities struct is unchanged so routing keeps its
  > exhaustive-match capability checks.
  >
  > Regenerate the TS bindings and document the new shape in the HTTP API
  > docs. Note in CLAUDE.md that bindings are generated via just build_sdk.
- **storage:** rename get_latest_trace to get_session_trace_events
  > The method returns every trace event for a session, oldest first; there is
  > no per-run grouping, so "latest" implied a selection it never made. Rename
  > it to describe what it does and align the doc wording and tests.
- **core:** reconcile /execute policy with canonical routing types
  > The /execute request body carried a separate, lossy policy wire model
  > (RuleMatch, ExecuteRoutingPolicy, ExecutePermissions, ExecutePrivacy) that
  > could not represent a real CLI routing rule. Replace it with the canonical
  > smista-core policy types so the API speaks the same vocabulary the CLI loads
  > from config.toml and the router evaluates.
  >
  > - ExecutePolicy now embeds RoutingPolicy, ToolsConfig and PrivacyPolicy
  > - export RoutingPolicy, RoutingRule, DefaultRoute, ToolsConfig, PrivacyPolicy,
  >   RemotePrivacy, LocalPrivacy, Effort and ModelCapabilities to TypeScript
  > - regenerate SDK bindings; wipe stale ones before generation
  > - document the full /execute request body in docs/api/http-api.md
  > - warn that an ollama/ model is the public Ollama Cloud, not the local
  >   instance, unless the route enforces local_only
- **providers:** long-lived providers with per-request authentication
  > Providers no longer hold credentials. They are built once with connection
  > and configuration only, and the credential travels per request as a new
  > `Authentication` enum (`None`, `ApiKey`, `Headers`) passed to `resolve` and
  > `list_models`. This lets a single long-lived provider serve many callers.
  >
  > - Add `auth::Authentication` with redacting `Debug`, no serde/ts-rs, and a
  >   `require_api_key` helper returning `MissingCredentials` when absent.
  > - Drop `api_key` from `AnthropicModelArgs`/`OpenAIModelArgs`; model
  >   constructors take `&Authentication`.
  > - `OpenAICompatEndpoint` is base_url only; the bearer comes from the request.
  > - `OllamaEndpoint` keeps locality and base_url only; the client and model
  >   take `&Authentication`. The anti-leak invariant still holds because the
  >   base URL stays on the endpoint, separate from the credential.
  > - `OllamaProvider` owns its own models cache with a fixed TTL instead of
  >   receiving it from outside; the cache type and module are now private.
- **providers:** one shared model struct per provider
  > Collapse the per-model adapter types (Gpt_5_4, Opus_4_8, Gemini_2_5_Pro,
  > …) into one shared struct per provider: OpenAIModel, AnthropicModel,
  > GeminiModel. Each takes the model's ModelDescriptor at construction
  > instead of hard-coding it.
  >
  > Per-model functions now return the descriptor (the facts) rather than a
  > ModelReference, and a new catalog() per provider lists every offered
  > descriptor. Providers resolve by matching the requested reference against
  > catalog() and list models by mapping catalog() to references.
  >
  > Behaviour (completion and streaming) was already identical across the
  > per-model types, so it is defined once on the shared struct. No external
  > behaviour change.
- **router:** move route handlers to `routes` submodule
- 💥 **core:** remove skill from routing rules
  > Skills no longer participate in routing. A skill is purely a model-behavior
  > concern: invoked skills are added to the preamble and available skills are
  > offered for the model to activate, but neither selects the model. Routing keys
  > on intent and touched paths only.
  >
  > Drops RoutingRule.skill and the Skill, SkillPath and SkillPathIntent rungs of
  > the specificity ladder (now path+intent > path > intent > default). Removes the
  > normalizer's skill passthrough and its routing-rule skill match, the CLI's
  > routing-rule skill validation (the UnknownSkill check), and all references in
  > the configuration and routing docs.
  >
  > The invoked and available skill channels in the request are unchanged; only
  > their effect on routing is removed.

### Fixed

- **core:** redact api_key and token from Debug output
- **ci:** store CLA signatures on unprotected branch
  > The CLA Assistant action pushes the signatures file directly to the
  > configured branch. Pointing it at `main` failed because `main` is
  > protected by a ruleset requiring PRs and status checks, so the bot's
  > direct push was rejected:
  >
  >     Make sure the branch where signatures are stored is NOT protected.
  >
  > Store signatures at `signatures.json` on the dedicated, unprotected
  > `cla-signatures` branch instead.
- **ci:** correct actions/checkout v6 pin SHA
  > The pinned SHA de0fac2e did not match the v6 version comment; GitHub
  > flagged a mismatched hash pin. Repointed all checkout uses to the real
  > v6 commit df4cb1c069e1.
- seed router providers and inject credentials (#314)
  > Default router config now registers the known providers, and the CLI injects provider credentials into the reqwest client from stored credentials or resolved config secret references.
  >
  > Also suppresses the macOS compact-unwind linker warning surfaced by Rust 1.97.
- 💥 **preview:** mirror execute routing context (#324)
  > - fix(preview)!: mirror execute routing context
  >
  > Forward provider credentials for preview requests so model availability and fallbacks match execution. Carry the complete routing decision through the API and TUI, including its explanation.
- **cli:** rename `/log` command to `/logs` (#331)
- stabilize CLI tool and continuation flows (#335)
  > Add deterministic app-level integration coverage for interactive sessions, encrypted continuations, interruption, approvals, session commands, skills, and previews.
  >
  > Recover textual Ollama tool calls, reject unoffered tools in the router, attach file contents to the active prompt, and improve approval behavior.

### Performance

- **router:** run on a multi-threaded tokio runtime with a larger stack
  > Build the tokio runtime explicitly with new_multi_thread and a 10 MiB
  > thread stack size, then block_on the async entrypoint, instead of the
  > default runtime.

### Build

- **cargo:** move dependency features from workspace to member crates
  > Workspace dependencies now declare version only; each member crate
  > specifies the features and default-features it needs. axum and
  > rust_decimal keep default-features = false at the workspace level
  > because cargo forbids a member from overriding inherited
  > default-features to false.
- add dprint to format markdown, toml, yaml and rust
  > Wire dprint as the single formatting entrypoint. It formats Markdown,
  > TOML (Cargo.toml via cargo.applyConventions), YAML and Rust (delegating
  > to nightly rustfmt through the exec plugin).
  >
  > Replace the cargo fmt just recipes with 'fmt' (dprint fmt) and
  > 'fmt_check' (dprint check), and switch check_code and the CI fmt job to
  > run dprint check.
  >
  > Reformat the repository accordingly. Adopt dprint's [package] key order
  > (name, version, rest alphabetical, description last).
- **core:** put TypeScript bindings behind optional 'ts' feature (#195)
  > The ts-rs tooling that generates the SDK bindings was always compiled,
  > adding weight to every normal build and test run. Gate the `ts_rs::TS`
  > derives and `#[ts(...)]` attributes behind a new `ts` feature, off by
  > default, mirroring the existing `openapi` pattern. The `_generate_sdk_bindings`
  > recipe enables `--features ts` so the committed bindings stay identical.

## 0.0.0
