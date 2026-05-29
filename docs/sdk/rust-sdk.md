# Rust SDK

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

## What's coming

The async router client will be added as `smista_sdk::client`, so a consumer can
talk to a running router and reuse the same types it already imports from
`smista_sdk::core`. Until then, depend on `smista-sdk` for the types and add the
router client separately if you need it early.
