# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Context

smista.ai is a local-first agent and CLI that deterministically routes each
phase of an AI workflow to the most suitable model. Public design and usage
documentation lives under [docs/](docs/). The complete product specification is
private and is provided directly when needed.

Core invariant: **routing is deterministic and never depends on an LLM.**
Routing, policy evaluation, context selection, and tool mediation belong to
`smista-router`; the CLI expresses user preferences and renders results.

## Working Rules

- Read the relevant crate, docs, and tests before changing behavior.
- Keep changes scoped to the requested task and the owning crate boundaries.
- Do not revert or overwrite user changes unless explicitly asked.
- Prefer existing project patterns over new abstractions.
- Update docs for any user-facing behavior change, including config, CLI
  commands, APIs, providers, and workflows.
- Add or update focused tests when behavior changes.
- Use `just` recipes for builds, checks, tests, docs, and publishing. Do not
  invoke the underlying `cargo` or `npm` commands directly when a recipe exists.
  If a task has no recipe, add one instead of running an ad hoc command.
- Never log, print, trace, persist, or expose API keys, provider credentials,
  or auth tokens. Use `secrecy` for sensitive values and redact before
  persistence.

## Repository Layout

This is a Cargo workspace. Rust crates live under `crates/`.

| Crate                  | Kind | Responsibility                                          | License     |
| ---------------------- | ---- | ------------------------------------------------------- | ----------- |
| `smista-core`          | lib  | Shared domain types, config, policy, errors.            | MIT         |
| `smista-storage`       | lib  | Storage traits, entities, and SurrealDB implementation. | Elastic-2.0 |
| `smista-providers`     | lib  | Model abstraction and provider adapters via `rig`.      | Elastic-2.0 |
| `smista-router`        | bin  | Routing and orchestration service.                      | Elastic-2.0 |
| `smista-router-client` | lib  | Async Rust client for the router HTTP API.              | MIT         |
| `smista-sdk`           | lib  | Rust SDK facade re-exporting core types and client.     | MIT         |
| `smista-cli`           | bin  | The `smista` CLI built with `ratatui` and `clap`.       | Elastic-2.0 |

The TypeScript SDK lives in [sdk/](sdk/) as `@smista-ai/sdk`. Documentation is
an mdBook under [docs/](docs/).

## Licensing

The project is dual-licensed per crate. Consumer-facing libraries
(`smista-core`, `smista-router-client`, `smista-sdk`, and the TypeScript
`@smista-ai/sdk`) are MIT; everything else is Elastic-2.0.

There is no workspace-level `license` default. Every crate must set its own
`license` field explicitly.

When creating a crate:

- Ask whether it should be `MIT` or `Elastic-2.0` before scaffolding it.
- Set the license in the crate `Cargo.toml`.
- Reflect the license in the crate `README.md`, this file's layout table, and
  the root `README.md` components table.
- Ensure MIT crates do not depend on Elastic-2.0 crates.

## Rust

- Follow the repository `rustfmt.toml`.
- `clippy` must pass with `-D warnings`.
- Use `module_name.rs`; do not introduce `mod.rs`.
- Keep shared domain types, config, policy, and errors in `smista-core`.
- Keep routing and orchestration behavior in `smista-router`.
- Keep HTTP API concerns in `smista-router` and `smista-core` (api.rs module in particular).
- Keep storage traits, entities, and SurrealDB implementation in
  `smista-storage`.

## TypeScript SDK

- Use Biome for linting and formatting.
- Keep `tsconfig` strict.
- Do not use `any`.
- Do not edit generated files under `sdk/src/bindings/` by hand.
- After changing any Rust `#[ts(export)]` type, regenerate bindings with
  `just build_sdk`.

## Documentation

- Write docs for users performing tasks, not as internal spec dumps.
- Use task-oriented section names and runnable examples.
- Avoid internal jargon unless it is necessary and explained.
- Register any new page under [docs/](docs/) in `docs/SUMMARY.md`.
- Keep Markdown tables aligned and end files with a single newline.

## Database Schema

Any database schema change, including tables, fields, and indexes, must be
reflected in both:

- `crates/smista-storage/src/database/surreal/schema.rs`
- `docs/technical/schema.md`

Keep the migration and the authoritative schema reference in sync.

## GitHub Actions

- Workflows must pass `zizmor`.
- Pin actions by commit SHA.
- Set `persist-credentials: false` on checkout steps.

## Commands

Tasks are driven by [`just`](https://just.systems). Run `just` to list recipes.

```sh
# Build
just build_all          # crates + SDK
just build_crates       # cargo build --workspace

# Format (dprint: Markdown, TOML, YAML, Rust via nightly rustfmt)
just fmt                # format every supported file in place
just fmt_check          # check formatting without writing

# Checks (what CI runs)
just check_code         # dprint check, clippy -D warnings, SDK lint + typecheck
just test_all           # cargo test --workspace + SDK vitest

# SDK (all npm scripts are exposed as `sdk_*` recipes)
just sdk_check          # biome ci
just sdk_test           # vitest

# Publish (crates in dependency order with retry, then SDK)
just publish_all

# Docs
mdbook build docs

# Workflow security
zizmor .github/workflows
```

## Git And Commits

- Use [Conventional Commits](https://conventionalcommits.org).
- Do not add `Co-Authored-By` lines.
- Branches should be named `feat/{issue_number}-{issue_name}`, for example
  `feat/1-scaffolding`.
- Before committing, inspect the diff and include only changes relevant to the
  task.
