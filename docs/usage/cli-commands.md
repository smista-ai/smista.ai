# CLI Commands

- [CLI Commands](#cli-commands)
  - [From your shell](#from-your-shell)
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
| `smista --version` | Print the version.                                            |
| `smista --help`    | Show help.                                                    |

A one-shot prompt is just a shortcut: it starts a session, sends the prompt
through the active routing policy, displays the response, and persists the
session and trace.

```sh
smista "refactor the auth middleware"
```

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
