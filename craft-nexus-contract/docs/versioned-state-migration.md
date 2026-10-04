# Versioned State Migration Runbook

This document details the step-by-step procedure required to safely execute state migrations for the smart contract across different schema versions.

---

## Migration Toolkit (#944)

`scripts/migration_toolkit.sh` wraps the on-chain primitives below into single commands so a runbook step is one command instead of a hand-typed `stellar contract invoke`. Every command needs `CONTRACT_ID` and `SOURCE` (the admin identity) set in the environment; `NETWORK` defaults to `testnet`.

| Command | Contract entrypoint | Purpose |
|---|---|---|
| `version` | `get_version` | Read the current on-chain contract version. |
| `preconditions` | `get_migration_preconditions` | Evaluate the #1118 migration preconditions. Read-only and permissionless; exits non-zero when the migration would be refused. |
| `audit-preconditions` | `audit_migration_preconditions` | Persist the precondition verdict to the audit trail (admin only) so a blocked migration stays explainable after the fact. |
| `check <expected_version>` | `pre_migration_check` | Fail fast (`Error::VersionMismatch`) if the contract is not at the expected version, instead of letting a migration step run twice or out of order. |
| `backup` | `backup_platform_config` | Snapshot `PlatformConfig` and print the assigned `backup_id`. |
| `list-backups` | `get_platform_config_backups` | List retained backups (bounded FIFO log, capped at `MAX_CONFIG_BACKUPS`). |
| `rollback <backup_id>` | `rollback_platform_config` | Restore `PlatformConfig` from a prior backup. |

### General Migration Lifecycle Workflow

For every migration version, operators must strictly adhere to the following sequence:

1. **Version check:** `./scripts/migration_toolkit.sh check <expected_version>` — confirm the contract is at the version this migration expects before touching anything.
2. **Preconditions (#1118):** `./scripts/migration_toolkit.sh preconditions` — confirm the gate will admit the migration. This is read-only, and a non-zero exit means the migration will refuse; resolve the reported condition first. Optionally `./scripts/migration_toolkit.sh audit-preconditions` to leave a durable record in the audit trail.
3. **Backup:** `./scripts/migration_toolkit.sh backup` — snapshot `PlatformConfig` and record the returned `backup_id` in the migration ticket/runbook. Cheap and safe to run even for migrations that don't touch config, since it's the rollback anchor if anything downstream goes wrong.
4. **Migration Invocation:** Execute the targeted Soroban contract command (see per-migration sections below).
5. **Post-Migration Verification:** Ensure the state matches the structural rules of the new schema version.
6. **Rollback (only if verification fails):** `./scripts/migration_toolkit.sh rollback <backup_id>` — restores the exact pre-migration `PlatformConfig` snapshot. Storage-shape migrations (below) are separate, idempotent functions (`migrate_user_profile`, `migrate_token_whitelist`, `migrate_stake_queue`, ...) that read legacy layout and write the new layout without deleting the legacy keys outright, so state remains recoverable by re-running or by admin intervention if a migration is interrupted partway.

### Staged Deployment

For a WASM upgrade (as opposed to an in-place storage migration), combine this toolkit with the existing upgrade proposal flow: `propose_upgrade_wasm` (starts the `wasm_upgrade_cooldown` review window) → take a config backup → `execute_upgrade` once the cooldown elapses → run the relevant `migrate_*` functions → verify → only then consider the migration complete. `cancel_upgrade_wasm` remains available up until `execute_upgrade` is called, giving a staged, reviewable rollout instead of an atomic code swap.

## Migration Preconditions (#1118)

Every operator-initiated `migrate_*` function is gated. Before its first write
it evaluates a bounded set of preconditions, and **a violated precondition
aborts the migration having written nothing** — the layout version, the legacy
blobs, and every indexed record are exactly as they were.

The gate is deliberately *not* applied to `migrate_legacy_artisan_stake`, which
is a lazy per-record rewrite on the stake read/write path rather than an
operator-initiated migration. Gating it would make ordinary stake reads fail
whenever an unrelated precondition was violated.

### Running the check

```bash
./scripts/migration_toolkit.sh preconditions     # read-only, no admin needed
```

Read this **before** attempting a migration: because a refusal writes nothing —
not even a record of why — this call is the only way to observe the blocking
condition at the moment it matters. Quote the report's `digest` in the migration
ticket so a pre-flight result cannot be re-interpreted after the fact.

To leave a durable record, run the admin-only variant, which persists the
verdict, appends it to a bounded FIFO log, and emits
`migration_precondition_audited`:

```bash
./scripts/migration_toolkit.sh audit-preconditions
```

Audit records can be re-verified with `verify_precondition_audit <sequence>`,
which recomputes the digest and returns `Error::CorruptedMigrationAudit` if the
stored record no longer matches its own contents.

### Conditions checked

| Family | Conditions | Why it blocks |
|---|---|---|
| Versions (0–9) | `PlatformInitialized`, `SourceVersionRecognised`, `LayoutVersionMigratable`, `LegacyShapeReadable` | A fresh or unrecognised deployment has no legacy state to migrate, and a layout version *ahead* of this build means a newer build already owns these records. Every legacy blob must decode into the shape the migration will read it as, or the migration would treat unreadable data as "nothing to migrate" and write a fresh layout on top of it. |
| Terminal states (10–19) | `NoOpenDisputes`, `NoActiveRecurringEscrows`, `NoPendingAdminTransfer`, `NoEmergencyOperationInFlight`, `NoIncompleteTransitions` | A dispute pins an escrow plus its evidence and escalation chain; a half-completed admin transfer means `PlatformConfig.admin` and `PendingAdmin` disagree about who holds authority; a `*Pending` status is an exclusive in-flight claim that outlives the layout that issued it. Re-keying the layout underneath any of these strands them. |
| Outstanding liabilities (20–29) | `NoLockedFunds`, `NoBatchJobInFlight`, `NoReconciliationInFlight`, `NoPendingRepairPlan`, `StakeQueueConsistent` | The migrations re-key `TotalLocked`. A non-zero value would drop the platform's own accounting of funds it is holding. A partially executed batch, an unfinished reconciliation, or an unapplied repair plan is an outstanding claim on those same balances. A stake queue whose declared count disagrees with its entries would be silently reset by a re-run. |
| Pending governance (30–39) | `NoPendingWasmUpgrade`, `NoPendingUpgradeApprovals` | An open upgrade round is aimed at state whose shape is about to change. |
| Token identities (40–49) | `NoMixedWhitelistLayout`, `FeeTokenIndexDeduplicated`, `NoUnknownStakeTokens` | `migrate_whitelist_storage` assigns the token count from the legacy blob alone, so running it over a half-migrated whitelist would *lower* the count and make genuinely whitelisted tokens fail. A duplicated fee index entry would be migrated twice and double-count the seeded fee total. An artisan whose stake cannot be resolved to one token identity cannot be re-keyed safely. |
| Gate integrity (50–59) | `ScanBudgetExceeded` | The gate is **fail-closed**. If it cannot examine the whole state within `MAX_MIGRATION_PRECONDITION_SCAN`, it refuses rather than reporting a clean bill of health over state it never looked at. Exceeding this means the deployment is too large for a single pre-flight and must be migrated in reviewed stages. |

### Coverage and limits

Soroban has no storage-key enumeration, so "every token" is the set derivable
from the three places a token can be reached from: the legacy `FeeTokenIndex`,
the legacy `WhitelistedTokens` blob, and the stake tokens of indexed artisans.
The report's `tokens_scanned`, `escrows_scanned`, and `artisans_scanned`
counters state exactly how much of the deployment the verdict covers — read
them rather than assuming full coverage. Escrow records are resolved in memory
across all four historical shapes (`Escrow` v5, v4, pre-`batch_id`, and
`LegacyEscrow`) without writing, since the gate runs against pre-migration
state by definition.

## Differential Upgrade Compatibility Gate

An uploaded WASM and a successful unit-test run are not sufficient evidence for
an upgrade. Before execution, run the old and new artifacts against an isolated
fixture containing legacy profiles, active and disputed escrows, recurring
balances, stake queues, pending upgrades, and paused configuration. Compare
read results, authorization decisions, error classifications, invariants, and
events. Commit the pre-migration snapshot returned by
`get_upgrade_state_commitment` and the interface/authentication test results in
an `UpgradeCompatibilityManifest`.

Submit the manifest with `submit_upgrade_compatibility_manifest`. It must:

- identify the exact source and target contract versions;
- include reproducible fixture definitions and artifact versions;
- commit to storage preconditions, postconditions, interface behavior,
  authorization behavior, and rollback limitations;
- include a resumable migration checkpoint;
- report `migration_complete: true` and `manual_records: 0`.

`execute_upgrade` rejects missing, stale, incomplete, or manually unresolved
manifests before calling `update_current_contract_wasm`. On success,
`UpgradeHistory` records the source and target versions, WASM hash, state
commitment, and migration checkpoint. The manifest is removed only after the
upgrade record and version update are written. A migration runner may replace
the manifest for the same hash while it resumes; each execution attempt is
idempotently blocked until the final checkpoint is submitted.

The manifest is an attestation boundary, not a substitute for isolated test
execution. CI and release tooling must fail closed when the differential
fixture, invariant suite, or rollback documentation does not produce all
non-zero commitments required by the on-chain gate. Records that cannot be
automatically migrated must remain outside execution until they are handled
and the manifest is resubmitted with a new checkpoint.

---

## Migration 1: UserProfile (v1 -&gt; v2)

### 1. Pre-Migration Checks

Verify that the `UserProfile` entries are on `v1` structure before applying the layout change. Ensure contract balance constraints are satisfied.

### 2. Migration Invocation Command

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  migrate_user_profile
```

### 3. Post-Migration Verification
Query individual user state fields using a read-only instance to verify the presence of the updated fields introduced in v2.

## Migration 2: WhitelistedTokens (Map -> Individual Keys)
### 1. Pre-Migration Checks
Read the legacy configuration Map to ensure total token allocations match current baseline expectations.

### 2. Migration Invocation Command

stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  migrate_token_whitelist

### 3. Post-Migration Verification
Confirm that separate storage slot configurations can be fetched individually per token address instead of a singular monolith Map structure.

## Migration 3: ArtisanStakeQueue (Vec -> Indexed Queue)
### 1. Pre-Migration Checks
Assert that the legacy sequential Vec structure does not exceed maximum heap layout sizes, checking data continuity flags.

### 2. Migration Invocation Command

stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  migrate_stake_queue

### 3. Post-Migration Verification
Run an index query range verification step to ensure elements read correctly from their respective indexed queue positions without errors.
