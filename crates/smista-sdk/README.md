# smista-sdk

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista-sdk.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-sdk.svg?logo=rust)](https://crates.io/crates/smista-sdk)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The Rust SDK for [smista.ai](https://smista.ai), and the single dependency a
Rust program reaches for when building on top of it. It bundles the shared
domain vocabulary and — once it lands — the router client behind one crate, so
you do not have to wire several internal crates together yourself.

The domain types live under `smista_sdk::core`, re-exported verbatim from
[`smista-core`](../smista-core). The router client will arrive as
`smista_sdk::client`, re-exported from
[`smista-router-client`](../smista-router-client).

## Add it to your project

```sh
cargo add smista-sdk
```

```rust
use smista_sdk::core::policy::PermissionMode;

let mode = PermissionMode::default();
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-sdk). Guides and the wider
project documentation live at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE-MIT](../../LICENSE-MIT) for
details.
