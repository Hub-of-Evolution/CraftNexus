# CraftNexus Smart Contract - Implementation Guide & Best Practices

## Table of Contents
1. [Admin Access Recovery Scenarios](#admin-access-recovery-scenarios)
2. [Stake Queue Management](#stake-queue-management)
3. [Integration Patterns](#integration-patterns)
4. [Monitoring & Debugging](#monitoring--debugging)
5. [Migration Guide](#migration-guide)

---

## Admin Access Recovery Scenarios

### Scenario 1: Normal Admin Transfer

**Situation**: Platform operator needs to change admin (e.g., key rotation, personnel change)

```rust
// Step 1: Current admin proposes new admin
// This can be done by current admin or via governance
current_admin.call_method("update_admin")
    .arg(new_admin_address)
    .invoke();

// Event emitted: admin_changed
// Topic: "admin_changed" with type "admin_proposed"
// Data: (current_admin, new_admin_address)

// Step 2: New admin claims the role (usually within hours/days)
new_admin.call_method("claim_admin").invoke();

// Events emitted:
// 1. admin_changed - type "admin_claimed"
// 2. Set fallback admin internally (automatic)

// Result: new_admin_address is now admin, can perform admin operations
```

**Key Points**:
- Two-step process ensures new admin has access before old admin loses it
- Fallback admin automatically set for recovery capability
- Clear audit trail of who proposed and who claimed
- New admin should verify they have claim success event before considering role transfer complete

---

### Scenario 2: Fallback Admin Recovery (after 7 days)

**Situation**: Primary admin key is compromised or lost, no way to propose new admin

```rust
// Step 1: Fallback admin initiates recovery
// Fallback admin is typically a multisig contract or highly secure account
fallback_admin.call_method("recover_admin_access")
    .arg(recovery_admin_address)
    .invoke();

// Result: Returns Error::AdminRecoveryFailed (expected)
// Event emitted: admin_recovery_initiated
// Topic: "admin_recovery_initiated"
// Data: "7-day time lock initiated for admin recovery"

// Timeline: Admin must wait 7 days from this call

// Step 2: After 7 calendar days have passed, fallback admin calls again
// Must use SAME fallback admin that initiated recovery
fallback_admin.call_method("recover_admin_access")
    .arg(recovery_admin_address)
    .invoke();

// Result: Ok(())
// Event emitted: admin_changed
// Topic: "admin_changed" with type "admin_recovered"
// Data: (previous_admin, recovery_admin_address)

// Result: recovery_admin_address is now admin
```

**Timing Calculation**:
```
Initiation Time: 1000 (arbitrary timestamp)
Recovery Delay: 604800 seconds (7 days)
Earliest Recovery: 1604800

Call 1 (timestamp 1000): Returns Error::AdminRecoveryFailed (timer starts)
Call 2 (timestamp 1604799): Returns Error::AdminRecoveryFailed (not yet elapsed)
Call 2 (timestamp 1604800+): Returns Ok(()) (recovery completes)
```

**Key Points**:
- 7-day delay is critical for security - prevents immediate exploitation if fallback key compromised
- Same fallback admin must initiate and complete recovery
- Can only have one active recovery at a time (timer cleared after completion)
- Recovery sets admin to provided address and clears timer for next cycle

---

### Scenario 3: Recovery from Config Corruption

**Situation**: Primary platform config storage is corrupted (rare but possible)

```rust
// When any admin operation is attempted:
let config = Self::get_platform_config_safe(&env);

match config {
    Ok(valid_config) => {
        // Primary config was valid, use normally
        // Proceed with operation using valid_config
    }
    Err(Error::CorruptedPlatformConfig) => {
        // Both primary and fallback configs are missing
        // This is a critical failure
        env.panic_with_error(Error::CorruptedPlatformConfig);
    }
}
```

**If fallback admin exists but primary config corrupted**:
- Automatic fallback to minimal valid configuration:
  - `platform_fee_bps`: 500 (5% default)
  - `admin`: fallback_admin_address
  - `platform_wallet`: fallback_admin_address
  - `is_paused`: true (conservative - pauses contract)
  - Default values for other fields

**Event Emitted**:
```
Topic: "admin_config_recovered"
Value: true
Data: "Using fallback admin after config corruption detected"
```

**Recovery Steps**:
1. **Detect**: Monitor for `admin_config_recovered` events
2. **Notify**: Alert platform operators immediately
3. **Investigate**: Determine what caused corruption
4. **Unpause**: Fallback admin can call `set_paused(false)` to resume operations
5. **Restore**: Re-initialize platform config with correct values

---

## Stake Queue Management

### Scenario 1: Normal Staking Workflow

```rust
// Artisan stakes 1000 tokens
artisan.call_method("stake_tokens")
    .arg(&token_address)
    .arg(1000i128)
    .invoke();

// Contract actions:
// 1. Transfers 1000 tokens from artisan to contract
// 2. Records stake in DataKey::ArtisanStake(artisan)
// 3. Records token in DataKey::ArtisanStakeToken(artisan)
// 4. Initializes cooldown: current_time + 604800 (7 days)
// 5. Records operation in history queue
// 6. Emits event: stake_operation with "stake_added" and amount 1000

// Event emitted:
// Topic: "stake_operation"
// Subtopic: "stake_added"
// Value: (artisan_address, 1000)

// Cooldown state after operation:
// DataKey::StakeCooldownEnd(artisan) = ledger.timestamp() + 604800
```

**Checking Current Stake**:
```rust
// Query stake amount
let stake = contract.get_stake(&artisan_address);  // Returns: 1000

// Query cooldown status
let cooldown_end: u64 = storage.get(&DataKey::StakeCooldownEnd(artisan));
let current_time = env.ledger().timestamp();

if current_time < cooldown_end {
    let time_remaining = cooldown_end - current_time;
    println!("Cooldown active, {} seconds remaining", time_remaining);
} else {
    println!("Artisan can now unstake");
}
```

---

### Scenario 2: Gaming Prevention - Cooldown Reset

**The Problem (OLD BEHAVIOR)**:
```
Day 1: Stake 100 tokens → Cooldown = Day 1 + 7 days = Day 8
Day 4: Stake 50 more tokens → Cooldown = Day 4 + 7 days = Day 11 (EXTENDED!)
Day 10: Stake 1 token → Cooldown = Day 10 + 7 days = Day 17 (EXTENDED AGAIN!)
Result: Artisan can continuously extend cooldown period indefinitely
```

**The Solution (NEW BEHAVIOR)**:
```
Day 1: Stake 100 tokens → Cooldown = Day 1 + 7 days = Day 8
Day 4: Stake 50 more tokens → Cooldown = Day 8 (NOT RESET, still the same)
Day 10: Stake 1 token → Cooldown = Day 8 (NOT RESET, still the same)
Day 8: Unstake all tokens → Success (cooldown expired, even after additional stakes)
Result: Artisan CANNOT extend cooldown period via additional stakes
```

**Implementation Detail**:
```rust
// In stake_tokens:
let existing_cooldown: u64 = storage.get(&StakeCooldownEnd(artisan)).unwrap_or(0);

if existing_cooldown == 0 {
    // First stake: initialize cooldown
    let config = get_platform_config();
    let cooldown_end = current_time + config.stake_cooldown;
    storage.set(&StakeCooldownEnd(artisan), &cooldown_end);
}
// If existing_cooldown > 0, do NOT update it
```

---

### Scenario 3: Stake History Queue at Capacity

```rust
// Assume artisan has made 100 stake operations (queue full)

// Next stake attempt:
artisan.stake_tokens(&token, 50);

// Contract actions:
// 1. Performs stake operation normally (no rejection)
// 2. Attempts to record in history: record_stake_history()
// 3. Checks: current_count >= MAX_STAKE_HISTORY_SIZE (100 >= 100)
// 4. Returns: Err(StakeQueueFull)
// 5. Panics with: Error::StakeQueueFull

// Transaction fails
// Result: Artisan cannot stake until queue is pruned
```

**Preventing Queue Saturation**:
```rust
// System automatically prunes when reaching 80% capacity
// At 80 entries: Prune keeps 40 most recent, deletes 40 oldest
// This prevents hitting the hard 100 limit

// Timeline:
// 0-79 entries: Normal operation
// 80 entries: Automatic pruning triggered
// After prune: Queue has 40 entries
// 41-119 entries: More operations possible
// 120+ entries: Would trigger pruning again

// In practice, this prevents hitting the hard limit
```

---

### Scenario 4: Unstake During Full History Queue

```rust
// Assume stake history queue is full

// Artisan unstakes (assuming cooldown expired):
artisan.unstake_tokens(&token);

// Contract actions:
// 1. Validates cooldown has expired (passes)
// 2. Validates stake exists and token matches (passes)
// 3. Records unstake in history: record_stake_history()
// 4. Attempts to record removal: Returns Err(StakeQueueFull)
// 5. Does NOT panic (graceful error handling)
// 6. Emits warning event: stake_history_warning
// 7. Clears stake metadata
// 8. Transfers tokens back to artisan
// 9. Transaction SUCCEEDS despite queue being full

// Events emitted:
// 1. stake_history_warning: "Could not record stake removal in history"
// 2. Tokens successfully returned to artisan

// Result: Artisan gets their tokens back, warning logged for monitoring
```

**Key Point**: The unstake ALWAYS succeeds, even if history queue is full. This prevents permanent fund locks.

---

## Integration Patterns

### Pattern 1: Monitoring Admin Changes

```rust
// Off-chain monitoring service

struct AdminChangeEvent {
    previous_admin: Address,
    new_admin: Address,
    change_type: String,  // "admin_proposed", "admin_claimed", "admin_recovered"
    timestamp: u64,
}

fn handle_admin_changed_event(event: AdminChangeEvent) {
    match event.change_type.as_str() {
        "admin_proposed" => {
            // Alert: New admin proposed
            notify_admins(&format!(
                "New admin proposed: {}",
                event.new_admin
            ));
        }
        "admin_claimed" => {
            // Alert: Admin role claimed (transfer complete)
            notify_admins(&format!(
                "Admin role claimed by: {}",
                event.new_admin
            ));
        }
        "admin_recovered" => {
            // CRITICAL: Recovery mechanism used
            alert_critical(&format!(
                "ADMIN RECOVERY USED! Previous: {}, New: {}",
                event.previous_admin, event.new_admin
            ));
            // Investigate immediately
        }
    }
}
```

### Pattern 2: Stake History Analytics

```rust
// On-chain event emission in stake_tokens/unstake_tokens
// Off-chain consumer for analytics

#[derive(Debug)]
struct StakeOperation {
    artisan: Address,
    operation: String,  // "stake_added" or "stake_removed"
    new_stake: i128,
    timestamp: u64,
}

fn build_artisan_timeline(artisan: Address) -> Vec<StakeOperation> {
    // Query all stake_operation events for this artisan
    // Sort by timestamp
    // Build timeline of stake changes
}

fn detect_suspicious_patterns(artisan: Address) {
    let timeline = build_artisan_timeline(artisan);
    
    // Pattern detection examples:
    // 1. Frequent small stakes (potential test transactions)
    // 2. Stakes all exactly same amount (automated tool?)
    // 3. Unusual timing patterns (suspicious activity)
    
    for operation in timeline.windows(2) {
        if is_suspicious(operation[0], operation[1]) {
            alert_moderator(artisan);
        }
    }
}
```

### Pattern 3: Graceful Config Recovery

```rust
// Wrapping contract calls with config recovery

fn call_admin_function<F: Fn(PlatformConfig) -> Result<T, Error>>(
    operation: F
) -> Result<T, Error> {
    // Try to get safe config (includes fallback mechanism)
    match Self::get_platform_config_safe(&env) {
        Ok(config) => {
            // Use safe config for operation
            operation(config)
        }
        Err(Error::CorruptedPlatformConfig) => {
            // Cannot recover, fail loudly
            // Triggers emergency procedures
            env.panic_with_error(Error::CorruptedPlatformConfig);
        }
        Err(e) => {
            env.panic_with_error(e);
        }
    }
}

// Example usage:
pub fn some_admin_operation(env: Env) {
    call_admin_function(|config| {
        // Perform operation using config
        // Config is guaranteed valid or recovered
        Ok(())
    })
}
```

---

## Monitoring & Debugging

### Monitoring Dashboard Metrics

```
Admin Security:
  - Last admin change: <timestamp>
  - Current admin: <address>
  - Fallback admin set: yes/no
  - Recovery time lock active: yes/no
  - Days until recovery allowed: <number>

Stake Queue Health:
  - Active artisans: <count>
  - Total artisans ever: <count>
  - Max history queue size per artisan: 100
  - Avg artisan history entries: <count>
  - Pruning events triggered: <count>
  - StakeQueueFull errors: <count>
  
Storage Usage:
  - Admin recovery storage: ~40 bytes
  - Per artisan stake queue: ~4 KB max
  - Total estimated storage: <bytes>
```

### Debug Queries

```rust
// Check fallback admin status
let has_fallback = storage.contains(&DataKey::FallbackAdmin);
let fallback_admin = storage.get::<_, Address>(&DataKey::FallbackAdmin);

// Check recovery time lock
let recovery_time: u64 = storage.get(&DataKey::AdminRecoveryTime).unwrap_or(0);
let now = env.ledger().timestamp();
if recovery_time > 0 && now < recovery_time {
    let wait_seconds = recovery_time - now;
    println!("Recovery available in {} seconds", wait_seconds);
}

// Check stake history queue
let history_count: u32 = 
    storage.get(&DataKey::StakeHistoryCount(artisan)).unwrap_or(0);
let last_modified: u64 = 
    storage.get(&DataKey::StakeLastModified(artisan)).unwrap_or(0);
println!("Artisan {} history entries: {}", artisan, history_count);
println!("Last modified: {}", last_modified);

// Check if pruning needed
if history_count >= STAKE_HISTORY_PRUNE_THRESHOLD {
    println!("Pruning threshold reached for artisan {}", artisan);
}
```

---

## Migration Guide

### For Existing Deployments

**Important**: This implementation is **fully backward compatible**. Existing contracts do NOT require migration.

**Steps to Add Recovery Capability to Existing Contract**:

1. **Deploy new contract version** with new code
2. **Call `set_fallback_admin`** after deployment:
   ```rust
   // New admin address (after claiming role)
   new_admin.call_method("claim_admin").invoke();
   // Automatically sets fallback admin internally
   ```
3. **Monitor recovery mechanism**:
   - Subscribe to `admin_changed` events
   - Watch for `admin_config_recovered` events
   - Alert on `admin_recovery_initiated` events

4. **Test recovery procedure** (optional, on testnet):
   - Initiate recovery with fallback admin
   - Wait 7 days
   - Complete recovery
   - Verify new admin can execute operations

### For New Deployments

**Initial Setup**:

1. **Deploy contract** with new code
2. **Initialize platform** with `initialize_platform`:
   ```rust
   contract.initialize_platform(
       admin_address,           // Primary admin
       platform_wallet,         // Fee recipient
       platform_fee_bps,        // Fee percentage
   );
   ```
3. **Set fallback admin** immediately:
   - Call `claim_admin()` or `update_admin` + `claim_admin`
   - Fallback admin automatically set
4. **Verify recovery capability**:
   - Check `fallback_admin` exists in storage
   - Test recovery procedure on testnet before mainnet

---

## Troubleshooting

### Problem: "InvalidAdminAddress" error

**Causes**:
- Trying to set contract itself as admin
- Trying to set zero address as admin

**Solution**:
- Ensure you're using a valid Address, not contract address
- Use an external account or multisig contract

### Problem: "StakeQueueFull" error when staking

**Causes**:
- Artisan has made 100+ stake operations
- Queue has not been pruned

**Solution**:
- Wait for automatic pruning (happens at 80% capacity)
- Or manually call `prune_stake_history` if available
- Queue will reduce from 100+ to 50 entries

### Problem: "AdminRecoveryFailed" when trying to recover

**Causes**:
- Trying to recover before 7-day time lock elapsed
- Using wrong fallback admin account
- No fallback admin exists

**Solution**:
- Calculate time remaining: `recovery_time - current_time`
- Ensure using the fallback admin that initiated recovery
- Set fallback admin by claiming role (`claim_admin`)

### Problem: "CorruptedPlatformConfig" error

**Causes**:
- Primary platform config storage corrupted
- Fallback admin not set (or also corrupted)

**Solution**:
1. Check if fallback admin exists: `storage.get(&DataKey::FallbackAdmin)`
2. If exists, perform admin recovery
3. If not, database administrator intervention needed
4. Re-initialize platform config with correct values

---

## References

- [Issue #240: Admin Access Hardcoding](https://github.com/Hub-of-Evolution/CraftNexus/issues/240)
- [Issue #237: Stake Queue Bounding](https://github.com/Hub-of-Evolution/CraftNexus/issues/237)
- [Implementation Summary](./IMPLEMENTATION_SUMMARY.md)
- [API Reference](./API_REFERENCE.md)
