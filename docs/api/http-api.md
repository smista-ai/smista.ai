# HTTP API

- [HTTP API](#http-api)
  - [Conventions](#conventions)
  - [Authentication](#authentication)
    - [Auth endpoints](#auth-endpoints)
  - [Sessions](#sessions)
  - [Executing a task](#executing-a-task)
    - [Policy](#policy)
    - [Streaming](#streaming)
  - [Previewing a route](#previewing-a-route)
  - [Approvals](#approvals)
  - [Traces](#traces)
  - [Providers, models and usage](#providers-models-and-usage)
  - [Errors](#errors)
    - [Status codes](#status-codes)
    - [Error codes](#error-codes)


`smista-router` exposes a JSON REST API. The CLI uses it, and so can your own
tools, scripts, editors or web clients. The API authenticates users, manages
sessions, previews routes, executes tasks, and reports traces and usage. Routing
logic stays in the router — clients never reimplement it.

> [!TIP]
> For TypeScript and JavaScript, use the [`@smista-ai/sdk`](https://www.npmjs.com/package/@smista-ai/sdk)
> typed client instead of calling these endpoints by hand.

## Conventions

- All endpoints live under `/api/v1` — e.g. `/api/v1/auth/sign-in`.
- `GET` reads, `POST` creates or executes, `PUT` replaces, `DELETE` removes.
- Request and response bodies are JSON.
- Paths are resource-oriented: `/auth`, `/sessions`, `/sessions/{id}/...`,
  `/llm`.

## Authentication

smista.ai separates **router authentication** (who you are) from **provider
credentials** (keys for OpenAI, Anthropic, Gemini, Ollama, …). They travel in
different headers and are never mixed.

| Header                                        | Used for                                    |
| --------------------------------------------- | ------------------------------------------- |
| `Authorization: Bearer <session-token>`       | Authenticated requests after sign-in.       |
| `X-Smista-Api-Key: <api-key>`                 | Auth endpoints only, to obtain a token.     |
| `X-Smista-Provider-<Provider>-Api-Key: <key>` | Provider credential for a specific request. |

For example: `X-Smista-Provider-Anthropic-Api-Key: <key>`. Provider credentials
are sent only when the selected model needs them, used for that one request, and
never logged, traced or forwarded to the model. Credentials are never accepted
in query parameters.

The flow: `POST /auth/bootstrap` returns a user ID and a long-lived API key
(shown once). `POST /auth/sign-in` exchanges that key for a short-lived session
token, which you send as a bearer token on every other request.

### Auth endpoints

```http
POST /api/v1/auth/bootstrap
```

Creates a user. Returns `{ "user_id": "...", "api_key": "sk-smista-api01-..." }`.

```http
POST /api/v1/auth/sign-in
X-Smista-Api-Key: <api-key>

{ "user_id": "user:abc123" }
```

Returns `{ "token": "st_...", "expires_at": "2026-05-25T12:00:00Z" }`.

```http
POST /api/v1/auth/sign-out
Authorization: Bearer <session-token>
```

Revokes the current token. Returns `{ "revoked": true }`.

```http
GET /api/v1/auth/me
Authorization: Bearer <session-token>
```

Returns the authenticated user's sessions:

```json
{
  "sessions": [
    {
      "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
      "title": "Refactor auth middleware",
      "created_at": "2026-05-25T09:00:00Z",
      "updated_at": "2026-05-25T09:30:00Z",
      "archived": false
    }
  ]
}
```

## Sessions

```http
POST   /api/v1/sessions                  # create (title required)
GET    /api/v1/sessions/{session_id}     # fetch / resume
PUT    /api/v1/sessions/{session_id}     # update title or archive
DELETE /api/v1/sessions/{session_id}     # delete
```

All session routes require `Authorization: Bearer <session-token>`. A user can
only access their own sessions; others return `403`.

```json
// POST /api/v1/sessions
{ "title": "Refactor auth middleware" }
```

Creating a session returns the new session summary:

```json
{
  "session": {
    "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
    "title": "Refactor auth middleware",
    "created_at": "2026-05-25T09:00:00Z",
    "updated_at": "2026-05-25T09:00:00Z",
    "archived": false
  }
}
```

Fetching a session returns the full detail, including its messages and
free-form metadata (archived sessions are not returned):

```json
{
  "session": {
    "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
    "title": "Refactor auth middleware",
    "created_at": "2026-05-25T09:00:00Z",
    "updated_at": "2026-05-25T09:30:00Z",
    "messages": [
      { "role": "user", "content": "Refactor the auth middleware." },
      { "role": "assistant", "content": "Here is the plan..." }
    ],
    "metadata": {}
  }
}
```

Updating a session takes a partial body; omit a field to leave it unchanged:

```json
// PUT /api/v1/sessions/{session_id}
{ "title": "Refactor auth and sessions", "archived": false }
```

Deleting a session returns `{ "deleted": true }`.

## Executing a task

```http
POST /api/v1/sessions/{session_id}/execute
Authorization: Bearer <session-token>
X-Smista-Provider-{provider}-Api-Key: <api-key>
```

The body carries everything the router needs to make a deterministic decision:
the user input, a workspace snapshot, the merged policy, local preferences, the
available providers with their credential status, and the assembled context. The
`policy` block is the same routing, tool-permission and privacy vocabulary the
CLI loads from `config.toml` — sent verbatim, not a separate, lossy shape:

```json
{
  "input": {
    "text": "refactor the auth middleware",
    "command": "edit",
    "explicit_model": null
  },
  "workspace": {
    "root": "/Users/christian/project",
    "git_branch": "main",
    "git_diff": "...",
    "referenced_paths": ["src/auth/middleware.rs"],
    "active_file": null
  },
  "policy": {
    "version": 1,
    "source": "merged",
    "routing": {
      "default": {
        "model": "anthropic/claude-sonnet",
        "fallbacks": ["openai/gpt-5.5-thinking", "ollama/qwen2.5-coder"]
      },
      "rules": [
        {
          "name": "auth edits use Claude",
          "priority": 30,
          "effort": "high",
          "intent": "edit",
          "paths": ["src/auth/**"],
          "local_only": false,
          "model": "anthropic/claude-sonnet",
          "fallbacks": ["openai/gpt-5.5-thinking"],
          "required_permissions": { "permissions": { "file_write": "ask" } },
          "cost_limit": "0.50"
        }
      ]
    },
    "tools": {
      "permissions": { "file_read": "allow", "file_write": "ask", "shell": "ask", "network": "deny" }
    },
    "privacy": {
      "restricted_paths": [".env", "secrets/**", "target/**"],
      "remote": { "mode": "ask", "blocked_paths": [] },
      "local": { "mode": "allow" }
    }
  },
  "local_preferences": { "auto_apply": false, "stream": true, "local_only": false, "no_network": false },
  "providers": [
    { "id": "anthropic", "models": [{ "model": "claude-sonnet", "requires_api_key": true, "credential_available": true }] },
    { "id": "ollama", "models": [{ "model": "qwen2.5-coder", "requires_api_key": false, "credential_available": true }] }
  ],
  "context": {
    "messages": [{ "role": "user", "content": "Previous relevant message..." }],
    "files": [{ "path": "src/auth/middleware.rs", "content": "...", "content_hash": "sha256:..." }],
    "instructions": [{ "source": "SMISTA.md", "content": "..." }],
    "skills": [],
    "prompt_template": null
  }
}
```

The top-level fields are:

| Field               | Purpose                                                                         |
| ------------------- | ------------------------------------------------------------------------------- |
| `input`             | The prompt `text`, an optional `command` and an optional `explicit_model`.      |
| `workspace`         | Repository snapshot: `root`, `git_branch`, `git_diff`, referenced/active files. |
| `policy`            | The deterministic `routing`, `tools` and `privacy` policy (see below).          |
| `local_preferences` | Resolved client toggles: `auto_apply`, `stream`, `local_only`, `no_network`.    |
| `providers`         | Providers offered for this request and per-model credential status.             |
| `context`           | Assembled context: prior `messages`, `files`, `instructions` and `skills`.      |

`input.command` forces a task type (`edit`, `review`, …) and `input.explicit_model`
forces a `provider/model`, bypassing routing entirely; both may be `null`.

### Policy

`policy.version` is the snapshot schema version and `policy.source` records how
it was assembled (e.g. `merged`). The three sub-blocks mirror the CLI's
`[routing]`, `[tools]` and `[privacy]` config sections exactly.

`routing` holds ordered `rules` plus an optional `default` route (`model` and
ordered `fallbacks`) used when no rule matches. Each rule:

| Field                   | Type            | Purpose                                                                 |
| ----------------------- | --------------- | ----------------------------------------------------------------------- |
| `name`                  | string          | Human-readable rule name.                                               |
| `priority`              | integer         | Evaluation order, ascending; first match wins. Defaults to `1000`.      |
| `effort`                | string          | Reasoning effort for the matched model (`low`/`medium`/`high`/`xhigh`). |
| `intent`                | task type, null | Required task intent, if scoped.                                        |
| `skill`                 | string, null    | Required invoked skill, if scoped.                                      |
| `paths`                 | list of strings | Path globs; a relevant path must match one when non-empty.              |
| `local_only`            | bool            | Restrict the fallback chain to local models.                            |
| `requires_capabilities` | object          | Capability gate the matched model must satisfy; omitted if none.        |
| `model`                 | reference       | Model selected when the rule matches.                                   |
| `fallbacks`             | list of refs    | Models tried, in order, when `model` is unavailable.                    |
| `required_permissions`  | object          | Tool permissions the matched route requires.                            |
| `cost_limit`            | string, omitted | Per-task cost ceiling as a decimal string; omitted if unset.            |

`tools.permissions` is a flat map of tool name to mode (`allow`, `ask` or
`deny`). `privacy` carries `restricted_paths` globs plus a `remote` and `local`
sub-policy, each with an optional `mode` (`remote` defaults to `ask`, `local` to
`allow`) and the `remote` block adds `blocked_paths` never sent to remote models.

Each entry in `providers` carries the provider `id` and its `models`, where each
model reports `requires_api_key` and whether a `credential_available` for it was
supplied. The credentials themselves never appear in the body — they travel as
`X-Smista-Provider-<Provider>-Api-Key` headers.

The router classifies the task, applies the policy, selects a model, builds the
request and returns the result with a routing explanation:

```json
{
  "status": "completed",
  "message": { "role": "assistant", "content": "..." },
  "routing": {
    "task_type": "edit",
    "provider": "anthropic",
    "model": "claude-sonnet",
    "matched_rule": "edit + src/auth/** -> anthropic/claude-sonnet",
    "fallback_used": false,
    "override_used": false
  },
  "context": {
    "included": ["src/auth/middleware.rs", "SMISTA.md", "current git diff"],
    "excluded": [".env", "secrets/**"]
  },
  "usage": {
    "input_tokens": 1200,
    "output_tokens": 500,
    "estimated_cost": "0.08",
    "currency": "USD"
  },
  "trace_id": "trace:xyz"
}
```

If the model requests a tool call that needs approval, the response returns a
pending approval instead of a final message.

### Streaming

```http
POST /api/v1/sessions/{session_id}/stream
```

Same body as `/execute`. The response is a stream (Server-Sent Events) of
structured events:

```json
{ "type": "text_delta", "delta": "The first step is..." }
```

Event types: `text_delta`, `reasoning_delta`, `tool_call_started`,
`tool_call_requested`, `approval_required`, `tool_result`, `usage`, `error`,
`done`.

Models that expose their reasoning stream it as `reasoning_delta` chunks.
When the model starts calling a tool, a `tool_call_started` event announces
the call's name as soon as it is known; the matching `tool_call_requested`
event follows once the arguments are complete, correlated by `call_id`.

The `usage` event reports token counts and, when the model declares prices,
the actual cost of the invocation. Local models report a zero cost. Models
that cannot stream still answer on this endpoint: the full response is
replayed as a short stream of the same events.

## Previewing a route

```http
POST /api/v1/sessions/{session_id}/preview
```

Same body as `/execute`, but the selected model is **never called**. Returns the
task type, chosen provider/model, matched rule, included/excluded context, an
estimated cost range and the required permissions:

```json
{
  "task_type": "review",
  "provider": "openai",
  "model": "gpt-5.5-thinking",
  "matched_rule": "task.review -> openai/gpt-5.5-thinking",
  "included_context": ["current git diff", "SMISTA.md"],
  "excluded_context": [".env", "target/**"],
  "estimated_cost": { "min": "0.03", "max": "0.09", "currency": "USD" },
  "required_permissions": [
    { "permission": "read_repository", "mode": "allow" },
    { "permission": "write_files", "mode": "ask" }
  ]
}
```

## Approvals

When a model-requested action needs confirmation:

```http
POST /api/v1/sessions/{session_id}/approvals/{approval_id}

{ "decision": "approved", "reason": null }
```

`decision` is `approved` or `rejected`.

## Traces

```http
GET /api/v1/sessions/{session_id}/traces/latest
GET /api/v1/sessions/{session_id}/traces/{trace_id}
```

Returns the execution trace for the session: the ordered events that were
emitted while routing and running its tasks. The trace is wrapped under a
`trace` key:

```json
{
  "trace": {
    "session_id": "0194f1e2-...",
    "events": [
      {
        "event_type": "routing_decision",
        "task_type": "review",
        "provider": "openai",
        "model": "gpt-5.5-thinking",
        "matched_rule": "task.review -> openai/gpt-5.5-thinking",
        "created_at": "2026-06-04T10:15:00Z",
        "payload": { "provider": "openai", "model": "gpt-5.5-thinking", "reason": "best for review" }
      },
      {
        "event_type": "tool_call",
        "task_type": "review",
        "provider": "openai",
        "model": "gpt-5.5-thinking",
        "created_at": "2026-06-04T10:15:02Z",
        "payload": { "tool_name": "read_file", "status": "completed" }
      }
    ]
  }
}
```

`events` is ordered oldest first. Each event carries its own routing context
(`task_type`, `provider`, `model`, optional `matched_rule`) and a `payload`
whose shape depends on `event_type`. `event_type` is one of `message`,
`routing_decision`, `context_selection`, `tool_call`, `approval` or `cost`; the
per-type `payload` shapes are listed under `trace_event_content` in the
[storage schema reference](../technical/schema.md).

## Providers, models and usage

```http
GET /api/v1/llm/providers              # available providers
GET /api/v1/llm/models                 # available models with capabilities
GET /api/v1/sessions/{session_id}/usage  # token and cost breakdown
```

`GET /llm/providers` lists the providers that are currently **available** —
those configured with usable credentials (or a base URL, for local providers).
A provider that is not available is omitted entirely, so every entry returned is
ready to route to. Each entry carries a `local` flag: `true` when the provider
serves its models on your own host or network with no request leaving the
machine (a local Ollama, a self-hosted OpenAI-compatible endpoint), and `false`
for a cloud API. This is the same locality every one of that provider's models
reports, so the two can never disagree:

```json
{
  "providers": [
    { "id": "anthropic", "display_name": "Anthropic", "local": false },
    { "id": "ollama", "display_name": "Ollama", "local": true }
  ]
}
```

`GET /llm/models` lists the available models, returning each one as a full model
descriptor. Like `/execute`, it accepts `X-Smista-Provider-<Provider>-Api-Key`
headers and needs them: the router queries each provider's `list_models`, and
remote providers such as Anthropic and Gemini reject that call without an API
key. Providers whose credentials are missing are omitted from the result.
`capabilities` is a nested object of boolean flags — `streaming`,
`tools`, `json_output`, `system_prompt`, `images`, `reasoning` and `memory` —
where an absent or `false` flag means the capability is not supported. `auth`
records how the model authenticates (`none`, `api_key`, `optional_api_key` or a
`{ "custom": "<scheme>" }` object); `display_name`, `max_output_tokens` and the
cost fields are present only when known, and the cost fields are decimal
strings:

```json
{
  "models": [
    {
      "provider": "anthropic",
      "model": "claude-sonnet",
      "display_name": "Claude Sonnet",
      "local": false,
      "auth": "api_key",
      "capabilities": { "streaming": true, "tools": true, "json_output": true },
      "max_context_tokens": 200000,
      "max_output_tokens": 8192,
      "input_cost_per_million_tokens": "3",
      "output_cost_per_million_tokens": "15",
      "default_parameters": {}
    },
    {
      "provider": "ollama",
      "model": "qwen2.5-coder",
      "display_name": null,
      "local": true,
      "auth": "none",
      "capabilities": { "streaming": true },
      "max_context_tokens": 32768,
      "max_output_tokens": null,
      "default_parameters": {}
    }
  ]
}
```

`GET /sessions/{session_id}/usage` reports the session total plus per-model and
per-task-type breakdowns. Cost fields are decimal strings; tokens absent from a
provider's report are omitted:

```json
{
  "session_id": "00000000-0000-0000-0000-000000000000",
  "usage": {
    "total": {
      "input_tokens": 12000,
      "output_tokens": 4200,
      "total_tokens": 16200,
      "estimated_cost": "0.42",
      "currency": "USD"
    },
    "by_model": [
      {
        "provider": "openai",
        "model": "gpt-5.5-thinking",
        "input_tokens": 8000,
        "output_tokens": 2200,
        "total_tokens": 10200,
        "estimated_cost": "0.31",
        "currency": "USD",
        "request_count": 3
      }
    ],
    "by_task_type": [
      {
        "task_type": "plan",
        "input_tokens": 4000,
        "output_tokens": 1200,
        "estimated_cost": "0.18",
        "request_count": 1
      }
    ]
  }
}
```

## Errors

Errors use a consistent JSON shape and never expose secrets:

```json
{
  "error": {
    "code": "missing_provider_credentials",
    "message": "The selected model requires provider credentials, but none were provided.",
    "details": { "provider": "anthropic", "model": "claude-sonnet" }
  }
}
```

### Status codes

| Code | Meaning                                          |
| ---- | ------------------------------------------------ |
| 200  | Successful read or completed command             |
| 201  | Resource created                                 |
| 202  | Accepted — long-running or pending operation     |
| 204  | Deleted, no body                                 |
| 400  | Invalid request payload                          |
| 401  | Missing or invalid authentication                |
| 403  | Authenticated but blocked by ownership or policy |
| 404  | Resource not found                               |
| 409  | Conflicting resource state                       |
| 422  | Valid JSON that fails domain validation          |
| 429  | Rate limited                                     |
| 500  | Unexpected server error                          |
| 502  | Provider error                                   |
| 503  | Provider or storage unavailable                  |
| 504  | Provider timeout                                 |

### Error codes

The `code` field is the stable identifier clients should match on. The
`message` is human-readable and may change; the HTTP status is provided
alongside for convenience.

| Code                              | Status | Meaning                                                                                                                                                |
| --------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `context_length_exceeded`         | 422    | Request exceeds the provider model's context window.                                                                                                   |
| `context_window_exceeded`         | 422    | Routing rejected a model whose context window cannot fit the input.                                                                                    |
| `fallback_exhausted`              | 503    | Primary route failed and every configured fallback also failed.                                                                                        |
| `forbidden`                       | 403    | Caller is authenticated but not the resource owner.                                                                                                    |
| `internal_error`                  | 500    | Unexpected server-side failure. Details intentionally omitted.                                                                                         |
| `invalid_model_reference`         | 422    | A model reference was not in the expected `provider/model` form.                                                                                       |
| `invalid_provider_configuration`  | 500    | A provider was configured with contradictory settings, such as an OpenAI-compatible instance whose declared locality disagrees with one of its models. |
| `invalid_provider_credentials`    | 503    | Provider rejected the configured credentials.                                                                                                          |
| `invalid_request`                 | 422    | Provider rejected the request body as malformed.                                                                                                       |
| `invalid_token`                   | 401    | Session token is malformed or unknown.                                                                                                                 |
| `missing_capability`              | 422    | Selected model lacks a capability the task requires.                                                                                                   |
| `missing_credentials`             | 401    | No session token was presented to a protected endpoint.                                                                                                |
| `missing_provider_credentials`    | 503    | The selected model requires provider credentials none were configured.                                                                                 |
| `model_not_found`                 | 404    | The referenced model is not offered by the provider asked to resolve it.                                                                               |
| `no_route`                        | 422    | No routing rule matched and no default route is configured.                                                                                            |
| `override_not_allowed`            | 403    | Caller asked for a model override that policy forbids.                                                                                                 |
| `permission_expansion`            | 422    | An override tried to loosen a tool permission that may only be tightened.                                                                              |
| `provider_authentication`         | 503    | Provider rejected the request at the authentication layer.                                                                                             |
| `provider_error`                  | 502    | Provider returned an error that did not match any known category.                                                                                      |
| `provider_unavailable`            | 503    | Provider returned a service-level error and may recover later.                                                                                         |
| `provider_unsupported_capability` | 422    | Provider reported it does not support a capability the request needed.                                                                                 |
| `rate_limited`                    | 429    | Provider rate-limited the request.                                                                                                                     |
| `request_timeout`                 | 504    | Call to the provider timed out before a response was returned.                                                                                         |
| `routing_unsupported_capability`  | 422    | Routing rejected the selected model because it lacks a required capability.                                                                            |
| `storage_error`                   | 502    | An error occurred while reading or writing from memory storage.                                                                                        |
| `token_expired`                   | 401    | Session token is past its expiry timestamp.                                                                                                            |
| `token_revoked`                   | 401    | Session token was previously valid but has been revoked.                                                                                               |
| `unknown_effort`                  | 422    | A reasoning effort name in the request was not recognized.                                                                                             |
| `unknown_intent`                  | 422    | A task intent name in the request was not recognized.                                                                                                  |
| `unknown_model`                   | 422    | A referenced model is not configured on the router.                                                                                                    |
| `unknown_provider`                | 422    | A provider identifier in the request was not recognized.                                                                                               |
