# TypeScript SDK

- [TypeScript SDK](#typescript-sdk)
  - [Add it to your project](#add-it-to-your-project)
  - [Talk to the router](#talk-to-the-router)
  - [Reuse a session token](#reuse-a-session-token)
  - [Reach upstream models](#reach-upstream-models)
  - [Stream a turn](#stream-a-turn)
  - [Handle errors](#handle-errors)

The `@smista-ai/sdk` package is a thin, typed client over the smista-router HTTP
API. It sends requests and returns typed results; it never reimplements routing,
policy evaluation, provider selection or tool mediation — that logic stays in the
router.

## Add it to your project

```sh
npm install @smista-ai/sdk
```

The package targets Node.js 22 or newer and ships as ES modules.

## Talk to the router

`SmistaClient` implements every router endpoint and holds your credentials for
you. `bootstrap` mints and stores the API key, `signIn` exchanges it for a
session token the client then keeps, and every authenticated call reuses that
token — you never pass a credential per call:

```ts
import { SmistaClient } from '@smista-ai/sdk';

// Defaults target http://localhost:7331; pass a baseUrl to point elsewhere.
const client = new SmistaClient({ baseUrl: 'http://localhost:7331' });

// First run only: create the first user and store its API key.
await client.bootstrap();
// Exchange the held API key for a session token the client keeps.
await client.signIn();

const sessions = await client.listSessions();
console.log(`you have ${sessions.sessions.length} session(s)`);

await client.signOut();
```

An authenticated call made before you sign in fails right away, before any
network request, so a missing token never reaches the router.

## Reuse a session token

If you already hold an API key or a still-valid session token (for example from
a secure store), seed it when constructing the client and skip the step you no
longer need:

```ts
const client = new SmistaClient({ sessionToken: savedToken });

// The seeded token authenticates calls without a fresh sign-in.
const me = await client.me();
```

## Reach upstream models

The keys the router needs to call upstream models are configured once with
`providerCredentials`. They travel as request headers only on the calls that can
reach a model — `execute`, `continueRun`, `streamExecute`, `streamContinue` and
`listModels` — and never appear in a query parameter, log or error:

```ts
import { ProviderCredentials, SmistaClient } from '@smista-ai/sdk';

const client = new SmistaClient({
  sessionToken: savedToken,
  providerCredentials: ProviderCredentials.empty().withProvider('anthropic', 'sk-ant-...'),
});

const models = await client.listModels();
console.log(`${models.models.length} model(s) available`);
```

## Stream a turn

`streamExecute` and `streamContinue` yield the turn one event at a time. Iterate
the result with `for await`; the stream ends after the terminal `turn_end` event,
which carries the final turn:

```ts
for await (const event of await client.streamExecute(sessionId, request)) {
  if (event.type === 'text_delta') {
    process.stdout.write(event.delta);
  } else if (event.type === 'turn_end') {
    console.log('\nturn complete');
  }
}
```

## Handle errors

Every method rejects with a `SmistaError`, whose `kind` says what went wrong: an
`api` error pairs the HTTP status with the router's structured error `code`, while
`decode`, `transport` and `notAuthenticated` cover the rest. Use `isSmistaError`
to narrow an unknown value in a `catch`:

```ts
import { isSmistaError, SmistaClient } from '@smista-ai/sdk';

try {
  await client.getSession(sessionId);
} catch (error) {
  if (isSmistaError(error) && error.kind === 'api') {
    console.error(`router said ${error.code} (status ${error.status})`);
  } else {
    throw error;
  }
}
```
