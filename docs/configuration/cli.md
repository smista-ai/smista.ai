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

Skill- or rule-specific permissions may narrow these defaults, but must not
silently widen project restrictions.

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
