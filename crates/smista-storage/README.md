# smista-storage

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista-storage.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/veeso/smista.ai?style=flat)](https://github.com/veeso/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-storage.svg?logo=rust)](https://crates.io/crates/smista-storage)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/veeso/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/veeso/smista.ai/actions)
[![docs](https://github.com/veeso/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The persistence layer for [smista.ai](https://smista.ai). It defines the stored
entities — users, sessions, tokens, messages, routing decisions, tool calls,
approvals, plans, diffs and trace events — and the storage traits used to read
and write them.

Application code depends on the traits, not on SurrealDB directly. The
SurrealDB-backed implementation stays behind this boundary and supports both
embedded (local-first) and remote (SaaS) deployments.

## When you need this crate

Depend on it when you implement a new backend against the storage traits or
wire persistence into a service. Day-to-day use goes through the
[router](../smista-router), which owns reads and writes.

## Add it to your project

```sh
cargo add smista-storage
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-storage). Guides and the
wider project documentation live at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE](../../LICENSE) for details.
