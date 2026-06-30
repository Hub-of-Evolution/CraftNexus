# Versioned State Migration Runbook

This document details the step-by-step procedure required to safely execute state migrations for the smart contract across different schema versions.

---

## General Migration Lifecycle Workflow

For every migration version, operators must strictly adhere to the following sequence:

1. **Pre-Migration Checks:** Verify the current version in contract storage.
2. **Migration Invocation:** Execute the targeted Soroban contract command.
3. **Post-Migration Verification:** Ensure the state matches the structural rules of the new schema version.

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
