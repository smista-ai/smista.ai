# @smista-ai/sdk

<p align="center">
  <img src="https://smista.ai/logo-150.png" alt="smista.ai logo" width="150" />
</p>

[![license-mit](https://img.shields.io/npm/l/@smista-ai/sdk.svg?logo=npm)](https://opensource.org/licenses/MIT)
[![repo-stars](https://img.shields.io/github/stars/veeso/smista.ai?style=flat)](https://github.com/veeso/smista.ai/stargazers)
[![npm-version](https://img.shields.io/npm/v/@smista-ai/sdk.svg?logo=npm)](https://www.npmjs.com/package/@smista-ai/sdk)
[![conventional-commits](https://img.shields.io/badge/Conventional%20Commits-1.0.0-%23FE5196?logo=conventionalcommits&logoColor=white)](https://conventionalcommits.org)

[![ci](https://github.com/veeso/smista.ai/actions/workflows/ci.yml/badge.svg)](https://github.com/veeso/smista.ai/actions)
[![docs](https://github.com/veeso/smista.ai/actions/workflows/pages.yml/badge.svg)](https://docs.smista.ai)

The TypeScript/JavaScript SDK for [smista.ai](https://smista.ai), and the way
you talk to the router from Node.

It is a thin client over the smista-router HTTP API. It does **not**
reimplement routing, policy evaluation, provider selection or tool mediation —
that behaviour stays owned by the router.

> [!NOTE]
> This package is scaffolding. The full client is implemented in milestone M7.

## Install

```sh
npm install @smista-ai/sdk
```

## Usage

```ts
import { SmistaClient } from '@smista-ai/sdk';

const client = new SmistaClient({
  routerUrl: 'http://127.0.0.1:7331',
  token: process.env.SMISTA_TOKEN,
});
```

## Development

```sh
npm install
npm run build      # compile with tsc
npm run check      # lint + format check (Biome)
npm run format     # apply formatting
```

## Documentation

Read the guides at <https://docs.smista.ai>.

## License

Licensed under the MIT License. See [LICENSE](../LICENSE) for details.
