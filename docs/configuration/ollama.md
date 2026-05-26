# Using Local Models with Ollama

Local models are first-class in smista.ai. Use them to cut costs on simple tasks
and to keep sensitive code off remote providers. smista.ai runs local models
through [Ollama](https://ollama.com).

Setting up a local model touches two places that must agree:

1. **The router** connects to Ollama and discovers models — `[router.ollama]`
   in [`router.toml`](router.md).
2. **Your routing policy** declares the Ollama provider, its models, and the
   rules that send tasks to them — in [`config.toml`](cli.md).

> [!NOTE]
> Ollama is a local model backend, not the router. smista-router stays the
> source of truth for every routing decision.

## 1. Install and start Ollama

Install Ollama, then pull the models you want to use:

```sh
ollama pull qwen2.5-coder:7b
ollama pull llama3.1:8b
```

By default Ollama serves its API at `http://127.0.0.1:11434`.

## 2. Point the router at Ollama

In `router.toml`, enable Ollama and tell the router where it lives. With
`auto_discover_models` the router lists what Ollama already has; `preload` warms
models at startup; `allowed_models` bounds what may run.

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

Set `startup_required = true` if the router should refuse to start when Ollama is
unreachable. Leave `allow_pull = false` to prevent the router from downloading
models on demand.

## 3. Declare the provider and models in your policy

In `config.toml`, register Ollama as a provider and describe each model's
capabilities. The `base_url` here must match the one the router uses.

```toml
[providers.ollama]
type = "ollama"
base_url = "http://localhost:11434"

[models."ollama/qwen2.5-coder"]
provider = "ollama"
name = "qwen2.5-coder"
requires_api_key = false
local = true
supports_streaming = true
supports_tools = false
max_context_tokens = 32768
```

> [!IMPORTANT]
> Keep the model names consistent across both files and Ollama itself. A model
> referenced as `ollama/qwen2.5-coder` in a routing rule must exist in
> `[models.*]`, be allowed by `[router.ollama.models]`, and be a model Ollama
> can serve.

## 4. Route tasks to the local model

Now write rules that send work to Ollama — for cost, for privacy, or both.

```toml
# Cheap, local summaries
[[routing.rules]]
name = "summaries run locally"
priority = 40
intent = "summarize"
model = "ollama/qwen2.5-coder"

# Keep sensitive code on-device
[[routing.rules]]
name = "review crypto locally"
priority = 5
intent = "review"
paths = ["src/crypto/**", "src/auth/**"]
local_only = true
model = "ollama/qwen2.5-coder"
```

You can also make a local model the fallback for a remote one, so work continues
when a provider is down:

```toml
[routing.default]
model = "openai/gpt-5.5-mini"
fallbacks = ["ollama/qwen2.5-coder"]
```

## Capability checks

The router validates capabilities before running a task. Local models often lack
tool support (`supports_tools = false`), so a task that needs tools won't be
routed to one unless the policy explicitly allows degraded execution. Declare
capabilities honestly so routing stays predictable.
