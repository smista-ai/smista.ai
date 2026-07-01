# CLI Commands

- [CLI Commands](#cli-commands)
  - [From your shell](#from-your-shell)
    - [Running the router](#running-the-router)
    - [Global flags](#global-flags)
    - [Version and help](#version-and-help)
  - [Interactive slash commands](#interactive-slash-commands)
    - [Session](#session)
    - [Running tasks](#running-tasks)
    - [Routing and inspection](#routing-and-inspection)
    - [Models and providers](#models-and-providers)
    - [Configuration](#configuration)
  - [Referencing files](#referencing-files)

The `smista` CLI runs in two modes: a one-shot prompt from your shell, and an
interactive session where commands are typed as slash commands.

## From your shell

| Command            | What it does                                                  |
| ------------------ | ------------------------------------------------------------- |
| `smista`           | Start an interactive session.                                 |
| `smista <prompt>`  | Start a session, run the prompt, show the result.             |
| `smista init`      | Scaffold `.smista/` and `.smista/config.toml` in the project. |
| `smista start`     | Start the local router. Daemonizes by default.                |
| `smista stop`      | Stop the local router recorded in the pidfile.                |
| `smista --version` | Print the CLI version.                                        |
| `smista --help`    | Show help.                                                    |

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

Inside a session, every command starts with `/`.

### Session

| Command                 | What it does                                             |
| ----------------------- | -------------------------------------------------------- |
| `/new`, `/clear`        | Start a new session; the current one stays saved.        |
| `/resume`               | List resumable sessions.                                 |
| `/resume <session-id>`  | Resume a session (the current one is saved first).       |
| `/archive <session-id>` | Archive a session.                                       |
| `/exit`, `/quit`, `/q`  | Persist and leave the session.                           |
| `/init`                 | Create `SMISTA.md` with project instructions for agents. |
| `/help`                 | List all commands.                                       |

### Running tasks

| Command                        | What it does                                                 |
| ------------------------------ | ------------------------------------------------------------ |
| `/plan <prompt>`               | Run the prompt as a planning task.                           |
| `/edit <prompt>`               | Run the prompt as an edit task.                              |
| `/review <prompt>`             | Run the prompt as a review task.                             |
| `/summarize <prompt>`          | Run the prompt as a summarize task (`/summarise` works too). |
| `/prompt <template> [args...]` | Run a configured prompt template.                            |
| `/skills`                      | Show configured skills.                                      |

Explicit commands override automatic intent classification — `/plan ...` is
always treated as a plan.

### Routing and inspection

| Command                     | What it does                                              |
| --------------------------- | --------------------------------------------------------- |
| `/route <prompt>`           | Preview how a prompt would be routed, without running it. |
| `/trace <latest\|trace-id>` | Show the trace for a task.                                |
| `/why`                      | Explain why the last model was selected.                  |
| `/usage [session\|global]`  | Show usage and cost.                                      |

### Models and providers

| Command                   | What it does                                     |
| ------------------------- | ------------------------------------------------ |
| `/model <provider/model>` | Set an explicit model override for the session.  |
| `/model default`          | Clear the override and return to policy routing. |
| `/models`                 | List available models and their capabilities.    |
| `/providers`              | List available providers.                        |

### Configuration

| Command            | What it does                                            |
| ------------------ | ------------------------------------------------------- |
| `/config validate` | Check whether the merged configuration is valid.        |
| `/config path`     | Show which configuration files are in use.              |
| `/config show`     | Show the merged configuration that is currently active. |

## Referencing files

Reference a file in a prompt with `@`:

```txt
review @src/auth/middleware.rs
```

The CLI auto-completes paths typed after `@`.
