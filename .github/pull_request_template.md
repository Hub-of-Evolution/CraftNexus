## Description

<!-- Describe what this PR does -->

## Checklist

- [ ] `cargo check --tests` passes with zero errors
- [ ] `cargo test` all suites pass
- [ ] `cargo build --target wasm32-unknown-unknown --release` succeeds
- [ ] All token transfers and cross-contract calls follow CEI ordering (Checks → Effects → Interactions). See README Security Patterns section.
- [ ] Snapshot files unchanged or updated intentionally
- [ ] PR description references the relevant issue number

