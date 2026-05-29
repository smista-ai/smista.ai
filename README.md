# smista.ai

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista-sdk.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/veeso/smista.ai?style=flat)](https://github.com/veeso/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-sdk.svg?logo=rust)](https://crates.io/crates/smista-sdk)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/veeso/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/veeso/smista.ai/actions)
[![docs](https://github.com/veeso/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

A local-first agent and CLI that routes each phase of an AI workflow to the most
suitable model using deterministic, configurable policies.

## Overview

In 2026 most developers no longer use a single model or a single provider.
Switching between CLIs, web apps and providers — copying context around and
remembering which model to use for each task — is slow and error-prone.

smista.ai keeps **one coherent workflow** while letting different models handle
different phases. Its main differentiator is **deterministic multi-model
routing**: routing never depends on an LLM's judgment, and every decision is
explainable through a trace.

It is not a clone of Claude Code or Codex. It provides the core primitives of a
modern coding/text agent — prompt templates, plan mode, skills, tool
permissions, context management, diff and review, traceability — on top of
deterministic routing.

## Components

| Component                                                       | Description                                                       |
| --------------------------------------------------------------- | ----------------------------------------------------------------- |
| [`smista-cli`](crates/smista-cli/README.md)                     | The `smista` command-line interface for developers.               |
| [`smista-core`](crates/smista-core/README.md)                   | Shared domain types, config, routing policy and validation.       |
| [`smista-providers`](crates/smista-providers/README.md)         | Model abstraction and provider adapters (OpenAI, Anthropic, …).   |
| [`smista-router`](crates/smista-router/README.md)               | Routing and orchestration service exposing a local HTTP JSON API. |
| [`smista-router-client`](crates/smista-router-client/README.md) | Async Rust client for the router HTTP JSON API.                   |
| [`smista-sdk`](crates/smista-sdk/README.md)                     | Rust SDK facade re-exporting the domain types (and the client).   |
| [`smista-storage`](crates/smista-storage/README.md)             | Storage traits and the SurrealDB-backed persistence layer.        |
| [`smista-trace`](crates/smista-trace/README.md)                 | Execution trace types and recording logic.                        |
| [`smista-web`](crates/smista-web/README.md)                     | `axum` HTTP JSON API server for the router.                       |
| [`@smista-ai/sdk`](sdk/README.md)                               | TypeScript SDK for building clients on top of the router.         |

## Golden workflow

```sh
smista "refactor the auth middleware"
```

smista shows the detected task, the selected model, the matched routing rule,
the included and excluded context, the estimated cost and the required
permissions. Before any write, it presents a diff and asks for confirmation.

Preview a route without executing it:

```sh
smista route "review this PR"
```

Inspect the full routing decision afterwards:

```sh
smista trace
```

## Status

smista.ai is under active development. Work is tracked through milestones and
issues; see the [issues](https://github.com/veeso/smista.ai/issues).

## Documentation

Read the documentation at <https://docs.smista.ai>.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and
the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file
for details.
