# Rust SDK

- [Rust SDK](#rust-sdk)
  - [Add it to your project](#add-it-to-your-project)
  - [Use the domain types](#use-the-domain-types)
  - [Talk to the router](#talk-to-the-router)
  - [What's coming](#whats-coming)

The `smista-sdk` crate is the single dependency you reach for when building a
Rust program on top of smista.ai — a companion tool, an automation, or your own
frontend. It re-exports everything you need from one place so you don't have to
track which internal crate a type lives in.

## Add it to your project

```sh
cargo add smista-sdk
```

## Use the domain types

The shared domain vocabulary — task intents, model descriptors, routing policy,
permission and privacy models, configuration schemas and errors — lives under
`smista_sdk::core`:

```rust
use smista_sdk::core::policy::PermissionMode;

let mode = PermissionMode::default();
println!("default permission mode: {mode:?}");
```

Any path you would have reached at `smista_core::*` is available as
`smista_sdk::core::*`.

## Talk to the router

The async router client lives under `smista_sdk::client`. The
backend-agnostic `Client` trait and its types are always available; the default
HTTP backend, `ReqwestClient`, ships behind the `reqwest-client` feature:

```sh
cargo add smista-sdk --features reqwest-client
```

The client holds your credentials for you. `bootstrap` mints and stores the API
key, `sign_in` exchanges it for a session token the client then keeps, and every
authenticated call reuses that token — you never pass a credential per call:

```rust,ignore
use smista_sdk::client::{Client, ReqwestClient, RouterClientConfig};

#[tokio::main]
async fn main() -> smista_sdk::client::Result<()> {
    // Defaults target http://localhost:7331; pass a URL to point elsewhere.
    let client = ReqwestClient::new(RouterClientConfig::default())?;

    // First run only: create the first user and store its API key.
    client.bootstrap().await?;
    // Exchange the held API key for a session token the client keeps.
    client.sign_in().await?;

    let sessions = client.list_sessions().await?;
    println!("you have {} session(s)", sessions.sessions.len());

    client.sign_out().await?;
    Ok(())
}
```

If you already hold an API key or a still-valid token (for example from a
keyring), seed it with `ReqwestClient::new(config)?.with_api_key(key)` or
`.with_session_token(token)` and skip the step you no longer need. The client is
cheaply cloneable, and every clone shares the same authentication state.

The keys the router needs to reach upstream models are configured once with
`.with_provider_credentials(...)`; they travel as request headers on the calls
that can reach a model and are never logged, traced or sent as model context.

## What's coming

The client and the `smista_sdk::core` types already share the single
`smista-sdk` dependency. Future releases add more backends behind their own
features, all reachable through `smista_sdk::client`.
