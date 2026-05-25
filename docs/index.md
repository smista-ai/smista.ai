# smista.ai

![logo](logo-150.png)

[![license-mit](https://img.shields.io/github/license/veeso/smista.ai)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/veeso/smista.ai?style=flat)](https://github.com/veeso/smista.ai/stargazers)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)
[![ci](https://github.com/veeso/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/veeso/smista.ai/actions)

---

## What is smista.ai?

smista.ai is a local-first agent and CLI that routes each phase of an AI
workflow to the most suitable model using deterministic, configurable policies.

Most developers no longer rely on a single model or a single provider. Switching
between CLIs, web apps and providers — copying context around and remembering
which model to use for each task — is slow and error-prone. smista.ai keeps one
coherent workflow while letting different models handle different phases.

Its main differentiator is **deterministic multi-model routing**: routing never
depends on an LLM's judgment, and every decision is explainable.

## Core idea

You configure how work should be routed, for example:

- Planning to the strongest reasoning model
- Simple edits to a local model
- Code review to a reliable coding model
- Sensitive files to a local-only model

smista.ai applies these rules consistently and explains every routing decision
through a trace.

## Components

- **smista-cli** — the `smista` command-line interface for developers.
- **smista-router** — the routing and orchestration service exposing a local
  HTTP JSON API.
- **smista-core** — the shared internal runtime and domain types.
- **@smista-ai/sdk** — a TypeScript SDK for building clients on top of the
  router.

## Where to next

- Read the [Get Started](guides/get-started.md) guide.
- Understand the [Architecture](technical/architecture.md).
- Dive into the full [Specification](reference/specification.md).
