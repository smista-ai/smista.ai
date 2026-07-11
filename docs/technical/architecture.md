# Architecture

- [Architecture](#architecture)
  - [Components](#components)
  - [Principles](#principles)
  - [How the pieces talk](#how-the-pieces-talk)
  - [Sign-in and session start](#sign-in-and-session-start)
  - [Executing a task](#executing-a-task)
  - [Tool calls and approvals](#tool-calls-and-approvals)
  - [Route preview](#route-preview)

smista.ai is composed of a small set of clearly separated components. The goal
is to keep the CLI experience simple while providing an easy-to-run router as
its backend.

## Components

| Component              | Kind           | Responsibility                                                                                         |
| ---------------------- | -------------- | ------------------------------------------------------------------------------------------------------ |
| `smista-cli`           | CLI binary     | User interaction, command parsing, rendering, approvals, runs and talks to router.                     |
| `smista-router`        | Library        | Auth, sessions, classification, routing, context, tool mediation, traces; embedded and run by the CLI. |
| `smista-router-client` | Library        | Async Rust client for the router HTTP API; used by the CLI and Rust frontends.                         |
| `smista-core`          | Library        | Shared domain types, config, policy, trace structures, validation, errors.                             |
| `smista-providers`     | Library        | Model abstraction and provider adapters (OpenAI, Anthropic, Ollama, …).                                |
| `smista-storage`       | Library        | Storage traits, entities (incl. trace events) and the SurrealDB layer.                                 |
| `@smista-ai/sdk`       | TypeScript SDK | Typed client over the router HTTP API.                                                                 |

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

## How the pieces talk

```mermaid
graph LR
    User([Developer])
    CLI[smista-cli]
    Router[smista-router]
    Storage[(SurrealDB)]
    Providers[Providers<br/>OpenAI · Anthropic · Ollama]

    User -->|prompt, approvals| CLI
    CLI -->|HTTP JSON API| Router
    Router -->|sessions, traces| Storage
    Router -->|model calls| Providers
```

The CLI never decides which model runs a task — it only expresses preferences
and renders results. Every routing decision belongs to the router.

## Sign-in and session start

```mermaid
sequenceDiagram
    actor U as Developer
    participant C as smista-cli
    participant R as smista-router
    participant DB as SurrealDB

    U->>C: smista "..."
    C->>R: POST /auth/sign-in (X-Smista-Api-Key)
    R->>DB: look up user by api_key_hash
    R-->>C: session token (short-lived)
    C->>R: POST /sessions (Bearer token)
    R->>DB: create session
    R-->>C: session id
```

## Executing a task

This is the golden path: classify, route, select context, call the model, return
a result and a trace.

```mermaid
sequenceDiagram
    actor U as Developer
    participant C as smista-cli
    participant R as smista-router
    participant P as Provider
    participant DB as SurrealDB

    U->>C: prompt
    C->>R: POST /sessions/{id}/execute
    Note over R: Deterministic — no LLM
    R->>R: classify task (intent)
    R->>R: evaluate policy → select provider/model
    R->>R: select minimum context, exclude restricted files
    R->>P: invoke model (provider adapter)
    P-->>R: response or tool-call request
    R->>DB: record routing decision + trace
    R-->>C: result + routing explanation + trace_id
    C-->>U: render response, cost, matched rule
```

## Tool calls and approvals

Models never touch the filesystem, shell or network directly. The router
mediates every tool call against the active permissions.

```mermaid
sequenceDiagram
    participant P as Provider
    participant R as smista-router
    participant C as smista-cli
    actor U as Developer

    P-->>R: tool-call request
    R->>R: validate against permissions
    alt mode = deny
        R-->>P: blocked (reported in trace)
    else mode = ask
        R-->>C: approval_required
        C->>U: show diff / command
        U-->>C: approve or reject
        C->>R: POST /approvals/{id}
    end
    R->>R: execute allowed tool via tool runtime
    R->>P: tool result
    P-->>R: continues until final response
```

## Route preview

`/preview` explains the decision without calling any model.

```mermaid
sequenceDiagram
    actor U as Developer
    participant C as smista-cli
    participant R as smista-router

    U->>C: /preview "review this PR"
    C->>R: POST /sessions/{id}/preview
    R->>R: classify + evaluate policy + select context
    Note over R: Model is never called
    R-->>C: task, model, matched rule,<br/>context, estimated cost, permissions
    C-->>U: render preview
```
