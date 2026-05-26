# Running the Router

`smista-router` is the routing and orchestration service the CLI talks to. This
page covers how the router process itself runs — the server binding, storage,
authentication, runtime limits and logging. It is separate from the routing
policy (which model handles a task); that lives in
[Configuring the CLI](cli.md).

## Where configuration lives

| Layer            | Location                                |
| ---------------- | --------------------------------------- |
| Global (POSIX)   | `~/.config/smista/router.toml`          |
| Global (Windows) | `C:\Users\$USER\.smista\router.toml`    |
| Project          | `.smista/router.toml`                   |

The default format is TOML. An invalid router configuration prevents the router
from starting and reports which field to fix.

## A complete example

```toml
[router]
host = "127.0.0.1"
port = 7331

[router.storage]
engine = "surrealdb"
mode = "embedded"
path = ".smista/db"
namespace = "smista"
database = "local"

[router.auth]
token_ttl_seconds = 86400
api_key_version = "01"
local_bootstrap_enabled = true

[router.limits]
max_request_body_bytes = 10485760
max_context_bytes = 5242880
max_concurrent_requests = 8
request_timeout_ms = 120000
provider_timeout_ms = 180000
tool_timeout_ms = 60000

[router.logging]
level = "info"
format = "compact"
redact_secrets = true

[router.cors]
enabled = false
allowed_origins = []

[router.retention]
trace_retention_days = 90
session_retention_days = 365
deleted_session_retention_days = 30
```

## Server

```toml
[router]
host = "127.0.0.1"
port = 7331
```

The router is meant to be reached locally. Binding it to a public interface in
local mode is flagged as unsafe by validation.

## Storage

Where users, sessions, tokens, traces and execution metadata are persisted.
smista.ai uses SurrealDB, which supports both an embedded local database and a
remote server for future deployments.

```toml
[router.storage]
engine = "surrealdb"
mode = "embedded"
path = ".smista/db"
namespace = "smista"
database = "local"
```

## Authentication

Controls API-key bootstrap and the lifetime of short-lived session tokens.

```toml
[router.auth]
token_ttl_seconds = 86400
api_key_version = "01"
local_bootstrap_enabled = true
```

`local_bootstrap_enabled` lets the router mint a user and API key locally
without a SaaS account. It must be disabled in remote/SaaS mode.

## Runtime limits

Protect the router from oversized requests, runaway tools, excessive context and
hanging provider calls.

```toml
[router.limits]
max_request_body_bytes = 10485760
max_context_bytes = 5242880
max_concurrent_requests = 8
request_timeout_ms = 120000
provider_timeout_ms = 180000
tool_timeout_ms = 60000
```

## Logging

```toml
[router.logging]
level = "info"
format = "compact"
redact_secrets = true
```

Keep `redact_secrets = true`. API keys, provider credentials and auth tokens are
never written to logs or traces.

## CORS

Disabled by default. Only needed for browser-based clients or a future web
dashboard.

```toml
[router.cors]
enabled = true
allowed_origins = ["https://app.smista.ai"]
```

> [!WARNING]
> Never enable CORS with unrestricted origins in production.

## Retention

```toml
[router.retention]
trace_retention_days = 90
session_retention_days = 365
deleted_session_retention_days = 30
```

## Local models with Ollama

The router connects to Ollama to run local models. Connection and discovery are
configured here under `[router.ollama]`; which tasks actually use an Ollama
model is decided by your routing policy in [Configuring the CLI](cli.md). The
two must stay consistent — see
[Using Local Models with Ollama](ollama.md) for the full setup.

```toml
[router.ollama]
enabled = true
base_url = "http://127.0.0.1:11434"
auto_discover_models = true
startup_healthcheck = true
startup_required = false
model_refresh_interval_seconds = 300

[router.ollama.limits]
max_concurrent_requests = 4
request_timeout_ms = 180000
pull_timeout_ms = 600000

[router.ollama.models]
preload = ["llama3.1:8b", "qwen2.5-coder:7b"]
allow_pull = false
allowed_models = ["llama3.1:8b", "qwen2.5-coder:7b", "mistral:7b"]
```

## Validation

Router configuration is validated at startup. Validation rejects an invalid host
or port, an unsafe public binding in local mode, missing or unsupported storage
configuration, local bootstrap enabled in remote mode, invalid timeouts or size
limits, unsafe CORS, and inline secrets. A failure prevents startup and explains
the offending field.
