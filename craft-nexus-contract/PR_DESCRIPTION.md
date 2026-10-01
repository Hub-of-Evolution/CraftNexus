# fix(contract): handle missing collateral records (#1352)

## Description

Make `is_account_under_collateralized` safe when an artisan's stake or active-obligation storage record is missing. Missing records retain the existing boolean semantics, and persistent TTLs are extended only for records that exist. Host tests cover missing records before an obligation and after the obligation reaches a terminal state.

This branch also resolves the malformed merge-conflict content in `craft-nexus-contract/src/lib.rs`. The conflict markers were embedded in a duplicated, truncated copy of the contract implementation. The intact implementation and tests were retained, and APIs unique to the duplicate were restored in the primary implementation.

## Type of Change

- [x] Bug fix
- [ ] New feature
- [ ] Breaking change
- [x] Documentation update

## Files Modified

- `craft-nexus-contract/src/lib.rs`
- `craft-nexus-contract/src/liquidation_test.rs`
- `craft-nexus-contract/src/lib.rs-conflict-resolution.md`
- `craft-nexus-contract/PR_DESCRIPTION.md`

## Testing

- [x] Added host tests for missing stake and active-obligation records.
- [ ] Contract compilation — attempted in Ubuntu WSL; blocked by existing source errors described below.
- [ ] Tested on Stellar Testnet (not applicable to this storage-read fix).

## Code Quality Checks

- [x] `git diff --check`
- [ ] `cargo check --manifest-path craft-nexus-contract/Cargo.toml --lib` — attempted in Ubuntu WSL; compilation is blocked before tests can run.
- [ ] Focused diagnostic-scan tests — attempted in Ubuntu WSL; test compilation is blocked by existing syntax errors.

## Behavioral Changes

The public return type remains `bool`. With no active-obligation record, the function returns `false`. If obligations are active but the stake record is missing, the missing stake is treated as zero collateral.

## Existing Build Blockers

The conflict markers and truncated text in `craft-nexus-contract/src/lib.rs` have been resolved. Cargo validation is still blocked by unrelated existing errors:

- The library check reports the duplicate error discriminant `80` for `PaginationLimitZero` and `OnboardingVerificationRevoked`, followed by cascading compiler errors in the contract and onboarding sources.
- The focused test build stops on syntax errors in `craft-nexus-contract/src/prop_test/escrow_props.rs` and `craft-nexus-contract/src/prop_test/harness.rs`.

These issues prevent the test binary from compiling; the requested tests have not yet executed.

## Related Issues

Closes #1352.
