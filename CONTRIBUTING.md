# Contributing to smista.ai

Thanks for your interest in contributing! This document explains how to get set
up and the conventions we follow.

## Code of Conduct

This project adheres to the [Contributor Covenant](CODE_OF_CONDUCT.md). By
participating, you are expected to uphold it.

## Contributor License Agreement

Before your first contribution can be merged, you must sign the
[Contributor License Agreement](CLA.md) (CLA). It lets the Project be licensed
and relicensed coherently — including the per-component MIT and Elastic License
2.0 split and any future commercial licensing — while you keep ownership of your
work.

Signing is automatic: when you open your first pull request, the CLA assistant
bot comments asking you to agree. Post a pull request comment with exactly:

```text
I have read the CLA Document and I hereby sign the CLA
```

You only need to sign once; later pull requests are recognized automatically.

## Getting started

1. Fork and clone the repository.
2. Install the toolchain pinned in `rust-toolchain.toml` (rustup handles this
   automatically), [Node.js](https://nodejs.org) 22+ for the SDK, and
   [`just`](https://just.systems).
3. Build everything:

   ```sh
   just sdk_install
   just build_all
   ```

Run `just` to list every available recipe.

## Branches and commits

- Create a branch named `feat/{issue_number}-{issue_name}`, for example
  `feat/12-routing-policy-eval`. Use the matching type prefix (`fix/`,
  `docs/`, `chore/`, …) when appropriate.
- Commit messages follow [Conventional Commits](https://conventionalcommits.org).
- Do **not** add `Co-Authored-By` lines.

## Before opening a pull request

Run the same checks CI runs:

```sh
just check_code   # fmt --check, clippy -D warnings, SDK lint + typecheck
just test_all     # crate + SDK tests
just build_all
```

If you changed any GitHub Actions workflow, run `zizmor .github/workflows` and
resolve all findings.

## Documentation

User-facing documentation lives in `docs/` and is built with
[mdBook](https://rust-lang.github.io/mdBook/). Preview locally with
`mdbook serve docs`.

## Extending providers

Providers live in [`smista-providers`](crates/smista-providers) and adapt an
upstream account or endpoint (Anthropic, Gemini, OpenAI, Ollama, …) to the
[`Provider`](crates/smista-providers/src/provider.rs) trait. New providers are
always welcome, and existing ones can always be improved — as long as the
contribution is testable and follows the rules below. A pull request that
ignores them will be asked for changes; a pull request that breaks a provider's
Terms of Service is closed automatically (see
[Terms of Service](#terms-of-service)).

This section applies whenever you **add a new provider** or **change an existing
one** (its models, pricing facts, authentication, or discovery).

### Quick steps (new provider)

A new provider touches two crates, the tests, the CI and the docs. The rest of
this section explains the rules behind each step.

1. **Add the `Provider` variant** in
   [`crates/smista-core/src/model/provider.rs`](crates/smista-core/src/model/provider.rs):
   extend the `Provider` enum and update `from_canonical` and `Display` (and any
   other `match` over the enum) so the textual form round-trips. Re-export the
   `ts-rs` bindings with `just build_sdk`.
2. **Add the `Provider` impl** in `crates/smista-providers/src/provider/<name>.rs`
   and register the module in
   [`provider.rs`](crates/smista-providers/src/provider.rs). Implement `id`,
   `descriptor` (set `local` correctly), `resolve` and `list_models`. Take
   credentials from the per-request `Authentication` — never hold them.
3. **Add the `Model` impl** in `crates/smista-providers/src/model/<name>.rs`
   (register it in [`model.rs`](crates/smista-providers/src/model.rs)),
   implementing `complete` and `stream` plus the model `descriptor` (including
   pricing facts).
4. **Add an integration test** under
   [`crates/integration-tests/provider-integration-tests/tests/`](crates/integration-tests/provider-integration-tests/tests),
   modelled on `gemini_models.rs`; read any API key from an env variable.
5. **Update CI**: add a fetch/manual section to
   [`.github/workflows/model-review.yml`](.github/workflows/model-review.yml) and
   wire the required secret into
   [`.github/workflows/provider-integration.yml`](.github/workflows/provider-integration.yml).
6. **Document it**: add a docs page (pricing table + available models +
   credentials) and register it in
   [`docs/SUMMARY.md`](docs/SUMMARY.md).
7. **Run the checks**: `just check_code`, `just test_all`, `just build_all`, and
   `zizmor .github/workflows`.

### Credentials always come from the caller

Credentials never live inside a provider. The flow is strictly one-directional:

```text
user → CLI/SDK config → router → provider (per request)
```

A provider holds only connection and configuration facts and is built once. API
keys, bearer tokens and auth headers are supplied per request as an
[`Authentication`](crates/smista-providers/src/auth.rs) value the router passes
into `resolve` and `list_models`. Concretely:

- Never store, default, or read a key from the environment **inside the
  provider**. The provider receives an `Authentication` and maps the variant
  onto its wire credentials.
- Reject an `Authentication` variant you cannot use with
  `ProviderErrorCategory::MissingCredentials` (for example `Authentication::None`
  where a key is required).
- Wrap every secret in `secrecy::SecretString`; never log, trace, print or
  persist it.

### Discover models from the API when you can

When the upstream exposes a model-listing endpoint that returns enough to build
the catalog (Gemini, Anthropic), fetch the list from the API in `list_models`
rather than hardcoding it. Hardcode model facts only when the API does not
return enough to drive discovery (as OpenAI's listing does not). Hardcoded
**pricing** is unavoidable today — keep it accurate via the model-review
workflow below.

### Register the provider in the monthly model review

Every provider must appear in
[`.github/workflows/model-review.yml`](.github/workflows/model-review.yml),
which opens a reminder issue on the first of every month so maintainers refresh
model lists and pricing. Add a section for your provider:

- If the upstream has a usable listing endpoint, add a fetch step that builds a
  Markdown table from it (copy the Anthropic/Gemini steps).
- If it does not, add manual review instructions with direct links to the
  upstream's model documentation and pricing page (copy the OpenAI section).

### Document the provider with pricing and models

Add user-facing documentation under [`docs/`](docs) and register the new page in
[`docs/SUMMARY.md`](docs/SUMMARY.md). The page must include:

- A clearly stated **pricing table** (input/output rates per model).
- The list of **available models** the provider exposes.
- How a user supplies credentials for it (which config key feeds the
  `Authentication`).

### Provide integration tests

Every provider ships an integration test under
[`crates/integration-tests/provider-integration-tests/`](crates/integration-tests/provider-integration-tests),
modelled on the existing
[`gemini_models.rs`](crates/integration-tests/provider-integration-tests/tests/gemini_models.rs)
/ `anthropic_models.rs` / `openai_models.rs` suites: resolve a cheap model, then
exercise both `complete` and `stream` to prove the transport works (assert that
text came back, not its wording).

- If the provider needs an API key, read it from an **environment variable**
  (e.g. `std::env::var("YOURPROVIDER_API_KEY")`); never commit a key.
- Wire the key into
  [`.github/workflows/provider-integration.yml`](.github/workflows/provider-integration.yml)
  as a required repository secret, following the existing
  `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `GEMINI_API_KEY` pattern.
- In your pull request, include **clear instructions for the maintainer on how
  to obtain an API key** for the secret, since only a maintainer can add it.

Run the suite locally with:

```sh
just provider_integration_test                # all providers
just provider_integration_test yourprovider   # a single test
```

### Terms of Service

Authentication must never rely on a method that breaks the upstream's Terms of
Service — for example impersonating a first-party client or reusing a
subscription session as an API. Some libraries we depend on (such as
`rig-core`) expose such methods anyway; we do **not** support them and will not
build on them. If a user wires up such a method themselves, the responsibility
is entirely theirs (the sole exception being the hosted smista SaaS, where it
falls on us). **Any pull request that introduces a ToS-breaking integration is
closed automatically.**

## Reporting issues

Use the [issue tracker](https://github.com/smista-ai/smista.ai/issues). Please
include reproduction steps and the relevant configuration (with secrets
redacted).
