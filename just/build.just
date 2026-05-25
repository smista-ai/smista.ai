# Build everything: Rust crates and the TypeScript SDK
[group('build')]
build_all: build_crates build_sdk

# Build all Rust crates in the workspace
[group('build')]
build_crates args="":
  cargo build --workspace {{ args }}

# Build all Rust crates in release mode
[group('build')]
build_crates_release:
  just build_crates "--release"

# Build the TypeScript SDK
[group('build')]
build_sdk: sdk_build

# Clean build artifacts
[group('build')]
[confirm("Are you sure you want to clean the build artifacts?")]
clean:
  cargo clean
  rm -rf "{{SDK_DIR}}/dist"
