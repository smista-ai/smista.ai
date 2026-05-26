# Run all tests: Rust workspace and the TypeScript SDK
[group('test')]
test_all: test test_sdk

# Run the Rust workspace tests
[group('test')]
test test_name="":
  cargo test --workspace {{ test_name }}

# Run the TypeScript SDK tests (Vitest)
[group('test')]
test_sdk: sdk_test

# Run tests for a single crate, e.g. `just test_crate smista-core`
[group('test')]
test_crate crate test_name="":
  cargo test -p {{ crate }} {{ test_name }}
