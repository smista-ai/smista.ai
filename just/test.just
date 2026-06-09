# Run all tests: Rust workspace, container-backed integration tests and the TypeScript SDK
[group('test')]
test_all: test integration_test test_sdk

# Run the Rust workspace tests, excluding the integration test crates
[group('test')]
test test_name="":
  cargo test --workspace --exclude storage-integration-tests --exclude provider-integration-tests {{ test_name }}

# Run the container-backed storage integration tests (requires a running Docker daemon)
[group('test')]
integration_test test_name="":
  cargo test -p storage-integration-tests {{ test_name }}

# Run the API-key-gated provider integration tests (requires live provider credentials, e.g. ANTHROPIC_API_KEY)
[group('test')]
provider_integration_test test_name="":
  cargo test -p provider-integration-tests {{ test_name }}

# Run the TypeScript SDK tests (Vitest)
[group('test')]
test_sdk: sdk_test

# Run tests for a single crate, e.g. `just test_crate smista-core`
[group('test')]
test_crate crate test_name="":
  cargo test -p {{ crate }} {{ test_name }}
