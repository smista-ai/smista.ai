# smista-cli

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-elastic](https://img.shields.io/crates/l/smista.svg?logo=rust)](https://www.elastic.co/licensing/elastic-license)
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

Install the latest release with the install script (macOS and Linux):

```sh
curl -sSLf https://smista.ai/install/install.sh | sh
```

On Windows (PowerShell):

```powershell
irm https://smista.ai/install/install.ps1 | iex
```

With [Homebrew](https://brew.sh):

```sh
brew install smista-ai/smista/smista
```

With [Cargo](https://doc.rust-lang.org/cargo/):

```sh
cargo install smista --locked
```

The install scripts pick the right binary for your platform, verify its
checksum and use Homebrew when it is available. To install a specific
version, pass `--version=X.Y.Z` (`-Version X.Y.Z` on Windows).

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

Licensed under the Elastic License 2.0 (ELv2). See
[LICENSE-ELv2](../../LICENSE-ELv2) for details. You may use, self-host, modify
and embed it freely; you may not offer it to third parties as a hosted or
managed service.
