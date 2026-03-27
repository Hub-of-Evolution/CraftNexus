use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, String, Symbol, Vec};
use soroban_sdk::{TryFromVal, Val};

/// Standard TTL threshold for persistent storage (approx 14 hours at 5s ledger)
const TTL_THRESHOLD: u32 = 10_000;
/// Standard TTL extension for persistent storage (approx 30 days)
const TTL_EXTENSION: u32 = 518_400;
const CURRENT_USER_PROFILE_VERSION: u32 = 2;

#[cfg(test)]
#[path = "onboarding_test.rs"]
mod onboarding_test;

/// Storage keys for the onboarding contract
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Maps a user address to their profile
    UserProfile(Address),
    /// Maps a normalized username to the owning address (uniqueness index)
    Username(String),
    /// Contract configuration
    Config,
    /// Activity metrics per user (escrow count and volume for auto-verification) (#63)
    UserMetrics(Address),
    /// Queue of addresses that have requested manual verification (#63)
    VerificationQueue,
    /// Verification history log per user (#63)
    VerificationHistory(Address),
}

/// User roles in the CraftNexus platform
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum UserRole {
    None = 0,      // User has not onboarded
    Buyer = 1,     // Can purchase items
    Artisan = 2,   // Can sell items and create escrow
    Admin = 3,     // Platform administrator
    Moderator = 4, // Can help manage disputes
}

/// Onboarding status for users
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserProfile {
    pub version: u32,
    pub address: Address,
    pub role: UserRole,
    pub username: String,
    pub registered_at: u64,
    pub is_verified: bool,
    /// Count of escrows where this user was on the winning side (#100)
    pub successful_trades: u32,
    /// Count of escrows that ended in a dispute against this user (#100)
    pub disputed_trades: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct LegacyUserProfile {
    pub address: Address,
    pub role: UserRole,
    pub username: String,
    pub registered_at: u64,
    pub is_verified: bool,
    /// Count of escrows where this user was on the winning side (#100)
    pub successful_trades: u32,
    /// Count of escrows that ended in a dispute against this user (#100)
    pub disputed_trades: u32,
}

/// Activity metrics used to determine eligibility for auto-verification (#63)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserMetrics {
    /// Total number of escrows the user participated in as seller
    pub total_escrow_count: u32,
    /// Total USDC volume (in stroops) the user transacted as seller
    pub total_volume: i128,
}

/// A single entry in a user's verification history log (#63)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationEntry {
    pub timestamp: u64,
    /// "requested" | "approved" | "rejected" | "auto_verified"
    pub action: String,
    /// Address that performed the action (None for auto-verification)
    pub by: Option<Address>,
}

/// Contract configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnboardingConfig {
    pub require_username: bool,
    pub min_username_length: u32,
    pub max_username_length: u32,
    pub platform_admin: Address,
    /// Minimum completed escrow count for auto-verification (#63; default 5)
    pub min_escrow_count_for_verify: u32,
    /// Minimum total USDC volume (in stroops) for auto-verification (#63; default 10_000_000_000)
    pub min_volume_for_verify: i128,
    /// Address of the escrow contract authorized to update reputation/metrics (#63, #100)
    pub escrow_contract: Option<Address>,
}

fn normalize_username(env: &Env, username: &String) -> String {
    const MAX_INPUT_BYTES: usize = 256;
    const MAX_OUTPUT_BYTES: usize = 256;
    let len = username.len() as usize;
    if len > MAX_INPUT_BYTES {
        panic!("Username too long");
    }

    let mut buf = [0u8; MAX_INPUT_BYTES];
    username.copy_into_slice(&mut buf[..len]);
    let mut normalized = [0u8; MAX_OUTPUT_BYTES];
    let mut out_len = 0usize;
    let mut last_was_separator = false;
    let mut index = 0usize;

    while index < len {
        let byte = buf[index];

        if byte.is_ascii_alphanumeric() {
            normalized[out_len] = byte.to_ascii_lowercase();
            out_len += 1;
            last_was_separator = false;
            index += 1;
            continue;
        }

        if matches!(byte, b' ' | b'_' | b'-' | b'.') {
            if out_len > 0 && !last_was_separator {
                normalized[out_len] = b'_';
                out_len += 1;
                last_was_separator = true;
            }
            index += 1;
            continue;
        }

        if let Some((mapped, consumed)) = map_username_bytes(&buf[index..len]) {
            for mapped_byte in mapped {
                if *mapped_byte == b'_' {
                    if out_len == 0 || last_was_separator {
                        continue;
                    }
                    normalized[out_len] = b'_';
                    out_len += 1;
                    last_was_separator = true;
                } else {
                    normalized[out_len] = *mapped_byte;
                    out_len += 1;
                    last_was_separator = false;
                }
            }
            index += consumed;
            continue;
        }

        if out_len > 0 && !last_was_separator {
            normalized[out_len] = b'_';
            out_len += 1;
            last_was_separator = true;
        }
        index += utf8_char_len(byte);
    }

    while out_len > 0 && normalized[out_len - 1] == b'_' {
        out_len -= 1;
    }

    String::from_bytes(env, &normalized[..out_len])
}

fn map_username_bytes(input: &[u8]) -> Option<(&'static [u8], usize)> {
    match input {
        [0xC3, 0x84, ..]
        | [0xC3, 0xA4, ..]
        | [0xC3, 0x80, ..]
        | [0xC3, 0xA0, ..]
        | [0xC3, 0x81, ..]
        | [0xC3, 0xA1, ..]
        | [0xC3, 0x82, ..]
        | [0xC3, 0xA2, ..]
        | [0xC3, 0x83, ..]
        | [0xC3, 0xA3, ..]
        | [0xC3, 0x85, ..]
        | [0xC3, 0xA5, ..]
        | [0xCE, 0x91, ..]
        | [0xD0, 0xB0, ..] => Some((b"a", 2)),
        [0xC3, 0x87, ..] | [0xC3, 0xA7, ..] | [0xD0, 0xA1, ..] | [0xD1, 0x81, ..] => {
            Some((b"c", 2))
        }
        [0xC3, 0x88, ..]
        | [0xC3, 0xA8, ..]
        | [0xC3, 0x89, ..]
        | [0xC3, 0xA9, ..]
        | [0xC3, 0x8A, ..]
        | [0xC3, 0xAA, ..]
        | [0xC3, 0x8B, ..]
        | [0xC3, 0xAB, ..]
        | [0xCE, 0x95, ..]
        | [0xD0, 0x95, ..]
        | [0xD0, 0xB5, ..] => Some((b"e", 2)),
        [0xC3, 0x8D, ..]
        | [0xC3, 0xAD, ..]
        | [0xC3, 0x8E, ..]
        | [0xC3, 0xAE, ..]
        | [0xC3, 0x8F, ..]
        | [0xC3, 0xAF, ..]
        | [0xD0, 0x86, ..]
        | [0xD1, 0x96, ..] => Some((b"i", 2)),
        [0xC3, 0x91, ..] | [0xC3, 0xB1, ..] => Some((b"n", 2)),
        [0xC3, 0x96, ..]
        | [0xC3, 0xB6, ..]
        | [0xC3, 0x93, ..]
        | [0xC3, 0xB3, ..]
        | [0xC3, 0x94, ..]
        | [0xC3, 0xB4, ..]
        | [0xC3, 0x95, ..]
        | [0xC3, 0xB5, ..]
        | [0xC3, 0x92, ..]
        | [0xC3, 0xB2, ..]
        | [0xC3, 0x98, ..]
        | [0xC3, 0xB8, ..]
        | [0xC5, 0x90, ..]
        | [0xC5, 0x91, ..]
        | [0xCE, 0x9F, ..]
        | [0xD0, 0x9E, ..]
        | [0xD0, 0xBE, ..] => Some((b"o", 2)),
        [0xC3, 0x9C, ..]
        | [0xC3, 0xBC, ..]
        | [0xC3, 0x9A, ..]
        | [0xC3, 0xBA, ..]
        | [0xC3, 0x99, ..]
        | [0xC3, 0xB9, ..]
        | [0xC3, 0x9B, ..]
        | [0xC3, 0xBB, ..] => Some((b"u", 2)),
        [0xC3, 0x9F, ..] => Some((b"ss", 2)),
        [0xC3, 0x86, ..] | [0xC3, 0xA6, ..] => Some((b"ae", 2)),
        [0xC5, 0x92, ..] | [0xC5, 0x93, ..] => Some((b"oe", 2)),
        [0xD0, 0xA0, ..] | [0xD1, 0x80, ..] => Some((b"p", 2)),
        [0xD0, 0xA5, ..] | [0xD1, 0x85, ..] => Some((b"x", 2)),
        [0xD0, 0xA3, ..] | [0xD1, 0x83, ..] => Some((b"y", 2)),
        [0xD0, 0x9D, ..] | [0xD2, 0xBB, ..] => Some((b"h", 2)),
        [0xE2, 0x80, 0x8B, ..]
        | [0xE2, 0x80, 0x8C, ..]
        | [0xE2, 0x80, 0x8D, ..]
        | [0xE2, 0x81, 0xA0, ..]
        | [0xEF, 0xBB, 0xBF, ..] => Some((b"", 3)),
        _ => None,
    }
}

fn utf8_char_len(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[contract]
pub struct OnboardingContract;

#[contractimpl]
impl OnboardingContract {
    fn get_user_profile(env: &Env, user: Address) -> UserProfile {
        let key = DataKey::UserProfile(user.clone());
        let stored: Val = env
            .storage()
            .persistent()
            .get(&key)
            .expect("User not found");
        let map =
            Map::<Symbol, Val>::try_from_val(env, &stored).expect("User profile storage corrupted");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let profile =
                UserProfile::try_from_val(env, &stored).expect("User profile storage corrupted");
            if profile.version < CURRENT_USER_PROFILE_VERSION {
                return Self::upgrade_user_profile(env, user, profile);
            }
            Self::extend_persistent(env, &key);
            return profile;
        }

        let legacy =
            LegacyUserProfile::try_from_val(env, &stored).expect("User profile storage corrupted");
        let upgraded = UserProfile {
            version: CURRENT_USER_PROFILE_VERSION,
            address: legacy.address.clone(),
            role: legacy.role,
            username: legacy.username.clone(),
            registered_at: legacy.registered_at,
            is_verified: legacy.is_verified,
            successful_trades: legacy.successful_trades,
            disputed_trades: legacy.disputed_trades,
        };
        env.storage().persistent().set(&key, &upgraded);
        Self::extend_persistent(env, &key);
        upgraded
    }

    fn upgrade_user_profile(env: &Env, user: Address, mut profile: UserProfile) -> UserProfile {
        profile.version = CURRENT_USER_PROFILE_VERSION;
        let key = DataKey::UserProfile(user);
        env.storage().persistent().set(&key, &profile);
        Self::extend_persistent(env, &key);
        profile
    }

    /// Extend the TTL of a persistent storage entry using standardized values.
    fn extend_persistent(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Initialize the onboarding contract
    ///
    /// # Arguments
    /// * `admin` - Platform administrator address
    pub fn initialize(env: Env, admin: Address) -> OnboardingConfig {
        // Only the deployer can initialize
        admin.require_auth();

        let config = OnboardingConfig {
            require_username: true,
            min_username_length: 3,
            max_username_length: 50,
            platform_admin: admin.clone(),
            min_escrow_count_for_verify: 5,
            min_volume_for_verify: 10_000_000_000, // 1000 USDC at 7 decimals
            escrow_contract: None,
        };

        // Store the configuration
        env.storage().persistent().set(&DataKey::Config, &config);
        Self::extend_persistent(&env, &DataKey::Config);

        let admin_username = String::from_str(&env, "admin");
        let normalized = normalize_username(&env, &admin_username);

        // Store admin as initial admin role
        let admin_profile = UserProfile {
            version: CURRENT_USER_PROFILE_VERSION,
            address: admin.clone(),
            role: UserRole::Admin,
            username: normalized.clone(),
            registered_at: env.ledger().timestamp(),
            is_verified: true,
            successful_trades: 0,
            disputed_trades: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(admin.clone()), &admin_profile);
        Self::extend_persistent(&env, &DataKey::UserProfile(admin.clone()));

        // Reserve the "admin" username
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized.clone()), &admin);
        Self::extend_persistent(&env, &DataKey::Username(normalized));

        config
    }

    /// Onboard a new user to the platform
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    /// * `username` - Desired username
    /// * `role` - Desired role (Buyer or Artisan)
    ///
    /// # Reverts if
    /// - User already onboarded
    /// - Username already taken (case-insensitive)
    /// - Username too short or too long
    /// - Invalid role specified
    pub fn onboard_user(env: Env, user: Address, username: String, role: UserRole) -> UserProfile {
        user.require_auth();

        // Validate role is valid (only Buyer or Artisan for self-onboarding)
        assert!(
            role == UserRole::Buyer || role == UserRole::Artisan,
            "Invalid role: can only onboard as Buyer or Artisan"
        );

        // Get configuration
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        // Normalize the username (lowercase + trim whitespace)
        let normalized = normalize_username(&env, &username);

        // Validate normalized username length
        let username_len = normalized.len() as u32;
        assert!(
            username_len >= config.min_username_length,
            "Username too short"
        );
        assert!(
            username_len <= config.max_username_length,
            "Username too long"
        );

        // Check if user already onboarded
        let existing: Option<UserProfile> = env
            .storage()
            .persistent()
            .get(&DataKey::UserProfile(user.clone()));
        if existing.is_some() {
            Self::extend_persistent(&env, &DataKey::UserProfile(user.clone()));
        }

        assert!(existing.is_none(), "User already onboarded");

        // Check username uniqueness
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKey::Username(normalized.clone())),
            "Username already taken"
        );

        // Create user profile with normalized username
        let profile = UserProfile {
            version: CURRENT_USER_PROFILE_VERSION,
            address: user.clone(),
            role,
            username: normalized.clone(),
            registered_at: env.ledger().timestamp(),
            is_verified: false,
            successful_trades: 0,
            disputed_trades: 0,
        };

        // Store profile
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
        Self::extend_persistent(&env, &DataKey::UserProfile(user.clone()));

        // Store username → address mapping for uniqueness enforcement
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized.clone()), &user);
        Self::extend_persistent(&env, &DataKey::Username(normalized));

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "UserOnboarded"),), &user);

        profile
    }

    /// Get user profile by address
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    ///
    /// # Returns
    /// UserProfile if user exists, reverts otherwise
    pub fn get_user(env: Env, user: Address) -> UserProfile {
        Self::get_user_profile(&env, user)
    }

    /// Get user profile by username (case-insensitive)
    ///
    /// # Arguments
    /// * `username` - Username to look up
    ///
    /// # Returns
    /// UserProfile if username exists, reverts otherwise
    pub fn get_user_by_username(env: Env, username: String) -> UserProfile {
        let normalized = normalize_username(&env, &username);

        let owner: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Username(normalized.clone()))
            .expect("Username not found");
        Self::extend_persistent(&env, &DataKey::Username(normalized));

        Self::get_user_profile(&env, owner)
    }

    /// Check if a username is already taken (case-insensitive)
    ///
    /// # Arguments
    /// * `username` - Username to check
    ///
    /// # Returns
    /// true if username is taken, false if available
    pub fn is_username_taken(env: Env, username: String) -> bool {
        let normalized = normalize_username(&env, &username);
        let has = env
            .storage()
            .persistent()
            .has(&DataKey::Username(normalized.clone()));
        if has {
            Self::extend_persistent(&env, &DataKey::Username(normalized));
        }
        has
    }

    /// Check if user is onboarded
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    ///
    /// # Returns
    /// true if user has onboarded, false otherwise
    pub fn is_onboarded(env: Env, user: Address) -> bool {
        let key = DataKey::UserProfile(user.clone());
        let has = env.storage().persistent().has(&key);
        if has {
            let _ = Self::get_user_profile(&env, user);
        }
        has
    }

    /// Get user's role
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    ///
    /// # Returns
    /// UserRole if user exists, UserRole::None otherwise
    pub fn get_user_role(env: Env, user: Address) -> UserRole {
        if env
            .storage()
            .persistent()
            .has(&DataKey::UserProfile(user.clone()))
        {
            Self::get_user_profile(&env, user).role
        } else {
            UserRole::None
        }
    }

    /// Assign or update the moderator role for a user (admin only).
    pub fn set_moderator(env: Env, user: Address) -> UserProfile {
        Self::update_user_role(env, user, UserRole::Moderator)
    }

    /// Update user role (admin only)
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    /// * `new_role` - New role to assign
    ///
    /// # Reverts if
    /// - Caller is not admin
    /// - User not found
    pub fn update_user_role(env: Env, user: Address, new_role: UserRole) -> UserProfile {
        // Get config to verify admin
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        // Only admin can update roles
        config.platform_admin.require_auth();

        // Get existing profile
        let mut profile = Self::get_user_profile(&env, user.clone());

        // Update role
        let _old_role = profile.role;
        profile.role = new_role;

        // Store updated profile
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
        Self::extend_persistent(&env, &DataKey::UserProfile(user.clone()));

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "RoleUpdated"),), &user);

        profile
    }

    /// Verify user (admin only)
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    ///
    /// # Reverts if
    /// - Caller is not admin
    /// - User not found
    pub fn verify_user(env: Env, user: Address) -> UserProfile {
        // Get config to verify admin
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        // Only admin can verify users
        config.platform_admin.require_auth();

        // Get existing profile
        let mut profile = Self::get_user_profile(&env, user.clone());

        // Set verified
        profile.is_verified = true;

        // Store updated profile
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
        Self::extend_persistent(&env, &DataKey::UserProfile(user.clone()));

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "UserVerified"),), &user);

        profile
    }

    /// Get onboarding configuration
    ///
    /// # Returns
    /// OnboardingConfig struct
    pub fn get_config(env: Env) -> OnboardingConfig {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);
        config
    }

    /// Check if user has specific role
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    /// * `role` - Role to check
    ///
    /// # Returns
    /// true if user has the specified role, false otherwise
    pub fn has_role(env: Env, user: Address, role: UserRole) -> bool {
        Self::get_user_role(env, user) == role
    }

    /// Check if user is verified
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    ///
    /// # Returns
    /// true if user is verified, false otherwise
    pub fn is_verified(env: Env, user: Address) -> bool {
        if env
            .storage()
            .persistent()
            .has(&DataKey::UserProfile(user.clone()))
        {
            Self::get_user_profile(&env, user).is_verified
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Issue #63 – Artisan Verification Logic Enhancement
    // -----------------------------------------------------------------------

    /// Register the address of the deployed EscrowContract so it can update
    /// reputation and activity metrics via cross-contract calls (admin only).
    pub fn set_escrow_contract(env: Env, contract_address: Address) {
        let mut config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        config.platform_admin.require_auth();
        config.escrow_contract = Some(contract_address);

        env.storage().persistent().set(&DataKey::Config, &config);
        Self::extend_persistent(&env, &DataKey::Config);
    }

    /// Update the minimum thresholds used for automatic user verification (admin only).
    ///
    /// # Arguments
    /// * `min_escrow_count` - Minimum number of completed escrows required
    /// * `min_volume` - Minimum total transaction volume required (in stroops)
    pub fn set_verification_thresholds(env: Env, min_escrow_count: u32, min_volume: i128) {
        let mut config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        config.platform_admin.require_auth();
        config.min_escrow_count_for_verify = min_escrow_count;
        config.min_volume_for_verify = min_volume;

        env.storage().persistent().set(&DataKey::Config, &config);
        Self::extend_persistent(&env, &DataKey::Config);
    }

    /// Get activity metrics for a user.
    /// Returns zeroed metrics if no escrow activity has been recorded yet.
    pub fn get_user_metrics(env: Env, address: Address) -> UserMetrics {
        let metrics = env
            .storage()
            .persistent()
            .get::<DataKey, UserMetrics>(&DataKey::UserMetrics(address.clone()))
            .unwrap_or(UserMetrics {
                total_escrow_count: 0,
                total_volume: 0,
            });
        if env
            .storage()
            .persistent()
            .has(&DataKey::UserMetrics(address.clone()))
        {
            Self::extend_persistent(&env, &DataKey::UserMetrics(address));
        }
        metrics
    }

    /// Increment a user's activity metrics (called by the escrow contract).
    ///
    /// Auth: requires the registered escrow contract address, or admin if none is set.
    pub fn update_user_metrics(
        env: Env,
        address: Address,
        escrow_count_delta: u32,
        volume_delta: i128,
    ) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        // Only the registered escrow contract (or admin if none set) may call this.
        match config.escrow_contract {
            Some(ref escrow_addr) => escrow_addr.require_auth(),
            None => config.platform_admin.require_auth(),
        }

        let key = DataKey::UserMetrics(address.clone());
        let mut metrics: UserMetrics =
            env.storage().persistent().get(&key).unwrap_or(UserMetrics {
                total_escrow_count: 0,
                total_volume: 0,
            });

        metrics.total_escrow_count = metrics
            .total_escrow_count
            .saturating_add(escrow_count_delta);
        metrics.total_volume = metrics.total_volume.saturating_add(volume_delta);

        env.storage().persistent().set(&key, &metrics);
        Self::extend_persistent(&env, &key);

        // Check whether the user now meets the auto-verification threshold.
        Self::try_auto_verify(&env, &address, &config, &metrics);
    }

    /// Internal helper: verify a user automatically if they meet the configured thresholds.
    fn try_auto_verify(
        env: &Env,
        address: &Address,
        config: &OnboardingConfig,
        metrics: &UserMetrics,
    ) {
        let profile_key = DataKey::UserProfile(address.clone());
        let profile_opt: Option<UserProfile> = env.storage().persistent().get(&profile_key);
        let mut profile = match profile_opt {
            Some(p) => p,
            None => return,
        };

        if profile.is_verified {
            return;
        }

        if metrics.total_escrow_count >= config.min_escrow_count_for_verify
            && metrics.total_volume >= config.min_volume_for_verify
        {
            profile.is_verified = true;
            env.storage().persistent().set(&profile_key, &profile);
            Self::extend_persistent(env, &profile_key);

            // Append auto-verify entry to history
            let hist_key = DataKey::VerificationHistory(address.clone());
            let mut history: Vec<VerificationEntry> = env
                .storage()
                .persistent()
                .get(&hist_key)
                .unwrap_or(Vec::new(env));
            history.push_back(VerificationEntry {
                timestamp: env.ledger().timestamp(),
                action: String::from_str(env, "auto_verified"),
                by: None,
            });
            env.storage().persistent().set(&hist_key, &history);
            Self::extend_persistent(env, &hist_key);

            env.events()
                .publish((Symbol::new(env, "UserVerified"),), address);
        }
    }

    /// Trigger an auto-verification check for a user.
    /// Anyone may call this; it is a no-op if thresholds are not yet met.
    ///
    /// # Returns
    /// `true` if the user was just auto-verified, `false` if thresholds not met or already verified.
    pub fn auto_verify_user(env: Env, address: Address) -> bool {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        let profile_key = DataKey::UserProfile(address.clone());
        let profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .expect("User not found");
        Self::extend_persistent(&env, &profile_key);

        if profile.is_verified {
            return false;
        }

        let metrics: UserMetrics = env
            .storage()
            .persistent()
            .get(&DataKey::UserMetrics(address.clone()))
            .unwrap_or(UserMetrics {
                total_escrow_count: 0,
                total_volume: 0,
            });

        if metrics.total_escrow_count >= config.min_escrow_count_for_verify
            && metrics.total_volume >= config.min_volume_for_verify
        {
            Self::try_auto_verify(&env, &address, &config, &metrics);
            return true;
        }

        false
    }

    /// Submit a manual verification request.
    /// The user's address is added to the verification queue for admin review.
    /// Calling this a second time before the request is processed is a no-op.
    pub fn request_verification(env: Env, user: Address) {
        user.require_auth();

        assert!(
            env.storage()
                .persistent()
                .has(&DataKey::UserProfile(user.clone())),
            "User not found"
        );

        let queue_key = DataKey::VerificationQueue;
        let mut queue: Vec<Address> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(&env));

        // Idempotent — skip if already in queue.
        for i in 0..queue.len() {
            if queue.get(i).as_ref() == Some(&user) {
                return;
            }
        }

        queue.push_back(user.clone());
        env.storage().persistent().set(&queue_key, &queue);
        Self::extend_persistent(&env, &queue_key);

        // Append to history
        let hist_key = DataKey::VerificationHistory(user.clone());
        let mut history: Vec<VerificationEntry> = env
            .storage()
            .persistent()
            .get(&hist_key)
            .unwrap_or(Vec::new(&env));
        history.push_back(VerificationEntry {
            timestamp: env.ledger().timestamp(),
            action: String::from_str(&env, "requested"),
            by: Some(user.clone()),
        });
        env.storage().persistent().set(&hist_key, &history);
        Self::extend_persistent(&env, &hist_key);
    }

    /// Approve or reject a pending verification request (admin only).
    ///
    /// # Arguments
    /// * `user` - Address of the user whose request is being processed
    /// * `approve` - `true` to verify the user, `false` to reject
    pub fn process_verification_request(env: Env, user: Address, approve: bool) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);
        config.platform_admin.require_auth();

        let profile_key = DataKey::UserProfile(user.clone());
        let mut profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .expect("User not found");
        Self::extend_persistent(&env, &profile_key);

        profile.is_verified = approve;
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        // Remove from queue
        let queue_key = DataKey::VerificationQueue;
        let queue: Vec<Address> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(&env));
        let mut new_queue: Vec<Address> = Vec::new(&env);
        for i in 0..queue.len() {
            if let Some(addr) = queue.get(i) {
                if addr != user {
                    new_queue.push_back(addr);
                }
            }
        }
        env.storage().persistent().set(&queue_key, &new_queue);
        Self::extend_persistent(&env, &queue_key);

        // Append to history
        let action = if approve { "approved" } else { "rejected" };
        let hist_key = DataKey::VerificationHistory(user.clone());
        let mut history: Vec<VerificationEntry> = env
            .storage()
            .persistent()
            .get(&hist_key)
            .unwrap_or(Vec::new(&env));
        history.push_back(VerificationEntry {
            timestamp: env.ledger().timestamp(),
            action: String::from_str(&env, action),
            by: Some(config.platform_admin.clone()),
        });
        env.storage().persistent().set(&hist_key, &history);
        Self::extend_persistent(&env, &hist_key);

        if approve {
            env.events()
                .publish((Symbol::new(&env, "UserVerified"),), &user);
        }
    }

    /// Get the full verification history for a user.
    pub fn get_verification_history(env: Env, user: Address) -> Vec<VerificationEntry> {
        let hist_key = DataKey::VerificationHistory(user.clone());
        let history = env
            .storage()
            .persistent()
            .get(&hist_key)
            .unwrap_or(Vec::new(&env));
        if env.storage().persistent().has(&hist_key) {
            Self::extend_persistent(&env, &hist_key);
        }
        history
    }

    /// Get all addresses currently awaiting manual verification (admin helper).
    pub fn get_verification_queue(env: Env) -> Vec<Address> {
        let queue_key = DataKey::VerificationQueue;
        let queue = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(Vec::new(&env));
        if env.storage().persistent().has(&queue_key) {
            Self::extend_persistent(&env, &queue_key);
        }
        queue
    }

    // -----------------------------------------------------------------------
    // Issue #100 – Reputation System (Trust Score)
    // -----------------------------------------------------------------------

    /// Update a user's reputation counters.
    ///
    /// This is called by the EscrowContract after a state change (release /
    /// refund / resolve). Auth: registered escrow contract, or admin if none set.
    ///
    /// # Arguments
    /// * `address` - User whose counters to update
    /// * `successful_delta` - Increment for successful_trades
    /// * `disputed_delta` - Increment for disputed_trades
    pub fn update_reputation(
        env: Env,
        address: Address,
        successful_delta: u32,
        disputed_delta: u32,
    ) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .expect("Contract not initialized");
        Self::extend_persistent(&env, &DataKey::Config);

        match config.escrow_contract {
            Some(ref escrow_addr) => escrow_addr.require_auth(),
            None => config.platform_admin.require_auth(),
        }

        let profile_key = DataKey::UserProfile(address.clone());
        let profile_opt: Option<UserProfile> = env.storage().persistent().get(&profile_key);
        let mut profile = match profile_opt {
            Some(p) => {
                Self::extend_persistent(&env, &profile_key);
                p
            }
            None => return, // User not onboarded; skip silently
        };

        profile.successful_trades = profile.successful_trades.saturating_add(successful_delta);
        profile.disputed_trades = profile.disputed_trades.saturating_add(disputed_delta);

        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);
    }

    /// Get a user's reputation counters.
    ///
    /// # Returns
    /// Tuple of (successful_trades, disputed_trades). Returns (0, 0) if not onboarded.
    pub fn get_user_reputation(env: Env, address: Address) -> (u32, u32) {
        match env
            .storage()
            .persistent()
            .get::<DataKey, UserProfile>(&DataKey::UserProfile(address.clone()))
        {
            Some(profile) => {
                Self::extend_persistent(&env, &DataKey::UserProfile(address));
                (profile.successful_trades, profile.disputed_trades)
            }
            None => (0, 0),
        }
    }
}
