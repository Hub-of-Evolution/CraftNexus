# Contract Upgradeability Security Guide

This document covers the security model that governs WASM bytecode upgrades for the CraftNexus escrow contract. It describes who may propose and execute upgrades, how multi-signature threshold voting works, and what emergency lock-out mechanisms are available.

---

## Table of Contents

1. [Upgrade Permission Model](#1-upgrade-permission-model)
2. [Voting Threshold Requirements](#2-voting-threshold-requirements)
3. [Cooldown and Anti-Bypass Timers](#3-cooldown-and-anti-bypass-timers)
4. [Executing a Committed Upgrade](#4-executing-a-committed-upgrade)
5. [Cancelling an Upgrade](#5-cancelling-an-upgrade)
6. [Emergency Lock-Out Procedures](#6-emergency-lock-out-procedures)
7. [Admin Key Management](#7-admin-key-management)
8. [Audit Trail and History](#8-audit-trail-and-history)
9. [Security Assumptions and Invariants](#9-security-assumptions-and-invariants)
10. [Quick-Reference CLI Commands](#10-quick-reference-cli-commands)

---

## 1. Upgrade Permission Model

### Who can propose an upgrade?

Any address in the **UpgradeSigners** list may call `propose_upgrade_wasm`. If no explicit list has been configured, the **platform admin** is the sole default signer — providing a safe, backward-compatible baseline (effective threshold = 1).

```
UpgradeSigners list (if configured)
  └─ One call per signer per WASM hash
  └─ Approvals accumulate until threshold is met

Fallback (no list configured)
  └─ Admin address is the implicit single signer
```

The signers list is managed exclusively by the admin via `set_upgrade_signers`. Passing an empty list reverts to the admin-only default.

### Who can execute an approved upgrade?

Only the **admin** may call `execute_upgrade`. Even after a proposal has been fully approved and the cooldown period has elapsed, a non-admin caller cannot trigger execution. This separates the approval panel (signers) from the execution key (admin).

### Who can cancel an upgrade?

Only the **admin** may call `cancel_upgrade_wasm`.

### Permission summary

| Action | Required Caller |
|---|---|
| `propose_upgrade_wasm` | Any address in UpgradeSigners (or admin if no list set) |
| `set_upgrade_signers` | Admin only |
| `set_upgrade_threshold` | Admin only |
| `set_wasm_upgrade_cooldown` | Admin only |
| `execute_upgrade` | Admin only |
| `cancel_upgrade_wasm` | Admin only |
| `get_upgrade_proposal` | Anyone (read-only) |
| `get_upgrade_approvals` | Anyone (read-only) |
| `get_upgrade_history` | Anyone (read-only) |

---

## 2. Voting Threshold Requirements

The upgrade process uses an **M-of-N multi-signature** model:

- **N** is the number of addresses in the UpgradeSigners list.
- **M** (the threshold) is configured via `set_upgrade_threshold` by the admin.
- Default threshold is **1** (single-signer, admin-equivalent baseline).
- The threshold must be `>= 1`; setting it to 0 is rejected with `Error::InvalidFee`.

### How approval accumulation works

Each signer calls `propose_upgrade_wasm` with the **exact same `new_wasm_hash`**. Approvals for that hash are stored and counted. Two integrity checks enforce correctness:

1. **No duplicate approvals** — if the same address calls again for the same hash, the call returns `Error::AlreadyApproved`. The approval is not counted twice.
2. **Only current signers count** — when threshold is checked, only approvals from addresses still present in the active UpgradeSigners list are counted. If a signer is removed after approving, their approval is no longer counted toward the threshold. This prevents stale or rotated keys from lingering as an implicit vote.

Once the threshold is reached, the proposal is **committed**: the `WasmUpgradeProposal` record is stored on-chain, the approval accumulator is cleared, and the cooldown clock starts. The event `UPG_PROP` is emitted on the `wasm_upgrade` topic.

Only **one proposal may be pending at a time**. Attempting to propose while one already exists returns `Error::UpgradeProposalExists`.

### Zero-hash rejection

The hash `0x00…00` (32 zero bytes) is always rejected with `Error::InvalidUpgradeHash`. This prevents accidentally deploying a blank or uninitialized WASM image.

---

## 3. Cooldown and Anti-Bypass Timers

The upgrade system enforces three independent time-based gates, all defaulting to **7 days** (604 800 seconds):

| Timer | Constant | Default | Purpose |
|---|---|---|---|
| Execution cooldown | `DEFAULT_WASM_UPGRADE_COOLDOWN` | 7 days | Mandatory review window between approval and execution |
| Cancel-repropose cooldown | `CANCEL_REPROPOSE_COOLDOWN` | 7 days | Prevents cancelling a proposal to immediately re-propose and skip the review window |
| Admin recovery delay | `ADMIN_RECOVERY_DELAY` | 7 days | Timelock before a fallback admin can complete a recovery |

### Execution cooldown

When a proposal is committed (threshold reached), `upgrade_at` is set to `proposed_at + wasm_upgrade_cooldown`. The `execute_upgrade` call will fail with `Error::UpgradeCooldownActive` if the current ledger timestamp is still before `upgrade_at`. This gives the community and operators time to inspect the proposal and cancel it if it is malicious or unintended.

The cooldown can be adjusted by the admin via `set_wasm_upgrade_cooldown`, but only affects **future** proposals. Any currently committed proposal retains the `upgrade_at` timestamp calculated at proposal time.

### Cancel-repropose cooldown (Issue #618)

Without this protection, an admin could cancel a proposal immediately after it was made (avoiding the review window), then re-propose the same hash — bypassing the cooldown entirely. The `LastUpgradeCancelledAt` timestamp is recorded on every cancel. Any new `propose_upgrade_wasm` call within 7 days of the last cancellation is blocked with `Error::UpgradeCooldownActive`.

---

## 4. Executing a Committed Upgrade

Once the cooldown has elapsed, the admin calls:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  execute_upgrade \
  --expected_wasm_hash <HEX_HASH>
```

The `expected_wasm_hash` parameter is a **defense-in-depth check**: the contract verifies that the hash the admin claims to be executing matches what is actually stored on-chain as the pending proposal. This prevents a scenario where the admin's signing tool presents a different hash than what was approved (e.g., a UI spoofing attack or a race condition). If the hashes do not match, `Error::InvalidUpgradeHash` is returned and no upgrade occurs.

On success:
- The new WASM is deployed to the contract via Soroban's `update_current_contract_wasm`.
- `ContractVersion` is incremented by exactly 1.
- An `UpgradeRecord` (previous version, new version, WASM hash, admin address, timestamp) is appended to `UpgradeHistory`.
- The pending proposal is cleared atomically.
- The `UPG_EXEC` event is emitted on the `wasm_upgrade` topic.

---

## 5. Cancelling an Upgrade

The admin may cancel a pending proposal at any time before execution:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  cancel_upgrade_wasm
```

If no proposal is pending, the call returns `Error::NoUpgradeProposed` rather than silently succeeding — making accidental double-cancels visible. Cancellation records the timestamp in `LastUpgradeCancelledAt` to enforce the cancel-repropose cooldown (see §3). The `UPG_CANC` event is emitted so cancellations appear in the audit trail alongside proposals.

---

## 6. Emergency Lock-Out Procedures

### 6.1 Emergency pause

The admin can immediately halt all escrow operations (fund creation, release, refund, dispute, batch operations) by calling:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  set_paused \
  --paused true
```

When `is_paused` is `true`, escrow creation, release, refund initiation, dispute initiation, staking, and recurring cycle releases return `Error::ContractPaused` (code 15). Read-only views remain available. Admin/arbitrator paths needed to *clear* emergency conflicts — dispute resolution, upgrade cancel, fund sweep, and admin recovery — stay reachable while paused. Emits the `platform_paused` event.

To resume normal operations:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  set_paused \
  --paused false
```

Emits the `platform_unpaused` event.

**Use the pause mechanism when:**
- A malicious upgrade proposal has been committed and you need time to cancel it before the cooldown expires.
- An exploit is being actively exploited and you need to freeze escrow state while investigating.
- A planned maintenance window requires freezing new fund movements.

### 6.2 Blocking a pending upgrade

If a committed proposal has not yet reached its `upgrade_at` timestamp, the admin can cancel it via `cancel_upgrade_wasm` (§5). The 7-day cancel-repropose cooldown then prevents the cancelled payload from being immediately re-proposed.

### 6.3 Admin key compromise — recovery procedure

If the primary admin key is suspected to be compromised, the **fallback admin** can initiate recovery. The fallback admin is set during contract initialization (or after an admin transfer) and stored under `DataKey::FallbackAdmin`.

Recovery is a two-step process:

**Step 1 — Initiate the timelock:**

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <FALLBACK_ADMIN_KEY> \
  --network <NETWORK> \
  -- \
  recover_admin_access \
  --recovered_admin <NEW_ADMIN_ADDRESS>
```

On first call the contract records `current_time + 7 days` as `AdminRecoveryTime`, emits an `admin_recovery_initiated` event, parks an `AdminRecovery` emergency operation in `Executing`, auto-pauses the platform, and returns **`Ok(())`** so the timelock persists (Soroban reverts all storage writes when a contract returns `Err`).

**Step 2 — Complete recovery after 7 days:**

After 7 days have elapsed, repeat the identical call. The contract verifies:
- The recorded cooldown is at least `MIN_ADMIN_RECOVERY_COOLDOWN` (7 days) — direct writes to storage that attempt to shorten the delay are rejected.
- The current ledger time has passed `AdminRecoveryTime`.
- No active disputes, pending WASM upgrade, or active recurring escrows remain (otherwise `Error::EmergencyConflictActive`).

On success, the admin is updated to `recovered_admin`, the timelock entries are cleared, the emergency operation is marked `Completed`, and the `admin_recovered` event is emitted. The platform remains paused until an explicit `set_paused(false)`.

Calling `recover_admin_access` again before the timelock elapses returns `Error::AdminRecoveryFailed`. Operators can abort a parked recovery with `abort_emergency_operation`.

**Security properties of admin recovery:**
- The fallback admin alone cannot instantly seize admin rights; the 7-day window allows the legitimate admin to respond.
- The minimum cooldown floor is enforced in code, not just in storage, preventing direct-write bypasses.
- The `recovered_admin` address is validated; it cannot be the contract's own address.

### 6.4 Unified emergency operations framework

Pause, admin recovery, fund sweep, and upgrade execution are serialized through an on-chain emergency operation lock so they cannot interleave unsafely.

| Kind | Behavior |
|---|---|
| `Pause` / `Unpause` | Takes the lock for the duration of the call; unpause is rejected while recovery/sweep is `Executing` |
| `AdminRecovery` | Parks in `Executing` across the 7-day timelock; blocks sweep/propose/unpause until completed or aborted |
| `Sweep` | Requires clear dependencies; moves only `balance - TotalLocked - TotalStaked` |
| `UpgradeExecute` / `UpgradeCancel` | Audited; propose/execute are blocked while a fund-emergency lock is held. Cancel remains available so operators can clear upgrade conflicts |

**Dependency gate** (recovery and sweep): no pending `WasmUpgradeProposal`, `ActiveDisputeCount == 0`, and `ActiveRecurringCount == 0`.

**Views / control:** `get_emergency_operation`, `get_emergency_operation_history`, `abort_emergency_operation`, `get_fund_allocation`, `get_active_dispute_count`, `get_active_recurring_count`.

**Partial recovery:** abort a parked `AdminRecovery` (clears the timelock) then start a new emergency sequence, or wait out the timelock and complete.

---

## 7. Admin Key Management

### Standard rotation (two-step transfer)

Admin rotation uses a two-step co-signing flow to ensure the incoming key is live and controlled:

1. Current admin calls `update_admin(new_admin)` — both the current admin and the new admin must authorize this transaction simultaneously.
2. New admin calls `claim_admin()` to finalize the transfer.

The current admin can abort the pending transfer at any time with `cancel_admin_transfer`. Both steps emit audit events on the `admin_change` topic.

**Assumption:** The self-address (`env.current_contract_address()`) is rejected as an admin address by `validate_admin_address`. This prevents the contract from being locked into an uncontrollable state.

---

## 8. Audit Trail and History

All upgrade lifecycle events are published to the `wasm_upgrade` event topic:

| Symbol | Event | Trigger |
|---|---|---|
| `UPG_PROP` | Proposal committed | Threshold reached in `propose_upgrade_wasm` |
| `UPG_CANC` | Proposal cancelled | `cancel_upgrade_wasm` called |
| `UPG_EXEC` | Upgrade executed | `execute_upgrade` succeeded |

Each event includes the WASM hash, the admin/signer address, the current ledger timestamp, and the scheduled `upgrade_at` time.

The on-chain `UpgradeHistory` log (readable via `get_upgrade_history`) retains up to **32** records. Once the cap is reached, the oldest entry is dropped (FIFO). For long-term compliance or forensic audit trails, operators should mirror `wasm_upgrade` events to off-chain storage.

`get_version` and `get_version_info` return the current `ContractVersion` counter, which increments by exactly 1 for each successful execution.

---

## 9. Security Assumptions and Invariants

The upgrade model relies on the following assumptions. Violating them weakens the security guarantees:

| # | Assumption | Risk if violated |
|---|---|---|
| 1 | The admin key is held in a hardware or multi-sig wallet, not a hot wallet | A compromised admin key can cancel, re-propose, and execute any upgrade without signer cooperation if threshold=1 |
| 2 | The UpgradeSigners list is maintained and rotated as personnel changes | Removed signers cannot accumulate approvals once rotated off the list, but stale keys still exist on-chain until explicitly replaced |
| 3 | The 7-day cooldown is sufficient review time for the community/security team | Malicious code changes require more than 7 days to detect if the audit pipeline is slow |
| 4 | The `expected_wasm_hash` passed to `execute_upgrade` is verified off-chain before signing | The on-chain check is defense-in-depth; the admin must also independently verify the WASM contents match the intended changes |
| 5 | The fallback admin key is stored separately from the primary admin key | If both keys are stored together, a single compromise defeats the recovery mechanism |
| 6 | Off-chain monitoring of `wasm_upgrade` events is active | Without monitoring, a proposal could be committed and executed within the cooldown window without any human review |
| 7 | The threshold is set to at least 2 for production deployments | A threshold of 1 (default) means a single signer key compromise is sufficient to commit an upgrade proposal |

---

## 10. Quick-Reference CLI Commands

All commands target a deployed Soroban contract. Replace placeholders in angle brackets.

```bash
# --- Inspecting upgrade state ---

# Check for a pending upgrade proposal
stellar contract invoke --id <CONTRACT_ID> --source <ANY> --network <NETWORK> \
  -- get_upgrade_proposal

# Check accumulated approvals for a specific WASM hash
stellar contract invoke --id <CONTRACT_ID> --source <ANY> --network <NETWORK> \
  -- get_upgrade_approvals --wasm_hash <HEX_HASH>

# Read the upgrade history log (up to 32 entries)
stellar contract invoke --id <CONTRACT_ID> --source <ANY> --network <NETWORK> \
  -- get_upgrade_history

# Read current contract version
stellar contract invoke --id <CONTRACT_ID> --source <ANY> --network <NETWORK> \
  -- get_version

# Read current threshold
stellar contract invoke --id <CONTRACT_ID> --source <ANY> --network <NETWORK> \
  -- get_upgrade_threshold

# --- Proposing an upgrade (signer) ---

stellar contract invoke --id <CONTRACT_ID> --source <SIGNER_KEY> --network <NETWORK> \
  -- propose_upgrade_wasm \
  --signer <SIGNER_ADDRESS> \
  --new_wasm_hash <HEX_HASH>

# --- Admin: configure upgrade parameters ---

# Set a 2-of-3 threshold
stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- set_upgrade_threshold --threshold 2

# Replace the signers list
stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- set_upgrade_signers \
  --signers '["<SIGNER_1>", "<SIGNER_2>", "<SIGNER_3>"]'

# Adjust cooldown (e.g., 14 days = 1209600 seconds)
stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- set_wasm_upgrade_cooldown --cooldown_seconds 1209600

# --- Admin: execute or cancel ---

stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- execute_upgrade --expected_wasm_hash <HEX_HASH>

stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- cancel_upgrade_wasm

# --- Emergency pause / unpause ---

stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- set_paused --paused true

stellar contract invoke --id <CONTRACT_ID> --source <ADMIN_KEY> --network <NETWORK> \
  -- set_paused --paused false
```
