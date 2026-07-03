# CLI Commands

- [CLI Commands](#cli-commands)
  - [From your shell](#from-your-shell)
    - [Running the router](#running-the-router)
    - [Global flags](#global-flags)
    - [Credential storage](#credential-storage)
    - [Managing router API keys](#managing-router-api-keys)
    - [Managing provider credentials](#managing-provider-credentials)
    - [Managing configuration files](#managing-configuration-files)
    - [Checking router status](#checking-router-status)
    - [Version and help](#version-and-help)
  - [Interactive slash commands](#interactive-slash-commands)
  - [Referencing files](#referencing-files)

The `smista` CLI runs in two modes: a one-shot prompt from your shell, and an
interactive session where commands are typed as slash commands.

## From your shell

| Command                  | What it does                                                  |
| ------------------------ | ------------------------------------------------------------- |
| `smista`                 | Start an interactive session.                                 |
| `smista <prompt>`        | Start a session, run the prompt, show the result.             |
| `smista config ...`      | Create, inspect, edit, or check configuration files.          |
| `smista apikey ...`      | Add, check, or remove the router API key.                     |
| `smista credentials ...` | Add, check, or remove provider API keys.                      |
| `smista start`           | Start the local router. Daemonizes by default.                |
| `smista status`          | Check whether the router is reachable and report its version. |
| `smista stop`            | Stop the local router recorded in the pidfile.                |
| `smista --version`       | Print the CLI version.                                        |
| `smista --help`          | Show help.                                                    |

A one-shot prompt is just a shortcut: it starts a session, sends the prompt
through the active routing policy, displays the response, and persists the
session and trace.

```sh
smista "refactor the auth middleware"
```

### Running the router

The CLI talks to a local router. Start it once and leave it running:

```sh
smista start
```

By default `smista start` daemonizes — it spawns the router as a detached
background process and returns, so your shell is free again. Pass `--foreground`
to run it in the current process instead, which is what a service manager wants.
`smista stop` reads the router's process id from the pidfile and shuts it down.

The main `smista` command can also start a missing local router automatically.
Enable it in CLI configuration:

```toml
[router]
url = "http://localhost:7331"
auto_start = true
```

Auto-start only applies to loopback router URLs such as `localhost`,
`127.0.0.1`, or `[::1]`. When `/status` is already reachable, `smista` uses the
running router no matter whether `auto_start` is enabled. When `/status` is not
reachable, `auto_start = true` starts the local router and waits for it to
become healthy. If auto-start is disabled, the command tells you to run
`smista start`; if the configured router URL is remote, the command reports the
router as unreachable instead of starting anything.

Both commands accept flags to point at a specific configuration file or pidfile,
and `smista start` exposes flags to configure OpenTelemetry trace export. See
[Running the Router](../configuration/router.md) for the full list and for the
configuration file itself.

### Global flags

The logging flags are global: they may appear before or after the subcommand,
and each has an environment-variable equivalent.

| Flag                      | Environment variable       | What it does                                                |
| ------------------------- | -------------------------- | ----------------------------------------------------------- |
| `-L`, `--log-file <path>` | `SMISTA_ROUTER_LOG_FILE`   | Write logs to a file instead of stdout.                     |
| `-l`, `--log-filter <f>`  | `SMISTA_ROUTER_LOG_FILTER` | Set the log level filter (e.g. `debug`). Defaults to `off`. |

```sh
smista --log-filter debug start --foreground
```

### Credential storage

The main interactive `smista` command stores API keys and provider credentials
in the operating-system keyring when it is available. If the keyring cannot be
used, the CLI falls back to file-backed storage.

Pass `--enforce-keyring` to make startup fail instead of falling back:

```sh
smista --enforce-keyring
```

### Managing router API keys

Use `smista apikey` to store, check, and remove the API key used to sign in to
the local router. This is the smista.ai router key returned by bootstrap, not an
upstream provider key.

```sh
smista apikey set sk-smista-api01-...
smista apikey check
smista apikey remove
```

API keys are stored in the project-local scope by default. Add `--global`
before the apikey subcommand to store or remove the global key instead:

```sh
smista apikey --global set sk-smista-api01-...
smista apikey --global remove
```

| Command                       | Aliases        | What it does                         |
| ----------------------------- | -------------- | ------------------------------------ |
| `smista apikey set <api-key>` | `add`          | Store or replace the router API key. |
| `smista apikey check`         | `get`          | Print whether a router key is set.   |
| `smista apikey remove`        | `delete`, `rm` | Remove the router key from scope.    |

### Managing provider credentials

Use `smista credentials` to store, check, and remove provider API keys without
putting the secret value in `config.toml`.

```sh
smista credentials set openai sk-...
smista credentials check openai
smista credentials remove openai
```

Credentials are stored in the project-local scope by default. Add `--global`
before the credentials subcommand to store or remove the global credential
instead:

```sh
smista credentials --global set anthropic sk-ant-...
smista credentials --global remove anthropic
```

| Command                                       | Aliases        | What it does                                   |
| --------------------------------------------- | -------------- | ---------------------------------------------- |
| `smista credentials set <provider> <api-key>` | `add`          | Store or replace a provider API key.           |
| `smista credentials check <provider>`         | `get`          | Print whether a provider API key is available. |
| `smista credentials remove <provider>`        | `delete`, `rm` | Remove a provider API key from the scope.      |

The `<provider>` value must be one of:

| Provider form                   | Use it for                                     |
| ------------------------------- | ---------------------------------------------- |
| `anthropic`                     | Anthropic Claude models.                       |
| `gemini`                        | Google Gemini models.                          |
| `openai`                        | OpenAI GPT models.                             |
| `ollama`                        | Ollama models.                                 |
| `openai-compat:<provider_name>` | A named OpenAI-compatible provider or gateway. |

For OpenAI-compatible providers, replace `<provider_name>` with the configured
instance name, for example:

```sh
smista credentials set openai-compat:my-vllm sk-...
```

### Managing configuration files

Use `smista config init` to create starter configuration files. The command
creates missing parent folders and prints the path it wrote.

```sh
smista config init
```

With no target, `config init` creates the project CLI configuration at
`.smista/config.toml`. It also creates `.smista/.gitignore` with `secrets` so
project credentials stay out of Git.

Choose another target when needed:

| Command                      | What it creates                               |
| ---------------------------- | --------------------------------------------- |
| `smista config init`         | Project CLI config at `.smista/config.toml`.  |
| `smista config init project` | Project CLI config at `.smista/config.toml`.  |
| `smista config init global`  | Global CLI config for every project.          |
| `smista config init router`  | Router runtime config, separate from the CLI. |

The command refuses to overwrite an existing file. Pass `--force` to replace
the target file, or `--config <path>` to write a specific path:

```sh
smista config --config ./router.toml init router
smista config init --force project
```

Use `smista config show` to inspect configuration:

| Command                      | What it prints                          |
| ---------------------------- | --------------------------------------- |
| `smista config show`         | Effective merged CLI configuration.     |
| `smista config show project` | The project CLI configuration layer.    |
| `smista config show global`  | The global CLI configuration layer.     |
| `smista config show router`  | The router runtime configuration layer. |

The merged view applies the same precedence used by the CLI: built-in defaults,
then global configuration, then project configuration. Single-layer views read
one file. Sensitive keys such as `api_key`, `password`, and `secret` are printed
as `[redacted]`.

Use `smista config path` to find configuration files:

| Command                      | What it prints                                       |
| ---------------------------- | ---------------------------------------------------- |
| `smista config path`         | Router, global, and project paths with exist status. |
| `smista config path project` | Project CLI configuration path only.                 |
| `smista config path global`  | Global CLI configuration path only.                  |
| `smista config path router`  | Router runtime configuration path only.              |

Use `smista config edit` to open a configuration file:

```sh
smista config edit project
smista config edit global
smista config edit router
```

The editor comes from `VISUAL`, then `EDITOR`. If neither is set, smista uses
the platform default opener. The command refuses to open a missing file and
points at `smista config init <target>` instead.

Use `smista config check` to validate a single configuration file:

```sh
smista config check project
smista config check global
smista config check router
```

Pass `--config <path>` before the subcommand to operate on a specific file:

```sh
smista config --config ./custom.toml show project
smista config --config ./custom.toml check project
```

### Checking router status

Use `smista status` to query the router's `/status` endpoint and print the
router state and version.

```sh
smista status
```

By default, the command uses the configured router URL from CLI configuration.
If no router URL is configured, it falls back to the default local router URL.
Pass `--url` to check a specific router instance:

```sh
smista status --url http://127.0.0.1:7331
```

### Version and help

`smista --version` prints the CLI version. Include it in bug reports so the
exact build in use is unambiguous.

```sh
$ smista --version
smista 0.0.0
```

`smista --help` shows the full command and flag reference; `smista <command>
--help` shows the flags for a single command.

## Interactive slash commands

Interactive slash-command documentation will be added as commands are
implemented.

## Referencing files

Reference a file in a prompt with `@`:

```txt
review @src/auth/middleware.rs
```

The CLI auto-completes paths typed after `@`.
