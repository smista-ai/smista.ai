# Using Local Models with Ollama

- [Using Local Models with Ollama](#using-local-models-with-ollama)
  - [1. Install and start Ollama](#1-install-and-start-ollama)
  - [2. Point the router at Ollama](#2-point-the-router-at-ollama)
  - [3. Enable the provider in your policy](#3-enable-the-provider-in-your-policy)
  - [4. Route tasks to the local model](#4-route-tasks-to-the-local-model)
  - [Capability checks](#capability-checks)

Local models are first-class in smista.ai. Use them to cut costs on simple tasks
and to keep sensitive code off remote providers. smista.ai runs local models
through [Ollama](https://ollama.com).

Setting up a local model touches two places that must agree:

1. **The router** connects to Ollama and discovers models — `[router.ollama]`
   in [`router.toml`](router.md).
2. **Your routing policy** enables the Ollama provider and the rules that send
   tasks to it — in [`config.toml`](cli.md).

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

## 3. Enable the provider in your policy

In `config.toml`, enable Ollama as a provider. That is all the CLI needs — you
do **not** declare Ollama models or their facts. The router discovers the
installed models and obtains each one's facts (capabilities, context window,
whether it runs locally) through the provider layer.

```toml
[providers.ollama]
type = "ollama"
```

> [!IMPORTANT]
> Keep model names consistent across your routing rules and Ollama itself. A
> model referenced as `ollama/qwen2.5-coder:7b` in a routing rule must be allowed by
> `[router.ollama.models]` and be a model Ollama can serve. The endpoint and
> discovery behaviour are controlled by `[router.ollama]` — `base_url`,
> `auto_discover_models`, and `allowed_models`.

## 4. Route tasks to the local model

Now write rules that send work to Ollama — for cost, for privacy, or both.

```toml
# Cheap, local summaries
[[routing.rules]]
name = "summaries run locally"
priority = 40
intent = "summarize"
model = "ollama/qwen2.5-coder:7b"

# Keep sensitive code on-device
[[routing.rules]]
name = "review crypto locally"
priority = 5
intent = "review"
paths = ["src/crypto/**", "src/auth/**"]
local_only = true
model = "ollama/qwen2.5-coder:7b"
```

You can also make a local model the fallback for a remote one, so work continues
when a provider is down:

```toml
[routing.default]
model = "openai/gpt-5.5-mini"
fallbacks = ["ollama/qwen2.5-coder:7b"]
```

## Capability checks

The router checks capabilities before running a task, using the facts the
provider reports for each model. Local models often lack tool support, so a task
that needs tools won't be routed to one unless the policy explicitly allows
degraded execution. You don't declare these facts yourself — the provider
supplies them — so routing stays predictable without any per-model config.
