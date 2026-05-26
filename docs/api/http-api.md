# HTTP API

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
credentials** (keys for OpenAI, Anthropic, Ollama, …). They travel in different
headers and are never mixed.

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
Returns the authenticated user.

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

## Executing a task

```http
POST /api/v1/sessions/{session_id}/execute
Authorization: Bearer <session-token>
X-Smista-Provider-{provider}-Api-Key: <api-key>
```

The body carries the user input, workspace context, the merged policy and the
selected context. The router classifies the task, applies the policy, selects a
model, builds the request and returns the result with a routing explanation:

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
    "estimated_cost": 0.08,
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

Event types: `text_delta`, `tool_call_requested`, `approval_required`,
`tool_result`, `usage`, `error`, `done`.

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
  "estimated_cost": { "min": 0.03, "max": 0.09, "currency": "USD" },
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

Returns the execution trace: selected model, matched rule, task type, fallbacks,
overrides, context, tool calls, approvals and cost.

## Providers, models and usage

```http
GET /api/v1/llm/providers              # configured providers
GET /api/v1/llm/models                 # models with capabilities
GET /api/v1/sessions/{session_id}/usage  # token and cost breakdown
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
