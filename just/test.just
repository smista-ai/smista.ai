# Run all tests: Rust workspace, container-backed integration tests and the TypeScript SDK
[group('test')]
test_all: test integration_test test_sdk

# Run the Rust workspace tests, excluding the container-backed integration tests
[group('test')]
test test_name="":
  cargo test --workspace --exclude integration-tests {{ test_name }}

# Run the container-backed integration tests (requires a running Docker daemon)
[group('test')]
integration_test test_name="":
  cargo test -p integration-tests {{ test_name }}

# Run the TypeScript SDK tests (Vitest)
[group('test')]
test_sdk: sdk_test

# Run tests for a single crate, e.g. `just test_crate smista-core`
[group('test')]
test_crate crate test_name="":
  cargo test -p {{ crate }} {{ test_name }}
