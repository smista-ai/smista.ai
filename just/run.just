# Build, sign, and run the macOS CLI binary with a stable development identity
[group('run')]
cargo_run_signed *args:
    @test "$(uname -s)" = "Darwin" || (echo "cargo_run_signed is only supported on macOS." >&2; exit 1)
    cargo build -p smista-cli --bin smista
    codesign --force --sign "${SMISTA_CODESIGN_IDENTITY:-Apple Development}" target/debug/smista
    target/debug/smista {{ args }}
