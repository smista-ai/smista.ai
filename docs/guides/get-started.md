# Get Started

> [!NOTE]
> smista.ai is under active development. This guide describes the intended
> workflow; commands land progressively as the milestones are implemented.

## Installation

Once binaries are published, the official installer will be available at
<https://install.smista.ai>. The script detects your OS and CPU architecture
and installs the `smista` and `smista-router` binaries.

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

See the [Specification](../reference/specification.md) for the full
configuration and policy model.
