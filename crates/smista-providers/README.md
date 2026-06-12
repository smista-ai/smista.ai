# smista-providers

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-elastic](https://img.shields.io/crates/l/smista-providers.svg?logo=rust)](https://www.elastic.co/licensing/elastic-license)
[![repo-stars](https://img.shields.io/github/stars/smista-ai/smista.ai?style=flat)](https://github.com/smista-ai/smista.ai/stargazers)
[![latest-version](https://img.shields.io/crates/v/smista-providers.svg?logo=rust)](https://crates.io/crates/smista-providers)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/smista-ai/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/smista-ai/smista.ai/actions)
[![docs](https://github.com/smista-ai/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The provider integration layer for [smista.ai](https://smista.ai). It exposes
one common model interface so the router can run a request against any provider
without coupling routing logic to a provider's API.

Out of the box it targets OpenAI, Anthropic, Gemini, Ollama and
OpenAI-compatible endpoints, integrated through
[`rig`](https://crates.io/crates/rig-core) where
practical. `rig` stays an implementation detail behind this boundary.

## When you need this crate

You usually do not depend on this directly: the router owns model execution.
Reach for it when you are adding a new provider adapter or embedding the model
abstraction inside the workspace.

## Add it to your project

```sh
cargo add smista-providers
```

## Documentation

API reference is on [docs.rs](https://docs.rs/smista-providers). Guides and the
wider project documentation live at <https://docs.smista.ai>.

## License

Licensed under the Elastic License 2.0 (ELv2). See
[LICENSE-ELv2](../../LICENSE-ELv2) for details. You may use, self-host, modify
and embed it freely; you may not offer it to third parties as a hosted or
managed service.
