# smista-web

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-elastic](https://img.shields.io/crates/l/smista-web.svg?logo=rust)](https://www.elastic.co/licensing/elastic-license)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-web.svg?logo=rust)](https://crates.io/crates/smista-web)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The HTTP JSON API server for [smista-router](../smista-router), built on
[`axum`](https://crates.io/crates/axum). It exposes the local REST API under
`/api/v1`: authentication, sessions, execution, streaming, route preview,
approvals, traces, providers and models, and usage.

It also owns the edges: request authentication, session tokens, credential
headers, streaming responses and secret redaction.

## When you need this crate

Depend on it when you embed the router's API in your own binary. To call the
API as a client, use [`smista-router-client`](../smista-router-client) (Rust)
or [`@smista-ai/sdk`](../../sdk) (TypeScript) instead.

## Add it to your project

```sh
cargo add smista-web
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-web). Guides and the wider
project documentation live at <https://docs.smista.ai>.

## License

Licensed under the Elastic License 2.0 (ELv2). See
[LICENSE-ELv2](../../LICENSE-ELv2) for details. You may use, self-host, modify
and embed it freely; you may not offer it to third parties as a hosted or
managed service.
