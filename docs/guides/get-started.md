# Get Started

> [!NOTE]
> smista.ai is under active development. This guide describes the intended
> workflow; commands land progressively as the milestones are implemented.

- [Get Started](#get-started)
  - [Understand the workflow](#understand-the-workflow)
  - [Create your configuration](#create-your-configuration)
  - [Choose a provider and model](#choose-a-provider-and-model)
    - [Use a remote provider](#use-a-remote-provider)
    - [Use a local Ollama model](#use-a-local-ollama-model)
  - [Check your configuration](#check-your-configuration)
  - [Start the router](#start-the-router)
  - [Log in](#log-in)
  - [Start using smista](#start-using-smista)
  - [Preview routing](#preview-routing)
  - [Choose what to configure next](#choose-what-to-configure-next)

## Understand the workflow

smista.ai is a local-first agent and CLI. It lets you use different models in
one workflow without choosing a model for every request.

For each request, smista follows the same visible steps:

```text
request -> task intent -> routing rule -> safety checks -> model -> trace
```

The task intent describes the kind of work, such as `plan`, `edit`, or
`review`. Your routing rules map that intent, and optionally matching file
paths, to a model. Privacy and tool rules control what the model may see and
do. This process is deterministic: an LLM never chooses the route.

The `smista` binary contains both the CLI and the router. The CLI reads your
configuration and shows results. The router selects the model, runs the task,
mediates tools, and records an explanation of the decision.

## Create your configuration

Start in the project where you want to use smista. Create a project
configuration:

```sh
smista config init
```

This creates `.smista/config.toml`. The file controls providers, routing,
privacy, tool permissions, and the CLI connection to the router. You can commit
it so that the project team shares the same policy.

Use these commands to work with the file:

```sh
smista config path project
smista config edit project
smista config show project
smista config check project
```

The global configuration applies to every project. Create it with
`smista config init global`. Project values take precedence over global values.

## Choose a provider and model

Before you start the router, make sure the configuration contains a provider
and a default model that the provider can serve.

### Use a remote provider

This example enables OpenAI and sends unmatched tasks to one OpenAI model:

```toml
[providers.openai]
type = "openai"

[routing.default]
model = "openai/gpt-5.5-mini"
```

Store the provider key with the CLI:

```sh
smista credentials set openai YOUR_API_KEY
```

Credentials are local to the current project by default. Add `--global` to
make a credential available to every project:

```sh
smista credentials --global set openai YOUR_API_KEY
```

The credential command uses the operating-system keyring when available and a
protected local file as a fallback. It never writes the key to `config.toml`.

Replace `openai` with `anthropic`, `gemini`, or a named OpenAI-compatible
provider when needed. The model in `[routing.default]` must use the same
provider identifier.

### Use a local Ollama model

The built-in policy enables the Ollama provider and uses
`ollama/qwen2.5-coder:7b` as its default route. Make sure Ollama can serve that
model, then enable the router connection in `router.toml`:

```sh
smista config init router
```

```toml
[router.ollama]
enabled = true
base_url = "http://127.0.0.1:11434"
```

To require the local Ollama instance and never use Ollama Cloud, add this to
`.smista/config.toml`:

```toml
[local_preferences]
local_only = true
```

Ollama does not need an API key. See
[Use Local Models with Ollama](../configuration/ollama.md) for the complete
setup.

## Check your configuration

Validate the project policy before starting:

```sh
smista config check project
```

If you changed `router.toml`, validate it too:

```sh
smista config check router
```

Validation reports the field to fix. It does not print credential values.

## Start the router

Start the local router in the background:

```sh
smista start
```

Check that it is ready:

```sh
smista status
```

Stop it later with `smista stop`. Run `smista start --foreground` when you want
to watch logs or use a service manager.

## Log in

The first time you connect to a local router, create a local user and store its
router API key:

```sh
smista login
```

This key signs the CLI in to the router. It is different from the provider API
key that you stored with `smista credentials set`.

## Start using smista

Start an interactive session:

```sh
smista
```

You can also run one task directly from the shell:

```sh
smista "refactor the auth middleware"
```

smista detects the task intent, applies the current routing policy, and shows
the result. Before a file write, it shows a diff and asks for confirmation
unless your policy already allows that action.

## Preview routing

From an interactive session, preview a routing decision without calling a
model:

```text
/preview review this PR
```

The preview reports the task classification, selected provider and model,
matched rule, estimated cost, context, and required permissions.

## Choose what to configure next

smista uses two configuration files with different jobs:

| File                  | Controls                                                        |
| --------------------- | --------------------------------------------------------------- |
| `.smista/config.toml` | Providers, routing, privacy, tools, and CLI-to-router settings. |
| `router.toml`         | The router process, storage, limits, and provider endpoints.    |

Most project changes belong in `.smista/config.toml`. Most local users only
need `router.toml` to enable Ollama or change a runtime default.

Continue with:

- [Configure routing and the CLI](../configuration/cli.md) to choose models and
  define project policy.
- [Configure the router](../configuration/router.md) to change how the local
  service runs.
- [CLI Commands](../usage/cli-commands.md) for the full command reference.
