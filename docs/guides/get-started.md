# Get Started

> [!NOTE]
> smista.ai is under active development. This guide describes the intended
> workflow; commands land progressively as the milestones are implemented.

- [Get Started](#get-started)
  - [Installation](#installation)
  - [Running the local router](#running-the-local-router)
  - [The golden workflow](#the-golden-workflow)
  - [Previewing a route](#previewing-a-route)
  - [Inspecting a decision](#inspecting-a-decision)
  - [Configuration](#configuration)

## Installation

Once binaries are published, the official installer will be available at
<https://install.smista.ai>. The script detects your OS and CPU architecture
and installs the single `smista` binary.

## Running the local router

smista.ai ships as one binary. The router runs inside the `smista` process
rather than as a separate program, so you start and stop it through the CLI.

Start a local router in the background:

```sh
smista start
```

It records its process id in a pidfile, so a second `smista start` refuses to
launch a duplicate. Stop it again with:

```sh
smista stop
```

To keep the router in the foreground — for a service manager, or to watch its
logs — pass `--foreground`:

```sh
smista start --foreground
```

The pidfile defaults to your per-user runtime directory. Point both commands at
a different location with `--pidfile <path>`, or the `SMISTA_ROUTER_PIDFILE`
environment variable.

## The golden workflow

Run a one-shot prompt:

```sh
smista "refactor the auth middleware"
```

smista then shows the detected task, the selected model, the matched routing
rule, the included and excluded context, the estimated cost and the required
permissions. Before any write, it presents a diff and asks for confirmation.

## Previewing a route

To see how a task would be routed without executing it:

```sh
smista route "review this PR"
```

## Inspecting a decision

After execution, inspect the full routing decision, context selection, tool
calls, approvals and cost:

```sh
smista trace
```

## Configuration

Configuration lives locally and can be versioned per project:

- Global: `~/.config/smista/config.toml`
- Project: `.smista/config.toml` (overrides global)
