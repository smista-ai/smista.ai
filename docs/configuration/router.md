# Running the Router

- [Running the Router](#running-the-router)
  - [Where configuration lives](#where-configuration-lives)
  - [A complete example](#a-complete-example)
  - [Server](#server)
  - [Starting and stopping](#starting-and-stopping)
  - [Storage](#storage)
  - [Authentication](#authentication)
  - [Runtime limits](#runtime-limits)
  - [Rate limiting](#rate-limiting)
  - [Logging](#logging)
  - [OpenTelemetry](#opentelemetry)
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

[router.opentelemetry]
enabled = false
endpoint = "http://localhost:4317"
protocol = "grpc"
service_name = "smista-router"
sample_ratio = 1.0

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

## Starting and stopping

smista.ai ships as a single binary, so the router runs inside the `smista`
process rather than as a separate program. Start a local router with the CLI:

```sh
smista start
```

By default this daemonizes: the router starts in the background and the command
returns. It records its process id in a pidfile, and refuses to start a second
router on top of one that is already running. Stop it again with:

```sh
smista stop
```

Pass `--foreground` to run the router in the current process instead, which is
what a service manager (systemd, launchd) wants:

```sh
smista start --foreground
```

The pidfile defaults to a per-user location under the runtime directory. Set a
different path with `--pidfile <path>` on both `start` and `stop`, or with the
`SMISTA_ROUTER_PIDFILE` environment variable.

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

## OpenTelemetry

The router can export its traces to an OpenTelemetry collector you already run,
so you can watch timings and errors in your own dashboards. This is layered on
top of the existing logging and changes nothing about how the router behaves or
what it decides. It is disabled by default, and when disabled the router exports
nothing and does no extra work.

Only span and trace metadata is exported. Secrets are never recorded on traces,
so API keys, provider credentials and auth tokens are never sent to the
collector.

Turn it on and point it at your collector:

```toml
[router.opentelemetry]
enabled = true
endpoint = "http://localhost:4317"
protocol = "grpc"
service_name = "smista-router"
sample_ratio = 1.0
```

The `[router.opentelemetry]` table accepts:

| Key            | Type   | Default                 | Purpose                                                             |
| -------------- | ------ | ----------------------- | ------------------------------------------------------------------- |
| `enabled`      | bool   | `false`                 | Whether trace export is enabled.                                    |
| `endpoint`     | string | `http://localhost:4317` | Collector endpoint to export to.                                    |
| `protocol`     | string | `grpc`                  | Wire protocol: `grpc` (port `4317`) or `http-binary` (port `4318`). |
| `service_name` | string | `smista-router`         | Service name reported on every trace.                               |
| `sample_ratio` | float  | `1.0`                   | Fraction of traces to sample, from `0.0` (none) to `1.0` (all).     |

You can also set the most common options when starting the router, without
editing the file. A value given on the command line wins over the file:

```sh
smista start \
  --otel \
  --otel-endpoint http://localhost:4317 \
  --otel-protocol grpc \
  --otel-service-name smista-router \
  --otel-sample-ratio 1.0
```

`--otel` turns export on and `--no-otel` turns it off, each overriding the
file. The remaining flags map one to one to the table above.

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

| Key            | Type           | Default  | Purpose                                                                                                                                                                                                              |
| -------------- | -------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `base_url`     | string         | provider | Endpoint base URL. Honored only by `openai-compat:<name>` endpoints and the Ollama cloud endpoint; the built-in API providers (OpenAI, Anthropic, Gemini) use their fixed endpoint and ignore it (validation warns). |
| `local`        | bool           | `false`  | Whether the provider runs locally; surfaced to consumers to tell local providers from hosted ones.                                                                                                                   |
| `display_name` | string         | none     | Human-readable name for the provider; consumers fall back to the provider identifier when omitted.                                                                                                                   |
| `models`       | list of tables | `[]`     | Models advertised for an `openai-compat:<name>` endpoint with no model-listing API; each entry declares one model and its facts, see below. Ignored by the built-in providers.                                       |

### Generic OpenAI-compatible endpoints

You can point the router at any number of OpenAI-compatible endpoints — a local
vLLM or LM Studio server, llama.cpp, or a hosted gateway. Each one is a _named
instance_: you choose a name, and routing rules address it as
`openai-compat:<name>/<model>` (for example
`openai-compat:my-vllm/llama-3.1-70b`).

Configure each instance under its full identity as the table key. The name is
quoted because it contains a colon. Each model is its own `[[...models]]` table:

```toml
[router.providers."openai-compat:my-vllm"]
base_url = "http://localhost:8000/v1"
local = true
display_name = "My vLLM"

[[router.providers."openai-compat:my-vllm".models]]
name = "llama-3.1-70b"
max_context_tokens = 131072
max_output_tokens = 8192

[router.providers."openai-compat:lmstudio"]
base_url = "http://localhost:1234/v1"

[[router.providers."openai-compat:lmstudio".models]]
name = "qwen2.5-coder-7b"
max_context_tokens = 32768
input_cost_per_million_tokens = "0.0"
output_cost_per_million_tokens = "0.0"
```

These endpoints expose no catalog of model facts, so routing reads each model's
facts from the `[[...models]]` tables you declare. The credential, if the
endpoint needs one, is set on the CLI side — see the `openai-compat:<name>`
provider in [Configuring the CLI](cli.md).

Each `[[router.providers.<id>.models]]` table accepts:

| Key                              | Type    | Default     | Purpose                                                                        |
| -------------------------------- | ------- | ----------- | ------------------------------------------------------------------------------ |
| `name`                           | string  | required    | Model name, exactly as the endpoint expects it.                                |
| `max_context_tokens`             | integer | required    | Maximum context window the model accepts, in tokens.                           |
| `display_name`                   | string  | none        | Human-readable name; consumers fall back to `name` when omitted.               |
| `auth`                           | string  | `none`      | How the model authenticates: `none`, `api_key`, or `optional_api_key`.         |
| `max_output_tokens`              | integer | none        | Maximum tokens the model emits, if bounded.                                    |
| `capabilities`                   | table   | OpenAI-like | What the model can do; see below. Defaults suit an OpenAI-compatible endpoint. |
| `input_cost_per_million_tokens`  | string  | none        | Input price per million tokens, as a decimal string for exact precision.       |
| `output_cost_per_million_tokens` | string  | none        | Output price per million tokens, as a decimal string for exact precision.      |

The `capabilities` table defaults to `streaming`, `tools`, `json_output`,
`system_prompt` and `memory` enabled, with `images` and `reasoning` disabled.
Override any of them per model:

```toml
[[router.providers."openai-compat:my-vllm".models]]
name = "llava-1.6"
max_context_tokens = 32768

[router.providers."openai-compat:my-vllm".models.capabilities]
images = true
```

> [!NOTE]
> The built-in providers (OpenAI, Anthropic, Gemini) and Ollama discover their
> model facts at runtime and ignore the `models` tables. Only generic
> `openai-compat:<name>` endpoints, which publish no catalog, read facts from
> here.

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
```

The `[router.ollama]` table accepts:

| Key        | Type   | Default                  | Purpose                               |
| ---------- | ------ | ------------------------ | ------------------------------------- |
| `enabled`  | bool   | `false`                  | Whether the Ollama backend is active. |
| `base_url` | string | `http://127.0.0.1:11434` | Ollama endpoint base URL.             |

## Validation

Router configuration is validated at startup. Validation rejects an invalid host
or port, an unsafe public binding in local mode, missing or unsupported storage
configuration, local bootstrap enabled in remote mode, invalid timeouts or size
limits, a zero rate-limit period or burst while rate limiting is enabled, unsafe
CORS, and inline secrets. A failure prevents startup and explains the offending
field.

Validation also emits non-blocking warnings, such as setting a `base_url` on a
built-in API provider that ignores it. Warnings are surfaced but do not prevent
startup.
