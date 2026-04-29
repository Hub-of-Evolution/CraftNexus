# CraftNexus Smart Contract - New API Reference

## Admin Management Functions (Issue #240)

### `validate_admin_address(env: &Env, admin: &Address) -> Result<(), Error>`

**Purpose**: Validate that an admin address is acceptable for use in the contract

**Parameters**:
- `env: &Env` - Soroban environment reference
- `admin: &Address` - Address to validate

**Returns**:
- `Ok(())` if address is valid
- `Err(Error::InvalidAdminAddress)` if address is invalid

**Validation Rules**:
- Address must not be the contract itself
- Address must not be zero/default

**Example**:
```rust
if Self::validate_admin_address(&env, &new_admin).is_err() {
    env.panic_with_error(Error::InvalidAdminAddress);
}
```

**Called By**: `update_admin`, `claim_admin`, `recover_admin_access`, `set_fallback_admin`

---

### `get_platform_config_safe(env: &Env) -> Result<PlatformConfig, Error>`

**Purpose**: Retrieve platform config with fallback mechanism for corrupted storage

**Parameters**:
- `env: &Env` - Soroban environment reference

**Returns**:
- `Ok(PlatformConfig)` with valid configuration
- `Err(Error::CorruptedPlatformConfig)` if both primary and fallback configs unavailable

**Behavior**:
1. Attempts to load primary config from storage
2. If primary exists and valid, returns it
3. If primary corrupted, tries to load fallback admin
4. Creates minimal valid config using fallback admin (paused state)
5. Emits `admin_config_recovered` event if fallback used
6. Returns error only if completely unrecoverable

**Storage Keys Used**:
- `PLATFORM_FEE` - Primary configuration
- `DataKey::FallbackAdmin` - Fallback admin for recovery

**Example**:
```rust
match Self::get_platform_config_safe(&env) {
    Ok(config) => {
        // Use config for operations
    }
    Err(Error::CorruptedPlatformConfig) => {
        env.panic_with_error(Error::CorruptedPlatformConfig);
    }
    Err(e) => {
        env.panic_with_error(e);
    }
}
```

---

### `set_fallback_admin(env: &Env, admin: Address) -> Result<(), Error>`

**Purpose**: Store a fallback admin address for recovery purposes

**Parameters**:
- `env: &Env` - Soroban environment reference
- `admin: Address` - Address to use as fallback admin

**Returns**:
- `Ok(())` if fallback admin set successfully
- `Err(Error::InvalidAdminAddress)` if validation fails

**Side Effects**:
- Stores address in `DataKey::FallbackAdmin`
- Extends TTL for persistent storage
- Validates address before storage

**Called By**: `claim_admin` (automatically when claiming role)

---

### `emit_admin_changed(env: &Env, previous_admin: Address, new_admin: Address, change_type: &str)`

**Purpose**: Emit audit event for admin changes

**Parameters**:
- `env: &Env` - Soroban environment reference
- `previous_admin: Address` - Previous admin address
- `new_admin: Address` - New admin address
- `change_type: &str` - Type of change: `"admin_proposed"`, `"admin_claimed"`, or `"admin_recovered"`

**Event Topic**: `"admin_changed"`

**Event Data**: `(Symbol("admin_changed"), change_type_bytes)`

**Event Value**: `(previous_admin, new_admin)`

**Emitted By**: `update_admin`, `claim_admin`, `recover_admin_access`

---

### `update_admin(env: Env, new_admin: Address)` - ENHANCED

**Purpose**: Propose a new administrator (step 1 of 2-step transfer)

**Authorization**: Requires current admin signature

**Parameters**:
- `env: Env` - Soroban environment
- `new_admin: Address` - Address to propose as new admin

**New Behavior**:
- Validates `new_admin` address before proposal
- Panics with `InvalidAdminAddress` if validation fails
- Emits `admin_proposed` audit event

**Panics**:
- `Unauthorized` - If caller is not current admin
- `InvalidAdminAddress` - If new admin address is invalid

**Event Emitted**: `admin_changed` with type `"admin_proposed"`

---

### `claim_admin(env: Env)` - ENHANCED

**Purpose**: Claim the administrative role (step 2 of 2-step transfer)

**Authorization**: Requires pending admin signature

**Parameters**:
- `env: Env` - Soroban environment

**New Behavior**:
- Validates pending admin address before accepting
- Automatically calls `set_fallback_admin` with new admin
- Emits `admin_claimed` audit event

**Panics**:
- `Unauthorized` - If caller is not pending admin
- `InvalidAdminAddress` - If pending admin address is invalid

**Event Emitted**: `admin_changed` with type `"admin_claimed"`

**Side Effects**:
- Sets `DataKey::FallbackAdmin` to new admin
- Provides automatic recovery capability

---

### `recover_admin_access(env: Env, recovered_admin: Address) -> Result<(), Error>`

**Purpose**: Recover admin access using fallback admin after time lock

**Authorization**: Requires fallback admin signature

**Parameters**:
- `env: Env` - Soroban environment
- `recovered_admin: Address` - Address to recover as new admin

**Time Lock**: 7 days (`ADMIN_RECOVERY_DELAY`)

**Returns**:
- `Ok(())` if recovery successful
- `Err(Error::Unauthorized)` if no fallback admin exists
- `Err(Error::InvalidAdminAddress)` if recovered_admin is invalid
- `Err(Error::AdminRecoveryFailed)` if time lock not yet elapsed

**Behavior**:
1. **First Call**: Initiates recovery and schedules 7-day time lock
   - Validates fallback admin authorization
   - Validates recovered admin address
   - Sets recovery time in `DataKey::AdminRecoveryTime`
   - Emits event indicating time lock initiated
   - Returns `Err(Error::AdminRecoveryFailed)`

2. **Subsequent Calls** (after 7 days):
   - Validates time lock has elapsed
   - Updates admin to recovered address
   - Clears time lock for next recovery
   - Emits `admin_recovered` audit event
   - Returns `Ok(())`

**Events Emitted**:
- `admin_recovery_initiated` - When time lock starts
- `admin_changed` - When recovery completes (type: `"admin_recovered"`)

**Use Case**: Recovery from compromised or lost admin key

---

## Stake Queue Functions (Issue #237)

### `record_stake_history(env: &Env, artisan: &Address, new_stake: i128, operation: &str) -> Result<(), Error>`

**Purpose**: Record a stake operation in the history queue

**Parameters**:
- `env: &Env` - Soroban environment reference
- `artisan: &Address` - Artisan whose stake changed
- `new_stake: i128` - New stake amount after operation
- `operation: &str` - Operation type: `"stake_added"` or `"stake_removed"`

**Returns**:
- `Ok(())` if recorded successfully
- `Err(Error::StakeQueueFull)` if queue is at maximum capacity

**Capacity Limits**:
- Maximum: 100 entries per artisan (`MAX_STAKE_HISTORY_SIZE`)
- Pruning threshold: 80 entries (`STAKE_HISTORY_PRUNE_THRESHOLD`)
- After pruning: Retains 50 most recent entries

**Storage Updates**:
- Increments `DataKey::StakeHistoryCount(artisan)`
- Updates `DataKey::StakeLastModified(artisan)` timestamp
- May trigger automatic pruning at 80% capacity

**Event Emitted**: `stake_operation` with operation name and stake amount

**Called By**: `stake_tokens`, `unstake_tokens`

---

### `prune_stake_history(env: &Env, artisan: &Address)`

**Purpose**: Manually prune old stake history entries

**Parameters**:
- `env: &Env` - Soroban environment reference
- `artisan: &Address` - Artisan whose history to prune

**Behavior**:
- Reduces history count to 50% of current value
- Preserves 50 most recent entries
- Updates `DataKey::StakeHistoryCount(artisan)`

**Note**: This function is called automatically when queue reaches 80% capacity. It can also be called manually for maintenance.

---

### `stake_tokens(env: Env, artisan: Address, token: Address, amount: i128)` - ENHANCED

**Purpose**: Deposit tokens for artisan staking

**Authorization**: Requires artisan signature

**Parameters**:
- `env: Env` - Soroban environment
- `artisan: Address` - Address of artisan staking
- `token: Address` - Token contract address
- `amount: i128` - Amount to stake

**New Behavior**:
- Records stake operation in history queue
- **CRITICAL FIX**: Cooldown is NOT reset on additional stakes
- Cooldown initialized only on first stake

**Cooldown Logic**:
```rust
// Check if cooldown already exists
if existing_cooldown == 0 {
    // First stake: initialize cooldown
    let cooldown_end = current_time + config.stake_cooldown;
    set_cooldown(cooldown_end);
}
// If cooldown already exists, do NOT reset it
```

**Panics**:
- `AmountBelowMinimum` - If amount <= 0
- `StakeTokenMismatch` - If artisan tries to stake different token
- `StakeQueueFull` - If history queue at capacity

**Events Emitted**:
- `stake_operation` - Operation `"stake_added"` with new stake amount

**Prevents**: Artisans from gaming the system by continuously staking to extend cooldown

---

### `unstake_tokens(env: Env, artisan: Address, token: Address)` - ENHANCED

**Purpose**: Withdraw staked tokens after cooldown expires

**Authorization**: Requires artisan signature

**Parameters**:
- `env: Env` - Soroban environment
- `artisan: Address` - Address of artisan withdrawing
- `token: Address` - Token contract address (must match stake token)

**New Behavior**:
- Records stake removal in history queue
- If history queue full, emits warning event but does NOT fail
- Ensures unstake cannot be blocked by queue saturation

**Panics**:
- `StakeCooldownActive` - If cooldown period not elapsed
- `AmountBelowMinimum` - If no stake exists
- `StakeTokenMismatch` - If token doesn't match stake token

**Events Emitted**:
- `stake_operation` - Operation `"stake_removed"` with new stake amount (0)
- `stake_history_warning` - If history queue is full (warning only)

**Side Effects**:
- Clears `DataKey::ArtisanStake`
- Clears `DataKey::ArtisanStakeToken`
- Clears `DataKey::StakeCooldownEnd`
- Returns tokens to artisan

---

## New Error Codes

```rust
InvalidAdminAddress = 25,        // Admin address validation failed
CorruptedPlatformConfig = 26,   // Platform config corrupted, recovery failed
StakeQueueFull = 27,            // Stake history queue at capacity
AdminRecoveryFailed = 28,       // Admin recovery mechanism failed
```

---

## New Storage Keys

```rust
DataKey::FallbackAdmin                    // Backup admin address
DataKey::AdminRecoveryTime                // Time lock for recovery (u64 timestamp)
DataKey::StakeHistory(Address)            // Stake operation history per artisan
DataKey::StakeHistoryCount(Address)       // Queue size counter per artisan
DataKey::StakeLastModified(Address)       // Last modification timestamp per artisan
```

---

## New Constants

```rust
ADMIN_RECOVERY_DELAY: u64 = 604800;           // 7 days in seconds
MAX_STAKE_HISTORY_SIZE: u32 = 100;            // Max entries per artisan
STAKE_HISTORY_PRUNE_THRESHOLD: u32 = 80;      // Pruning trigger (80% capacity)
```

---

## Event Types

### `admin_config_recovered`
- **Topic**: `"admin_config_recovered"`
- **Value**: `true`
- **Data**: Recovery message string
- **Emitted When**: Primary config corrupted, fallback used

### `admin_changed`
- **Topic**: `"admin_changed"`
- **Subtopic**: `"admin_proposed"`, `"admin_claimed"`, or `"admin_recovered"`
- **Value**: `(previous_admin, new_admin)` addresses
- **Emitted When**: Admin role transferred, claimed, or recovered

### `admin_recovery_initiated`
- **Topic**: `"admin_recovery_initiated"`
- **Value**: `true`
- **Data**: Recovery message string
- **Emitted When**: Recovery mechanism time lock starts

### `stake_operation`
- **Topic**: `"stake_operation"`
- **Subtopic**: `"stake_added"` or `"stake_removed"`
- **Value**: `(artisan_address, new_stake_amount)`
- **Emitted When**: Artisan stakes or unstakes tokens

### `stake_history_warning`
- **Topic**: `"stake_history_warning"`
- **Subtopic**: `"queue_full"`
- **Value**: Warning message
- **Emitted When**: History queue full during unstake (operation still succeeds)

