# Use Local Models with Ollama

- [Use Local Models with Ollama](#use-local-models-with-ollama)
  - [Prepare Ollama](#prepare-ollama)
  - [Connect the router to Ollama](#connect-the-router-to-ollama)
  - [Enable Ollama in your policy](#enable-ollama-in-your-policy)
  - [Require local execution](#require-local-execution)
  - [Route tasks to a local model](#route-tasks-to-a-local-model)
  - [Check the setup](#check-the-setup)
  - [Capability checks](#capability-checks)

Use a local model to reduce cost or keep sensitive code on your machine.
smista.ai connects to local models through
[Ollama](https://ollama.com).

The setup uses two files:

1. `router.toml` tells the router how to connect to Ollama.
2. `config.toml` enables the provider and routes tasks to its models.

Ollama is a model service, not the smista router. The smista router still owns
every routing decision.

## Prepare Ollama

Start Ollama and pull the models you want to use. For example:

```sh
ollama pull qwen2.5-coder:7b
```

Ollama normally serves its API at `http://127.0.0.1:11434`.

## Connect the router to Ollama

Create the router configuration if it does not exist:

```sh
smista config init router
```

Enable the Ollama connection in `router.toml`:

```toml
[router.ollama]
enabled = true
base_url = "http://127.0.0.1:11434"
```

The router discovers the models that Ollama can serve. The current router
configuration supports only `enabled` and `base_url` in this section.

## Enable Ollama in your policy

Create the project configuration if it does not exist:

```sh
smista config init
```

Enable the Ollama provider in `.smista/config.toml`:

```toml
[providers.ollama]
type = "ollama"
```

You do not declare individual Ollama models or their capabilities in this file.
The router obtains that information from the provider at run time.

## Require local execution

An `ollama/<model>` reference is not enough to guarantee local execution. To
keep every task on local models, add:

```toml
[local_preferences]
local_only = true
```

This also prevents routes from falling back to a remote provider. If your
policy mixes local and remote models, omit this preference and set
`local_only = true` only on the routing rules that must stay local.

## Route tasks to a local model

Set a local default when every task should use Ollama unless another rule
matches:

```toml
[routing.default]
model = "ollama/qwen2.5-coder:7b"
```

You can also route only selected work to a local model:

```toml
[[routing.rules]]
name = "summaries run locally"
priority = 40
intent = "summarize"
local_only = true
model = "ollama/qwen2.5-coder:7b"

[[routing.rules]]
name = "review sensitive code locally"
priority = 5
intent = "review"
paths = ["src/crypto/**", "src/auth/**"]
local_only = true
model = "ollama/qwen2.5-coder:7b"
```

Keep the model name identical to the name shown by Ollama.

## Check the setup

Check both configuration files:

```sh
smista config check project
smista config check router
```

Then start the router and log in:

```sh
smista start
smista login
smista
```

Inside the interactive CLI, use `/providers` to check that Ollama is available
and `/model` to view its models.

## Capability checks

The router checks each model's capabilities before using it. For example, a
route with `requires_capabilities = { tools = true }` cannot use a model that
does not support tools.

When a model does not meet the rule, the router tries the next configured
fallback. If no model is suitable, the task stops with an explanation instead
of running with missing capabilities.
