# Contract Safety Gate

Release and upgrade workflows share one fail-closed gate. Any required suite
failure blocks deployment. The gate is issue **#1148**.

## What the gate covers

| Suite | Evidence |
|---|---|
| Native unit | Host `cargo test --lib` across settlement, disputes, recurring escrow, staking, onboarding, recovery, and upgrades |
| Property invariants | `prop_` tests for fund conservation, no double settlement, fee allocation, pause/upgrade interactions. Failures print `seed=0x…` |
| Expired-dispute policy | Exact deadline, mutual exclusion with arbitrator/partial-refund paths, conserved refund/fee accounting |
| Admin revisions | Replay and stale-revision rejection for pause, config, recovery, and governance |
| Upgrade approvals | Duplicate signer rejection, count bounded by the signer set, approval events carry nonce + signer |
| Reconciliation | Accounting reports and repair-plan guards |
| Native/WASM validation | `wasm32v1-none` release build plus size ceiling |

## How to run

From `craft-nexus-contract/`:

```bash
./scripts/safety_gate.sh
```

Reproduce a property failure:

```bash
PROP_SEED=0xdeadbeef ./scripts/safety_gate.sh
```

The JSON report (`target/safety-gate-report.json`) always identifies:

- `artifact` — WASM path (or `n/a` if the build was skipped)
- `source_state` — git revision of the tree that was gated
- `failed_invariant` — the suite invariant that failed
- `reproducible_seed` — `PROP_SEED` used for property tests

CI runs the same script (`.github/workflows/contract-safety-gate.yml`).
`scripts/deploy.sh` refuses to deploy unless the gate passes.
Set `SKIP_SAFETY_GATE=1` only for local dry-runs; production releases must not.

## Differential execution and migration manifests

Upgrade execution still requires an `UpgradeCompatibilityManifest` whose
`state_commitment` matches `get_upgrade_state_commitment()`. Migration tooling
(`scripts/migration_toolkit.sh` and `docs/versioned-state-migration.md`)
produces the backup, version check, and rollback handle that the manifest
commits to. The safety gate does not replace that on-chain check; it proves
the candidate artifact still satisfies the host invariants before operators
submit a manifest.

## Accepted residual risks

- **Host vs. WASM divergence.** Property tests run on the Soroban host, not
  inside the uploaded WASM. The WASM build + size check proves the guest
  compiles and fits resource limits, but does not re-execute the full suite
  under the WASM interpreter. Residual: a host-only `cfg(test)` path could
  theoretically diverge. Mitigation: keep production logic outside `#[cfg(test)]`.
- **Property-test horizon.** Sequences are bounded (`DEFAULT_CASE_COUNT`,
  `MAX_SEQUENCE_LEN`). Rare multi-step traces may be unexplored. Mitigation:
  failures print a reproducible seed; expand `PROP_SEED` / case count in CI
  when hunting regressions.
- **Conceptual seller-fee on expiry.** `DeductFeeFromSeller` refunds the buyer
  in full and does not pull extra funds from the seller's wallet. The "seller
  fee" is opportunity cost so the escrow pot stays conserved. See
  `docs/EXPIRED_DISPUTE_POLICY.md`.
- **Keyed upgrade approvals persist after commit.**
  `UpgradeSignerApproval(nonce, signer)` slots are not deleted when a round
  commits. They cannot be reused because the nonce is monotonic, but they
  occupy storage. Residual storage growth is one slot per historical approval.

## Rollback limitations

- WASM upgrades are **forward-only** at the ledger layer. Rolling back
  bytecode requires a new proposal of a previously audited hash, subject to
  the same multi-sig, cooldown, and compatibility-manifest rules.
- `migration_toolkit.sh rollback <backup_id>` restores the snapshotted
  `PlatformConfig` only. It does **not** rewind escrow, stake, or dispute
  storage, and it cannot un-execute a WASM `update_current_contract_wasm`.
- Admin mutations that passed the revision gate are durable. Replaying the
  same revision fails with `AdminActionAlreadyApplied`; compensating action
  requires a **new** revision (for example pause → unpause).
- Expired-dispute settlement is terminal (`SettlementPath::ExpiredDispute`).
  There is no undo path; funds have already left the contract.

Operators should treat a failed safety-gate report as a hard stop: do not
propose or execute an upgrade, and do not run `deploy.sh`, until the reported
seed reproduces green on the same `source_state`.
