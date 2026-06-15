# Running the Router

- [Running the Router](#running-the-router)
  - [Where configuration lives](#where-configuration-lives)
  - [A complete example](#a-complete-example)
  - [Server](#server)
  - [Storage](#storage)
  - [Authentication](#authentication)
  - [Runtime limits](#runtime-limits)
  - [Rate limiting](#rate-limiting)
  - [Logging](#logging)
  - [CORS](#cors)
  - [Retention](#retention)
  - [Providers](#providers)
    - [Generic OpenAI-compatible endpoints](#generic-openai-compatible-endpoints)
  - [Local models with Ollama](#local-models-with-ollama)
  - [Validation](#validation)

`smista-router` is the routing and orchestration service the CLI talks to. This
page covers how the router process itself runs — the server binding, storage,
authentication, runtime limits and logging. It is separate from the routing
policy (which model handles a task); that lives in
[Configuring the CLI](cli.md).

## Where configuration lives

| Layer            | Location                             |
| ---------------- | ------------------------------------ |
| Global (POSIX)   | `~/.config/smista/router.toml`       |
| Global (Windows) | `C:\Users\$USER\.smista\router.toml` |
| Project          | `.smista/router.toml`                |

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

[router.rate_limit]
enabled = true
period_ms = 10
burst_size = 200
trust_proxy_headers = false

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
cleanup_interval_seconds = 3600

[router.providers.openai]
base_url = "https://api.openai.com/v1"
```

## Server

```toml
[router]
host = "127.0.0.1"
port = 7331
```

The router is meant to be reached locally. Binding it to a public interface in
local mode is flagged as unsafe by validation.

The `[router]` table accepts:

| Key    | Type    | Default     | Purpose    |
| ------ | ------- | ----------- | ---------- |
| `host` | string  | `127.0.0.1` | Bind host. |
| `port` | integer | `7331`      | Bind port. |

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

The `[router.storage]` table accepts:

| Key         | Type   | Default     | Purpose                                         |
| ----------- | ------ | ----------- | ----------------------------------------------- |
| `engine`    | string | `surrealdb` | Storage engine; only `surrealdb` is supported.  |
| `mode`      | string | `embedded`  | `embedded` (on-disk) or `remote` (server).      |
| `path`      | string | none        | Database file path, used in `embedded` mode.    |
| `url`       | string | none        | Database server URL, used in `remote` mode.     |
| `username`  | string | none        | Authentication username, used in `remote` mode. |
| `password`  | string | none        | Authentication password, used in `remote` mode. |
| `namespace` | string | `smista`    | SurrealDB namespace.                            |
| `database`  | string | `local`     | SurrealDB database name.                        |

> [!NOTE]
> `username` and `password` authenticate against a remote SurrealDB server and
> are only used in `remote` mode. The password is treated as a secret: it is
> never logged and is never written back out when configuration is serialized.

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

The `[router.auth]` table accepts:

| Key                       | Type    | Default | Purpose                                            |
| ------------------------- | ------- | ------- | -------------------------------------------------- |
| `token_ttl_seconds`       | integer | `86400` | Session token lifetime, in seconds.                |
| `api_key_version`         | string  | `"01"`  | API key version segment.                           |
| `local_bootstrap_enabled` | bool    | `true`  | Allow local API-key bootstrap; off in remote mode. |

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

The `[router.limits]` table accepts:

| Key                       | Type    | Default    | Purpose                                   |
| ------------------------- | ------- | ---------- | ----------------------------------------- |
| `max_request_body_bytes`  | integer | `10485760` | Maximum request body size, in bytes.      |
| `max_context_bytes`       | integer | `5242880`  | Maximum context size, in bytes.           |
| `max_concurrent_requests` | integer | `8`        | Maximum concurrent requests.              |
| `request_timeout_ms`      | integer | `120000`   | Overall request timeout, in milliseconds. |
| `provider_timeout_ms`     | integer | `180000`   | Provider call timeout, in milliseconds.   |
| `tool_timeout_ms`         | integer | `60000`    | Tool execution timeout, in milliseconds.  |

## Rate limiting

Caps how fast a single client can hit the router, so a runaway script or a
misbehaving client cannot overwhelm it. Limiting is applied per client IP
address using a token bucket: each client may send a burst of up to `burst_size`
requests, and its allowance refills by one request every `period_ms`
milliseconds. Requests over the limit get a `429 Too Many Requests` response.

Enabled by default with limits sized for local use — roughly 100 requests per
second per client with room for a burst of 200. Raise the limits if you drive
the router hard from scripts, or set `enabled = false` to turn it off entirely.

```toml
[router.rate_limit]
enabled = true
period_ms = 10
burst_size = 200
trust_proxy_headers = false
```

The `[router.rate_limit]` table accepts:

| Key                   | Type    | Default | Purpose                                                                               |
| --------------------- | ------- | ------- | ------------------------------------------------------------------------------------- |
| `enabled`             | bool    | `true`  | Whether rate limiting is enabled.                                                     |
| `period_ms`           | integer | `10`    | Refill period, in milliseconds: the allowance grows by one request every `period_ms`. |
| `burst_size`          | integer | `200`   | Maximum number of requests a client may send in a burst before being limited.         |
| `trust_proxy_headers` | bool    | `false` | Identify clients by proxy headers instead of the connection's source address.         |

The sustained rate is one request every `period_ms` milliseconds — so the
default of `10` allows about 100 requests per second per client. When rate
limiting is enabled, both `period_ms` and `burst_size` must be greater than
zero; validation rejects a zero value.

By default each client is identified by the source address of its connection.
When the router runs behind a reverse proxy, every request appears to come from
the proxy, so they would all share one limit. Set `trust_proxy_headers = true`
to identify clients by the `X-Forwarded-For`, `X-Real-IP` or `Forwarded` header
the proxy sets instead.

> [!WARNING]
> Only enable `trust_proxy_headers` when a trusted reverse proxy sits in front
> of the router and sets these headers itself. With clients connecting directly,
> anyone can put any value in the header and hand themselves a fresh limit on
> every request, which defeats rate limiting entirely.

## Logging

```toml
[router.logging]
level = "info"
format = "compact"
redact_secrets = true
```

Keep `redact_secrets = true`. API keys, provider credentials and auth tokens are
never written to logs or traces.

The `[router.logging]` table accepts:

| Key              | Type   | Default   | Purpose                                  |
| ---------------- | ------ | --------- | ---------------------------------------- |
| `level`          | string | `info`    | Log level filter (e.g. `info`, `debug`). |
| `format`         | string | `compact` | Log output format.                       |
| `redact_secrets` | bool   | `true`    | Redact secrets from logs; keep enabled.  |

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

The `[router.cors]` table accepts:

| Key               | Type            | Default | Purpose                       |
| ----------------- | --------------- | ------- | ----------------------------- |
| `enabled`         | bool            | `false` | Whether CORS is enabled.      |
| `allowed_origins` | list of strings | `[]`    | Allowed origins when enabled. |

## Retention

```toml
[router.retention]
trace_retention_days = 90
session_retention_days = 365
archived_session_retention_days = 30
cleanup_interval_seconds = 3600
```

The `[router.retention]` table accepts:

| Key                               | Type    | Default | Purpose                                        |
| --------------------------------- | ------- | ------- | ---------------------------------------------- |
| `trace_retention_days`            | integer | `90`    | Days to retain traces.                         |
| `session_retention_days`          | integer | `365`   | Days to retain sessions.                       |
| `archived_session_retention_days` | integer | `30`    | Days to retain archived sessions before purge. |
| `cleanup_interval_seconds`        | integer | `3600`  | Interval between cleanup runs, in seconds.     |

## Providers

Each provider the CLI enables can have an optional connection override on the
router. This is where you point a provider at a custom endpoint — for example an
OpenAI-compatible proxy or a self-hosted gateway. Omit the section to use the
provider's default endpoint.

```toml
[router.providers.openai]
base_url = "https://api.openai.com/v1"
```

Each `[router.providers.<id>]` table accepts:

| Key            | Type            | Default  | Purpose                                                                                                                           |
| -------------- | --------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `base_url`     | string          | provider | Endpoint base URL; omit to use the provider's default.                                                                            |
| `local`        | bool            | `false`  | Whether the provider runs locally; surfaced to consumers to tell local providers from hosted ones.                                |
| `display_name` | string          | none     | Human-readable name for the provider; consumers fall back to the provider identifier when omitted.                                |
| `models`       | list of strings | `[]`     | Models advertised for an `openai-compat:<name>` endpoint with no model-listing API; see below. Ignored by the built-in providers. |

### Generic OpenAI-compatible endpoints

You can point the router at any number of OpenAI-compatible endpoints — a local
vLLM or LM Studio server, llama.cpp, or a hosted gateway. Each one is a *named
instance*: you choose a name, and routing rules address it as
`openai-compat:<name>/<model>` (for example
`openai-compat:my-vllm/llama-3.1-70b`).

Configure each instance under its full identity as the table key. The name is
quoted because it contains a colon:

```toml
[router.providers."openai-compat:my-vllm"]
base_url     = "http://localhost:8000/v1"
local        = true
display_name = "My vLLM"
models       = ["llama-3.1-70b"]

[router.providers."openai-compat:lmstudio"]
base_url = "http://localhost:1234/v1"
```

The router lists an endpoint's models from its `/v1/models` API when it offers
one; otherwise it advertises the `models` you list here. A model name still
routes even when it is not listed. The credential, if the endpoint needs one, is
set on the CLI side — see the `openai-compat:<name>` provider in
[Configuring the CLI](cli.md).

> [!NOTE]
> Model facts — capabilities, context window, costs, whether a model is local,
> and whether it requires authentication — are **not** configured here. The
> router obtains them from the provider at runtime and uses them when it selects
> a model. There is no model catalog in `router.toml`.

> [!NOTE]
> Ollama's endpoint and model discovery are configured separately, under
> `[router.ollama]` — see the next section.

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

The `[router.ollama]` table accepts:

| Key                              | Type    | Default                  | Purpose                                  |
| -------------------------------- | ------- | ------------------------ | ---------------------------------------- |
| `enabled`                        | bool    | `false`                  | Whether the Ollama backend is active.    |
| `base_url`                       | string  | `http://127.0.0.1:11434` | Ollama endpoint base URL.                |
| `auto_discover_models`           | bool    | `true`                   | Auto-discover installed models.          |
| `startup_healthcheck`            | bool    | `true`                   | Health-check Ollama at startup.          |
| `startup_required`               | bool    | `false`                  | Abort startup if the health-check fails. |
| `model_refresh_interval_seconds` | integer | `300`                    | Model-list refresh interval, in seconds. |

The `[router.ollama.limits]` table accepts:

| Key                       | Type    | Default  | Purpose                              |
| ------------------------- | ------- | -------- | ------------------------------------ |
| `max_concurrent_requests` | integer | `4`      | Maximum concurrent Ollama requests.  |
| `request_timeout_ms`      | integer | `180000` | Request timeout, in milliseconds.    |
| `pull_timeout_ms`         | integer | `600000` | Model pull timeout, in milliseconds. |

The `[router.ollama.models]` table accepts:

| Key              | Type            | Default | Purpose                                            |
| ---------------- | --------------- | ------- | -------------------------------------------------- |
| `preload`        | list of strings | `[]`    | Models pulled or warmed at startup.                |
| `allow_pull`     | bool            | `false` | Whether the router may pull models on demand.      |
| `allowed_models` | list of strings | `[]`    | Allowed models; empty means all discovered models. |

## Validation

Router configuration is validated at startup. Validation rejects an invalid host
or port, an unsafe public binding in local mode, missing or unsupported storage
configuration, local bootstrap enabled in remote mode, invalid timeouts or size
limits, a zero rate-limit period or burst while rate limiting is enabled, unsafe
CORS, and inline secrets. A failure prevents startup and explains the offending
field.
