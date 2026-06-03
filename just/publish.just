# Publish everything: all crates in dependency order, then the SDK
[group('publish')]
publish_all: publish_crates publish_sdk

# Publish all crates in dependency order
[group('publish')]
publish_crates: publish_smista_core publish_smista_storage publish_smista_providers publish_smista_web publish_smista_router publish_smista_router_client publish_smista_sdk publish_smista_cli

[group('publish')]
publish_smista_core: (_publish "smista-core")

[group('publish')]
publish_smista_storage: (_publish "smista-storage")

[group('publish')]
publish_smista_providers: (_publish "smista-providers")

[group('publish')]
publish_smista_web: (_publish "smista-web")

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
  #!/usr/bin/env bash
  set -euo pipefail
  max_retries=5
  delay=15
  for attempt in $(seq 1 $max_retries); do
    echo "Publishing {{crate}} (attempt $attempt/$max_retries)..."
    if cargo publish -p {{crate}}; then
      echo "Successfully published {{crate}}"
      exit 0
    fi
    if [ "$attempt" -lt "$max_retries" ]; then
      echo "Publish failed, retrying in ${delay}s..."
      sleep "$delay"
    fi
  done
  echo "Failed to publish {{crate}} after $max_retries attempts"
  exit 1
