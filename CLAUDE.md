# CLAUDE.md

Guidance for Claude Code (and other agents) working in this repository.

## Project

smista.ai is a local-first agent and CLI that deterministically routes each
phase of an AI workflow to the most suitable model. The full design lives in
[docs/reference/specification.md](docs/reference/specification.md). Read it
before making non-trivial changes.

Core invariant: **routing is deterministic and never depends on an LLM.**
Routing, policy evaluation, context selection and tool mediation belong to
`smista-router`; the CLI only expresses preferences and renders results.

## Layout

This is a Cargo workspace. All crates live under `crates/`:

| Crate              | Kind | Responsibility                                       |
| ------------------ | ---- | ---------------------------------------------------- |
| `smista-core`      | lib  | Shared domain types, config, policy, errors.         |
| `smista-storage`   | lib  | Storage traits and SurrealDB implementation.         |
| `smista-providers` | lib  | Model abstraction and provider adapters (via `rig`). |
| `smista-trace`     | lib  | Execution trace types and recording.                 |
| `smista-router`    | bin  | Routing/orchestration service.                       |
| `smista-web`       | lib  | `axum` HTTP JSON API for the router.                 |
| `smista-cli`       | bin  | The `smista` CLI (`ratatui` + `clap`).               |

The TypeScript SDK lives in `sdk/` (`@smista-ai/sdk`). Documentation (mdBook)
lives in `docs/`.

## Conventions

- **Commits**: [Conventional Commits](https://conventionalcommits.org). Never
  add a `Co-Authored-By` line.
- **Branches**: `feat/{issue_number}-{issue_name}` (e.g. `feat/1-scaffolding`).
- **Rust**: follow the project rustfmt config; `clippy` must pass with
  `-D warnings`. Use `module_name.rs`, not `mod.rs`.
- **TypeScript**: Biome for lint + format; strict `tsconfig`. No `any`.
- **Markdown**: keep tables aligned; end files with a single newline.
- **GitHub Actions**: must pass `zizmor`; pin actions by commit SHA and set
  `persist-credentials: false` on checkout.

## Commands

```sh
# Rust
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# SDK
cd sdk && npm install && npm run build && npm run check

# Docs
mdbook build docs

# Workflows security
zizmor .github/workflows
```

## Secrets

Never log, trace, print or persist API keys, provider credentials or auth
tokens. Use the `secrecy` crate for sensitive values and redact before
persistence.
