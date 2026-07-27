# smista.ai

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/badge/SDK%20%26%20core-MIT-blue.svg)](LICENSE-MIT)
[![license-elastic](https://img.shields.io/badge/CLI%2C%20router%20%26%20more-Elastic--2.0-005571.svg)](LICENSE-ELv2)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-sdk.svg?logo=rust)](https://crates.io/crates/smista-sdk)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![coverage](https://coveralls.io/repos/github/smista-ai/smista.ai/badge.svg?branch=main)](https://coveralls.io/github/smista-ai/smista.ai?branch=main)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

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

## Install

Install the latest release with the install script (macOS and Linux):

```sh
curl -sSLf https://smista.ai/install/install.sh | sh
```

On Windows (PowerShell):

```powershell
irm https://smista.ai/install/install.ps1 | iex
```

With [Homebrew](https://brew.sh):

```sh
brew install smista-ai/smista/smista
```

With [Cargo](https://doc.rust-lang.org/cargo/):

```sh
cargo install smista --locked
```

The install scripts pick the right binary for your platform, verify its
checksum and use Homebrew when it is available. To install a specific
version, pass `--version=X.Y.Z` (`-Version X.Y.Z` on Windows).

## Components

| Component                                                       | Description                                                                                    | License     |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | ----------- |
| [`smista-mock-web-server`](crates/mock-web-server/README.md)    | Unpublished test helper for mocking the router HTTP API.                                       | MIT         |
| [`smista-cli`](crates/smista-cli/README.md)                     | The `smista` command-line interface for developers.                                            | Elastic-2.0 |
| [`smista-core`](crates/smista-core/README.md)                   | Shared domain types, config, routing policy and validation.                                    | MIT         |
| [`smista-providers`](crates/smista-providers/README.md)         | Model abstraction and provider adapters (OpenAI, Anthropic, Gemini).                           | Elastic-2.0 |
| [`smista-router`](crates/smista-router/README.md)               | Routing and orchestration service exposing a local HTTP JSON API; embedded and run by the CLI. | Elastic-2.0 |
| [`smista-router-client`](crates/smista-router-client/README.md) | Async Rust client for the router HTTP JSON API.                                                | MIT         |
| [`smista-sdk`](crates/smista-sdk/README.md)                     | Rust SDK facade re-exporting the domain types (and the client).                                | MIT         |
| [`smista-storage`](crates/smista-storage/README.md)             | Storage traits, entities and the SurrealDB-backed persistence layer.                           | Elastic-2.0 |
| [`@smista-ai/sdk`](sdk/README.md)                               | TypeScript SDK for building clients on top of the router.                                      | MIT         |

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
issues; see the [issues](https://github.com/smista-ai/smista.ai/issues).

## Documentation

Read the documentation at <https://docs.smista.ai>.

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and
the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

smista.ai is **source available**, dual-licensed per component:

- The consumer-facing libraries — `smista-core`, `smista-router-client`,
  `smista-sdk` and the TypeScript `@smista-ai/sdk` — are licensed under the
  [MIT License](LICENSE-MIT). Build on them freely.
- Everything else — the `smista` CLI, the router, its web API, providers and
  storage — is licensed under the [Elastic License 2.0](LICENSE-ELv2) (ELv2).
  You may use, self-host, modify and embed it freely; you may not offer it to
  third parties as a hosted or managed service.

The per-component license is listed in the [Components](#components) table
above. ELv2 is not an OSI-approved open-source license, so the project is
described as "source available" rather than "open source".
