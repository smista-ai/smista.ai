# Configure Routing and the CLI

- [Configure Routing and the CLI](#configure-routing-and-the-cli)
  - [Know which file to edit](#know-which-file-to-edit)
  - [Create and inspect configuration](#create-and-inspect-configuration)
  - [Where configuration lives](#where-configuration-lives)
  - [Configuration precedence](#configuration-precedence)
  - [Configure a remote provider](#configure-a-remote-provider)
  - [Built-in default configuration](#built-in-default-configuration)
  - [How a request becomes a route](#how-a-request-becomes-a-route)
  - [Task intents](#task-intents)
  - [Routing rules](#routing-rules)
    - [Rule fields](#rule-fields)
    - [Match semantics](#match-semantics)
    - [Effort](#effort)
    - [Rule precedence](#rule-precedence)
    - [Default route](#default-route)
  - [Privacy](#privacy)
  - [Tool permissions](#tool-permissions)
  - [Providers and models](#providers-and-models)
  - [Provider credentials](#provider-credentials)
  - [Connecting to the router](#connecting-to-the-router)
  - [Local preferences](#local-preferences)
  - [Validation](#validation)

smista.ai reads TOML configuration to decide which model handles each task, what
context may be sent where, and which tool calls require approval. Configuration
is deterministic, versionable and inspectable — routing never depends on an LLM.

## Know which file to edit

smista uses two configuration files. They have different jobs:

| File          | Purpose                                                               |
| ------------- | --------------------------------------------------------------------- |
| `config.toml` | Providers, routing policy, privacy, tools, and the CLI connection.    |
| `router.toml` | Router process settings, storage, limits, and provider endpoint URLs. |

This page describes `config.toml`. Use it when you want to choose models or
change project policy. See [Configure the Router](router.md) when you need to
change how the router process runs.

Both files contain a `[router]` table, but the meaning is different:

- `[router]` in `config.toml` tells the CLI where to find the router.
- `[router]` in `router.toml` configures the router server itself.

## Create and inspect configuration

Create a project configuration from the built-in defaults:

```sh
smista config init
```

Use the configuration commands to find, edit, inspect, and check the file:

```sh
smista config path project
smista config edit project
smista config show project
smista config check project
```

These commands use the project configuration by default. Add the `global`
argument to work with the global configuration instead:

```sh
smista config path global
smista config edit global
smista config show global
smista config check global
```

`smista config show` displays the effective configuration after all layers are
merged. Sensitive values are redacted.

## Where configuration lives

| Layer                | Location                            | Scope                  |
| -------------------- | ----------------------------------- | ---------------------- |
| Global (Linux/macOS) | `~/.config/smista/config.toml`      | All projects           |
| Global (Windows)     | `%USERPROFILE%\.smista\config.toml` | All projects           |
| Project              | `.smista/config.toml`               | The current repository |

Create the global file with `smista config init global`. Create the project
file with `smista config init` or `smista config init project`.

Project configuration is safe to commit when it contains no secrets. This lets
a team share one routing and safety policy.

## Configuration precedence

smista merges configuration from lowest to highest precedence:

1. Built-in defaults
2. Global configuration
3. Project configuration
4. Runtime command overrides, such as `/model`

Higher layers keep values that lower layers set unless they replace them. Some
safety settings are stricter: a runtime preference may add a restriction, but
it cannot weaken a privacy or tool rule from configuration.

## Configure a remote provider

Enable a provider and choose a default model before you start using smista. For
example:

```toml
[providers.openai]
type = "openai"

[routing.default]
model = "openai/gpt-5.5-mini"
```

Store the provider API key outside the configuration file:

```sh
smista credentials set openai YOUR_API_KEY
```

Credentials are project-local by default. Use
`smista credentials --global set openai YOUR_API_KEY` to share the credential
across your local projects. This does not put the key in `config.toml`.

## Built-in default configuration

With no global or project `config.toml`, the CLI starts from a valid built-in
configuration:

```toml
[providers.ollama]
type = "ollama"

[routing.default]
model = "ollama/qwen2.5-coder:7b"
```

This default is keyless and passes configuration validation on its own. Model
availability is still checked later by the router, so using it requires an
Ollama endpoint that can serve `qwen2.5-coder`.

If you keep the built-in default route, keep `providers.ollama` enabled. If you
replace the provider set with only remote providers, also replace
`[routing.default]` with a model from one of those providers.

The router disables its Ollama connection by default. To use this built-in
route, enable `[router.ollama]` in `router.toml` and make sure Ollama can serve
the model. See [Use Local Models with Ollama](ollama.md).

## How a request becomes a route

smista evaluates each request in a fixed order:

```text
request -> classification -> routing -> privacy and tools -> model
```

Classification assigns a task intent such as `plan`, `edit`, or `review`.
Routing rules match that intent and any relevant file paths. Privacy and tool
rules then limit the context and actions available to the selected model.

The following sections describe each part in detail.

## Task intents

An intent is the kind of work a request asks for. smista.ai classifies each
request into one of a fixed set of intents, then routes it with the rules you
wrote for that intent. Classification is deterministic — ordered rules, never an
LLM.

| Intent      | The request asks smista to…                        | Typical prompt                                     |
| ----------- | -------------------------------------------------- | -------------------------------------------------- |
| `chat`      | Talk, answer a question, explain something.        | "How does the retry backoff work?"                 |
| `plan`      | Think a task through and break it into steps.      | "Plan the migration to the new storage backend."   |
| `edit`      | Change code or text.                               | "Fix the off-by-one in the pagination cursor."     |
| `review`    | Judge existing code or text and give feedback.     | "Review this diff for security problems."          |
| `summarize` | Condense long content into a shorter form.         | "Summarize this 400-line changelog."               |
| `prompt`    | Write or improve a prompt meant for another model. | "Rewrite this system prompt to be less ambiguous." |
| `skill`     | Run a named skill or tool-driven capability.       | "Run the changelog skill for release 1.4."         |

Intents are a routing signal, nothing more: they exist so you can send cheap work
to a cheap model and hard work to a strong one. A common starting policy sends
`plan` and `review` to a reasoning model, `summarize` and `chat` to a small or
local one, and leaves `edit` on your everyday coding model.

`chat` is the default, used when no rule matches. For how the classifier picks
an intent — signals, rule order, confidence — see
[Task intent classification](../technical/task-classification.md).

```toml
[[classification.rules]]
intent = "review"
priority = 10
keywords = ["review", "audit", "check", "inspect"]
requires_any_context = ["git_diff", "pull_request"]

[[classification.rules]]
intent = "edit"
priority = 20
keywords = ["change", "modify", "refactor", "fix", "implement"]

[classification]
default_intent = "chat"
```

Lower `priority` wins. API clients may also send an explicit task intent in the
request. An explicit intent always beats automatic classification, even when
the prompt text would match another rule.

Each `[[classification.rules]]` entry accepts:

| Key                    | Type            | Default  | Purpose                                                       |
| ---------------------- | --------------- | -------- | ------------------------------------------------------------- |
| `intent`               | string          | required | Intent assigned when the rule matches.                        |
| `priority`             | integer         | `1000`   | Lower value wins.                                             |
| `keywords`             | list of strings | `[]`     | Rule matches when any keyword appears in the prompt.          |
| `requires_any_context` | list of strings | `[]`     | Rule matches when any named context is present (`git_diff`…). |

The `[classification]` table itself accepts:

| Key              | Type   | Default | Purpose                           |
| ---------------- | ------ | ------- | --------------------------------- |
| `default_intent` | string | `chat`  | Intent used when no rule matches. |

## Routing rules

A routing rule decides which model handles a task. Rules match on intent, file
path, or a combination, and may declare a fallback chain.

```toml
[[routing.rules]]
name = "plan with strongest reasoning model"
priority = 10
intent = "plan"
model = "openai/gpt-5.5-thinking"
fallbacks = ["anthropic/claude-sonnet"]

[[routing.rules]]
name = "summarize on a local model"
priority = 20
intent = "summarize"
model = "ollama/qwen2.5-coder:7b"
fallbacks = ["openai/gpt-5.5-mini"]

[[routing.rules]]
name = "auth code uses Claude"
priority = 30
intent = "edit"
paths = ["src/auth/**"]
model = "anthropic/claude-sonnet"
fallbacks = ["openai/gpt-5.5-thinking"]

[[routing.rules]]
name = "review security-sensitive code locally"
priority = 5
effort = "low"
intent = "review"
paths = ["src/crypto/**", "src/auth/**"]
local_only = true
model = "ollama/qwen2.5-coder:7b"
```

### Rule fields

Every routing rule supports the keys below. Match conditions are all optional; a
rule with none matches every task.

| Key                     | Type            | Purpose                                                                                               |
| ----------------------- | --------------- | ----------------------------------------------------------------------------------------------------- |
| `name`                  | string          | Human-readable rule name (required).                                                                  |
| `priority`              | integer         | Lower value wins; defaults to `1000`.                                                                 |
| `effort`                | string          | Reasoning effort for the matched model; defaults to `medium`.                                         |
| `intent`                | string          | Match only this task intent.                                                                          |
| `paths`                 | list of globs   | Match when a relevant path matches any glob.                                                          |
| `local_only`            | bool            | Pin the route to local models; an `ollama/` model resolves to the local instance, never Ollama Cloud. |
| `requires_capabilities` | table           | Capability gate; the model must satisfy each `true` flag.                                             |
| `model`                 | string          | Model selected on match, as `provider/model` (required).                                              |
| `fallbacks`             | list of strings | Models tried in order when the selected model is unavailable.                                         |
| `required_permissions`  | table           | Tool permissions the route requires (see below).                                                      |
| `cost_limit`            | string          | Per-task cost ceiling, as a decimal string (e.g. `"0.50"`).                                           |

`requires_capabilities` gates a rule on what the model can do. Each flag defaults
to `false`; set the ones a matched model must support: `streaming`, `tools`,
`json_output`, `system_prompt`, `images`, `reasoning`, `memory`.

`required_permissions` declares the [tool permissions](#tool-permissions) the
matched route needs. It is merged over the project defaults and may only _narrow_
them — see [Tool permissions](#tool-permissions).

`cost_limit` is written as a quoted decimal string for exact precision (it is
never a floating-point number).

```toml
[[routing.rules]]
name = "deep remote review of crypto"
priority = 5
intent = "review"
paths = ["src/crypto/**"]
requires_capabilities = { reasoning = true, tools = true }
required_permissions = { permissions = { shell = "deny", network = "ask" } }
cost_limit = "0.50"
model = "anthropic/claude-sonnet"
fallbacks = ["openai/gpt-5.5-thinking"]
```

### Match semantics

- Fields **within one rule** combine with **AND** — every defined field must
  match.
- Values **within one field** combine with **OR** — `paths = ["src/auth/**",
  "src/security/**"]` matches either path.
- Undefined fields are ignored.

### Effort

Each rule may set an `effort`, telling the matched model how much reasoning
effort to spend on the task. Accepted values, from least to most:

- `low`
- `medium`
- `high`
- `xhigh`

When omitted, a rule defaults to `medium`.

```toml
[[routing.rules]]
name = "plan with maximum reasoning"
priority = 10
effort = "xhigh"
intent = "plan"
model = "openai/gpt-5.5-thinking"
```

### Rule precedence

When several rules match, exactly one is chosen, in this order:

1. Explicit model override
2. Lower `priority` value (1 is higher priority than 10)
3. More specific rule

Specificity, most to least specific:

```txt
path + intent
> path
> intent
> default
```

If two overlapping rules share the same priority and specificity, validation
fails. Give the rules different priorities or make their match conditions
mutually exclusive.

### Default route

A policy must define a default route, used when no rule matches:

```toml
[routing.default]
model = "openai/gpt-5.5-mini"
fallbacks = ["ollama/qwen2.5-coder:7b"]
```

When no route is authored, the built-in default route is
`ollama/qwen2.5-coder:7b`.

## Privacy

Privacy policies control which context may reach which model class. Restricted
files are never sent to remote models unless the policy allows it _and_ the user
approves.

```toml
[privacy]
restricted_paths = [".env", "secrets/**", "*.pem", "*.key"]

[privacy.remote]
mode = "ask"
blocked_paths = [".env", "secrets/**"]

[privacy.local]
mode = "allow"
```

The `[privacy]` table accepts:

| Key                | Type          | Default | Purpose                                           |
| ------------------ | ------------- | ------- | ------------------------------------------------- |
| `restricted_paths` | list of globs | `[]`    | Paths treated as sensitive for every model class. |

The `[privacy.remote]` table controls disclosure to remote providers:

| Key             | Type          | Default | Purpose                                            |
| --------------- | ------------- | ------- | -------------------------------------------------- |
| `mode`          | string        | `ask`   | `allow`, `ask`, or `deny` for remote disclosure.   |
| `blocked_paths` | list of globs | `[]`    | Paths that must never be sent to remote providers. |

The `[privacy.local]` table controls disclosure to local models:

| Key    | Type   | Default | Purpose                                         |
| ------ | ------ | ------- | ----------------------------------------------- |
| `mode` | string | `allow` | `allow`, `ask`, or `deny` for local disclosure. |

## Tool permissions

Tool permissions define what models may request and what needs approval. Modes
are `allow`, `ask`, `deny`.

```toml
[tools.permissions]
file_read = "allow"
file_write = "ask"
shell = "ask"
network = "deny"
git = "allow"
```

Rule-specific permissions (a rule's `required_permissions`) may _narrow_ these
defaults, tightening a tool from `allow` to `ask` to `deny`, or
adding a tool not listed in the defaults. They may never _widen_ them: an
override that loosens a stricter mode (for example setting `shell = "allow"` when
the project default is `shell = "deny"`) is a configuration error naming the
offending tool, not a silent override.

`[tools.permissions]` is a map of tool name to mode; a tool with no entry has no
configured mode and falls back to the safe default. Each value is one of:

| Mode    | Effect                                     |
| ------- | ------------------------------------------ |
| `allow` | The tool runs without confirmation.        |
| `ask`   | The user is prompted before the tool runs. |
| `deny`  | The tool is blocked.                       |

The conventional tool keys are:

| Tool         | Governs                           |
| ------------ | --------------------------------- |
| `file_read`  | Reading files from the workspace. |
| `file_write` | Writing or modifying files.       |
| `shell`      | Running shell commands.           |
| `network`    | Outbound network access.          |
| `git`        | Git operations (commit, push, …). |

## Providers and models

The CLI configuration declares which providers are enabled and how tasks route
to them. Provider credentials stay outside the file. The configuration does
**not** describe individual models. Reference a model with `provider/model`
syntax, for example `anthropic/claude-sonnet`.

The part before the first `/` is the provider identifier (case-insensitive);
everything after it is the model name, which may itself contain `/` (e.g.
`ollama/library/llama3`). Both parts must be non-empty, and the provider must be
one of the identifiers below — otherwise the reference is rejected during
validation.

A generic OpenAI-compatible endpoint (a local vLLM or LM Studio server, a
gateway, …) is a _named instance_, and its identifier takes the form
`openai-compat:<name>` — for example `openai-compat:my-vllm/llama-3.1-70b`.
Instance names use lowercase letters, digits, `-` and `_`. You can configure as
many such instances as you like, each with its own name.

Enable a provider by adding a `[providers.<id>]` table, keyed by the provider
identity. Store remote provider keys with `smista credentials set`:

```toml
[providers.openai]
type = "openai"

[providers.anthropic]
type = "anthropic"

[providers.gemini]
type = "gemini"

[providers.ollama]
type = "ollama"

# A named OpenAI-compatible instance. The key is its full identity, quoted
# because it contains a colon; `type` is omitted.
[providers."openai-compat:my-vllm"]
```

The optional `type` field names the provider backend. When present it is
case-insensitive and must be one of the supported identifiers:

| Identifier             | Backend                                                                                  |
| ---------------------- | ---------------------------------------------------------------------------------------- |
| `anthropic`            | Anthropic, serving the Claude models.                                                    |
| `gemini`               | Google Gemini, serving the Gemini models.                                                |
| `openai`               | OpenAI, serving the GPT models.                                                          |
| `ollama`               | Ollama, serving local models.                                                            |
| `openai-compat:<name>` | A generic OpenAI-compatible endpoint; normally set by the table key with `type` omitted. |

An unknown identifier is rejected during validation. The endpoint URL for a
named instance lives on the router — see [Configure the Router](router.md).

Each `[providers.<id>]` table accepts:

| Key       | Type   | Default | Purpose                                                                                                 |
| --------- | ------ | ------- | ------------------------------------------------------------------------------------------------------- |
| `type`    | string | none    | Provider kind. Optional and redundant with the table key; omit it for `openai-compat:<name>` instances. |
| `api_key` | string | none    | Optional `${secret:NAME}` reference. Prefer `smista credentials set`.                                   |

> [!NOTE]
> A model's facts — its capabilities, context window, costs, whether it runs
> locally, and whether it needs authentication — are **not** declared here. They
> come from the provider at runtime. A rule's `requires_capabilities` is a
> _requirement_ you state; the router checks it against those facts when it
> selects a model. Endpoint overrides (such as a custom OpenAI-compatible URL)
> belong to the router — see [Configure the Router](router.md).

> [!NOTE]
> The Anthropic model list is fetched live from Anthropic, so new Claude models
> become available without updating smista.ai. The list is refreshed about once
> an hour. Anthropic does not report prices over its API, so per-token costs are
> maintained inside smista.ai by model family (Opus, Sonnet, Haiku, Fable); for
> a model from an unrecognised family the cost is reported as unknown.

> [!NOTE]
> The Gemini model list is fetched live from Google, so new Gemini models become
> available without updating smista.ai. The list is filtered to the chat models
> smista.ai routes to — Google's embedding, image, audio and other specialised
> models are left out — and refreshed about once an hour. Google does not report
> prices over its API, so per-token costs are maintained inside smista.ai per
> model; a model with no recorded price has its cost reported as unknown.

> [!NOTE]
> For local models through Ollama, see
> [Use Local Models with Ollama](ollama.md).

> [!WARNING]
> An `ollama/<model>` reference is **not** treated as local by default. Unless
> the matched route enforces `local_only` — a rule's `local_only = true` or the
> [`local_only`](#local-preferences) local preference — the router may resolve it
> against the **public Ollama Cloud**, not your local instance. That is a remote,
> billed, non-private endpoint, and your [`[privacy.remote]`](#privacy) rules
> apply to it like any other remote provider. Set `local_only` whenever a rule
> must run on the local Ollama instance.

## Provider credentials

Never write an API key directly into `config.toml`. The simplest way to store a
key is the credential command:

```sh
smista credentials set openai YOUR_API_KEY
smista credentials check openai
```

The CLI first tries the operating-system keyring. When no keyring is available,
it uses a protected local file. Credentials are project-local by default, so a
key stored in one project is not automatically available to another project.
Add `--global` to store it for every local project:

```sh
smista credentials --global set anthropic YOUR_API_KEY
```

For automation, a provider may instead use an environment variable through a
`${secret:NAME}` reference:

```toml
[providers.openai]
type = "openai"
api_key = "${secret:OPENAI_API_KEY}"
```

Here, smista first reads the `OPENAI_API_KEY` environment variable. If the
variable is absent, it checks credential storage for the same name. An
unresolved reference leaves the provider without a usable credential. Messages
may name the missing key, but they never print a secret value.

`smista config init` adds the project secrets file to `.smista/.gitignore`.
Use the credential commands instead of editing managed secret files by hand.

## Connecting to the router

The CLI needs to know where the router is and how to authenticate to it. This is
client configuration, separate from the router's own runtime config (see
[Configure the Router](router.md)).

```toml
[router]
url = "http://127.0.0.1:7331"
auto_start = true
connect_timeout_ms = 5000
request_timeout_ms = 120000
auth_source = "keychain"
```

The `[router]` table accepts:

| Key                  | Type    | Default    | Purpose                                                                    |
| -------------------- | ------- | ---------- | -------------------------------------------------------------------------- |
| `url`                | string  | none       | Router base URL, e.g. `http://127.0.0.1:7331`.                             |
| `auto_start`         | bool    | `false`    | Start a local router when none is reachable.                               |
| `connect_timeout_ms` | integer | none       | Connection timeout in milliseconds.                                        |
| `request_timeout_ms` | integer | none       | Request timeout in milliseconds.                                           |
| `auth_source`        | string  | `keychain` | Where the auth credential is read: `keychain`, `env`, `file`, or `helper`. |

## Local preferences

Local preferences tune your own experience. They live under
`[local_preferences]` in a global or project `config.toml`. Every field is
optional; an unset field keeps the value from a lower configuration layer.

Put personal preferences in the global file when you do not want to share them.
If you put them in the project file, they are part of the project configuration
and may be committed with it.

```toml
[local_preferences]
auto_apply = false
local_only = false
no_network = false
encrypt_sessions = true
```

| Field              | Effect                                                                                        |
| ------------------ | --------------------------------------------------------------------------------------------- |
| `auto_apply`       | Apply file writes without prompting for each diff.                                            |
| `local_only`       | Use only local models this session; pins `ollama/` to the local instance, never Ollama Cloud. |
| `no_network`       | Forbid network access for this session.                                                       |
| `encrypt_sessions` | Create new sessions as end-to-end encrypted. Defaults to `true`; set `false` to opt out.      |

> [!IMPORTANT]
> Local preferences may tighten safety, never loosen it. Enabling `local_only`
> or `no_network` here adds a restriction, but a preference can never weaken a
> project's privacy modes or a tool set to `deny`.

## Validation

Configuration is validated before execution. Validation rejects routing rules
that reference a provider you have not enabled, unknown intents, invalid globs,
duplicate rule names, a missing default route, invalid fallback
references, ambiguous rules, invalid permission values, and secrets stored
inline where forbidden. Whether a model exists and whether it satisfies a rule's
`requires_capabilities` is checked later, at model selection time, against the
facts the provider exposes — not from this file. Invalid configuration produces
an actionable error — run `smista config check project` to check it. See
[Configuration validation](validation.md) for the full list.
