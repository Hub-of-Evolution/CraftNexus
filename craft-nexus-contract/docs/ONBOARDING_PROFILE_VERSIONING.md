# Onboarding Profile Schema Versioning (#1056)

## Overview

Issue #1056 adds explicit schema versioning to user onboarding profiles. This prevents a critical class of bug where future code changes to profile field meanings or permission-derivation logic could silently misinterpret already-persisted profile data with unintended consequences.

## The Problem

Before this change:
- User profiles stored no explicit schema version field
- If profile interpretation logic changed (e.g., role-derivation rules, field structure), old profiles would be silently re-interpreted using new logic
- This could lead to:
  - Users losing/gaining permissions unexpectedly
  - Roles being misapplied
  - Trust/reputation scores being recomputed incorrectly
  - Compliance violations due to effective-permission drift

Example dangerous scenario:
```rust
// v1 code: Artisan role means "can sell"
let can_sell = profile.role == Artisan;

// v2 code: Artisan role + is_verified means "can sell" 
// (new anti-fraud gate added)
let can_sell = profile.role == Artisan && profile.is_verified;

// Old profiles that were never verified now can't sell
// without ANY notice that their effective permissions changed!
```

## The Solution

### 1. Explicit Version Field

Each `UserProfile` now includes a `version: u32` field:

```rust
pub const CURRENT_USER_PROFILE_VERSION: u32 = 5;

pub struct UserProfile {
    pub address: Address,
    pub username: String,
    pub role: UserRole,
    pub status: ProfileStatus,
    pub version: u32,  // <-- NEW (#1056)
    // ... other fields ...
}
```

New profiles are always assigned `CURRENT_USER_PROFILE_VERSION`.

### 2. Validation on Read

All profile reads validate the version before use:

```rust
fn assert_profile_version_supported(env: &Env, version: u32) {
    if version > CURRENT_USER_PROFILE_VERSION {
        env.panic_with_error(Error::UnsupportedProfileVersion);
    }
}
```

**Why panic instead of silently degrade?**
- Silent degradation defeats the purpose of the issue (avoiding silent misinterpretation)
- If a profile has a future version, something is critically wrong: data corruption, version mismatch, or a deployment mistake
- Explicit failure prevents subtle permission bugs
- Operators are forced to notice and investigate

### 3. In-Place Migration on Read

Legacy profiles (those without an explicit version or with version < CURRENT_USER_PROFILE_VERSION) are transparently upgraded on first read:

```rust
fn try_get_stored_user_profile(env: &Env, user: &Address) -> UserProfile {
    let mut profile = StoredUserProfile::load(...);
    
    // Validate version is not from the future (#1056)
    Self::assert_profile_version_supported(env, profile.version);
    
    // Migrate old profiles to current version
    let mut changed = false;
    if profile.version < CURRENT_USER_PROFILE_VERSION {
        profile.version = CURRENT_USER_PROFILE_VERSION;
        changed = true;
    }
    
    if changed {
        Self::store_user_profile(env, &profile);
    }
    
    profile
}
```

**Why migrate on read instead of bulk rewrite?**
- Safer: Only migrated profiles that are actually accessed
- More efficient: Matches Soroban storage constraints
- No operational overhead: Happens automatically
- Simpler: No need for separate migration jobs

### 4. Error Code

New error: `UnsupportedProfileVersion = 32`

Panics when:
- Profile version > `CURRENT_USER_PROFILE_VERSION` (unsupported future version)
- Typically indicates data corruption or a severe version mismatch

## Backward Compatibility

**Core Requirement: Legacy profiles must produce identical effective permissions after this change**

The implementation achieves this by:

1. **Non-destructive migration**: Profile fields are preserved exactly as-is during version upgrade
2. **No permission logic changes**: Version upgrade doesn't alter how permissions are computed
3. **Transparent access**: Legacy profiles work exactly like before once migrated

Example: An old Artisan profile with `is_verified = false` will continue to have that exact value after migration, so any permission logic dependent on it remains identical.

## Future Schema Changes

When profile schema changes in the future:

### Step 1: Increment the Version
```rust
pub const CURRENT_USER_PROFILE_VERSION: u32 = 6;  // Was 5
```

### Step 2: Handle Migration
If the new schema is backward-compatible (new fields only, old fields unchanged):
```rust
if profile.version == 5 {
    // Initialize new fields to sensible defaults
    profile.new_field = DEFAULT_VALUE;
    profile.version = 6;
}
```

If breaking changes are needed (e.g., field removal, renaming):
1. Plan a longer deprecation window
2. Implement explicit migration logic before the breaking change
3. Document the migration path
4. Test with real data extensively

### Step 3: Validation
Always add version-specific validation when interpretation logic changes:
```rust
// Only old v5 profiles could have this invalid state
if profile.version == 5 && profile.old_role == InvalidLegacyRole {
    // Migrate safely
    profile.role = SafeDefaultRole;
}
```

### Step 4: Test Regression
For each schema change, add explicit tests verifying:
- New profiles get the new version
- Old profiles are read-compatible
- Effective permissions are unchanged after migration
- Invalid/future versions are rejected

## Observability

### Query Current Version
```rust
pub fn get_user_profile_version(env: Env, user: Address) -> u32 {
    OnboardingContract::get_user_profile_version(&env, &user)
}
```

Operators can check:
- Which profiles have been migrated
- If any profiles are still on legacy versions
- For debugging during schema transitions

### Storage Keys
Profiles are stored with the version field serialized, so:
- Version is visible in storage dumps
- Migration history can be reconstructed
- Operators can audit version distribution

## Error Scenarios

| Scenario | Behavior | Recovery |
|----------|----------|----------|
| Read profile with version > current | Panic with `UnsupportedProfileVersion` | Investigate data corruption or deployment issue |
| Read profile with version < current | Auto-migrate on read | Transparent, no action needed |
| Read profile with version == current | Use as-is | No migration |
| Create new profile | Always assign `CURRENT_USER_PROFILE_VERSION` | Automatic |
| Future code changes profile interpretation | Version field prevents silent bugs | Forced to add explicit migration logic |

## Implementation Details

### Constants
```rust
pub const CURRENT_USER_PROFILE_VERSION: u32 = 5;
```

Located in `src/onboarding.rs`.

### Functions
- `assert_profile_version_supported(env, version)` - Validates version is supported, panics otherwise
- `try_get_stored_user_profile(env, user)` - Loads profile with validation and migration
- All profile creation paths enforce `CURRENT_USER_PROFILE_VERSION` assignment

### Storage
- Profile version is stored as part of the `UserProfile` serialization
- No separate storage key needed
- Migration happens in-place on first read

## Testing

The implementation includes regression tests verifying:
1. New profile creation assigns current version
2. Legacy profile migration preserves all fields
3. Unsupported versions are rejected with error
4. Version field is queryable
5. Effective permissions remain identical after migration

Run tests:
```bash
cd craft-nexus-contract
cargo test --lib onboarding
```

## References

- Issue #1056: Add explicit profile schema versioning with validation and migration
- `src/onboarding.rs`: Core versioning implementation
- `src/onboarding_test.rs`: Test coverage
