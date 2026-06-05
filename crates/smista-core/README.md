# smista-core

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista-core.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-core.svg?logo=rust)](https://crates.io/crates/smista-core)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The shared vocabulary that the rest of [smista.ai](https://smista.ai) is built
on: task intents, provider and model descriptors, routing policy, permission
and privacy models, configuration schemas, error types and validation logic.

It is intentionally a leaf crate — it depends on nothing terminal- or
server-specific, so both the `smista` CLI and the router can build on it.

## When you need this crate

If you are building a Rust program on top of smista.ai, reach for
[`smista-sdk`](../smista-sdk) instead: it re-exports these types behind a
single, stable entry point and will bundle the router client. Depend on
`smista-core` directly only when working inside the workspace.

## Add it to your project

```sh
cargo add smista-core
```

```rust
use smista_core::policy::PermissionMode;

let mode = PermissionMode::default();
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-core). Guides and the wider
project documentation live at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE](../../LICENSE) for details.
