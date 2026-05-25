# Contributing to smista.ai

Thanks for your interest in contributing! This document explains how to get set
up and the conventions we follow.

## Code of Conduct

This project adheres to the [Contributor Covenant](CODE_OF_CONDUCT.md). By
participating, you are expected to uphold it.

## Getting started

1. Fork and clone the repository.
2. Install the toolchain pinned in `rust-toolchain.toml` (rustup handles this
   automatically), plus [Node.js](https://nodejs.org) 20+ for the SDK.
3. Build everything:

   ```sh
   cargo build --workspace
   cd sdk && npm install && npm run build
   ```

## Branches and commits

- Create a branch named `feat/{issue_number}-{issue_name}`, for example
  `feat/12-routing-policy-eval`. Use the matching type prefix (`fix/`,
  `docs/`, `chore/`, …) when appropriate.
- Commit messages follow [Conventional Commits](https://conventionalcommits.org).
- Do **not** add `Co-Authored-By` lines.

## Before opening a pull request

Run the same checks CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd sdk && npm run check && npm run build
```

If you changed any GitHub Actions workflow, run `zizmor .github/workflows` and
resolve all findings.

## Documentation

User-facing documentation lives in `docs/` and is built with
[mdBook](https://rust-lang.github.io/mdBook/). Preview locally with
`mdbook serve docs`.

## Reporting issues

Use the [issue tracker](https://github.com/veeso/smista.ai/issues). Please
include reproduction steps and the relevant configuration (with secrets
redacted).
