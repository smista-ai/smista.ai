# Architecture

smista.ai is composed of a small set of clearly separated components. The goal
is to keep the CLI experience simple while providing an easy-to-run router as
its backend.

## Components

| Component          | Kind           | Responsibility                                                             |
| ------------------ | -------------- | -------------------------------------------------------------------------- |
| `smista-cli`       | CLI binary     | User interaction, command parsing, rendering, approvals, talks to router.  |
| `smista-router`    | Service binary | Auth, sessions, classification, routing, context, tool mediation, traces.  |
| `smista-core`      | Library        | Shared domain types, config, policy, trace structures, validation, errors. |
| `smista-providers` | Library        | Model abstraction and provider adapters (OpenAI, Anthropic, Ollama, …).    |
| `smista-storage`   | Library        | Storage traits and the SurrealDB-backed persistence layer.                 |
| `smista-trace`     | Library        | Execution trace types and append-only recording logic.                     |
| `smista-web`       | Library        | `axum` HTTP JSON API server for the router.                                |
| `@smista-ai/sdk`   | TypeScript SDK | Typed client over the router HTTP API.                                     |

## Principles

- **Local-first by default** — the CLI and router run on localhost; config,
  policies, skills, prompts and traces live locally and can be versioned.
- **Deterministic over magical** — routing never relies on an LLM. Explicit
  rules and policies guide model selection.
- **Traceability** — every routing decision is explainable via a trace.
- **Least-context routing** — each model receives only the minimum context
  required for its task; full session context is never forwarded by default.
- **User control before automation** — file writes, shell commands, network
  access and sensitive context disclosure require approval.

## Request flow

1. The CLI authenticates with the router and creates or resumes a session.
2. The router classifies the task deterministically.
3. The router evaluates the routing policy and selects a provider/model.
4. The router selects the minimum required context, excluding restricted files.
5. The provider adapter invokes the model.
6. Tool calls are mediated by the router and validated against permissions.
7. The router records the full trace.

See the [Specification](../reference/specification.md) for the complete design.
