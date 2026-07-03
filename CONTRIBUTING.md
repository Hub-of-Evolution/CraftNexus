# Contributing to CraftNexus

Thank you for your interest in contributing to CraftNexus! This guide covers the
development workflow, code standards, and submission process.

## Table of Contents

- [Development Setup](#development-setup)
- [Pre-commit Hooks](#pre-commit-hooks)
- [Code Formatting](#code-formatting)
- [Running Tests](#running-tests)
- [Build Verification](#build-verification)
- [Pull Request Guidelines](#pull-request-guidelines)

## Development Setup

### Prerequisites

- Rust (with `wasm32-unknown-unknown` target): `rustup target add wasm32-unknown-unknown`
- Soroban CLI: see `craft-nexus-contract/scripts/install-stellar-cli.sh`
- Python 3.x (for pre-commit)
- Optional: `wasm-opt` (via [binaryen](https://github.com/WebAssembly/binaryen)) for
  WASM size optimization

### Clone and install

```bash
git clone https://github.com/Hub-of-Evolution/CraftNexus
cd CraftNexus
pip install pre-commit
pre-commit install
```

## Pre-commit Hooks

CraftNexus uses [pre-commit](https://pre-commit.com/) to enforce code quality
checks before every commit. The configuration lives in `.pre-commit-config.yaml`
at the repository root.

### Installing pre-commit

```bash
pip install pre-commit
# Install the hooks into your local .git/hooks directory
pre-commit install
```

### Hooks summary

| Hook | What it checks |
|------|---------------|
| `cargo-fmt-check` | All Rust source files must be formatted with `rustfmt`. Run `cargo fmt` inside `craft-nexus-contract/` to auto-fix. |
| `snapshot-change-warning` | Warns when test snapshot files under `test_snapshots/` are staged. Review snapshot diffs carefully before committing. |

### Running hooks manually

```bash
# Run all hooks against all files (useful before opening a PR)
pre-commit run --all-files

# Run a single hook
pre-commit run cargo-fmt-check
pre-commit run snapshot-change-warning
```

### Bypassing hooks

Pre-commit hooks can be bypassed with `git commit --no-verify`, but this is
**strongly discouraged** for formatting or snapshot checks. Only skip hooks when
you have a deliberate, documented reason.

## Code Formatting

All Rust code in `craft-nexus-contract/` must be formatted with `rustfmt`.

```bash
# Auto-format all Rust files
cd craft-nexus-contract
cargo fmt

# Check without modifying (mirrors the CI check)
cargo fmt -- --check
```

The CI pipeline (`.github/workflows/ci.yml`) runs `cargo fmt -- --check` and
will fail on any formatting drift.

## Running Tests

Before submitting a PR, ensure all checks pass:

```bash
cd craft-nexus-contract

# 1. Check for compile errors (including test code)
cargo check --tests

# 2. Run the full test suite
cargo test

# 3. Build the WASM artifact
cargo build --target wasm32-unknown-unknown --release
```

All three commands must succeed with zero errors. Warnings are acceptable but
should be minimised.

### Active build errors (fix before submitting)

If `cargo check --tests` reports errors, consult the table in each linked issue
for the canonical fix:

| Error | Location | Fix | Issue |
|-------|----------|-----|-------|
| E0609 no field on Option | `src/test.rs` | Add `.unwrap()` before `.1`/`.2` on `events.last()` | #682 |
| E0599 no method `to_string` | `src/onboarding.rs` | Add `use crate::alloc::string::ToString;` | #682 |
| E0382 moved value | `src/expired_dispute_fee_test.rs:68` | Add `.clone()` before first move | #682 |
| Unclosed delimiter | `src/onboarding_test.rs` | Add missing closing `}` | #682 |
| Cannot find macro `vec` | `src/min_release_window_test.rs` | Add `use soroban_sdk::vec;` | #682 |

## Build Verification

The `craft-nexus-contract/scripts/build.sh` script compiles the contract and,
if `wasm-opt` is available, runs size optimization:

```bash
cd craft-nexus-contract
./scripts/build.sh
```

The script will:
1. Compile to `wasm32-unknown-unknown` with release profile
2. Run `wasm-opt -Oz` to reduce binary size (if `wasm-opt` is installed)
3. Report unoptimized vs. optimized WASM sizes

Install `wasm-opt` via [binaryen](https://github.com/WebAssembly/binaryen):

```bash
# macOS
brew install binaryen

# Ubuntu / Debian
sudo apt-get install binaryen

# Cargo (cross-platform)
cargo install wasm-opt
```

## Pull Request Guidelines

1. **Reference the issue number** in your PR description (e.g., `Closes #682`).
2. **Ensure `cargo check --tests` passes** with zero errors before opening a PR.
3. **Run `cargo fmt`** before committing — the CI will fail on formatting drift.
4. **Review snapshot changes** — if `test_snapshots/` files changed, explain why
   in the PR description.
5. **Add tests** for any new behaviour introduced.
6. Keep PRs focused. One issue per PR is preferred.
