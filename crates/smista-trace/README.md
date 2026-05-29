# smista-trace

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/crates/l/smista-trace.svg?logo=rust)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/veeso/smista.ai?style=flat)](https://github.com/veeso/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-trace.svg?logo=rust)](https://crates.io/crates/smista-trace)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/veeso/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/veeso/smista.ai/actions)
[![docs](https://github.com/veeso/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The execution trace types and recording logic for
[smista.ai](https://smista.ai). For every task it captures the selected model,
the matched routing rule, the task type, provider and fallbacks, any overrides,
the included and excluded context, tool calls, approvals and costs.

Trace events are append-only — they are what makes a routing decision
explainable, and they back the `smista trace` and `smista why` commands.

## When you need this crate

Depend on it when you read or render traces from Rust. Most users instead get
traces through the [CLI](../smista-cli) or the router's HTTP API.

## Add it to your project

```sh
cargo add smista-trace
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-trace). Guides and the
wider project documentation live at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE](../../LICENSE) for details.
