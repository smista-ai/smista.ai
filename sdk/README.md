# @smista-ai/sdk

TypeScript/JavaScript SDK for interacting with the [smista.ai](https://smista.ai) router.

The SDK is a thin client over the smista-router HTTP API. It does **not**
reimplement routing, policy evaluation, provider selection or tool mediation —
that behaviour stays owned by `smista-router`.

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

## License

MIT
