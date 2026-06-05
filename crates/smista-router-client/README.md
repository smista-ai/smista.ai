# smista-router-client

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista-router-client.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-router-client.svg?logo=rust)](https://crates.io/crates/smista-router-client)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The async Rust client for the [smista-router](../smista-router) HTTP JSON API
(`/api/v1`). It is how a Rust frontend talks to the router without hand-rolling
HTTP.

The `SmistaRouterClient` trait covers every endpoint — authentication,
sessions, execution, streaming, route preview, approvals, traces, providers and
models, and usage — with a [`reqwest`](https://crates.io/crates/reqwest)-backed
implementation. Request and response types come from
[`smista-core`](../smista-core). Credentials travel in headers and are never
logged, traced or sent as model context.

## When you need this crate

Use it when building a Rust client of the router. If you also want the shared
domain types under one dependency, prefer [`smista-sdk`](../smista-sdk), which
will re-export this client.

## Add it to your project

```sh
cargo add smista-router-client
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-router-client). Guides and
the wider project documentation live at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE-MIT](../../LICENSE-MIT) for
details.
