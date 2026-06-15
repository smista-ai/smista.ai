# CLAUDE.md

Guidance for Claude Code (and other agents) working in this repository.

## Project

smista.ai is a local-first agent and CLI that deterministically routes each
phase of an AI workflow to the most suitable model. Reference the public
documentation under [docs/](docs/) for design and guidance. The full
specification is private and is not published; it will be provided to you
directly when needed.

Core invariant: **routing is deterministic and never depends on an LLM.**
Routing, policy evaluation, context selection and tool mediation belong to
`smista-router`; the CLI only expresses preferences and renders results.

## Layout

This is a Cargo workspace. All crates live under `crates/`:

| Crate                  | Kind | Responsibility                                         | License     |
| ---------------------- | ---- | ------------------------------------------------------ | ----------- |
| `smista-core`          | lib  | Shared domain types, config, policy, errors.           | MIT         |
| `smista-storage`       | lib  | Storage traits, entities and SurrealDB implementation. | Elastic-2.0 |
| `smista-providers`     | lib  | Model abstraction and provider adapters (via `rig`).   | Elastic-2.0 |
| `smista-router`        | bin  | Routing/orchestration service.                         | Elastic-2.0 |
| `smista-router-client` | lib  | Async Rust client for the router HTTP API.             | MIT         |
| `smista-sdk`           | lib  | Rust SDK facade re-exporting core types (+ client).    | MIT         |
| `smista-cli`           | bin  | The `smista` CLI (`ratatui` + `clap`).                 | Elastic-2.0 |

The TypeScript SDK lives in `sdk/` (`@smista-ai/sdk`). Documentation (mdBook)
lives in `docs/`.

## Conventions

- **Commits**: [Conventional Commits](https://conventionalcommits.org). Never
  add a `Co-Authored-By` line.
- **Branches**: `feat/{issue_number}-{issue_name}` (e.g. `feat/1-scaffolding`).
- **Licensing**: the project is dual-licensed per crate. Consumer-facing
  libraries (`smista-core`, `smista-router-client`, `smista-sdk`, the TS
  `@smista-ai/sdk`) are MIT; everything else is Elastic-2.0. There is **no**
  workspace-level `license` default — every crate MUST set its own `license`
  field explicitly. When creating a new crate, ALWAYS ask whether it should be
  `MIT` or `Elastic-2.0` before scaffolding it, and reflect the choice in the
  crate `Cargo.toml`, its `README.md` and both license tables (this file's
  Layout table and the root `README.md` Components table). MIT crates must never
  depend on Elastic-2.0 crates.
- **Rust**: follow the project rustfmt config; `clippy` must pass with
  `-D warnings`. Use `module_name.rs`, not `mod.rs`.
- **TypeScript**: Biome for lint + format; strict `tsconfig`. No `any`.
- **TS bindings**: the types under `sdk/src/bindings/` are generated from the
  Rust `ts-rs` types — never edit them by hand. After changing any
  `#[ts(export)]` type, regenerate them with `just build_sdk`.
- **Markdown**: keep tables aligned; end files with a single newline.
- **Docs**: any change to the codebase that affects user-facing behaviour
  (config, CLI commands, API, providers) must come with a user-friendly update
  to [docs/](docs/). Write for the user performing a task, not as a spec dump:
  task-oriented section names, runnable examples, no internal jargon. Whenever a
  new page is added under [docs/](docs/), register it in `docs/SUMMARY.md`.
  Whenever a page under [docs/](docs/) is updated, also update its table of
  contents to match the new heading structure; this does not apply to
  `docs/SUMMARY.md`, which has no table of contents.
- **Database schema**: any change to the database schema (tables, fields or
  indexes) must be reflected in both the migration at
  `crates/smista-storage/src/database/surreal/schema.rs` and the authoritative
  reference at `docs/technical/schema.md`; keep the two in sync.
- **GitHub Actions**: must pass `zizmor`; pin actions by commit SHA and set
  `persist-credentials: false` on checkout.
- **Tasks**: drive builds, checks, tests and publishing through the `just`
  recipes below — do not invoke the underlying `cargo`/`npm` commands directly.
  If a task has no recipe, add one rather than running it ad hoc.

## Commands

Tasks are driven by [`just`](https://just.systems); run `just` to list recipes.

```sh
# Build
just build_all          # crates + SDK
just build_crates       # cargo build --workspace

# Checks (what CI runs)
just check_code         # nightly fmt --check, clippy -D warnings, SDK lint + typecheck
just test_all           # cargo test --workspace + SDK vitest

# SDK (all npm scripts are exposed as `sdk_*` recipes)
just sdk_check          # biome ci
just sdk_test           # vitest

# Publish (crates in dependency order with retry, then SDK)
just publish_all

# Docs
mdbook build docs

# Workflows security
zizmor .github/workflows
```

## Secrets

Never log, trace, print or persist API keys, provider credentials or auth
tokens. Use the `secrecy` crate for sensitive values and redact before
persistence.
