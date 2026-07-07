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

# Update Cargo.lock; pass cargo update arguments to scope the update.
[group('build')]
update_lock args="":
  cargo update {{ args }}

# Build the TypeScript SDK
[group('build')]
build_sdk: _generate_sdk_bindings sdk_build

# Generate TypeScript bindings and build the SDK
[private]
_generate_sdk_bindings:
  echo "Generating TypeScript bindings from Rust code..."
  rm -rf "{{SDK_DIR}}/src/bindings"
  cargo test -p smista-core --features ts export_bindings

# Clean build artifacts
[group('build')]
[confirm("Are you sure you want to clean the build artifacts?")]
clean:
  cargo clean
  rm -rf "{{SDK_DIR}}/dist"
