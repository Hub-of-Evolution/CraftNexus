# PR: Enforce minimum admin recovery cooldown (#608)

## Summary

This change enforces a minimum 7-day cooldown for the admin recovery flow to prevent deployers or malicious actors from bypassing the time-lock by setting a zero-second window. It records the delay when recovery is initiated and validates the recorded delay before completing recovery.

Related issue: #608 — [SECURITY] Admin recovery window has no minimum floor

## What I changed

- Added a new constant `MIN_ADMIN_RECOVERY_COOLDOWN = 7 * 24 * 3600`.
- Persist the recovery delay at initiation (`DataKey::AdminRecoveryDelay`).
- Validate the recorded delay before completing recovery; fail with `Error::AdminRecoveryFailed` if missing or below the minimum.
- Clear the recorded delay when recovery completes.
- Added a unit test that simulates a zero-second recovery window and asserts recovery fails.
- Documented the minimum cooldown in the contract README.

## Files modified

- `src/lib.rs` — added constant, data key, persistence and validation logic
- `src/test.rs` — added `test_recover_admin_with_zero_window_fails`
- `README.md` — documented the 7-day minimum for admin recovery

See changes in this branch.

## Tests & Verification

- Run local checks and tests (required before merge):

```bash
cd craft-nexus-contract
cargo check --tests
cargo test
cargo build --target wasm32-unknown-unknown --release
```

- Expected: `cargo check --tests` passes with zero errors and the new test asserts that recovery fails for a zero-second window.

Note: I couldn't run `cargo` in this environment (not installed). Please run the commands above locally or in CI and report any failures.

## Checklist

- [x] Branch created: `fix/admin-recovery-min-cooldown-#608`
- [x] Add `MIN_ADMIN_RECOVERY_COOLDOWN` and validation
- [x] Persist recorded delay when initiating recovery
- [x] Validate recorded delay before completing recovery
- [x] Clear recorded delay post-recovery
- [x] Add unit test asserting failure on zero-second window
- [x] Update README security notes
- [ ] Run `cargo check --tests` and `cargo test` (pending in CI/local)

## Notes for reviewers

- The implementation defends against a trivial direct-storage bypass by recording the delay at initiation and validating it at recovery time. If you prefer an alternate design (e.g., storing only a single canonical initiation timestamp and deriving the delay from configuration), I'm happy to adjust.

---

Commit: fix(contract): enforce minimum admin recovery cooldown (#608)
# Pull Request

## Closes
- Closes #551
- Closes #550
- Closes #549
- Closes #530
