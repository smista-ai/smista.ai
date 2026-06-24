# smista-router

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-elastic](https://img.shields.io/crates/l/smista-router.svg?logo=rust)](https://www.elastic.co/licensing/elastic-license)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-router.svg?logo=rust)](https://crates.io/crates/smista-router)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The routing and orchestration service for [smista.ai](https://smista.ai), and
the source of truth for every routing decision. It authenticates users, loads
sessions, classifies tasks, evaluates routing policies, selects models and
providers, mediates tool calls and records traces — then hosts all of this
behind a local HTTP JSON API.

Routing here is **deterministic**: it never depends on an LLM's judgment, and
every decision is explainable through a trace.

## Use the library

```sh
cargo add smista-router
```

smista.ai ships as a single binary, so the router is a **library** rather than a
standalone program: the [`smista` CLI](../smista-cli) embeds it and launches it
in-process with `smista_router::run`. End users start and stop a local router
through the CLI:

```sh
smista start        # run a local router in the background
smista stop         # stop it again
```

Once running, the router listens locally and serves the API under `/api/v1`.
Point the [CLI](../smista-cli) or a client —
[`smista-router-client`](../smista-router-client) (Rust) or
[`@smista-ai/sdk`](../../sdk) (TypeScript) — at it.

## Documentation

Read the guides at <https://docs.smista.ai>.

## License

Licensed under the Elastic License 2.0 (ELv2). See
[LICENSE-ELv2](../../LICENSE-ELv2) for details. You may use, self-host, modify
and embed it freely; you may not offer it to third parties as a hosted or
managed service.
