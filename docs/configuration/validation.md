# Configuration validation

- [Configuration validation](#configuration-validation)
  - [CLI / policy configuration (`config.toml`)](#cli--policy-configuration-configtoml)
    - [Specificity](#specificity)
    - [Unsafe overrides and permission widening](#unsafe-overrides-and-permission-widening)
  - [Model selection (checked at run time)](#model-selection-checked-at-run-time)
  - [Router configuration (`router.toml`)](#router-configuration-routertoml)
    - [Example: correcting a zero timeout](#example-correcting-a-zero-timeout)
    - [Example: correcting unsafe CORS](#example-correcting-unsafe-cors)

smista.ai validates your configuration and reports every finding as either an
**error** or a **warning**. Errors make the configuration invalid and must be
fixed before smista can use it. Warnings are advisory — they surface potential
problems but do not block execution.

When smista validates your configuration it collects all findings in a single
pass and reports them together, so you can fix everything at once.

## CLI / policy configuration (`config.toml`)

These checks apply to the merged CLI and routing-policy configuration loaded
from your global, project, and local preference layers.

| Check                                                                                                                                                                        | Severity | How to fix                                                                                                                |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------- |
| **Unknown provider** — a routing rule, fallback, or default route references a provider identifier that is not enabled in `[providers]`                                      | Error    | Enable that provider with a `[providers.<id>]` table, or correct the reference                                            |
| **Invalid glob** — a pattern in `privacy.restricted_paths`, `privacy.remote.blocked_paths`, or a rule's `paths` list fails to compile                                        | Error    | Fix the glob syntax (e.g. close unclosed brackets)                                                                        |
| **Duplicate rule name** — two `[[routing.rules]]` entries share the same `name`                                                                                              | Error    | Give each rule a unique `name`                                                                                            |
| **Missing default route** — no `[routing.default]` table is present                                                                                                          | Error    | Add `[routing.default]` with a `model` field                                                                              |
| **Invalid fallback** — a rule lists its own `model` as a fallback, or lists the same fallback model more than once                                                           | Error    | Remove the self-reference or duplicate from `fallbacks`                                                                   |
| **Ambiguous rules** — two overlapping rules share the same `priority` value and the same specificity score, making their relative order undefined                            | Error    | Give them distinct `priority` values or make their match conditions mutually exclusive                                    |
| **Unsafe override** — a local or runtime preference layer sets `privacy.remote.mode` to a value less strict than the merged config-layer floor                               | Error    | Do not weaken the configured privacy mode in a preference layer                                                           |
| **Permission widening** — a local or runtime preference layer sets a tool permission (`file_read`, `file_write`, `shell`, etc.) to a less strict value than the config floor | Error    | Keep the configured stricter permission; preference layers may only tighten, never loosen                                 |
| **Inline secret** — a provider `api_key` is a literal string instead of a `${secret:NAME}` reference                                                                         | Error    | Replace the literal with `api_key = "${secret:NAME}"` and store the value in a secrets file or as an environment variable |

### Specificity

Two overlapping rules are considered ambiguous when they share both a
`priority` value and the same number of constrained match dimensions (intent,
path list, `local_only`). Rules with mutually exclusive match conditions, such
as different explicit intents, are not ambiguous. Give one overlapping rule a
lower priority number to resolve the ambiguity:

```toml
[[routing.rules]]
name = "review with claude"
priority = 10 # wins over the rule below
intent = "review"
model = "anthropic/claude-sonnet"

[[routing.rules]]
name = "review with gpt"
priority = 20 # distinct priority — no ambiguity
intent = "review"
model = "openai/gpt-5.5-mini"
```

### Unsafe overrides and permission widening

The merged global and project config layers set the safety floor. A
local-preferences or runtime-override layer may add restrictions — for example
switching a tool from `ask` to `deny` — but may not weaken a configured
restriction.

```toml
# project .smista/config.toml
[privacy.remote]
mode = "deny" # no remote sends

[tools.permissions]
shell = "ask" # shell requires approval
```

```toml
# local preferences — these are rejected
[privacy.remote]
mode = "allow" # error: unsafe override

[tools.permissions]
shell = "allow" # error: permission widening
```

## Model selection (checked at run time)

Two things the CLI does **not** check during configuration validation are
verified later, when the router selects a model for a task — against the facts
each provider exposes about its models, not against any config:

- **Model existence** — whether the `model` a rule selects, or any entry in its
  `fallbacks`, is actually offered by the provider.
- **Capability requirements** — whether a model satisfies a rule's
  `requires_capabilities`. This catches, for example, a rule that needs tool
  calls but falls back to a local model that cannot call tools.

```toml
[[routing.rules]]
name = "tool-using edits"
requires_capabilities = { tools = true }
model = "anthropic/claude-sonnet"
fallbacks = ["ollama/qwen2.5-coder"]
```

If `ollama/qwen2.5-coder` does not support tools, the router skips it when this
rule needs tool calls and falls through to the next viable option, rather than
failing configuration validation up front. Because model facts come from the
provider at run time, they are never declared in `config.toml`.

## Router configuration (`router.toml`)

These checks apply to the `smista-router` runtime configuration. All must hold
for a valid router configuration.

| Check                                                                                                                                        | Severity | How to fix                                                                                  |
| -------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------- |
| **Invalid port** — `router.port` is `0`                                                                                                      | Error    | Set a port in the range 1–65535                                                             |
| **Invalid host** — `router.host` is empty, contains whitespace, includes a port, or is not a valid hostname or IP address                    | Error    | Set a bare hostname or IP address (e.g. `127.0.0.1`)                                        |
| **Public bind in embedded mode** — `router.host` is `0.0.0.0` or `::` while `router.storage.mode = "embedded"`                               | Warning  | Bind to `127.0.0.1` unless you intentionally want to expose the local router to the network |
| **Missing storage path** — `router.storage.mode = "embedded"` but `router.storage.path` is not set                                           | Error    | Set `router.storage.path` to a writable directory (e.g. `.smista/db`)                       |
| **Missing storage URL** — `router.storage.mode = "remote"` but `router.storage.url` is not set                                               | Error    | Set `router.storage.url` to the SurrealDB WebSocket address (e.g. `ws://db:8000`)           |
| **Local bootstrap in remote mode** — `router.auth.local_bootstrap_enabled = true` while `router.storage.mode = "remote"`                     | Error    | Set `local_bootstrap_enabled = false`, or switch to `embedded` mode                         |
| **Zero timeout** — any of `router.limits.request_timeout_ms`, `router.limits.provider_timeout_ms`, or `router.limits.tool_timeout_ms` is `0` | Error    | Set a positive value in milliseconds                                                        |
| **Zero size limit** — `router.limits.max_request_body_bytes` or `router.limits.max_context_bytes` is `0`                                     | Error    | Set a positive byte limit                                                                   |
| **Excessive timeout** — a timeout exceeds 3,600,000 ms (1 hour)                                                                              | Warning  | Check that the value is in milliseconds, not seconds; lower it if so                        |
| **Unsafe CORS** — CORS is enabled (`router.cors.enabled = true`) but `router.cors.allowed_origins` is empty or contains `*`                  | Error    | List explicit origins: `allowed_origins = ["https://app.example.com"]`                      |

### Example: correcting a zero timeout

```toml
[router.limits]
# Error: request_timeout_ms = 0
# Fix:
request_timeout_ms = 120000 # 2 minutes
provider_timeout_ms = 180000 # 3 minutes
tool_timeout_ms = 60000 # 1 minute
```

### Example: correcting unsafe CORS

```toml
[router.cors]
enabled = true
# Error: allowed_origins = ["*"]
# Fix:
allowed_origins = ["https://app.example.com"]
```
