# Configuring the CLI

smista.ai reads TOML configuration to decide which model handles each task, what
context may be sent where, and which tool calls require approval. Configuration
is deterministic, versionable and inspectable — routing never depends on an LLM.

## Where configuration lives

| Layer            | Location                             | Scope                  |
| ---------------- | ------------------------------------ | ---------------------- |
| Global (POSIX)   | `~/.config/smista/config.toml`       | All projects           |
| Global (Windows) | `C:\Users\$USER\.smista\config.toml` | All projects           |
| Project          | `.smista/config.toml`                | The current repository |

Run `smista init` to scaffold `.smista/config.toml` in a project. Project
configuration is safe to commit when it contains no secrets, so a team shares
one routing policy.

## Precedence

Layers merge from least to most specific. Highest precedence wins:

1. Runtime command override (e.g. `/model`)
2. Local uncommitted preferences
3. Project configuration (`.smista/config.toml`)
4. Global user configuration (`~/.config/smista/config.toml`)
5. System defaults

Local always overrides global; values the local layer does not set are kept.

> [!IMPORTANT]
> Safety policies may be non-overridable. If a project forbids sending
> `secrets/**` to remote models, a local preference cannot silently bypass it.

## Providers and models

Providers and models are configured separately from routing rules. Reference a
model anywhere with `provider/model` syntax (e.g. `anthropic/claude-sonnet`).

The part before the first `/` is the provider identifier (case-insensitive);
everything after it is the model name, which may itself contain `/` (e.g.
`ollama/library/llama3`). Both parts must be non-empty, and the provider must be
one of the identifiers below — otherwise the reference is rejected during
validation.

```toml
[providers.openai]
type = "openai"
base_url = "https://api.openai.com/v1"
api_key = "${secret:OPENAI_API_KEY}"

[providers.anthropic]
type = "anthropic"
api_key = "${secret:ANTHROPIC_API_KEY}"

[providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"
```

The `type` field selects the provider backend. It is case-insensitive and must
be one of the supported identifiers:

| Identifier  | Backend                               |
| ----------- | ------------------------------------- |
| `anthropic` | Anthropic, serving the Claude models. |
| `openai`    | OpenAI, serving the GPT models.       |
| `ollama`    | Ollama, serving local models.         |

An unknown identifier is rejected during validation.

Each `[providers.<id>]` table accepts:

| Key        | Type   | Default  | Purpose                                              |
| ---------- | ------ | -------- | ---------------------------------------------------- |
| `type`     | string | required | Provider kind: `anthropic`, `openai`, or `ollama`.   |
| `base_url` | string | provider | Endpoint base URL; omit to use the provider default. |
| `api_key`  | string | none     | A `${secret:NAME}` reference resolving the API key.  |

Models declare their capabilities, which the router validates before execution
(for example, a task needing tools is never routed to a model without tool
support unless the policy allows degraded execution):

```toml
[models."openai/gpt-5.5-thinking"]
provider = "openai"
name = "gpt-5.5-thinking"
requires_api_key = true
local = false
supports_streaming = true
supports_tools = true
supports_json_output = true
supports_reasoning = true
max_context_tokens = 200000

[models."ollama/qwen2.5-coder"]
provider = "ollama"
name = "qwen2.5-coder"
requires_api_key = false
local = true
supports_streaming = true
supports_tools = false
max_context_tokens = 32768
```

Each `[models."provider/model"]` table accepts:

| Key                    | Type    | Default  | Purpose                                        |
| ---------------------- | ------- | -------- | ---------------------------------------------- |
| `provider`             | string  | required | Provider that offers the model.                |
| `name`                 | string  | required | Model name as defined by the provider.         |
| `requires_api_key`     | bool    | `false`  | Whether the model needs an API key.            |
| `local`                | bool    | `false`  | Whether the model runs locally.                |
| `supports_streaming`   | bool    | `false`  | Whether the model can stream responses.        |
| `supports_tools`       | bool    | `false`  | Whether the model can call tools.              |
| `supports_json_output` | bool    | `false`  | Whether the model can emit structured JSON.    |
| `supports_reasoning`   | bool    | `false`  | Whether the model performs explicit reasoning. |
| `max_context_tokens`   | integer | required | Maximum context tokens the model accepts.      |

> [!NOTE]
> For local models through Ollama, see
> [Using Local Models with Ollama](ollama.md).

## Provider credentials

Never write an API key directly into `config.toml`. A provider reads its key
from `api_key` using the `${secret:NAME}` reference form:

```toml
[providers.openai]
type = "openai"
api_key = "${secret:OPENAI_API_KEY}"

[providers.anthropic]
type = "anthropic"
api_key = "${secret:ANTHROPIC_API_KEY}"
```

A `${secret:NAME}` reference is resolved against the following sources, from
highest to lowest precedence:

1. An environment variable named `NAME` (e.g. set `OPENAI_API_KEY` in your
   shell).
2. The `.smista/secrets` file. The project file (`.smista/secrets` in the
   current directory) overrides the global file (`~/.smista/secrets`).

The first source that provides the key wins. A reference that resolves nowhere
is an error, and the message names the missing key and the field that referenced
it — never a secret value.

The `.smista/secrets` file uses a dotenv-style format: one `NAME=value` pair per
line, without quotes. Lines starting with `#` are comments. Keep this file out
of version control.

```dotenv
# .smista/secrets
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
```

## Routing rules

A routing rule decides which model handles a task. Rules match on intent, skill,
file path, or a combination, and may declare a fallback chain.

```toml
[[routing.rules]]
name = "plan with strongest reasoning model"
priority = 10
intent = "plan"
model = "openai/gpt-5.5-thinking"
fallbacks = ["anthropic/claude-sonnet"]

[[routing.rules]]
name = "use local model for changelog skill"
priority = 20
skill = "changelog"
model = "ollama/qwen2.5-coder"
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
model = "ollama/qwen2.5-coder"
```

### Rule fields

Every routing rule supports the keys below. Match conditions are all optional; a
rule with none matches every task.

| Key                     | Type            | Purpose                                                       |
| ----------------------- | --------------- | ------------------------------------------------------------- |
| `name`                  | string          | Human-readable rule name (required).                          |
| `priority`              | integer         | Lower value wins; defaults to `1000`.                         |
| `effort`                | string          | Reasoning effort for the matched model; defaults to `medium`. |
| `intent`                | string          | Match only this task intent.                                  |
| `skill`                 | string          | Match only this invoked skill.                                |
| `paths`                 | list of globs   | Match when a relevant path matches any glob.                  |
| `local_only`            | bool            | Restrict the fallback chain to local models.                  |
| `requires_capabilities` | table           | Capability gate; the model must satisfy each `true` flag.     |
| `model`                 | string          | Model selected on match, as `provider/model` (required).      |
| `fallbacks`             | list of strings | Models tried in order when the selected model is unavailable. |
| `required_permissions`  | table           | Tool permissions the route requires (see below).              |
| `cost_limit`            | string          | Per-task cost ceiling, as a decimal string (e.g. `"0.50"`).   |

`requires_capabilities` gates a rule on what the model can do. Each flag defaults
to `false`; set the ones a matched model must support: `streaming`, `tools`,
`json_output`, `system_prompt`, `images`, `reasoning`.

`required_permissions` declares the [tool permissions](#tool-permissions) the
matched route needs. It is merged over the project defaults and may only *narrow*
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
4. Configuration order

Specificity, most to least specific:

```txt
skill + path + intent
> skill + path
> path + intent
> skill
> path
> intent
> default
```

If two rules share the same priority and specificity, validation fails unless
ordering is explicitly allowed.

### Default route

A policy must define a default route, used when no rule matches:

```toml
[routing.default]
model = "openai/gpt-5.5-mini"
fallbacks = ["ollama/qwen2.5-coder"]
```

## Task intents

smista.ai classifies each request into a fixed set of intents: `chat`, `plan`,
`edit`, `review`, `summarize`, `prompt`, `skill`. Classification is
deterministic — ordered rules, never an LLM.

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

Lower `priority` wins. Explicit commands always beat automatic classification:
`/plan refactor the auth middleware` is classified `plan` even if the text reads
like an edit.

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

## Privacy

Privacy policies control which context may reach which model class. Restricted
files are never sent to remote models unless the policy allows it *and* the user
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

Skill- or rule-specific permissions (a rule's `required_permissions`) may
*narrow* these defaults — tightening a tool from `allow` to `ask` to `deny`, or
adding a tool not listed in the defaults. They may never *widen* them: an
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

## Connecting to the router

The CLI needs to know where the router is and how to authenticate to it. This is
client configuration, separate from the router's own runtime config (see
[Running the Router](router.md)).

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

Local preferences are personal, uncommitted overrides — the only layer that is
not version-controlled. They live under `[local_preferences]` and tune your own
experience without changing the shared project policy. Every field is optional;
an unset field defers to the layers below.

```toml
[local_preferences]
auto_apply = false
stream = true
local_only = false
no_network = false
```

| Field        | Effect                                             |
| ------------ | -------------------------------------------------- |
| `auto_apply` | Apply file writes without prompting for each diff. |
| `stream`     | Stream model output when the provider supports it. |
| `local_only` | Use only local models for this session.            |
| `no_network` | Forbid network access for this session.            |

> [!IMPORTANT]
> Local preferences may tighten safety, never loosen it. Enabling `local_only`
> or `no_network` here adds a restriction, but a preference can never weaken a
> project's privacy modes or a tool set to `deny`.

## Validation

Configuration is validated before execution. Validation rejects unknown
providers, models, intents or skills; invalid globs; duplicate rule names; a
missing default route; invalid fallback references; ambiguous rules; invalid
permission values; and secrets stored inline where forbidden. Invalid
configuration produces an actionable error — run `smista config validate` to
check it.
