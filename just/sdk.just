# Install the SDK dependencies from the lockfile
[group('sdk')]
sdk_install:
  cd "{{SDK_DIR}}" && npm ci

# Build the SDK (npm run build)
[group('sdk')]
sdk_build:
  cd "{{SDK_DIR}}" && npm run build

# Type-check the SDK (npm run typecheck)
[group('sdk')]
sdk_typecheck:
  cd "{{SDK_DIR}}" && npm run typecheck

# Lint the SDK (npm run lint)
[group('sdk')]
sdk_lint:
  cd "{{SDK_DIR}}" && npm run lint

# Format the SDK in place (npm run format)
[group('sdk')]
sdk_format:
  cd "{{SDK_DIR}}" && npm run format

# Lint + format check the SDK (npm run check)
[group('sdk')]
sdk_check:
  cd "{{SDK_DIR}}" && npm run check

# Run the SDK tests (npm run test)
[group('sdk')]
sdk_test:
  cd "{{SDK_DIR}}" && npm run test

# Run the SDK tests in watch mode (npm run test:watch)
[group('sdk')]
sdk_test_watch:
  cd "{{SDK_DIR}}" && npm run test:watch

# Publish the SDK to npm (npm run release)
[group('sdk')]
sdk_release:
  cd "{{SDK_DIR}}" && npm run release

# Publish a prerelease of the SDK to npm under the `next` tag (npm run release:pre)
[group('sdk')]
sdk_release_pre:
  cd "{{SDK_DIR}}" && npm run release:pre

# Run an arbitrary npm script in the SDK package
[group('sdk')]
sdk_npm +args:
  cd "{{SDK_DIR}}" && npm {{ args }}
