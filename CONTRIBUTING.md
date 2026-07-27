# Contributing to CraftNexus

## Table of Contents

- [Development Setup](#development-setup)
- [Building Contracts](#building-contracts)
- [Running Tests](#running-tests)
- [Code Style & Linting](#code-style--linting)
- [Pull Request Workflow](#pull-request-workflow)
- [Snapshot Management](#snapshot-management)
- [Issue References](#issue-references)

## Development Setup

### Prerequisites

- **Rust toolchain** – Install via [rustup](https://rustup.rs/). Minimum supported version: 1.70.0.
- **Soroban CLI** – Install with:
  ```bash
  cargo install --locked stellar-cli
  rustup target add wasm32-unknown-unknown
  ```
  Or run `./scripts/install-stellar-cli.sh` for the automated setup.
- **Node.js** (for frontend development) – Version 18+ recommended.

### Environment

1. Clone the repository:
   ```bash
   git clone https://github.com/Hub-of-Evolution/CraftNexus.git
   cd CraftNexus
   ```

2. Verify the Rust toolchain:
   ```bash
   rustup show
   ```

## Building Contracts

The smart contracts live in `craft-nexus-contract/`. See [craft-nexus-contract/README.md](craft-nexus-contract/README.md) for full contract documentation.

### Quick Build

```bash
cd craft-nexus-contract
cargo build --target wasm32v1-none --release --locked
```

### Using the Build Script

```bash
./scripts/build.sh
```

This runs an optimized release build, WASM size validation, and automated tests.

### Frontend

```bash
cd craft-nexus
npm install
npm run dev
```

## Running Tests

### Smart Contract Tests

```bash
cd craft-nexus-contract
cargo test -- --nocapture
```

Run a specific test module:

```bash
cargo test --lib onboarding
cargo test --lib test
cargo test expired_dispute_fee
```

### Critical Prerequisite

Before submitting any PR, run:

```bash
cargo check --tests
cargo test --lib
```

Both **must** pass with zero errors. The `cargo check --tests` command catches type and borrow-checker issues without running the full test suite, so use it for quick iteration.

### WASM Build Verification

```bash
cargo build --target wasm32-unknown-unknown --release
```

## Code Style & Linting

- Rust code follows standard `rustfmt` conventions. Format before committing:
  ```bash
  cargo fmt
  ```
- Run clippy for additional lint checks:
  ```bash
  cargo clippy -- -D warnings
  ```
- Frontend code uses ESLint (config in `craft-nexus/eslint.config.mjs`):
  ```bash
  cd craft-nexus && npm run lint
  ```

## Snapshot Management

The Soroban SDK test framework generates snapshot files under `craft-nexus-contract/test_snapshots/` on the first test run or when test inputs/outputs change.

- **Commit snapshots** when you intentionally change contract behavior that affects event output, storage layout, or invocation results.
- **Do not commit snapshots** that were generated from unrelated test changes — verify that snapshot diffs match your intended changes.
- To update all snapshots:
  ```bash
  cargo test -- --nocapture
  ```
  Then review the diff before committing.

## Pull Request Workflow

1. **Create a feature branch** from `main`:
   ```bash
   git checkout -b feat/my-change
   ```

2. **Make your changes** with clear, focused commits. Follow these conventions:
   - Commit messages should be concise and descriptive.
   - Reference issue numbers in commit messages where applicable (e.g., `#621`).

3. **Run the verification checklist** before pushing:
   - [ ] `cargo check --tests` — zero errors
   - [ ] `cargo test --lib` — all suites pass
   - [ ] `cargo build --target wasm32-unknown-unknown --release` succeeds
   - [ ] Snapshot files unchanged or updated intentionally
   - [ ] PR description references the relevant issue number

4. **Push and open a PR** against `main`:
   ```bash
   git push origin feat/my-change
   ```
   Open a pull request on GitHub. Use the PR template and link the issue(s) your change addresses.

5. **Address review feedback** with additional commits. Squash if requested.

## Issue References

- Always reference the issue number in your PR description (e.g., `Closes #621`).
- For partial fixes, use `Refs #621` or `Part of #621`.
