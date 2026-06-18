# Router authentication

- [Router authentication](#router-authentication)
  - [API keys](#api-keys)
  - [Session tokens](#session-tokens)
  - [Why secrets are looked up by id, never by hash](#why-secrets-are-looked-up-by-id-never-by-hash)
  - [Two hashing algorithms](#two-hashing-algorithms)
  - [The sign-in flow](#the-sign-in-flow)
  - [Validating a request](#validating-a-request)
  - [Signing out and revocation](#signing-out-and-revocation)
  - [Configuration](#configuration)

The router has to answer one question on every request: **who are you?** It
answers it with two credentials that work together — a long-lived **API key**
that identifies you once, and a short-lived **session token** that you present
on every request after signing in.

This is router authentication only. It is unrelated to the **provider
credentials** (your OpenAI, Anthropic, Gemini or Ollama keys) that travel in
separate headers and are never mixed in; see the
[HTTP API](../api/http-api.md#authentication) for how those headers look on the
wire.

## API keys

An API key is issued once, when a user is bootstrapped, and is shown only that
one time. It looks like this:

```text
sk-smista-api01-<user-id>-<secret>
```

- `sk-smista-api01-` is a fixed version prefix, so the format can evolve later.
- `<user-id>` is the owner's UUID. The key **embeds the user id**, so the router
  knows whose key it is from the key alone — you never send a user id alongside
  it.
- `<secret>` is a long random string.

The router never stores the raw key. It stores only an **Argon2id** hash of it,
salted with a fresh random salt, so the same key never produces the same stored
value twice. When you sign in, the router reads the user id out of the key,
loads that user, and verifies the presented key against the stored hash.

## Session tokens

You do not send your API key on every request. Instead you exchange it once for
a session token, which the router issues, stores and hands back. A token looks
like this:

```text
<token-id>-<64 lowercase alphanumeric>
```

- `<token-id>` is the token's UUID in its simple form: 32 hexadecimal digits
  with no hyphens. It is the token's stable identifier.
- The trailing part is a 64-character random secret drawn from lowercase letters
  and digits.

The router stores only a **SHA-512 crypt** (`$6$`) hash of the token, again
salted, never the raw token. Each token carries an expiry; its lifetime comes
from configuration (see [Configuration](#configuration)) and defaults to one
day. A token can also be revoked before it expires.

## Why secrets are looked up by id, never by hash

Both the API key and the session token are stored as **salted** hashes. A salted
hash of the same input is different every time, so you cannot find a row by
comparing hashes for equality — there is nothing stable to match on.

That is why both secrets carry an id in the clear: the user id inside the API
key, and the token id inside the session token. The router parses that id,
loads the single row it names, and only then verifies the presented secret
against that row's stored hash. A hash is never used as a lookup key.

## Two hashing algorithms

The two credentials are hashed with different algorithms on purpose:

| Credential    | Algorithm             | Why                                                                                                       |
| ------------- | --------------------- | --------------------------------------------------------------------------------------------------------- |
| API key       | Argon2id              | Verified rarely (only at sign-in), so a deliberately expensive hash is fine and resists offline cracking. |
| Session token | SHA-512 crypt (`$6$`) | Verified on every request, so it uses a cheaper hash to keep per-request cost low.                        |

Both algorithms salt every hash, so neither stored value can be queried by
equality — which is exactly why the load-by-id rule above applies to both.

## The sign-in flow

1. You call the sign-in endpoint with your API key in the `X-Smista-Api-Key`
   header. No body and no user id are needed, because the key already names the
   user.
2. The router parses the user id from the key and loads that user. An unknown
   user is rejected.
3. The router verifies the presented key against the user's stored Argon2id
   hash. A mismatch is rejected.
4. The router generates a new token, stores its SHA-512 crypt hash with an
   expiry, and returns the raw token to you. The raw token is never stored and
   cannot be recovered later.

From then on you send that token as `Authorization: Bearer <token>` on every
request.

## Validating a request

For each authenticated request the router:

1. Parses the token id from the presented token.
2. Loads the matching token row by that id, requiring it to be **not expired**
   and **not revoked**. A missing, expired or revoked token is rejected.
3. Verifies the presented token against the row's stored SHA-512 crypt hash.
4. Resolves the owning user from the token row and scopes the request to that
   user, so you only ever reach your own data.

## Signing out and revocation

Signing out revokes the presented token: the router marks the token row revoked,
and from that moment validation rejects it, even though it has not yet expired.
Expired and revoked tokens are also cleaned up over time.

## Configuration

Two router settings govern authentication, both under `[router.auth]`:

| Setting                   | Default | Meaning                                                                                                      |
| ------------------------- | ------- | ------------------------------------------------------------------------------------------------------------ |
| `token_ttl_seconds`       | `86400` | How long an issued session token stays valid, in seconds (one day).                                          |
| `local_bootstrap_enabled` | `true`  | Whether the local API-key bootstrap endpoint is available. Must be `false` when storage runs in remote mode. |

See [Running the Router](../configuration/router.md) for where these settings
live.
