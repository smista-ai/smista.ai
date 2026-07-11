# HTTP API

- [HTTP API](#http-api)
  - [OpenAPI schema](#openapi-schema)
  - [Conventions](#conventions)
  - [Health check](#health-check)
  - [Authentication](#authentication)
    - [Bootstrap a user](#bootstrap-a-user)
    - [Sign in](#sign-in)
    - [Sign out](#sign-out)
    - [Current user](#current-user)
  - [Sessions](#sessions)
    - [Create a session](#create-a-session)
    - [List sessions](#list-sessions)
    - [Fetch a session](#fetch-a-session)
    - [Update a session](#update-a-session)
    - [Delete a session](#delete-a-session)
  - [Executing a task](#executing-a-task)
    - [Policy](#policy)
    - [Execute the task](#execute-the-task)
    - [Advance a run](#advance-a-run)
    - [Streaming](#streaming)
    - [Preview a route](#preview-a-route)
  - [Approvals](#approvals)
  - [Traces](#traces)
    - [Fetch a session's trace](#fetch-a-sessions-trace)
  - [Providers and models](#providers-and-models)
    - [List providers](#list-providers)
    - [List models](#list-models)
  - [Usage](#usage)
    - [Session usage](#session-usage)
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

## OpenAPI schema

A machine-readable OpenAPI 3.1 schema for this API is published alongside this
page at [`./openapi.json`](./openapi.json). You can use it to generate typed
clients in any language, validate requests and responses against the schema, or
explore the API interactively in tools such as Swagger UI or Insomnia.

## Conventions

- All endpoints live under `/api/v1` — e.g. `/api/v1/auth/sign-in`.
- `GET` reads, `POST` creates or executes, `PUT` replaces, `DELETE` removes.
- Request and response bodies are JSON.
- Paths are resource-oriented: `/auth`, `/sessions`, `/sessions/{id}/...`,
  `/llm`.

## Health check

```http
GET /status
```

Public, unauthenticated, and the one endpoint that lives outside `/api/v1`. Use
it to check that the router is up and to read the version it is running. It needs
no token and no provider credentials:

```json
{ "status": "ok", "version": "0.1.0" }
```

`status` is `"ok"` whenever the server answers; `version` is the running
router's version.

## Authentication

smista.ai separates **router authentication** (who you are) from **provider
credentials** (keys for OpenAI, Anthropic, Gemini, Ollama, …). They travel in
different headers and are never mixed.

| Header                                        | Used for                                    |
| --------------------------------------------- | ------------------------------------------- |
| `Authorization: Bearer <session-token>`       | Authenticated requests after sign-in.       |
| `X-Smista-Api-Key: <api-key>`                 | Auth endpoints only, to obtain a token.     |
| `X-Smista-Provider-<Provider>-Api-Key: <key>` | Provider credential for a specific request. |

For example: `X-Smista-Provider-Anthropic-Api-Key: <key>`. The `<Provider>` part
is the provider name and is case-insensitive (`anthropic`, `openai`, `gemini`,
`ollama`). For an OpenAI-compatible endpoint, use its instance name directly —
`X-Smista-Provider-my-vllm-Api-Key` for an instance named `my-vllm` — since the
`openai-compat:` form cannot appear in a header name.

Provider credentials are sent only when the selected model needs them, used for
that one request, and never logged, traced or forwarded to the model.
Credentials are never accepted in query parameters.

The flow: `POST /auth/bootstrap` returns a user ID and a long-lived API key
(shown once). `POST /auth/sign-in` exchanges that key for a short-lived session
token, which you send as a bearer token on every other request. For how these
credentials are formatted, hashed and verified, see
[Router authentication](../technical/authentication.md).

### Bootstrap a user

```http
POST /api/v1/auth/bootstrap
```

Public endpoint, and the only public write: it needs no token, because it mints
the first credential you ever hold. It has no request body. Each call creates a
new user and returns `201` with that user's ID and a freshly generated,
long-lived API key:

```json
{
  "user_id": "018f9c3e-7a2b-7c4d-8e5f-1a2b3c4d5e6f",
  "api_key": "sk-smista-api01-<user-id>-<secret>"
}
```

The key is `sk-smista-api01-` followed by the user id and a random secret. It
embeds the user id, so the router identifies the owner from the key alone — you
never send the user id alongside it.

The plaintext API key is shown **only** in this response and can never be
retrieved again — the router stores it hashed. Save it now; if you lose it,
bootstrap a new user. The response carries no other secrets. A failure to
persist the user returns `500` with code `internal_error`.

### Sign in

```http
POST /api/v1/auth/sign-in
X-Smista-Api-Key: <api-key>
```

Public endpoint. The API key already identifies the user, so no body is needed.
Exchanges the API key for a short-lived session token and its expiry:

```json
{
  "token": "0194f1e23a2d7e6f9b0a1c2d3e4f5a6b-3k9q...<64 chars>",
  "expires_at": "2026-05-25T12:00:00Z"
}
```

The token is `<token-id>-<secret>`: a 32-hex-digit token id, a hyphen, then a
64-character lowercase-alphanumeric secret. Treat it as opaque and send it back
verbatim as `Authorization: Bearer <token>`. Its lifetime comes from
`router.auth.token_ttl_seconds`. See
[Router authentication](../technical/authentication.md#session-tokens) for the
format and hashing details.

A missing `X-Smista-Api-Key` header returns `401` with code
`missing_credentials`. A malformed, unknown or non-matching key returns `401`
with code `invalid_api_key`, reported uniformly so it never reveals which users
exist. The API key is never logged, echoed back, or accepted as a query
parameter.

### Sign out

```http
POST /api/v1/auth/sign-out
Authorization: Bearer <session-token>
```

Revokes the current session token:

```json
{ "revoked": true }
```

After sign-out the token can no longer be used: presenting it again fails with
`401` `token_revoked`. A token that simply lapses fails with `401`
`token_expired` instead. Both are reported only to a caller holding the genuine
token; an unknown or malformed token always fails with `401` `invalid_token`.
Your API key is unaffected, so you can sign in again for a fresh token.

### Current user

```http
GET /api/v1/auth/me
Authorization: Bearer <session-token>
```

Confirms the session token is valid and reports who you are:

```json
{ "user_id": "018f9c3e-7a2b-7c4d-8e5f-1a2b3c4d5e6f" }
```

To list a user's sessions, use `GET /api/v1/sessions`.

## Sessions

```http
POST   /api/v1/sessions                  # create (title required)
GET    /api/v1/sessions                  # list/filter every session, archived included
GET    /api/v1/sessions/{session_id}     # fetch / resume
PUT    /api/v1/sessions/{session_id}     # update title or archive
DELETE /api/v1/sessions/{session_id}     # delete
```

All session routes require `Authorization: Bearer <session-token>`. A user can
only access their own sessions. A session that belongs to another user is
treated as if it did not exist and returns `404`, so the API never reveals that
someone else's session exists.

### Create a session

```http
POST /api/v1/sessions

{ "title": "Refactor auth middleware" }
```

A `title` is required; omitting it returns `422`. You may also send a `scope`:
an opaque grouping key the router stores and matches verbatim, so you can later
list only the sessions that share it. The CLI sets it from your working
directory to group sessions by project, but it can be any string a client
chooses; omit it for a session with no scope.

To make the session
end-to-end encrypted, send a `key_id` — the fingerprint of the per-session key
your client holds. A session is encrypted when, and only when, a `key_id` is
present, so there is no separate `encrypted` flag to keep in step with it:

```json
{ "title": "Refactor auth middleware", "key_id": "kf_ab12" }
```

Whether a session is encrypted is fixed for its life and cannot be changed
later. See [End-to-end encryption](../technical/e2e_encryption.md). The response
echoes the resulting `encrypted` flag, which is `true` exactly when a `key_id`
was supplied, and an encrypted summary carries that `key_id` back; a plaintext
summary omits the field entirely. Returns `201` with the new session summary:

```json
{
  "session": {
    "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
    "title": "Refactor auth middleware",
    "encrypted": false,
    "created_at": "2026-05-25T09:00:00Z",
    "updated_at": "2026-05-25T09:00:00Z",
    "archived": false
  }
}
```

### List sessions

```http
GET /api/v1/sessions
```

Returns every session that belongs to you, archived ones included, each as a
summary and ordered most recently updated first. A summary's `title` may be
`null` for a session that has none, a summary carries its `scope` when the
session has one, and an encrypted summary carries its `key_id` while a plaintext
one omits the field:

```json
{
  "sessions": [
    {
      "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
      "title": "Refactor auth middleware",
      "scope": "/home/dev/project",
      "encrypted": false,
      "created_at": "2026-05-25T09:00:00Z",
      "updated_at": "2026-05-25T09:30:00Z",
      "archived": false
    }
  ]
}
```

Narrow the listing with two optional query parameters, which combine: `scope`
matches a session's scope exactly, and `title` matches sessions whose title
contains it, case-insensitively. With neither set, every session is returned.

```http
GET /api/v1/sessions?scope=/home/dev/project&title=auth
```

### Fetch a session

```http
GET /api/v1/sessions/{session_id}
```

Returns the full session, including its messages and free-form metadata.
`messages` are ordered oldest first, and `metadata` is always present even when
empty. An archived session is not returned here, and neither is a session owned
by another user; both respond `404`, the same as an unknown id.

The fetched session detail carries `key_id` when the session is encrypted and
omits it for plaintext sessions. It does not include the summary-only
`encrypted` flag; `key_id` presence is the detail view's encryption marker.

Each message's `content` is tagged with how it is stored. A plaintext session
returns `{ "plaintext": "..." }`; an end-to-end encrypted session returns
`{ "encrypted": { ... } }` with the sealed envelope, since the router holds no
key and cannot open it. `provider` and `model` name the model behind an
assistant turn and are omitted for the other roles.

```json
{
  "session": {
    "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
    "title": "Refactor auth middleware",
    "created_at": "2026-05-25T09:00:00Z",
    "updated_at": "2026-05-25T09:30:00Z",
    "messages": [
      { "role": "user", "content": { "plaintext": "Refactor the auth middleware." } },
      {
        "role": "assistant",
        "content": { "plaintext": "Here is the plan..." },
        "provider": "anthropic",
        "model": "claude-sonnet"
      }
    ],
    "metadata": {}
  }
}
```

A malformed `session_id` that is not a valid UUID responds `400` with
`invalid_session_id`.

### Update a session

```http
PUT /api/v1/sessions/{session_id}

{ "title": "Refactor auth and sessions", "archived": false }
```

The body is partial: send only the fields you want to change, and any field you
omit keeps its current value. Set `archived` to `true` to archive the session or
`false` to restore it. Every successful update refreshes `updated_at`.

It returns the updated session summary:

```json
{
  "session": {
    "id": "5f8b1c7e-3a2d-4e6f-9b0a-1c2d3e4f5a6b",
    "title": "Refactor auth and sessions",
    "encrypted": false,
    "created_at": "2026-05-25T09:00:00Z",
    "updated_at": "2026-05-25T09:30:00Z",
    "archived": false
  }
}
```

Only the owner can update a session. A session owned by another user, like an
unknown id, responds `404`, so its existence is never disclosed. A malformed
`session_id` that is not a valid UUID responds `400` with `invalid_session_id`.

### Delete a session

```http
DELETE /api/v1/sessions/{session_id}
```

Deletes the session and the context memory tied to it. Returns
`{ "deleted": true }`.

## Executing a task

```http
POST /api/v1/sessions/{session_id}/execute
Authorization: Bearer <session-token>
X-Smista-Provider-{provider}-Api-Key: <api-key>
```

The body carries everything the router needs to make a deterministic decision:
the user input, a workspace snapshot, the merged policy, local preferences, and
the local `attachments` (files, instructions and skills) the router cannot read
for itself. Session
history, memory and the assembled context are **not** sent — the router owns
them and recalls them from storage. The `policy` block is the same routing,
tool-permission and privacy vocabulary the CLI loads from `config.toml` — sent
verbatim, not a separate, lossy shape. For the full interaction model, the
continuations and the streaming flow, see
[the execution protocol](../technical/execution-protocol.md):

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
    "classification": {
      "default_intent": "chat",
      "rules": [
        { "intent": "review", "priority": 10, "keywords": ["review", "audit"], "requires_any_context": ["git_diff"] }
      ]
    },
    "routing": {
      "default": {
        "model": "anthropic/claude-sonnet",
        "fallbacks": ["openai/gpt-5.5-thinking", "ollama/qwen2.5-coder:7b"]
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
  "local_preferences": { "auto_apply": false, "local_only": false, "no_network": false },
  "attachments": {
    "files": [{ "path": "src/auth/middleware.rs", "content": "...", "content_hash": "sha256:...", "required": true }],
    "instructions": [{ "source": "AGENTS.md", "content": "..." }],
    "invoked_skills": [{ "name": "code-review", "content": "Report findings by severity." }],
    "available_skills": [{ "name": "changelog", "content": "Summarize changes under a heading." }]
  }
}
```

The top-level fields are:

| Field               | Purpose                                                                                                                                                                                                                     |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `input`             | The prompt `text`, an optional `command` and an optional `explicit_model`.                                                                                                                                                  |
| `workspace`         | Repository snapshot: `root`, `git_branch`, `git_diff`, referenced/active files.                                                                                                                                             |
| `policy`            | The deterministic `classification`, `routing`, `tools` and `privacy` policy (see below).                                                                                                                                    |
| `local_preferences` | Resolved client toggles: `auto_apply`, `local_only`, `no_network`.                                                                                                                                                          |
| `attachments`       | Local content the router cannot read: `files` (each `required` or discardable), `instructions`, `invoked_skills` (explicitly invoked, added to the model preamble), `available_skills` (offered for the model to activate). |

`input.command` forces a task type (`edit`, `review`, …) and `input.explicit_model`
forces a `provider/model`, bypassing routing entirely; both may be `null`.

The request never lists providers or credential status: the router owns the
model catalog and reads any supplied provider credentials from the
`X-Smista-Provider-<Provider>-Api-Key` headers, so it decides availability for
itself.

### Policy

`policy.version` is the snapshot schema version and `policy.source` records how
it was assembled (e.g. `merged`). The four sub-blocks mirror the CLI's
`[classification]`, `[routing]`, `[tools]` and `[privacy]` config sections
exactly.

`classification` holds the ordered intent rules and the `default_intent` the
router applies when none match; see
[Task intent classification](../technical/task-classification.md). `routing`
holds ordered `rules` plus an optional `default` route (`model` and ordered
`fallbacks`) used when no rule matches. Each rule:

| Field                   | Type            | Purpose                                                                 |
| ----------------------- | --------------- | ----------------------------------------------------------------------- |
| `name`                  | string          | Human-readable rule name.                                               |
| `priority`              | integer         | Evaluation order, ascending; first match wins. Defaults to `1000`.      |
| `effort`                | string          | Reasoning effort for the matched model (`low`/`medium`/`high`/`xhigh`). |
| `intent`                | task type, null | Required task intent, if scoped.                                        |
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

Provider credentials never appear in the body. They travel as
`X-Smista-Provider-<Provider>-Api-Key` headers, and the router combines them
with its own model catalog to decide which models are available — the client
declares nothing about providers or credential status.

### Execute the task

```http
POST /api/v1/sessions/{session_id}/execute
```

The router classifies the task, applies the policy, selects a model, builds the
request and runs one **turn**. A turn resolves to one envelope of
`{ status, data, allowed_continuations }`: `status` names the outcome, `data`
carries its payload, and `allowed_continuations` lists the messages the client
may send next. A `completed` turn carries the assistant message and a routing
explanation under `data`:

```json
{
  "status": "completed",
  "data": {
    "message": { "role": "assistant", "content": "..." },
    "classification": { "intent": "edit", "source": "inferred", "reason": "keyword matched rule 0", "confidence": "high" },
    "routing": {
      "task_type": "edit",
      "provider": "anthropic",
      "model": "claude-sonnet",
      "matched_rule": "edit + src/auth/** -> anthropic/claude-sonnet",
      "fallback_used": false,
      "override_used": false
    },
    "context": {
      "included": ["src/auth/middleware.rs", "AGENTS.md", "current git diff"],
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
}
```

When the model cannot be answered in one turn, `status` is a **continuation**
instead — the router needs the client to do the next step:

| `status`            | The router needs the client to                       |
| ------------------- | ---------------------------------------------------- |
| `completed`         | render; seal `to_encrypt` if present, else done.     |
| `awaiting_tool`     | run one or more tools and return the results.        |
| `awaiting_approval` | decide a yes/no with no tool to run.                 |
| `awaiting_decrypt`  | open sealed history so the prompt can be built.      |
| `awaiting_encrypt`  | seal router-authored content before it is persisted. |
| `idle`              | nothing; the run finished and was persisted.         |
| `error`             | nothing; the run is over.                            |

`allowed_continuations` lists the message types the client may send next; `break`
is always among them while the run is live, and it is empty for a terminal
outcome.

An `awaiting_tool` turn lists the calls to run under `data`, correlated by
`call_id`; each carries `requires_approval` of `allow` (run it) or `ask` (confirm
first):

```json
{
  "status": "awaiting_tool",
  "data": {
    "tool_requests": [
      { "call_id": "c1", "name": "shell", "arguments": { "command": "cargo test" }, "requires_approval": "ask" }
    ],
    "trace_id": "trace:xyz"
  },
  "allowed_continuations": ["tool_results", "inject", "break"]
}
```

The client does the work and resumes the run with [`/continue`](#advance-a-run).
See the [execution protocol](../technical/execution-protocol.md) for the full
set of continuation payloads.

By default `/execute` buffers the turn as a single JSON `TurnResponse`. Send
`Accept: text/event-stream` to stream it instead as the Server-Sent Events
described under [Streaming](#streaming), ending with the terminal `turn_end`
event that carries the same envelope.

### Advance a run

```http
POST /api/v1/sessions/{session_id}/continue
```

Resumes the in-flight run with a single tagged `{ type, data }` message that
answers the current pause. The valid `type` values are what the previous
response advertised in `allowed_continuations`; `break` is always valid. It
returns the next turn in the same shape as `/execute`, buffered or streamed by
the `Accept` header.

| `type`               | Answers             | `data`                                                               |
| -------------------- | ------------------- | -------------------------------------------------------------------- |
| `tool_results`       | `awaiting_tool`     | `{ results: [{ call_id, content, is_error, decision }], encrypted }` |
| `approval_decisions` | `awaiting_approval` | `{ decisions: [{ approval_id, decision, reason }], encrypted }`      |
| `decrypted`          | `awaiting_decrypt`  | `{ plaintext }` — a content-ref → plaintext map                      |
| `sealed`             | a folded encrypt    | `{ encrypted }` — a content-ref → envelope map                       |
| `inject`             | any live state      | `{ messages: [{ text, ciphertext }] }` — mid-run input; supersedes   |
| `break`              | any live state      | none — aborts the in-flight turn                                     |

The `encrypted` and `plaintext` maps are keyed by a content reference of the
form `kind:id` (`message`, `tool_call`, `diff`, `plan`, `memory` or `trace`).

```json
{
  "type": "tool_results",
  "data": {
    "results": [{ "call_id": "c1", "content": "test result: ok", "is_error": false, "decision": "approved" }]
  }
}
```

### Streaming

`/execute` and `/continue` buffer the turn as a single JSON `TurnResponse` by
default. Send `Accept: text/event-stream` on either to receive the turn as a
stream of Server-Sent Events instead, each a structured object with a `type`:

```json
{ "type": "text_delta", "delta": "The first step is..." }
```

Event types: `text_delta`, `reasoning_delta`, `tool_call_started`,
`tool_call_requested`, `usage`, and the terminal `turn_end`.

Models that expose their reasoning stream it as `reasoning_delta` chunks.
When the model starts calling a tool, a `tool_call_started` event announces
the call's name as soon as it is known; the matching `tool_call_requested`
event follows once the arguments are complete, correlated by `call_id`.

The `usage` event reports token counts and, when the model declares prices,
the actual cost of the invocation. Local models report a zero cost.

Every stream ends with exactly one `turn_end` event, whose `status` is the
same value the buffered response carries (`completed`, `awaiting_tool`,
`awaiting_approval`, `awaiting_decrypt`, `awaiting_encrypt`, `idle` or `error`).
It tells the client whether the turn finished or paused for a continuation, so
the client never has to infer it. Models that cannot stream still answer over
this stream: the full response is replayed as a short stream of the same events.

### Preview a route

```http
POST /api/v1/sessions/{session_id}/preview
```

Same body as `/execute`, but the selected model is **never called**: no provider
completion request is made and no tokens are spent. Send the same
`X-Smista-Provider-<Provider>-Api-Key` headers as `/execute`; the router may use
them to query provider model catalogs and runs the same credential-aware,
deterministic routing `/execute` would. It returns the task type, chosen
provider/model, matched rule, routing explanation, included/excluded context, an
estimated cost range, and the required permissions:

```json
{
  "classification": { "intent": "review", "source": "inferred", "reason": "keyword 'review' matched rule 0", "confidence": "high" },
  "routing": {
    "intent": "review",
    "provider": "openai",
    "model": "gpt-5.5-thinking",
    "matched_rule": "task.review -> openai/gpt-5.5-thinking",
    "fallback_used": false,
    "override_used": false,
    "reason": "rule 'task.review' matched the review intent"
  },
  "included_context": ["current git diff", "AGENTS.md"],
  "excluded_context": [".env", "target/**"],
  "estimated_cost": { "min": "0.03", "max": "0.09", "currency": "USD" },
  "required_permissions": [
    { "permission": "read_repository", "mode": "allow" },
    { "permission": "write_files", "mode": "ask" }
  ]
}
```

`routing` is the complete deterministic decision that `/execute` would apply.
Its `reason` explains why the route was selected, while `fallback_used` and
`override_used` identify whether selection moved away from the configured
primary or honored an explicit model override.

`required_permissions` is the project tool permissions tightened by the matched
rule's `required_permissions`. `estimated_cost` is a decimal-string range: `min`
prices only the input (the selected context and the prompt) and `max` adds an
assumed reply, so a model that declares no prices — a local model, for instance —
reports a `0`–`0` range. The preview is deterministic: the same body, policy,
provider credentials, and model catalogs yield the same result. Missing
credentials have the same effect as on `/execute`: models that require them are
unavailable, so routing uses an eligible fallback or fails with
`fallback_exhausted`.

Only the owner may preview, and previewing needs no run: it acquires no lock and
changes nothing, so it works even while a turn is in flight. An unknown,
archived or another user's session responds `404` `session_not_found`, alike so
existence stays private; a `session_id` that is not a valid UUID responds `400`
`invalid_session_id`; and a request whose routing cannot resolve responds `422`
(`no_route`, `context_window_exceeded`), `403` `override_not_allowed`, or `503`
`fallback_exhausted`.

## Approvals

Approvals travel through [`/continue`](#advance-a-run); there is no separate
approval endpoint.

For a tool that needs confirmation (`requires_approval: "ask"`), the client —
the same machine that approves and executes — asks the user, then runs the tool
if approved or reports a rejection, and returns the outcome in the tool result's
`decision`. The approval and the result arrive together.

A standalone `awaiting_approval` is raised only for a decision with **no tool to
run**: disclosing context to a remote provider when `privacy.remote.mode` is
`ask` (`remote_disclosure`), confirming a per-task cost ceiling (`cost_limit`),
or accepting a generated plan before execution begins (`plan`). The client
returns the decision in the `approval_decisions` bundle:

```json
{ "approval_decisions": [{ "approval_id": "a1", "decision": "approved", "reason": null }] }
```

`decision` is `approved` or `rejected`.

## Traces

A trace is the ordered list of events emitted while the router routed and ran a
session's tasks. A session has a single trace; it grows as the session runs.

### Fetch a session's trace

```http
GET /api/v1/sessions/{session_id}/traces
```

Returns the session's trace wrapped under a `trace` key. `events` is ordered
oldest first. Each event carries its own routing context (`task_type`,
`provider`, `model`, optional `matched_rule`) and a `payload`. `event_type` is
one of `message`, `classification`, `routing_decision`, `context_selection`,
`tool_call`, `approval` or `cost`.

The `payload` is either `plaintext` or `encrypted`. For a normal session it is
`{ "plaintext": <payload> }`, where `<payload>` is tagged by a `type` field
equal to `event_type`; the per-type shapes are listed under
`trace_event_content` in the [storage schema reference](../technical/schema.md).
For an end-to-end encrypted session it is `{ "encrypted": <envelope> }`, the
sealed AEAD envelope (`version`, `algorithm`, `key_id`, `nonce`, `ciphertext`)
that only a client holding the session key can open.

The events are paginated with two optional query parameters:

| Parameter | Type    | Default | Description                         |
| --------- | ------- | ------- | ----------------------------------- |
| `limit`   | integer | `50`    | Maximum number of events to return. |
| `offset`  | integer | `0`     | Number of leading events to skip.   |

A session with no events in the requested window returns an empty `events`
array, not a `404`. Only the owner may read a trace: an unknown, archived or
another user's session responds `404` `session_not_found`, reported alike so
existence stays private, and a `session_id` that is not a valid UUID responds
`400` `invalid_session_id`:

```http
GET /api/v1/sessions/{session_id}/traces?limit=50&offset=0
```

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
        "payload": { "plaintext": { "type": "routing_decision", "provider": "openai", "model": "gpt-5.5-thinking", "fallback_used": false, "override_used": false, "reason": "best for review" } }
      },
      {
        "event_type": "tool_call",
        "task_type": "review",
        "provider": "openai",
        "model": "gpt-5.5-thinking",
        "created_at": "2026-06-04T10:15:02Z",
        "payload": { "plaintext": { "type": "tool_call", "tool_name": "read_file", "status": "completed" } }
      }
    ]
  }
}
```

## Providers and models

### List providers

```http
GET /api/v1/llm/providers
```

Lists the provider registry the router can route through. The default router
configuration includes the known built-in providers (`anthropic`, `gemini`,
`ollama` and `openai`); additional OpenAI-compatible instances appear when they
are configured with a usable `base_url`. This endpoint does not prove model
credentials are present. Use `GET /api/v1/llm/models` to see which providers can
list models with the credentials supplied on that request.

Each entry carries a `local` flag: `true` when the provider serves its models on
your own host or network with no request leaving the machine (a local Ollama, a
self-hosted OpenAI-compatible endpoint), and `false` for a cloud API. This is
the same locality every one of that provider's models reports, so the two can
never disagree:

```json
{
  "providers": [
    { "id": "anthropic", "display_name": "Anthropic", "local": false },
    { "id": "ollama", "display_name": "Ollama", "local": true }
  ]
}
```

### List models

```http
GET /api/v1/llm/models
```

Lists the available models, returning each one as a full model descriptor. Like
`/execute`, it accepts `X-Smista-Provider-<Provider>-Api-Key` headers and needs
them: the router queries each provider's `list_models`, and remote providers
such as Anthropic and Gemini reject that call without an API key. A provider the
router could not list — most often because its credentials are missing or were
rejected — is left out of `models` and reported under `unavailable`, each entry
naming the `provider`, a machine-readable `reason` and an optional human-readable
`message`. This lets you tell an incomplete result from a genuinely empty one and
see why each provider dropped out; `unavailable` is absent when every configured
provider was listed. The `reason` is one of `authentication`, `context_length`,
`invalid_configuration`, `invalid_credentials`, `invalid_request`,
`missing_credentials`, `model_not_found`, `provider_unavailable`, `rate_limit`,
`storage`, `timeout`, `unknown` or `unsupported_capability`. `capabilities` is a
nested object of boolean flags — `streaming`, `tools`, `json_output`,
`system_prompt`, `images`, `reasoning` and `memory` — where an absent or `false`
flag means the capability is not supported. `auth` records how the model
authenticates (`none`, `api_key`, `optional_api_key` or a
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
  ],
  "unavailable": [
    {
      "provider": "gemini",
      "reason": "missing_credentials",
      "message": "no credentials configured for the provider"
    }
  ]
}
```

The router lists every provider in parallel under a per-provider deadline, so a
single slow provider cannot hold the response open: a provider that does not
answer in time is reported under `unavailable` with `reason` `timeout`. The
deadline defaults to 10 seconds; send the `X-Smista-Timeout-Ms` header to tune
it, in milliseconds. A value above the 60-second cap is clamped down to it, and a
missing, zero or non-numeric value falls back to the default.

## Usage

### Session usage

```http
GET /api/v1/sessions/{session_id}/usage
```

Reports the session total plus a per-model and a per-task-type breakdown,
aggregated from the session's cost events in one read. The top-level `total`,
`by_model` and `by_task_type` are returned directly, with no enclosing wrapper.
Each `by_model` entry carries its `provider`, `model` and `request_count`, and
each `by_task_type` entry its `task_type` and `request_count`. Cost fields are
decimal strings priced in `USD`, and a token count the provider never reported
is omitted rather than guessed:

```json
{
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
```

Only the owner may read a session's usage. An unknown, archived or another
user's session responds `404` `session_not_found`, reported alike so existence
stays private; a `session_id` that is not a valid UUID responds `400`
`invalid_session_id`. A session that exists but has recorded no cost yet answers
`200` with an empty `by_model` and `by_task_type`.

In an end-to-end encrypted session the cost figures are sealed and the router
holds no key, so it reports each request's `provider`, `model` and `task_type`
from the plaintext metadata and its `request_count`, but omits the token and
cost fields it cannot read.

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
| 501  | Endpoint recognized but not implemented yet      |
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
| `credentials_in_query`            | 400    | A credential was passed as a query parameter; credentials are accepted only in headers.                                                                |
| `fallback_exhausted`              | 503    | Primary route failed and every configured fallback also failed.                                                                                        |
| `forbidden`                       | 403    | Caller is authenticated but not the resource owner.                                                                                                    |
| `internal_error`                  | 500    | Unexpected server-side failure. Details intentionally omitted.                                                                                         |
| `invalid_api_key`                 | 401    | The API key presented to `POST /auth/sign-in` is malformed, unknown or does not match. Reported uniformly so it never leaks which users exist.         |
| `invalid_model_reference`         | 422    | A model reference was not in the expected `provider/model` form.                                                                                       |
| `invalid_provider_configuration`  | 500    | A provider was configured with contradictory settings, such as an OpenAI-compatible instance whose declared locality disagrees with one of its models. |
| `invalid_provider_credentials`    | 503    | Provider rejected the configured credentials.                                                                                                          |
| `invalid_provider_name`           | 422    | A provider identifier in a model or routing reference was not in the expected form.                                                                    |
| `invalid_request`                 | 422    | Provider rejected the request body as malformed.                                                                                                       |
| `invalid_session_id`              | 400    | A session id in the path was not a valid UUID.                                                                                                         |
| `invalid_token`                   | 401    | Session token is malformed or unknown.                                                                                                                 |
| `missing_capability`              | 422    | Selected model lacks a capability the task requires.                                                                                                   |
| `missing_credentials`             | 401    | No credential was presented: a session token on a protected endpoint, or the `X-Smista-Api-Key` header on `POST /auth/sign-in`.                        |
| `missing_provider_credentials`    | 503    | The selected model requires provider credentials none were configured.                                                                                 |
| `model_not_found`                 | 404    | The referenced model is not offered by the provider asked to resolve it.                                                                               |
| `no_route`                        | 422    | No routing rule matched and no default route is configured.                                                                                            |
| `not_implemented`                 | 501    | The endpoint is recognized but not implemented yet.                                                                                                    |
| `override_not_allowed`            | 403    | Caller asked for a model override that policy forbids.                                                                                                 |
| `permission_expansion`            | 422    | An override tried to loosen a tool permission that may only be tightened.                                                                              |
| `provider_authentication`         | 503    | Provider rejected the request at the authentication layer.                                                                                             |
| `provider_error`                  | 502    | Provider returned an error that did not match any known category.                                                                                      |
| `provider_unavailable`            | 503    | Provider returned a service-level error and may recover later.                                                                                         |
| `provider_unsupported_capability` | 422    | Provider reported it does not support a capability the request needed.                                                                                 |
| `rate_limited`                    | 429    | Provider rate-limited the request.                                                                                                                     |
| `request_timeout`                 | 504    | Call to the provider timed out before a response was returned.                                                                                         |
| `routing_unsupported_capability`  | 422    | Routing rejected the selected model because it lacks a required capability.                                                                            |
| `run_in_flight`                   | 409    | A turn is already in flight for the session; the run is busy until it reaches a checkpoint.                                                            |
| `session_not_found`               | 404    | The session does not exist, is archived, or belongs to another user; the three are reported alike so existence stays private.                          |
| `storage_error`                   | 502    | An error occurred while reading or writing from memory storage.                                                                                        |
| `token_expired`                   | 401    | Session token is past its expiry timestamp.                                                                                                            |
| `token_revoked`                   | 401    | Session token was previously valid but has been revoked.                                                                                               |
| `unknown_effort`                  | 422    | A reasoning effort name in the request was not recognized.                                                                                             |
| `unknown_intent`                  | 422    | A task intent name in the request was not recognized.                                                                                                  |
| `unknown_model`                   | 422    | A referenced model is not configured on the router.                                                                                                    |
| `unknown_provider`                | 422    | A provider identifier in the request was not recognized.                                                                                               |
