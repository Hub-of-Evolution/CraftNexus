use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Map,
    String, Symbol, TryFromVal, Val, Vec,
};

/// Standard TTL threshold for persistent storage (approx 14 hours at 5s ledger)
const TTL_THRESHOLD: u32 = 10_000;
/// Standard TTL extension for persistent storage (approx 30 days)
const TTL_EXTENSION: u32 = 518_400;
const CURRENT_USER_PROFILE_VERSION: u32 = 4;

/// Cooldown period for username changes to prevent squatting and rapid identity rotation.
/// 30 days in seconds.
const USERNAME_CHANGE_COOLDOWN: u64 = 30 * 24 * 60 * 60;

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
    /// Pending manual verification request marker keyed by user (#138)
    VerificationRequest(Address),
    /// Queue head pointer for manual verification requests (#138)
    VerificationQueueHead,
    /// Queue tail pointer for manual verification requests (#138)
    VerificationQueueTail,
    /// Queue index -> address mapping for manual verification requests (#138)
    VerificationQueueIndex(u64),
    /// Verification history log per user (#63)
    VerificationHistory(Address),
    /// Username change fee (in stroops) - Issue #114
    UsernameChangeFee,
    /// Token used to collect username change fees (#134)
    UsernameChangeFeeToken,
    /// Destination wallet for username change fees (#134)
    UsernameChangeFeeWallet,
    /// Timestamp of last username change per user - Issue #114
    LastUsernameChange(Address),
}

/// User roles in the CraftNexus platform
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum UserRole {
    None = 0,      // User has not onboarded
    Buyer = 1,     // Can purchase items
    Artisan = 2,   // Can sell items and create escrow
    Admin = 3,     // Platform administrator
    Moderator = 4, // Can help manage disputes
}

/// Profile status for users
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ProfileStatus {
    Active = 0,
    Deactivated = 1,
}

/// Onboarding status for users
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
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
    /// Portfolio CID for artisan showcase (IPFS) - Issue #112
    pub portfolio_cid: Option<String>,
    /// Status of the user profile - Issue #113
    pub status: ProfileStatus,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
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
    /// Portfolio CID for artisan showcase (IPFS) - Issue #112
    pub portfolio_cid: Option<String>,
}

/// Activity metrics used to determine eligibility for auto-verification (#63)
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserMetrics {
    /// Total number of escrows the user participated in as seller
    pub total_escrow_count: u32,
    /// Total USDC volume (in stroops) the user transacted as seller
    pub total_volume: i128,
}

/// A single entry in a user's verification history log (#63)
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct VerificationEntry {
    pub timestamp: u64,
    /// "requested" | "approved" | "rejected" | "auto_verified"
    pub action: String,
    /// Address that performed the action (None for auto-verification)
    pub by: Option<Address>,
}

/// Contract configuration
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
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

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    /// Contract not initialized
    NotInitialized = 1,
    /// User not found
    UserNotFound = 2,
    /// Username already taken
    UsernameTaken = 3,
    /// Username too short
    UsernameTooShort = 4,
    /// Username too long
    UsernameTooLong = 5,
    /// Invalid role
    InvalidRole = 6,
    /// User already onboarded
    AlreadyOnboarded = 7,
    /// Unauthorized operation
    Unauthorized = 8,
    /// Profile is deactivated
    ProfileDeactivated = 9,
    /// Cannot deactivate with active escrows
    ActiveEscrowsExist = 10,
    /// Username change fee cannot be negative
    InvalidFee = 11,
    /// User is not an artisan
    NotAnArtisan = 12,
    /// Invalid portfolio CID format
    InvalidPortfolioCid = 13,
    /// Cooldown period not yet elapsed
    CooldownActive = 14,
}

#[soroban_sdk::contractclient(name = "EscrowClient")]
pub trait EscrowInterface {
    fn has_active_escrows(env: Env, user: Address) -> bool;
}

fn normalize_username(env: &Env, username: &String) -> String {
    const MAX_INPUT_BYTES: usize = 256;
    const MAX_OUTPUT_BYTES: usize = 256;
    let len = username.len() as usize;
    if len > MAX_INPUT_BYTES {
        // Can't use env.panic_with_error here without Env. 
        // But we can just use unwrap() on a None or something similar if we want to save space,
        // or just let it panic without a string.
        panic!();
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

/// Validate IPFS CID format (v0 and v1 with multibase prefixes).
///
/// Supports:
/// - CIDv0: 46-char Base58btc starting with "Qm"
/// - CIDv1 base32lower (prefix 'b'): lowercase a-z + 2-7
/// - CIDv1 base16lower (prefix 'f'): lowercase hex 0-9 + a-f
/// - CIDv1 base58btc  (prefix 'z'): Base58 alphabet
fn validate_ipfs_cid(cid: &String) -> bool {
    let len = cid.len() as usize;
    if len == 0 || len > 128 {
        return false;
    }

    let mut buf = [0u8; 128];
    cid.copy_into_slice(&mut buf[0..len]);
    let cid_bytes = &buf[0..len];

    // CIDv0: exactly 46 chars, starts with "Qm", Base58btc alphabet
    let is_v0 = len == 46
        && cid_bytes[0] == b'Q'
        && cid_bytes[1] == b'm'
        && cid_bytes.iter().all(|b| {
            matches!(
                *b,
                b'1'..=b'9'
                    | b'A'..=b'H'
                    | b'J'..=b'N'
                    | b'P'..=b'Z'
                    | b'a'..=b'k'
                    | b'm'..=b'z'
            )
        });

    if is_v0 {
        return true;
    }

    // CIDv1: minimum 3 chars (multibase prefix + version byte + codec)
    if len < 3 {
        return false;
    }

    let prefix = cid_bytes[0];
    let payload = &cid_bytes[1..];

    match prefix {
        // base32lower (most common CIDv1 encoding)
        b'b' => payload
            .iter()
            .all(|b| matches!(*b, b'a'..=b'z' | b'2'..=b'7')),
        // base16lower (hex)
        b'f' => payload
            .iter()
            .all(|b| matches!(*b, b'0'..=b'9' | b'a'..=b'f')),
        // base58btc
        b'z' => payload.iter().all(|b| {
            matches!(
                *b,
                b'1'..=b'9'
                    | b'A'..=b'H'
                    | b'J'..=b'N'
                    | b'P'..=b'Z'
                    | b'a'..=b'k'
                    | b'm'..=b'z'
            )
        }),
        _ => false,
    }
}

#[contract]
pub struct OnboardingContract;

#[contractimpl]
impl OnboardingContract {
    fn get_queue_pointer(env: &Env, key: &DataKey) -> u64 {
        let pointer = env.storage().persistent().get(key).unwrap_or(0u64);
        if env.storage().persistent().has(key) {
            Self::extend_persistent(env, key);
        }
        pointer
    }

    fn set_queue_pointer(env: &Env, key: DataKey, value: u64) {
        env.storage().persistent().set(&key, &value);
        Self::extend_persistent(env, &key);
    }

    fn is_verification_pending(env: &Env, user: &Address) -> bool {
        let key = DataKey::VerificationRequest(user.clone());
        let is_pending = env.storage().persistent().has(&key);
        if is_pending {
            Self::extend_persistent(env, &key);
        }
        is_pending
    }

    fn enqueue_verification_request(env: &Env, user: &Address) {
        let tail = Self::get_queue_pointer(env, &DataKey::VerificationQueueTail);
        let queue_index_key = DataKey::VerificationQueueIndex(tail);
        env.storage().persistent().set(&queue_index_key, user);
        Self::extend_persistent(env, &queue_index_key);

        let pending_key = DataKey::VerificationRequest(user.clone());
        env.storage()
            .persistent()
            .set(&pending_key, &env.ledger().timestamp());
        Self::extend_persistent(env, &pending_key);

        Self::set_queue_pointer(env, DataKey::VerificationQueueTail, tail + 1);
    }

    fn advance_verification_head(env: &Env) {
        let mut head = Self::get_queue_pointer(env, &DataKey::VerificationQueueHead);
        let tail = Self::get_queue_pointer(env, &DataKey::VerificationQueueTail);

        while head < tail {
            let queue_index_key = DataKey::VerificationQueueIndex(head);
            let queued_user: Option<Address> = env.storage().persistent().get(&queue_index_key);

            let Some(queued_user) = queued_user else {
                head += 1;
                continue;
            };

            if Self::is_verification_pending(env, &queued_user) {
                Self::extend_persistent(env, &queue_index_key);
                break;
            }

            env.storage().persistent().remove(&queue_index_key);
            head += 1;
        }

        Self::set_queue_pointer(env, DataKey::VerificationQueueHead, head);
    }

    fn clear_verification_request(env: &Env, user: &Address) {
        let pending_key = DataKey::VerificationRequest(user.clone());
        env.storage().persistent().remove(&pending_key);
        Self::advance_verification_head(env);
    }

    fn read_username_fee_token(env: &Env) -> Option<Address> {
        let key = DataKey::UsernameChangeFeeToken;
        let token = env.storage().persistent().get(&key);
        if env.storage().persistent().has(&key) {
            Self::extend_persistent(env, &key);
        }
        token
    }

    fn read_username_fee_wallet(env: &Env, config: &OnboardingConfig) -> Address {
        let key = DataKey::UsernameChangeFeeWallet;
        let wallet = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(config.platform_admin.clone());
        if env.storage().persistent().has(&key) {
            Self::extend_persistent(env, &key);
        }
        wallet
    }

    fn collect_username_change_fee(env: &Env, user: &Address, config: &OnboardingConfig) {
        let fee_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::UsernameChangeFee)
            .unwrap_or(0);

        if fee_amount <= 0 {
            return;
        }

        Self::extend_persistent(env, &DataKey::UsernameChangeFee);

        let fee_token = Self::read_username_fee_token(env)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        let fee_wallet = Self::read_username_fee_wallet(env, config);

        let token_client = token::Client::new(env, &fee_token);
        token_client.transfer(user, &fee_wallet, &fee_amount);
    }

    fn get_user_profile(env: &Env, user: Address) -> UserProfile {
        let key = DataKey::UserProfile(user.clone());
        let stored: Val = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
        let map = Map::<Symbol, Val>::try_from_val(env, &stored)
            .expect("");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let profile =
                UserProfile::try_from_val(env, &stored)
                    .expect("");
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
            portfolio_cid: legacy.portfolio_cid,
            status: ProfileStatus::Active,
        };
        env.storage().persistent().set(&key, &upgraded);
        Self::extend_persistent(env, &key);
        upgraded
    }

    fn upgrade_user_profile(env: &Env, user: Address, mut profile: UserProfile) -> UserProfile {
        profile.version = CURRENT_USER_PROFILE_VERSION;
        // Initialize portfolio_cid to None for existing profiles
        if profile.portfolio_cid.is_none() {
            profile.portfolio_cid = None;
        }
        // Initialize status to Active for existing profiles
        profile.status = ProfileStatus::Active;
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
            portfolio_cid: None,
            status: ProfileStatus::Active,
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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
            portfolio_cid: None,
            status: ProfileStatus::Active,
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
        env.storage().persistent().has(&key)
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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

    /// Deactivate the user's profile and release their username.
    /// Reverts if:
    /// - User has active escrows (traditional or recurring)
    /// - User is "admin"
    /// - Profile is already deactivated
    pub fn deactivate_profile(env: Env, user: Address) {
        user.require_auth();
        let mut profile = Self::get_user_profile(&env, user.clone());

        if profile.status == ProfileStatus::Deactivated {
            env.panic_with_error(Error::ProfileDeactivated);
        }

        let normalized = normalize_username(&env, &profile.username);
        if normalized == String::from_str(&env, "admin") {
            env.panic_with_error(Error::Unauthorized);
        }

        // Check for active escrows via cross-contract call if available
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        if let Some(escrow_contract) = config.escrow_contract {
            let client = EscrowClient::new(&env, &escrow_contract);
            if client.has_active_escrows(&user) {
                env.panic_with_error(Error::ActiveEscrowsExist);
            }
        }

        // Release username so others can take it
        env.storage()
            .persistent()
            .remove(&DataKey::Username(normalized));

        // Update profile state
        profile.status = ProfileStatus::Deactivated;
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
        Self::extend_persistent(&env, &DataKey::UserProfile(user.clone()));

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "ProfileDeactivated"), user.clone()), user);
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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
        token_address: Address,
    ) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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

        // Normalize volume to 7 decimals (base decimal for auto-verification thresholds)
        let token_client = token::Client::new(&env, &token_address);
        let token_decimals = token_client.decimals();
        let base_decimals = 7u32;

        let normalized_delta = if token_decimals < base_decimals {
            let diff = base_decimals - token_decimals;
            volume_delta.saturating_mul(10i128.pow(diff))
        } else if token_decimals > base_decimals {
            let diff = token_decimals - base_decimals;
            volume_delta / 10i128.pow(diff)
        } else {
            volume_delta
        };

        metrics.total_volume = metrics.total_volume.saturating_add(normalized_delta);

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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        let profile_key = DataKey::UserProfile(address.clone());
        let profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
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

        if Self::is_verification_pending(&env, &user) {
            return;
        }

        Self::enqueue_verification_request(&env, &user);

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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);
        config.platform_admin.require_auth();

        let profile_key = DataKey::UserProfile(user.clone());
        let mut profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
        Self::extend_persistent(&env, &profile_key);

        profile.is_verified = approve;
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        Self::clear_verification_request(&env, &user);

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
        Self::advance_verification_head(&env);

        let head = Self::get_queue_pointer(&env, &DataKey::VerificationQueueHead);
        let tail = Self::get_queue_pointer(&env, &DataKey::VerificationQueueTail);
        let mut queue = Vec::new(&env);

        for index in head..tail {
            let queue_index_key = DataKey::VerificationQueueIndex(index);
            if let Some(user) = env.storage().persistent().get::<DataKey, Address>(&queue_index_key)
            {
                if Self::is_verification_pending(&env, &user) {
                    queue.push_back(user);
                    Self::extend_persistent(&env, &queue_index_key);
                }
            }
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
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
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

    // -----------------------------------------------------------------------
    // Issue #114 – Username Change Mechanism
    // -----------------------------------------------------------------------

    /// Change a user's username (Issue #114)
    ///
    /// Atomically removes the old username mapping and adds the new one.
    /// Validates the new username for uniqueness, length, and normalization.
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    /// * `new_username` - Desired new username
    ///
    /// # Reverts if
    /// - User not onboarded
    /// - New username already taken
    /// - New username too short or too long
    /// - Username change fee not paid (if configured)
    pub fn change_username(env: Env, user: Address, new_username: String) -> UserProfile {
        user.require_auth();

        // Get configuration
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        // Get current user profile
        let profile_key = DataKey::UserProfile(user.clone());
        let mut profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
        Self::extend_persistent(&env, &profile_key);

        // Normalize the new username
        let normalized_new = normalize_username(&env, &new_username);

        // Validate new username length
        let username_len = normalized_new.len() as u32;
        assert!(
            username_len >= config.min_username_length,
            "Username too short"
        );
        assert!(
            username_len <= config.max_username_length,
            "Username too long"
        );

        // Enforce cooldown between username changes for the same user.
        if let Some(last_change) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::LastUsernameChange(user.clone()))
        {
            let current_time = env.ledger().timestamp();
            assert!(
                current_time > last_change.saturating_add(USERNAME_CHANGE_COOLDOWN),
                "Username change cooldown active"
            );
        }

        // Check if new username is already taken
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKey::Username(normalized_new.clone())),
            "Username already taken"
        );

        Self::collect_username_change_fee(&env, &user, &config);

        // Atomically remove old username mapping and add new one
        let old_username = profile.username.clone();
        env.storage()
            .persistent()
            .remove(&DataKey::Username(old_username));

        // Store new username → address mapping
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized_new.clone()), &user);
        Self::extend_persistent(&env, &DataKey::Username(normalized_new.clone()));

        // Update profile with new username
        profile.username = normalized_new;

        // Store updated profile
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        // Record timestamp of username change
        env.storage().persistent().set(
            &DataKey::LastUsernameChange(user.clone()),
            &env.ledger().timestamp(),
        );
        Self::extend_persistent(&env, &DataKey::LastUsernameChange(user.clone()));

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "UsernameChanged"),), &user);

        profile
    }

    /// Set the username change fee (admin only) - Issue #114
    ///
    /// # Arguments
    /// * `fee` - Fee amount in stroops (0 to disable)
    pub fn set_username_change_fee(env: Env, fee: i128) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        config.platform_admin.require_auth();
        if fee < 0 {
            env.panic_with_error(Error::InvalidFee);
        }

        env.storage()
            .persistent()
            .set(&DataKey::UsernameChangeFee, &fee);
        Self::extend_persistent(&env, &DataKey::UsernameChangeFee);
    }

    /// Set the token used to collect username change fees (admin only).
    pub fn set_username_fee_token(env: Env, token: Address) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        config.platform_admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::UsernameChangeFeeToken, &token);
        Self::extend_persistent(&env, &DataKey::UsernameChangeFeeToken);
    }

    /// Set the wallet that receives username change fees (admin only).
    pub fn set_username_fee_wallet(env: Env, wallet: Address) {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        config.platform_admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::UsernameChangeFeeWallet, &wallet);
        Self::extend_persistent(&env, &DataKey::UsernameChangeFeeWallet);
    }

    /// Get the current username change fee - Issue #114
    pub fn get_username_change_fee(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::UsernameChangeFee)
            .unwrap_or(0)
    }

    /// Get the configured token used for username change fees.
    pub fn get_username_fee_token(env: Env) -> Option<Address> {
        Self::read_username_fee_token(&env)
    }

    /// Get the configured wallet used for username change fees.
    pub fn get_username_fee_wallet(env: Env) -> Address {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);
        Self::read_username_fee_wallet(&env, &config)
    }

    // -----------------------------------------------------------------------
    // Issue #112 – Artisan Portfolio Verification
    // -----------------------------------------------------------------------

    /// Update an artisan's portfolio CID (Issue #112)
    ///
    /// Allows artisans to update their portfolio showcase stored on IPFS.
    /// Validates the CID format using the same validation as escrow metadata.
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    /// * `portfolio_cid` - IPFS CID of the portfolio (or None to remove)
    ///
    /// # Reverts if
    /// - User not onboarded
    /// - User is not an artisan
    /// - Invalid CID format (if provided)
    pub fn update_portfolio(env: Env, user: Address, portfolio_cid: Option<String>) -> UserProfile {
        user.require_auth();

        // Get current user profile
        let profile_key = DataKey::UserProfile(user.clone());
        let mut profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
        Self::extend_persistent(&env, &profile_key);

        // Only artisans can update their portfolio
        assert!(
            profile.role == UserRole::Artisan,
            "Only artisans can update portfolio"
        );

        // Validate CID format if provided
        if let Some(ref cid) = portfolio_cid {
            assert!(validate_ipfs_cid(cid), "Invalid portfolio CID format");
        }

        // Update portfolio CID
        profile.portfolio_cid = portfolio_cid;

        // Store updated profile
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "PortfolioUpdated"),), &user);

        profile
    }
}
