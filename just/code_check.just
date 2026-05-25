# Format the Rust code
[group('code_check')]
fmt args="":
    cargo fmt --all {{ args }}

# Format the Rust code using the nightly version of rustfmt
[group('code_check')]
fmt_nightly args="":
    cargo +nightly fmt --all {{ args }}

# Run clippy on the workspace
[group('code_check')]
clippy args="":
    cargo clippy --workspace --all-targets {{ args }}

# Build the crate documentation, denying warnings
[group('code_check')]
doc args="":
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps {{ args }}

# Lint and format-check the TypeScript SDK (Biome)
[group('code_check')]
check_sdk: sdk_check

# Type-check the TypeScript SDK
[group('code_check')]
typecheck_sdk: sdk_typecheck

# Run all code checks. Fails if any check fails
[group('code_check')]
check_code:
    just fmt_nightly "--check"
    just clippy "-- -D warnings"
    just doc
    just check_sdk
    just typecheck_sdk
