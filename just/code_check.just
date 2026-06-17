# Format all sources (Markdown, TOML, YAML and Rust via nightly rustfmt) with dprint
[group('code_check')]
fmt args="":
    dprint fmt {{ args }}

# Check formatting of all sources with dprint (no writes)
[group('code_check')]
fmt_check args="":
    dprint check {{ args }}

# Run clippy on the workspace
[group('code_check')]
clippy args="":
    cargo clippy --workspace --all-targets {{ args }}

# Build the crate documentation, denying warnings
[group('code_check')]
doc args="":
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps {{ args }}

# Check dependencies for advisories, licenses, bans and sources (cargo-deny)
[group('code_check')]
deny args="":
    cargo deny check {{ args }}

# Lint and format-check the TypeScript SDK (Biome)
[group('code_check')]
check_sdk: sdk_check

# Type-check the TypeScript SDK
[group('code_check')]
typecheck_sdk: sdk_typecheck

# Run all code checks. Fails if any check fails
[group('code_check')]
check_code:
    just fmt_check
    just clippy "-- -D warnings"
    just doc
    just deny
    just check_sdk
    just typecheck_sdk
