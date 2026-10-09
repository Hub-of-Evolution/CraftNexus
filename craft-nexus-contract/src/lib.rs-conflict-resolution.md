# `lib.rs` Conflict Resolution Notes

This document records the unresolved merge regions in `craft-nexus-contract/src/lib.rs` and the intended recovery work.

## Conflict regions

1. **Around lines 16,016–16,728 — fee-token type declarations and constants**
   - The conflict starts in documentation for `FeeTokenInfo` and ends after a second `UpgradeApprovalState` declaration.
   - Preserve the required constants and intervening type declarations, retain one `UpgradeApprovalState` definition, and attach the fee-token documentation to `FeeTokenInfo`.

2. **Around lines 18,894–22,475 — dispute authorization and later contract methods**
   - The markers split `assert_dispute_actor_permissions`, but the conflicting text spans unrelated methods and lands inside `release_batch_funds`.
   - Reconstruct both functions and the intervening implementation from the relevant branch history; do not accept either side as one continuous replacement.

3. **Around lines 22,511–end — contract implementation tail**
   - The region has no closing conflict marker. The checked-in text ends partway through a loop in the batch escrow implementation.
   - Restore the intended implementation and closing delimiters from the branch history, then verify the contract parses and compiles.

## Resolution

The conflict markers were inside a duplicated copy of the contract implementation embedded after the deactivated-account test module. That copy repeated the primary contract, types, and methods, then ended partway through batch creation. The intact primary implementation and the test cases were retained; the duplicate copy was removed.

The APIs that existed only in the duplicate section were moved into the primary contract implementation: continuation resource-budget configuration, the preflight budget check, the bounded diagnostic scan and its report types/storage keys, and the admin accessor. This preserves those interfaces without retaining the conflicting duplicate definitions.

## Validation

- Confirmed no merge markers or truncated error text remain in `lib.rs`.
- `git diff --check` passes.
- Ubuntu WSL `cargo check --manifest-path craft-nexus-contract/Cargo.toml --lib` is blocked by existing source errors, including a duplicate error discriminant (`PaginationLimitZero` and `OnboardingVerificationRevoked` both use `80`) and cascading compilation errors.
- The focused diagnostic-scan test build is blocked by existing syntax errors in `craft-nexus-contract/src/prop_test/escrow_props.rs` and `craft-nexus-contract/src/prop_test/harness.rs`, so tests did not execute.
