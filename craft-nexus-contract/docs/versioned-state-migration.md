# Versioned State Migration Runbook

This document details the step-by-step procedure required to safely execute state migrations for the smart contract across different schema versions.

---

## Migration Toolkit (#944)

`scripts/migration_toolkit.sh` wraps the on-chain primitives below into single commands so a runbook step is one command instead of a hand-typed `stellar contract invoke`. Every command needs `CONTRACT_ID` and `SOURCE` (the admin identity) set in the environment; `NETWORK` defaults to `testnet`.

| Command | Contract entrypoint | Purpose |
|---|---|---|
| `version` | `get_version` | Read the current on-chain contract version. |
| `check <expected_version>` | `pre_migration_check` | Fail fast (`Error::VersionMismatch`) if the contract is not at the expected version, instead of letting a migration step run twice or out of order. |
| `backup` | `backup_platform_config` | Snapshot `PlatformConfig` and print the assigned `backup_id`. |
| `list-backups` | `get_platform_config_backups` | List retained backups (bounded FIFO log, capped at `MAX_CONFIG_BACKUPS`). |
| `rollback <backup_id>` | `rollback_platform_config` | Restore `PlatformConfig` from a prior backup. |

### General Migration Lifecycle Workflow

For every migration version, operators must strictly adhere to the following sequence:

1. **Version check:** `./scripts/migration_toolkit.sh check <expected_version>` — confirm the contract is at the version this migration expects before touching anything.
2. **Backup:** `./scripts/migration_toolkit.sh backup` — snapshot `PlatformConfig` and record the returned `backup_id` in the migration ticket/runbook. Cheap and safe to run even for migrations that don't touch config, since it's the rollback anchor if anything downstream goes wrong.
3. **Migration Invocation:** Execute the targeted Soroban contract command (see per-migration sections below).
4. **Post-Migration Verification:** Ensure the state matches the structural rules of the new schema version.
5. **Rollback (only if verification fails):** `./scripts/migration_toolkit.sh rollback <backup_id>` — restores the exact pre-migration `PlatformConfig` snapshot. Storage-shape migrations (below) are separate, idempotent functions (`migrate_user_profile`, `migrate_token_whitelist`, `migrate_stake_queue`, ...) that read legacy layout and write the new layout without deleting the legacy keys outright, so state remains recoverable by re-running or by admin intervention if a migration is interrupted partway.

### Staged Deployment

For a WASM upgrade (as opposed to an in-place storage migration), combine this toolkit with the existing upgrade proposal flow: `propose_upgrade_wasm` (starts the `wasm_upgrade_cooldown` review window) → take a config backup → `execute_upgrade` once the cooldown elapses → run the relevant `migrate_*` functions → verify → only then consider the migration complete. `cancel_upgrade_wasm` remains available up until `execute_upgrade` is called, giving a staged, reviewable rollout instead of an atomic code swap.

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
