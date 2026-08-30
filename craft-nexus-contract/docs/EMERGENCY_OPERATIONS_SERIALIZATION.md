# Emergency Operations Serialization (#1072)

## Overview

CraftNexus implements a mutual-exclusion mechanism for critical emergency operations (pause, recovery, sweep, upgrade) to prevent concurrent interference. This document describes the state machine, stranding prevention, and conflict resolution.

## State Machine

### Operation States

Emergency operations transition through the following lifecycle:

```
┌─────────────┐
│    Idle     │ ← Default: no operation in flight
└─────┬───────┘
      │ acquire()
      ↓
┌──────────────────┐
│   Executing      │ ← Operation in progress
│ (locked)         │
└────┬─────────┬───┘
     │success()│error()
     ↓        ↓
┌─────────┐┌───────┐
│Completed││ Failed│ ← Terminal: lock released
└─────────┘└───────┘
```

### State Representation

Each in-flight operation is tracked by `EmergencyOperation`:

```rust
pub struct EmergencyOperation {
    pub kind: EmergencyOpKind,      // Type: AdminRecovery, Sweep, Upgrade, Pause
    pub actor: Address,             // Who initiated
    pub phase: EmergencyOpPhase,    // Executing, Completed, or Failed
    pub revision: u32,              // Increments on each state transition
    pub started_at: u64,            // Ledger timestamp
    pub success: bool,              // Success flag (when phase != Executing)
    pub amount: i128,               // Optional: amount affected (e.g., swept)
}
```

**Revision Semantics**

Revisions serve dual purposes:
1. **Optimistic Concurrency Control**: A concurrent call with stale revision can be rejected
2. **Audit Trail**: Revision increments distinguish "1st recovery attempt" from "2nd recovery attempt"

Revisions increment:
- On lock acquisition: `revision + 1`
- On successful completion: `revision + 1`
- On failure: `revision + 1`

## Operation Types

### AdminRecovery

**Purpose**: Restore access via fallback admin after primary admin loss

**Authorization**: `fallback_admin.require_auth()`

**Atomicity Model**: Multi-step (time-locked, 7-day delay)

**Conflicts Block**: Disputes (ActiveDisputeCount > 0), Upgrades (WasmUpgradeProposal exists), Recurring Escrows (ActiveRecurringCount > 0)

**Failure Mode**: Lock releases on error; timeout/force-release via `abort_emergency_operation()` for multi-step failures

### Sweep

**Purpose**: Recover unallocated funds held by contract

**Authorization**: `admin.require_auth()`

**Atomicity Model**: Single-transaction

**Conflicts Block**: Any other in-flight operation

**Failure Mode**: Transaction atomicity ensures automatic rollback; lock never strands within a transaction

### Upgrade

**Purpose**: Deploy new WASM contract code

**Authorization**: `admin.require_auth()` (multi-sig via upgrade signers)

**Atomicity Model**: Single-transaction (proposal → execute)

**Conflicts Block**: Any other in-flight operation

**Failure Mode**: Transaction atomicity ensures automatic rollback

### Pause

**Purpose**: Pause/unpause platform to halt marketplace operations

**Authorization**: `admin.require_auth()`

**Atomicity Model**: Single-transaction

**Conflicts Block**: Unpause blocked when AdminRecovery executing; other operations blocked when paused by check_not_paused()

**Failure Mode**: Transaction atomicity ensures automatic rollback

## Lock Stranding Prevention

### Soroban Transaction Atomicity

**Platform Characteristic**: Soroban transactions are ACID-compliant. If any operation fails or panics:
- ALL state changes roll back automatically
- Lock acquisition (set CurrentEmergencyOperation) reverts
- Lock cannot strand due to within-transaction failure

**Implication**: Within-transaction stranding is impossible.

### Multi-Step Operation Risk

**Scenario**: `AdminRecovery` is a two-step operation (initiate on day 0, complete on day 7).
- Transaction 1: Caller initiates recovery, lock acquired, timelock set
- Days 1-6: Caller disappears or fails to call day-7 completion
- Transaction 2: Never happens
- Result: Lock stuck in Executing state indefinitely

**Solution**: Force-release via `abort_emergency_operation()` by authorized admin

### Force-Release Mechanism

```rust
pub fn abort_emergency_operation(admin: Address) -> Result<EmergencyOperation, Error> {
    admin.require_auth();  // Enforces authorization
    
    // Transition to Failed and release lock
    let op = read_current_emergency_operation();
    op.phase = Failed;
    remove_current_emergency_operation();
    append_to_history(op);
    
    Ok(op)
}
```

**Authorization**: Admin (same as other emergency operations)

**Effect**: 
- Transitions operation to Failed phase
- Releases CurrentEmergencyOperation lock
- Appends to history for audit trail

**When to Use**: Manually called after a multi-step operation times out (e.g., recovery not completed by day 8)

## Conflict Detection

### Mutual Exclusion Matrix

| Operation | Blocks | Blocked By |
|-----------|--------|-----------|
| AdminRecovery | All others | Disputes, Upgrades, Recurring |
| Sweep | All others | Any in-flight |
| Upgrade | All others | Any in-flight |
| Pause | Unpause only | AdminRecovery (unpause) |

### Conflict Resolution

**Same Operation Re-entrancy**
```rust
if current_op.kind != requested_op.kind {
    return Err(EmergencyOpInProgress);  // Different operation in progress
}
```

**Pre-Operation Conflicts** (AdminRecovery only)
```rust
if active_disputes > 0 {
    return Err(EmergencyConflictActive);
}
if upgrade_proposal_exists() {
    return Err(EmergencyConflictActive);
}
if active_recurring > 0 {
    return Err(EmergencyConflictActive);
}
```

**Error Diagnostics**: Each error type clearly indicates what is blocking:
- `EmergencyOpInProgress`: Another operation is in flight (generic)
- `EmergencyConflictActive`: Specific conflict exists (disputes, upgrades, recurring)

## Error Codes

| Code | Name | Scenario |
|------|------|----------|
| 85 | EmergencyOpInProgress | Another emergency operation is executing (all types) |
| 86 | EmergencyConflictActive | Pre-operation conflict: disputes, upgrades, recurring escrows exist |

## Query API (Read-Only)

### `get_emergency_operation() -> Option<EmergencyOperation>`

Returns current in-flight operation, if any.

**Authorization**: None (freely queryable)

**Use Case**: Operators diagnose active incident response operations

**Returns**:
- `None` if Idle (no operation in flight)
- `Some(op)` if any operation is Executing, Completed, or Failed

### `get_emergency_operation_history(offset: u32, limit: u32) -> Vec<EmergencyOperation>`

Returns paginated history of completed/failed operations (bounded at 100 entries).

**Authorization**: None (freely queryable)

**Use Case**: Audit trail for incident timelines

**Pagination**: Offset/limit follow standard patterns (0-indexed); max 50 entries per page

### `get_active_recurring_count() -> u32`

Returns count of active recurring escrows.

**Authorization**: None (freely queryable)

**Use Case**: Conflict detection for recovery; also useful for marketplace monitoring

## Integration Example

```rust
// Initiate emergency operation
Self::assert_emergency_op_idle_and_acquire(&env, &actor, EmergencyOpKind::AdminRecovery)?;

try {
    // Perform operation
    perform_recovery(&env)?;
    
    // On success, release lock and record history
    Self::release_emergency_op_on_success(&env, EmergencyOpKind::AdminRecovery, 0);
} catch (error) {
    // On failure, release lock with Failed phase
    Self::release_emergency_op_on_failure(&env);
    return Err(error);
}
```

## Testing Strategy

### Conflict Scenarios

1. **Concurrent Operations**: Two different ops attempted concurrently → second blocked
2. **Same-Op Re-entrancy**: Same operation attempted twice → second blocked
3. **Pre-Operation Conflicts**: Recovery blocked by disputes/upgrades/recurring
4. **Pause/Unpause Conflicts**: Unpause blocked during recovery

### Stranding Prevention

1. **Transaction Rollback**: Failed within-transaction operation releases lock automatically
2. **Multi-Step Timeout**: Force-release stranded operation via `abort_emergency_operation()`

### Audit Trail

1. **History Recording**: Completed/failed operations appended to history
2. **Revision Tracking**: Each transition increments revision for diagnostics
3. **Actor Tracking**: Original operation initiator recorded for accountability

## Implementation Details

### Storage

```rust
DataKey::CurrentEmergencyOperation           // In-flight op (present if Executing)
DataKey::EmergencyOperationHistory           // Bounded vec of completed ops
DataKey::EmergencyOperationHistoryCount      // Count (for pagination)
DataKey::EmergencyOperationHistoryIndexed(u32) // Indexed access
DataKey::ActiveRecurringCount                // For conflict detection
```

### Helper Functions

```rust
fn assert_emergency_op_idle_and_acquire(...)     // Atomic lock acquisition
fn release_emergency_op_on_success(...)           // Clean completion
fn release_emergency_op_on_failure(...)           // Failure without stranding
fn append_to_emergency_history(...)               // Audit trail recording
```

## Backwards Compatibility

- Existing authorization requirements preserved (each operation retains its auth check)
- New error codes (85, 86) are additive
- Existing operations (sweep, upgrade, recovery, pause) unchanged in happy path
- Only failure paths and cross-operation interactions differ

## Monitoring and Alerting

### Key Metrics

- **Current Operation**: `get_emergency_operation() != None`
- **Operation Kind**: For understanding which emergency response is active
- **Operation Actor**: For accountability and audit trails
- **Revision**: For detecting rapid repeated attempts (diagnostic)
- **History Count**: For trending emergency response frequency

### Suggested Alerts

- Alert if any emergency operation remains Executing > 1 hour
- Alert if recovery operation Executing > 24 hours (likely abandoned)
- Alert if same operation attempted multiple times in short window (diagnosis of stuck state)

## Conclusion

The emergency operations serialization mechanism ensures:
1. **Mutual Exclusion**: No concurrent emergency operations
2. **Conflict Awareness**: Clear diagnostic errors when conflicts exist
3. **Stranding Prevention**: Multi-step operations can be force-released
4. **Auditability**: All operations recorded with actor, revision, and outcome
5. **Transparency**: Public query API for operator visibility during incidents
