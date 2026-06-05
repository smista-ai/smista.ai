# smista-cli

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista.svg?logo=rust)](https://crates.io/crates/smista)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The `smista` command-line interface: the way you drive
[smista.ai](https://smista.ai) from your terminal. It handles command parsing,
terminal rendering, local workspace discovery and approval prompts, and talks
to [smista-router](../smista-router) over its HTTP API.

The CLI never decides which model runs a task — that belongs to the router. You
can, however, express a preference.

## Install

```sh
cargo install smista
```

## Run a task

```sh
smista "refactor the auth middleware"
```

smista shows the detected task, the selected model, the matched routing rule,
the included and excluded context, the estimated cost and the required
permissions. Before any write it presents a diff and asks for confirmation.

Preview a route without running it:

```sh
smista route "review this PR"
```

Inspect the full routing decision afterwards:

```sh
smista trace
```

## Documentation

Read the guides at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE](../../LICENSE) for details.
