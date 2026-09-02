# Upgrade State Snapshot Tooling

**Issue:** #1137 · **Area:** Upgrade operations

Before a WASM upgrade, operators need a **reproducible snapshot** of
representative live state and invariants. This document defines the snapshot
contract, its determinism guarantees, and how fixtures feed migration and
differential tests.

## What a snapshot captures

`get_upgrade_state_snapshot()` returns an [`UpgradeStateSnapshot`] — a compact,
deterministic projection of live contract storage. It covers:

| Dimension | Fields |
|---|---|
| **Versioned records** | `contract_version`, `storage_layout_version`, `upgrade_history_len`, `escrow_count` |
| **Balances** | `total_locked` (deterministic cumulative volume), `total_staked` |
| **Permissions** | `whitelisted_token_count`, `upgrade_signer_count`, `pending_admin_action_count`, `upgrade_threshold` |
| **Pending jobs** | `recurring_escrow_count`, `recurring_escrow_next_id`, `pending_batch_job_count`, `has_pending_upgrade_proposal` |
| **Operational flags** | `paused`, `onboarding_configured` |

`get_upgrade_state_commitment()` hashes the snapshot's canonical XDR. Tooling
places the pre-migration value in
`UpgradeCompatibilityManifest::state_commitment` (see
[`versioned-state-migration.md`](./versioned-state-migration.md)).

## Determinism guarantee

A snapshot is **deterministic for unchanged ledger state**:

- Every field is derived from a scalar counter, presence flag, next-id, or
  bounded-list length — never from iteration order over a `Map`/`Vec` that
  could vary between runs.
- Balance commitments use cumulative scalar aggregates (`TotalVolume`), not
  per-identity balances whose enumeration order could differ.
- Two independent reads of the same ledger state return byte-identical XDR and
  an identical SHA-256 commitment (proved by
  `snapshot_is_deterministic_for_unchanged_state`).

## Sensitive-data handling

The snapshot commits to **counts, sums, and structural presence** — it never
embeds raw addresses, user payloads, IPFS hashes, dispute reasons, or
per-identity balances. Off-chain tooling must still treat the raw snapshot and
its XDR as protocol state and protect it like other ledger data (no secrets, no
PII). Consumers that need per-record detail should read the specific storage
keys they require rather than deriving them from the snapshot.

## Fixtures feed migration and differential tests

The test harness in `src/upgrade_snapshot_test.rs` builds a **representative
fixture** (initialized contract + whitelisted token + funded buyer) and proves:

1. Determinism of the snapshot and commitment over unchanged state.
2. That the snapshot commits to structural counts (e.g. `whitelisted_token_count`
   reflects the fixture), not raw payloads.
3. That mutating state (creating an escrow) changes the commitment — the
   property differential tooling relies on to detect old/new divergence.

Operators should run the same fixture against the old and new WASM in an
isolated environment, compare commitments and read results, and record the
results in the upgrade compatibility manifest before `execute_upgrade`.

## Commands

```bash
# Run the snapshot regression harness
cargo test --features testutils upgrade_snapshot_
```
