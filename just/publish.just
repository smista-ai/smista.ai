# Publish everything: all crates in dependency order, then the SDK
[group('publish')]
publish_all: publish_crates publish_sdk

# Publish all crates in dependency order. `cargo publish` automatically infers the correct dependency order
[group('publish')]
publish_crates:
  cargo publish

[group('publish')]
publish_smista_core: (_publish "smista-core")

[group('publish')]
publish_smista_storage: (_publish "smista-storage")

[group('publish')]
publish_smista_providers: (_publish "smista-providers")

[group('publish')]
publish_smista_router: (_publish "smista-router")

[group('publish')]
publish_smista_router_client: (_publish "smista-router-client")

[group('publish')]
publish_smista_sdk: (_publish "smista-sdk")

[group('publish')]
publish_smista_cli: (_publish "smista-cli")

# Publish the TypeScript SDK to npm
[group('publish')]
publish_sdk: sdk_release

# Publish a crate with retry logic to handle registry lag
[private]
_publish crate:
  cargo publish -p {{crate}}
