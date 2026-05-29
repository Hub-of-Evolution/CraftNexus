//! # Onboarding Module
//!
//! Handles user profile creation, role management, reputation tracking, and
//! username administration on the Stellar Soroban network for the CraftNexus
//! platform.
//!
//! ## Integration Notes
//!
//! - All profiles are versioned using [`CURRENT_USER_PROFILE_VERSION`].
//!   Profile shape changes require corresponding upgrade scripts; the
//!   [`OnboardingContract::try_get_user_profile`] helper migrates legacy
//!   profiles transparently on first read.
//! - Persistent storage uses [`extend_ttl`] on every read/write to prevent
//!   key expiry. The constants [`TTL_THRESHOLD`] and [`TTL_EXTENSION`] govern
//!   the renewal window (~14 hours threshold, ~30 days extension).
//! - Cross-contract calls from the EscrowContract use
//!   [`OnboardingContract::update_reputation`] and
//!   [`OnboardingContract::update_user_metrics`]. Both require the caller to
//!   be the registered `escrow_contract` address (or `platform_admin` if none
//!   is set).
//! - Username normalization is applied on every write and lookup, making all
//!   username comparisons case-insensitive and separator-agnostic.
//! - `symbol_short!` is preferred over heap-allocated strings for storage
//!   keys and event topics because it fits in a single `Val` word, reducing
//!   on-chain storage cost.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, Map, String, Symbol,
    TryFromVal, Val, Vec,
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

/// Storage keys for the onboarding contract.
///
/// Each variant maps to a distinct persistent-storage slot. Keys that include
/// an [`Address`] or [`u64`] are per-entity; all others are global singletons.
///
/// ## On-chain cost note
/// Persistent storage entries incur rent. Every read/write in this contract
/// calls [`extend_ttl`] to keep entries alive for ~30 days, preventing
/// accidental expiry of user profiles.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Maps a user address to their [`UserProfile`]
    UserProfile(Address),
    /// Maps a normalized username to the owning address (uniqueness index)
    Username(String),
    /// Contract configuration ([`OnboardingConfig`])
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

/// User roles in the CraftNexus platform.
///
/// Roles are stored inside [`UserProfile`] and gate which operations a user
/// may perform. Self-onboarding via [`OnboardingContract::onboard_user`] only
/// allows `Buyer` or `Artisan`; `Admin` and `Moderator` are assigned by the
/// platform admin via [`OnboardingContract::update_user_role`].
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum UserRole {
    /// User has not onboarded (default / sentinel value)
    None = 0,
    /// Can purchase items and initiate escrows as buyer
    Buyer = 1,
    /// Can sell items, create escrows as seller, and maintain a portfolio CID
    Artisan = 2,
    /// Platform administrator — can update config, verify users, and manage roles
    Admin = 3,
    /// Can help manage disputes alongside the arbitrator
    Moderator = 4,
}

/// Lifecycle status of a user profile.
///
/// A deactivated profile releases the username back to the pool so another
/// user may claim it. Deactivation is blocked while the user has active
/// escrows (checked via cross-contract call to the registered EscrowContract).
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ProfileStatus {
    /// Profile is active and fully operational
    Active = 0,
    /// Profile has been deactivated by the user; username is released
    Deactivated = 1,
}

/// On-chain user profile stored under [`DataKey::UserProfile`].
///
/// Versioned via the `version` field (current: [`CURRENT_USER_PROFILE_VERSION`]).
/// Legacy profiles (missing `version` or `status`) are migrated transparently
/// on first read by [`OnboardingContract::try_get_user_profile`].
///
/// ## Storage cost note
/// Each `UserProfile` occupies a persistent storage entry. The `username`
/// field is a heap-allocated [`String`]; keep it within the configured
/// `max_username_length` (default 50 bytes) to bound entry size.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserProfile {
    /// Schema version — guards profile shape changes.
    /// Must equal [`CURRENT_USER_PROFILE_VERSION`]; older values trigger
    /// an in-place upgrade on read.
    pub version: u32,
    /// The user's Stellar account or contract address
    pub address: Address,
    /// Role assigned to this user (see [`UserRole`])
    pub role: UserRole,
    /// Normalized username (lowercase, separator-collapsed).
    /// Uniqueness is enforced via the [`DataKey::Username`] index.
    pub username: String,
    /// Ledger timestamp (seconds since Unix epoch) when the profile was created
    pub registered_at: u64,
    /// Whether the user has passed verification (manual or auto-threshold)
    pub is_verified: bool,
    /// Count of escrows where this user was on the winning side (#100)
    pub successful_trades: u32,
    /// Count of escrows that ended in a dispute against this user (#100)
    pub disputed_trades: u32,
    /// IPFS CID of the artisan's portfolio showcase — `None` for buyers.
    /// Must pass [`validate_ipfs_cid`] if set. Issue #112.
    pub portfolio_cid: Option<String>,
    /// Lifecycle status of the profile (see [`ProfileStatus`]). Issue #113.
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

/// Activity metrics used to determine eligibility for auto-verification (#63).
///
/// Stored under [`DataKey::UserMetrics`] and updated by the registered
/// EscrowContract via [`OnboardingContract::update_user_metrics`].
/// Volume is normalized to 7 decimal places (USDC base) before accumulation
/// so that thresholds remain token-agnostic.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserMetrics {
    /// Total number of escrows the user participated in as seller
    pub total_escrow_count: u32,
    /// Total volume (normalized to 7 decimals / USDC base) transacted as seller.
    /// Used to compare against [`OnboardingConfig::min_volume_for_verify`].
    pub total_volume: i128,
}

/// Event emitted when a new user successfully onboards via [`OnboardingContract::onboard_user`].
///
/// Topic: `("UserOnboarded",)` — emitted to the contract's event stream.
/// Data shape: `UserOnboardedEvent { user, username, role }`.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserOnboardedEvent {
    /// The newly onboarded user's address
    pub user: Address,
    /// Normalized username assigned to the user
    pub username: String,
    /// Role the user selected during onboarding
    pub role: UserRole,
}

/// A single entry in a user's verification history log (#63).
///
/// Stored in a bounded [`Vec`] (max 10 entries) under
/// [`DataKey::VerificationHistory`]. Oldest entries are dropped FIFO when
/// the cap is reached.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct VerificationEntry {
    /// Ledger timestamp when this action occurred
    pub timestamp: u64,
    /// Action taken: `"requested"` | `"approved"` | `"rejected"` | `"auto_verified"` | `"username_changed_revoked"`
    pub action: String,
    /// Address that performed the action (`None` for auto-verification)
    pub by: Option<Address>,
}

/// Global configuration for the onboarding contract.
///
/// Stored under [`DataKey::Config`] (persistent, singleton). All admin-only
/// functions read this entry first and call `platform_admin.require_auth()`
/// before mutating state — this is the checks-effects-interactions pattern
/// applied to authorization.
///
/// ## TTL note
/// `extend_ttl` is called on every read of this key to prevent the config
/// from expiring. A missing config causes most functions to panic with
/// [`Error::NotInitialized`].
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct OnboardingConfig {
    /// Whether a username is required during onboarding (default: `true`)
    pub require_username: bool,
    /// Minimum byte-length of a normalized username (default: 3)
    pub min_username_length: u32,
    /// Maximum byte-length of a normalized username (default: 50)
    pub max_username_length: u32,
    /// Platform administrator address — the only address that can call admin-gated functions
    pub platform_admin: Address,
    /// Whether threshold-based auto-verification is active (default: `true`)
    pub auto_verify_enabled: bool,
    /// Minimum completed escrow count for auto-verification (default: 5) (#63)
    pub min_escrow_count_for_verify: u32,
    /// Minimum total volume (7-decimal normalized) for auto-verification (default: 10_000_000_000) (#63)
    pub min_volume_for_verify: i128,
    /// Address of the EscrowContract authorized to call `update_reputation` / `update_user_metrics`.
    /// If `None`, the `platform_admin` is used as fallback caller. (#63, #100)
    pub escrow_contract: Option<Address>,
}

/// Errors returned by the onboarding contract.
///
/// All variants map to a `u32` discriminant so they can be returned as
/// Soroban contract errors and decoded by SDK clients.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    /// Contract has not been initialized — call `initialize` first
    NotInitialized = 1,
    /// No profile found for the given address
    UserNotFound = 2,
    /// The requested username is already registered by another user
    UsernameTaken = 3,
    /// Normalized username is shorter than `min_username_length`
    UsernameTooShort = 4,
    /// Normalized username is longer than `max_username_length`
    UsernameTooLong = 5,
    /// Role value is not valid for the requested operation
    InvalidRole = 6,
    /// A profile already exists for this address
    AlreadyOnboarded = 7,
    /// Caller is not authorized to perform this operation
    Unauthorized = 8,
    /// The profile has been deactivated and cannot be used
    ProfileDeactivated = 9,
    /// Cannot deactivate a profile that has active escrows
    ActiveEscrowsExist = 10,
    /// Username change fee must be ≥ 0
    InvalidFee = 11,
    /// Operation requires the user to have the `Artisan` role
    NotAnArtisan = 12,
    /// The provided portfolio CID does not pass IPFS CID validation
    InvalidPortfolioCid = 13,
    /// Username change cooldown period has not yet elapsed (30 days)
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
        b'b' => {
            // Stricter length check for typical CIDv1 base32 (sha256/dag-pb is 59 chars)
            // Allow range for different hash types but enforce minimum for valid multihash payload
            if len < 50 || len > 100 {
                return false;
            }
            // Logic check: CIDv1 base32 ALWAYS starts with 'ba' because version byte 0x01
            // starts with 'a' in base32 bit-alignment.
            if cid_bytes[1] != b'a' {
                return false;
            }
            payload
                .iter()
                .all(|b| matches!(*b, b'a'..=b'z' | b'2'..=b'7'))
        }
        // base16lower (hex)
        b'f' => {
            // CIDv1 base16 typically ~73 chars for sha256
            if len < 60 || len > 120 {
                return false;
            }
            // Logic check: CIDv1 base16 ALWAYS starts with 'f01' (0x01 version byte)
            if cid_bytes[1] != b'0' || cid_bytes[2] != b'1' {
                return false;
            }
            payload
                .iter()
                .all(|b| matches!(*b, b'0'..=b'9' | b'a'..=b'f'))
        }
        // base58btc
        b'z' => {
            // CIDv1 base58 typically ~50 chars
            if len < 40 || len > 100 {
                return false;
            }
            payload.iter().all(|b| {
                matches!(
                    *b,
                    b'1'..=b'9'
                        | b'A'..=b'H'
                        | b'J'..=b'N'
                        | b'P'..=b'Z'
                        | b'a'..=b'k'
                        | b'm'..=b'z'
                )
            })
        }
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
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(config.platform_admin.clone())
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

    fn try_get_user_profile(env: &Env, user: Address) -> Option<UserProfile> {
        let key = DataKey::UserProfile(user.clone());
        let stored: Val = env.storage().persistent().get(&key)?;
        let map = Map::<Symbol, Val>::try_from_val(env, &stored).expect("");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let profile = UserProfile::try_from_val(env, &stored).expect("");
            if profile.version < CURRENT_USER_PROFILE_VERSION {
                return Some(Self::upgrade_user_profile(env, user, profile));
            }
            return Some(profile);
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
        Some(upgraded)
    }

    fn get_user_profile(env: &Env, user: Address) -> UserProfile {
        Self::try_get_user_profile(env, user).unwrap_or_else(|| env.panic_with_error(Error::UserNotFound))
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
    ///
    /// Called on every read and write of persistent keys to prevent rent
    /// expiry. Uses [`TTL_THRESHOLD`] (~14 hours) as the minimum remaining
    /// ledger count before renewal triggers, and [`TTL_EXTENSION`] (~30 days)
    /// as the new TTL granted on renewal.
    ///
    /// ## Why TTL matters
    /// Soroban persistent storage entries expire if their TTL reaches zero.
    /// An expired profile would be indistinguishable from a non-existent one,
    /// causing `get_user` to panic with `UserNotFound`. Proactive TTL
    /// extension on every access ensures active users never lose their data.
    fn extend_persistent(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Initialize the onboarding contract.
    ///
    /// Must be called exactly once by the deployer before any other function.
    /// Creates the default [`OnboardingConfig`], registers the admin as the
    /// first user with role [`UserRole::Admin`] and username `"admin"`, and
    /// reserves that username in the uniqueness index.
    ///
    /// # Parameters
    /// - `admin`: `Address` — The platform administrator. Must authorize this
    ///   call (`admin.require_auth()`). Stored as `platform_admin` in config.
    ///
    /// # Preconditions
    /// - No prior call to `initialize` on this contract instance.
    ///   (Re-initializing overwrites config and the admin profile.)
    ///
    /// # Storage Side-Effects
    /// - **Write** [`DataKey::Config`] — stores the default [`OnboardingConfig`]
    /// - **Write** [`DataKey::UserProfile(admin)`] — admin profile with role `Admin`
    /// - **Write** [`DataKey::Username("admin")`] — reserves the normalized username
    /// - All three entries have their TTL extended via `extend_ttl`
    ///
    /// # Emitted Events
    /// None. (Profile creation events are only emitted by `onboard_user`.)
    ///
    /// # Errors
    /// - Panics if `admin.require_auth()` fails (unauthorized deployer).
    ///
    /// # Example
    /// ```ignore
    /// let config = client.initialize(&admin_address);
    /// assert_eq!(config.platform_admin, admin_address);
    /// assert_eq!(config.min_username_length, 3);
    /// ```
    pub fn initialize(env: Env, admin: Address) -> OnboardingConfig {
        // Only the deployer can initialize
        admin.require_auth();

        let config = OnboardingConfig {
            require_username: true,
            min_username_length: 3,
            max_username_length: 50,
            platform_admin: admin.clone(),
            auto_verify_enabled: true,
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

    /// Onboard a new user to the CraftNexus platform.
    ///
    /// Creates a versioned [`UserProfile`] for `user`, normalizes and reserves
    /// the requested `username`, and emits a `UserOnboarded` event. This is
    /// the primary entry point for new participants.
    ///
    /// ## Checks-Effects-Interactions
    /// All validation (auth, role, username length, uniqueness) is performed
    /// before any storage writes, following the CEI pattern to prevent
    /// partial-state corruption on revert.
    ///
    /// # Parameters
    /// - `user`: `Address` — The wallet address to onboard. Must authorize
    ///   this call (`user.require_auth()`).
    /// - `username`: `String` — Desired display name. Will be normalized
    ///   (lowercased, separators collapsed to `_`, Unicode mapped to ASCII).
    ///   The normalized form is what gets stored and indexed.
    /// - `role`: [`UserRole`] — Must be `Buyer` or `Artisan`. `Admin` and
    ///   `Moderator` cannot be self-assigned.
    ///
    /// # Preconditions
    /// - Contract must be initialized ([`DataKey::Config`] must exist).
    /// - `user` must not already have a profile.
    /// - Normalized `username` must be unique (not in [`DataKey::Username`] index).
    /// - Normalized `username` length must be within `[min_username_length, max_username_length]`.
    /// - `role` must be `Buyer` or `Artisan`.
    ///
    /// # Storage Side-Effects
    /// - **Write** [`DataKey::UserProfile(user)`] — new profile at version
    ///   [`CURRENT_USER_PROFILE_VERSION`], `is_verified = false`
    /// - **Write** [`DataKey::Username(normalized)`] — maps username → `user`
    /// - **Read** [`DataKey::Config`] — TTL extended on read
    /// - **Read** [`DataKey::UserProfile(user)`] — existence check (TTL extended if found)
    ///
    /// # Emitted Events
    /// - Topic: `(Symbol("UserOnboarded"),)` — Data: [`UserOnboardedEvent`]
    ///   `{ user, username: normalized, role }`
    ///
    /// # Errors
    /// - Panics with `"Invalid role: can only onboard as Buyer or Artisan"` if role is invalid
    /// - Panics with [`Error::NotInitialized`] if config is missing
    /// - Panics with `"User already onboarded"` if profile exists
    /// - Panics with `"Username already taken"` if normalized username is in use
    /// - Panics with `"Username too short"` / `"Username too long"` on length violation
    ///
    /// # Example
    /// ```ignore
    /// let profile = client.onboard_user(
    ///     &user_address,
    ///     &String::from_str(&env, "Alice"),
    ///     &UserRole::Artisan,
    /// );
    /// assert_eq!(profile.username, String::from_str(&env, "alice"));
    /// assert!(!profile.is_verified);
    /// ```
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
        Self::extend_persistent(&env, &DataKey::Username(normalized.clone()));

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "UserOnboarded"),),
            UserOnboardedEvent {
                user: user.clone(),
                username: normalized,
                role,
            },
        );

        profile
    }

    /// Get a user's profile by address.
    ///
    /// Transparently migrates legacy profiles (missing `version` or `status`
    /// fields) to [`CURRENT_USER_PROFILE_VERSION`] on first read and persists
    /// the upgraded form. This ensures callers always receive a fully-shaped
    /// [`UserProfile`] regardless of when the account was created.
    ///
    /// # Parameters
    /// - `user`: `Address` — The wallet address to look up.
    ///
    /// # Preconditions
    /// - A profile must exist for `user`.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(user)`] — TTL extended on read.
    /// - **Write** [`DataKey::UserProfile(user)`] — only if a legacy migration
    ///   is performed (upgrades the stored shape in-place).
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::UserNotFound`] if no profile exists.
    ///
    /// # Example
    /// ```ignore
    /// let profile = client.get_user(&user_address);
    /// assert_eq!(profile.version, CURRENT_USER_PROFILE_VERSION);
    /// ```
    pub fn get_user(env: Env, user: Address) -> UserProfile {
        Self::get_user_profile(&env, user)
    }

    /// Get a user's profile by username (case-insensitive).
    ///
    /// Normalizes the input username before looking up the owner address in
    /// the [`DataKey::Username`] index, then delegates to `get_user`.
    ///
    /// # Parameters
    /// - `username`: `String` — The username to look up (any case/separator variant).
    ///
    /// # Preconditions
    /// - The normalized form of `username` must be registered.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Username(normalized)`] — TTL extended on read.
    /// - **Read** [`DataKey::UserProfile(owner)`] — TTL extended on read.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with `"Username not found"` if the normalized username has no owner.
    /// - Panics with [`Error::UserNotFound`] if the owner has no profile (should not occur).
    ///
    /// # Example
    /// ```ignore
    /// let profile = client.get_user_by_username(&String::from_str(&env, "Alice"));
    /// assert_eq!(profile.username, String::from_str(&env, "alice"));
    /// ```
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

    /// Check if a username is already taken (case-insensitive).
    ///
    /// Normalizes the input before checking the [`DataKey::Username`] index.
    /// Safe to call without auth — read-only.
    ///
    /// # Parameters
    /// - `username`: `String` — Username to check (any case/separator variant).
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Username(normalized)`] — TTL extended if the key exists.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None — always returns a `bool`.
    ///
    /// # Example
    /// ```ignore
    /// assert!(!client.is_username_taken(&String::from_str(&env, "newuser")));
    /// ```
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

    /// Check if a user has completed onboarding.
    ///
    /// Returns `true` if a [`DataKey::UserProfile`] entry exists for `user`,
    /// regardless of profile status or version. Does NOT extend TTL.
    ///
    /// # Parameters
    /// - `user`: `Address` — The wallet address to check.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(user)`] — existence check only, no TTL extension.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None — always returns a `bool`.
    pub fn is_onboarded(env: Env, user: Address) -> bool {
        let key = DataKey::UserProfile(user.clone());
        env.storage().persistent().has(&key)
    }

    /// Get a user's role.
    ///
    /// Returns [`UserRole::None`] if the user has no profile, rather than
    /// panicking — safe for use in authorization checks.
    ///
    /// # Parameters
    /// - `user`: `Address` — The wallet address to query.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(user)`] — TTL extended if profile exists.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None — returns `UserRole::None` for unknown addresses.
    pub fn get_user_role(env: Env, user: Address) -> UserRole {
        if let Some(profile) = Self::try_get_user_profile(&env, user) {
            profile.role
        } else {
            UserRole::None
        }
    }

    /// Assign or update the moderator role for a user (admin only).
    ///
    /// Convenience wrapper around [`update_user_role`] that always sets
    /// [`UserRole::Moderator`]. Requires `platform_admin` authorization.
    ///
    /// # Parameters
    /// - `user`: `Address` — The address to promote to moderator.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    /// - `user` must have an existing profile.
    ///
    /// # Storage Side-Effects
    /// - **Read/Write** [`DataKey::Config`] — reads admin, TTL extended.
    /// - **Read/Write** [`DataKey::UserProfile(user)`] — role updated, TTL extended.
    ///
    /// # Emitted Events
    /// - Topic: `("RoleUpdated",)` — Data: `user` address.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::UserNotFound`] if `user` has no profile.
    pub fn set_moderator(env: Env, user: Address) -> UserProfile {
        Self::update_user_role(env, user, UserRole::Moderator)
    }

    /// Update a user's role (admin only).
    ///
    /// Allows the platform admin to assign any [`UserRole`] to an existing
    /// user. Used to promote buyers to artisans, assign moderators, etc.
    ///
    /// # Parameters
    /// - `user`: `Address` — The address whose role to update.
    /// - `new_role`: [`UserRole`] — The role to assign.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    /// - `user` must have an existing profile.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads admin address, TTL extended.
    /// - **Read/Write** [`DataKey::UserProfile(user)`] — role updated, TTL extended.
    ///
    /// # Emitted Events
    /// - Topic: `("RoleUpdated",)` — Data: `user` address.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::UserNotFound`] if `user` has no profile.
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
    ///
    /// Sets `status` to [`ProfileStatus::Deactivated`] and removes the
    /// username from the uniqueness index so another user may claim it.
    /// The profile record itself is retained for audit purposes.
    ///
    /// # Parameters
    /// - `user`: `Address` — The user deactivating their own profile. Must
    ///   authorize this call (`user.require_auth()`).
    ///
    /// # Preconditions
    /// - `user` must have an existing profile.
    /// - Profile must not already be deactivated.
    /// - Username must not be `"admin"` (the admin profile cannot be deactivated).
    /// - If an EscrowContract is registered, `user` must have no active escrows.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads `escrow_contract`, TTL extended.
    /// - **Read** [`DataKey::UserProfile(user)`] — reads current status and username.
    /// - **Remove** [`DataKey::Username(normalized)`] — releases username.
    /// - **Write** [`DataKey::UserProfile(user)`] — `status = Deactivated`, TTL extended.
    ///
    /// # Emitted Events
    /// - Topic: `("ProfileDeactivated", user)` — Data: `user` address.
    ///
    /// # Errors
    /// - Panics with [`Error::ProfileDeactivated`] if already deactivated.
    /// - Panics with [`Error::Unauthorized`] if username is `"admin"`.
    /// - Panics with [`Error::ActiveEscrowsExist`] if user has active escrows.
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
        env.events().publish(
            (Symbol::new(&env, "ProfileDeactivated"), user.clone()),
            user,
        );
    }

    /// Verify a user (admin only).
    ///
    /// Sets `is_verified = true` on the user's profile. Verification unlocks
    /// trust-gated features on the platform. Can also be triggered
    /// automatically via [`update_user_metrics`] when activity thresholds
    /// are met.
    ///
    /// # Parameters
    /// - `user`: `Address` — The address to verify.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    /// - `user` must have an existing profile.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads admin address, TTL extended.
    /// - **Read/Write** [`DataKey::UserProfile(user)`] — `is_verified` set to `true`, TTL extended.
    ///
    /// # Emitted Events
    /// - Topic: `("UserVerified",)` — Data: `user` address.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::UserNotFound`] if `user` has no profile.
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

    /// Get the onboarding contract configuration.
    ///
    /// Read-only. Returns the current [`OnboardingConfig`] singleton.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — no TTL extension (read-only path).
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    pub fn get_config(env: Env) -> OnboardingConfig {
        env.storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized))
    }

    /// Check if a user has a specific role.
    ///
    /// Convenience wrapper around [`get_user_role`]. Returns `false` for
    /// unknown addresses (no panic).
    ///
    /// # Parameters
    /// - `user`: `Address` — The address to check.
    /// - `role`: [`UserRole`] — The role to test for.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(user)`] — TTL extended if profile exists.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn has_role(env: Env, user: Address, role: UserRole) -> bool {
        Self::get_user_role(env, user) == role
    }

    /// Check if a user is verified.
    ///
    /// Returns `false` for unknown addresses (no panic).
    ///
    /// # Parameters
    /// - `user`: `Address` — The address to check.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(user)`] — TTL extended if profile exists.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn is_verified(env: Env, user: Address) -> bool {
        if let Some(profile) = Self::try_get_user_profile(&env, user) {
            profile.is_verified
        } else {
            false
        }
    }

    // -----------------------------------------------------------------------
    // Issue #63 – Artisan Verification Logic Enhancement
    // -----------------------------------------------------------------------

    /// Register the address of the deployed EscrowContract so it can update
    /// reputation and activity metrics via cross-contract calls (admin only).
    ///
    /// Once set, only this address (not `platform_admin`) is accepted as the
    /// caller for [`update_reputation`] and [`update_user_metrics`].
    ///
    /// # Parameters
    /// - `contract_address`: `Address` — The deployed EscrowContract address.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    ///
    /// # Storage Side-Effects
    /// - **Read/Write** [`DataKey::Config`] — `escrow_contract` field updated, TTL extended.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
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
    /// Changes take effect immediately — the next call to [`update_user_metrics`]
    /// or [`auto_verify_user`] will use the new values.
    ///
    /// # Parameters
    /// - `min_escrow_count`: `u32` — Minimum number of completed escrows required
    ///   for auto-verification. Stored in [`OnboardingConfig::min_escrow_count_for_verify`].
    /// - `min_volume`: `i128` — Minimum total transaction volume (7-decimal normalized,
    ///   USDC base) required. Stored in [`OnboardingConfig::min_volume_for_verify`].
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    ///
    /// # Storage Side-Effects
    /// - **Read/Write** [`DataKey::Config`] — thresholds updated, TTL extended.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
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

    /// Enable or disable threshold-based automatic verification (admin only).
    ///
    /// When disabled, [`update_user_metrics`] will still accumulate metrics
    /// but will not trigger auto-verification. Manual verification via
    /// [`process_verification_request`] and [`verify_user`] remains available.
    ///
    /// # Parameters
    /// - `enabled`: `bool` — `true` to enable auto-verification, `false` to disable.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    ///
    /// # Storage Side-Effects
    /// - **Read/Write** [`DataKey::Config`] — `auto_verify_enabled` updated, TTL extended.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    pub fn set_auto_verify_enabled(env: Env, enabled: bool) {
        let mut config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        config.platform_admin.require_auth();
        config.auto_verify_enabled = enabled;

        env.storage().persistent().set(&DataKey::Config, &config);
        Self::extend_persistent(&env, &DataKey::Config);
    }

    /// Get activity metrics for a user.
    ///
    /// Returns zeroed [`UserMetrics`] if no escrow activity has been recorded
    /// yet — never panics for unknown addresses.
    ///
    /// # Parameters
    /// - `address`: `Address` — The user whose metrics to retrieve.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserMetrics(address)`] — no TTL extension (read-only).
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn get_user_metrics(env: Env, address: Address) -> UserMetrics {
        env.storage()
            .persistent()
            .get::<DataKey, UserMetrics>(&DataKey::UserMetrics(address.clone()))
            .unwrap_or(UserMetrics {
                total_escrow_count: 0,
                total_volume: 0,
            })
    }

    /// Increment a user's activity metrics (called by the escrow contract).
    ///
    /// Accumulates escrow count and volume deltas into [`UserMetrics`], then
    /// optionally triggers auto-verification if thresholds are met. Volume is
    /// normalized to 7 decimal places before accumulation so that thresholds
    /// remain token-agnostic across different token decimals.
    ///
    /// ## Auth
    /// Requires the registered `escrow_contract` address. If none is set,
    /// falls back to `platform_admin`.
    ///
    /// # Parameters
    /// - `address`: `Address` — The user whose metrics to update (typically the seller).
    /// - `escrow_count_delta`: `u32` — Number of completed escrows to add.
    /// - `volume_delta`: `i128` — Raw token amount to add (will be normalized to 7 decimals).
    /// - `token_address`: `Address` — Token contract used to read `decimals()` for normalization.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be the registered `escrow_contract` (or `platform_admin` if unset).
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads auth address, TTL extended.
    /// - **Read/Write** [`DataKey::UserMetrics(address)`] — counters incremented, TTL extended.
    /// - **Read/Write** [`DataKey::UserProfile(address)`] — only if auto-verification fires.
    /// - **Read/Write** [`DataKey::VerificationHistory(address)`] — only if auto-verification fires.
    ///
    /// # Emitted Events
    /// - `("UserVerified",)` with data `address` — only if auto-verification threshold is crossed.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
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
        if config.auto_verify_enabled {
            Self::try_auto_verify(&env, &address, &config, &metrics);
        }
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
            if history.len() > 10 {
                history.remove(0);
            }
            env.storage().persistent().set(&hist_key, &history);
            Self::extend_persistent(env, &hist_key);

            env.events()
                .publish((Symbol::new(env, "UserVerified"),), address);
        }
    }

    /// Trigger an auto-verification check for a user.
    ///
    /// Anyone may call this — it is a no-op if thresholds are not yet met or
    /// if auto-verification is disabled. Useful for users who want to claim
    /// verification after accumulating enough activity off-chain.
    ///
    /// # Parameters
    /// - `address`: `Address` — The user to check.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - `address` must have an existing profile.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads thresholds, TTL extended.
    /// - **Read** [`DataKey::UserProfile(address)`] — TTL extended.
    /// - **Read** [`DataKey::UserMetrics(address)`] — no TTL extension.
    /// - **Write** [`DataKey::UserProfile(address)`] — only if verification fires.
    /// - **Write** [`DataKey::VerificationHistory(address)`] — only if verification fires.
    ///
    /// # Emitted Events
    /// - `("UserVerified",)` with data `address` — only if verification fires.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::UserNotFound`] if `address` has no profile.
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

        if !config.auto_verify_enabled {
            return false;
        }

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

        if config.auto_verify_enabled
            && metrics.total_escrow_count >= config.min_escrow_count_for_verify
            && metrics.total_volume >= config.min_volume_for_verify
        {
            Self::try_auto_verify(&env, &address, &config, &metrics);
            return true;
        }

        false
    }

    /// Submit a manual verification request.
    ///
    /// Adds the user's address to the FIFO verification queue for admin review.
    /// Calling this a second time before the request is processed is a no-op
    /// (idempotent). Appends a `"requested"` entry to the user's
    /// [`DataKey::VerificationHistory`].
    ///
    /// # Parameters
    /// - `user`: `Address` — The user requesting verification. Must authorize
    ///   this call (`user.require_auth()`).
    ///
    /// # Preconditions
    /// - `user` must have an existing profile ([`DataKey::UserProfile`] must exist).
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::VerificationRequest(user)`] — checks for pending request.
    /// - **Write** [`DataKey::VerificationQueueIndex(tail)`] — enqueues user address.
    /// - **Write** [`DataKey::VerificationRequest(user)`] — marks request as pending with timestamp.
    /// - **Write** [`DataKey::VerificationQueueTail`] — increments tail pointer.
    /// - **Read/Write** [`DataKey::VerificationHistory(user)`] — appends entry, TTL extended.
    /// - All written keys have TTL extended via `extend_ttl`.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with `"User not found"` if `user` has no profile.
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
        if history.len() > 10 {
            history.remove(0);
        }
        env.storage().persistent().set(&hist_key, &history);
        Self::extend_persistent(&env, &hist_key);
    }

    /// Approve or reject a pending verification request (admin only).
    ///
    /// Sets `is_verified` on the user's profile and removes the pending
    /// request from the queue. Appends an `"approved"` or `"rejected"` entry
    /// to the user's verification history.
    ///
    /// # Parameters
    /// - `user`: `Address` — Address of the user whose request is being processed.
    /// - `approve`: `bool` — `true` to verify the user, `false` to reject.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    /// - `user` must have an existing profile.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads admin address, TTL extended.
    /// - **Read/Write** [`DataKey::UserProfile(user)`] — `is_verified` updated, TTL extended.
    /// - **Remove** [`DataKey::VerificationRequest(user)`] — clears pending marker.
    /// - **Read/Write** [`DataKey::VerificationHistory(user)`] — appends entry, TTL extended.
    /// - Queue head pointer may advance (stale entries pruned).
    ///
    /// # Emitted Events
    /// - `("UserVerified",)` with data `user` — only if `approve` is `true`.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::UserNotFound`] if `user` has no profile.
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
        if history.len() > 10 {
            history.remove(0);
        }
        env.storage().persistent().set(&hist_key, &history);
        Self::extend_persistent(&env, &hist_key);

        if approve {
            env.events()
                .publish((Symbol::new(&env, "UserVerified"),), &user);
        }
    }

    /// Get the full verification history for a user.
    ///
    /// Returns a bounded [`Vec`] (max 10 entries) of [`VerificationEntry`]
    /// records in chronological order. Returns an empty vec for users with
    /// no history — never panics.
    ///
    /// # Parameters
    /// - `user`: `Address` — The user whose history to retrieve.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::VerificationHistory(user)`] — no TTL extension.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn get_verification_history(env: Env, user: Address) -> Vec<VerificationEntry> {
        let hist_key = DataKey::VerificationHistory(user.clone());
        env.storage()
            .persistent()
            .get(&hist_key)
            .unwrap_or(Vec::new(&env))
    }

    /// Get all addresses currently awaiting manual verification (admin helper).
    ///
    /// Advances the queue head past any stale entries (users whose pending
    /// request was cleared) before building the result. Returns only addresses
    /// that still have an active [`DataKey::VerificationRequest`] entry.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::VerificationQueueHead`] / [`DataKey::VerificationQueueTail`] — TTL extended.
    /// - **Read** [`DataKey::VerificationQueueIndex(i)`] for each slot — stale entries removed.
    /// - **Read** [`DataKey::VerificationRequest(user)`] for each candidate — TTL extended if active.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn get_verification_queue(env: Env) -> Vec<Address> {
        Self::advance_verification_head(&env);

        let head = Self::get_queue_pointer(&env, &DataKey::VerificationQueueHead);
        let tail = Self::get_queue_pointer(&env, &DataKey::VerificationQueueTail);
        let mut queue = Vec::new(&env);

        for index in head..tail {
            let queue_index_key = DataKey::VerificationQueueIndex(index);
            if let Some(user) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&queue_index_key)
            {
                if Self::is_verification_pending(&env, &user) {
                    queue.push_back(user);
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
    /// Called by the EscrowContract after a state change (release / refund /
    /// resolve). Increments `successful_trades` and/or `disputed_trades` on
    /// the user's profile using saturating addition to prevent overflow.
    /// Silently skips users who are not onboarded (no panic).
    ///
    /// ## Auth
    /// Requires the registered `escrow_contract` address. If none is set,
    /// falls back to `platform_admin`.
    ///
    /// # Parameters
    /// - `address`: `Address` — User whose counters to update.
    /// - `successful_delta`: `u32` — Amount to add to `successful_trades`.
    /// - `disputed_delta`: `u32` — Amount to add to `disputed_trades`.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be the registered `escrow_contract` (or `platform_admin` if unset).
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads auth address, TTL extended.
    /// - **Read/Write** [`DataKey::UserProfile(address)`] — counters updated, TTL extended.
    ///   No-op (returns early) if profile does not exist.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
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
    /// Returns `(0, 0)` for unknown addresses — never panics.
    ///
    /// # Parameters
    /// - `address`: `Address` — The user to query.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(address)`] — no TTL extension.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    ///
    /// # Returns
    /// Tuple `(successful_trades, disputed_trades)`.
    pub fn get_user_reputation(env: Env, address: Address) -> (u32, u32) {
        match env
            .storage()
            .persistent()
            .get::<DataKey, UserProfile>(&DataKey::UserProfile(address.clone()))
        {
            Some(profile) => {
                (profile.successful_trades, profile.disputed_trades)
            }
            None => (0, 0),
        }
    }

    // -----------------------------------------------------------------------
    // Issue #114 – Username Change Mechanism
    // -----------------------------------------------------------------------

    /// Change a user's username (Issue #114).
    ///
    /// Atomically removes the old username mapping and registers the new one.
    /// Resets `is_verified` to `false` (username change revokes verification
    /// status). Enforces a 30-day cooldown between changes to prevent
    /// username squatting and rapid identity rotation. Collects a fee if
    /// configured via [`set_username_change_fee`].
    ///
    /// ## Checks-Effects-Interactions
    /// Fee collection (token transfer) happens after all validation and
    /// before storage writes, following the CEI pattern.
    ///
    /// # Parameters
    /// - `user`: `Address` — The user changing their username. Must authorize
    ///   this call (`user.require_auth()`).
    /// - `new_username`: `String` — Desired new username (will be normalized).
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - `user` must have an existing profile.
    /// - Normalized `new_username` must be unique.
    /// - Normalized `new_username` length must be within configured bounds.
    /// - 30-day cooldown since last change must have elapsed
    ///   ([`USERNAME_CHANGE_COOLDOWN`]).
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads bounds, TTL extended.
    /// - **Read** [`DataKey::UserProfile(user)`] — reads current username, TTL extended.
    /// - **Read** [`DataKey::LastUsernameChange(user)`] — cooldown check.
    /// - **Read** [`DataKey::UsernameChangeFee`] — reads fee amount, TTL extended.
    /// - **Read** [`DataKey::UsernameChangeFeeToken`] — reads fee token, TTL extended.
    /// - **Remove** [`DataKey::Username(old_normalized)`] — releases old username.
    /// - **Write** [`DataKey::Username(new_normalized)`] — reserves new username, TTL extended.
    /// - **Write** [`DataKey::UserProfile(user)`] — new username + `is_verified = false`, TTL extended.
    /// - **Write** [`DataKey::LastUsernameChange(user)`] — records timestamp, TTL extended.
    /// - **Read/Write** [`DataKey::VerificationHistory(user)`] — appends `"username_changed_revoked"`.
    ///
    /// # Emitted Events
    /// - Topic: `("UsernameChanged",)` — Data: `user` address.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::UserNotFound`] if `user` has no profile.
    /// - Panics with `"Username already taken"` if new username is in use.
    /// - Panics with `"Username too short"` / `"Username too long"` on length violation.
    /// - Panics with `"Username change cooldown active"` if cooldown not elapsed.
    /// - Panics with [`Error::NotInitialized`] if fee token is not configured but fee > 0.
    ///
    /// # Example
    /// ```ignore
    /// // After 30+ days since last change:
    /// let profile = client.change_username(&user, &String::from_str(&env, "NewName"));
    /// assert_eq!(profile.username, String::from_str(&env, "newname"));
    /// assert!(!profile.is_verified); // verification revoked
    /// ```
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
        profile.is_verified = false;

        // Store updated profile
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        // Record timestamp of username change
        env.storage().persistent().set(
            &DataKey::LastUsernameChange(user.clone()),
            &env.ledger().timestamp(),
        );
        Self::extend_persistent(&env, &DataKey::LastUsernameChange(user.clone()));

        // Add history entry for revocation
        let hist_key = DataKey::VerificationHistory(user.clone());
        let mut history: Vec<VerificationEntry> = env
            .storage()
            .persistent()
            .get(&hist_key)
            .unwrap_or(Vec::new(&env));
        history.push_back(VerificationEntry {
            timestamp: env.ledger().timestamp(),
            action: String::from_str(&env, "username_changed_revoked"),
            by: Some(user.clone()),
        });
        if history.len() > 10 {
            history.remove(0);
        }
        env.storage().persistent().set(&hist_key, &history);
        Self::extend_persistent(&env, &hist_key);

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "UsernameChanged"),), &user);

        profile
    }

    /// Set the username change fee (admin only) — Issue #114.
    ///
    /// Sets the fee charged when a user calls [`change_username`]. A value of
    /// `0` disables the fee. The fee is collected in the token configured via
    /// [`set_username_fee_token`].
    ///
    /// # Parameters
    /// - `fee`: `i128` — Fee amount in the fee token's smallest unit (stroops
    ///   for XLM-based tokens). Must be ≥ 0.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    /// - `fee` must be ≥ 0.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads admin address, TTL extended.
    /// - **Write** [`DataKey::UsernameChangeFee`] — stores fee, TTL extended.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    /// - Panics with [`Error::InvalidFee`] if `fee < 0`.
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
    ///
    /// Must be called before [`set_username_change_fee`] sets a non-zero fee,
    /// otherwise [`change_username`] will panic when trying to collect.
    ///
    /// # Parameters
    /// - `token`: `Address` — The token contract address for fee collection.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads admin address, TTL extended.
    /// - **Write** [`DataKey::UsernameChangeFeeToken`] — stores token address, TTL extended.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
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
    ///
    /// Defaults to `platform_admin` if not explicitly set. Fees are
    /// transferred to this address during [`change_username`].
    ///
    /// # Parameters
    /// - `wallet`: `Address` — The destination address for collected fees.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads admin address, TTL extended.
    /// - **Write** [`DataKey::UsernameChangeFeeWallet`] — stores wallet address, TTL extended.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
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

    /// Get the current username change fee — Issue #114.
    ///
    /// Returns `0` if no fee has been configured.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UsernameChangeFee`] — no TTL extension.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn get_username_change_fee(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::UsernameChangeFee)
            .unwrap_or(0)
    }

    /// Get the configured token used for username change fees.
    ///
    /// Returns `None` if no fee token has been set via [`set_username_fee_token`].
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UsernameChangeFeeToken`] — TTL extended if key exists.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None.
    pub fn get_username_fee_token(env: Env) -> Option<Address> {
        Self::read_username_fee_token(&env)
    }

    /// Get the configured wallet used for username change fees.
    ///
    /// Falls back to `platform_admin` if no wallet has been explicitly set
    /// via [`set_username_fee_wallet`].
    ///
    /// # Preconditions
    /// - Contract must be initialized (needed for the `platform_admin` fallback).
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::Config`] — reads `platform_admin` for fallback.
    /// - **Read** [`DataKey::UsernameChangeFeeWallet`] — TTL extended if key exists.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// - Panics with [`Error::NotInitialized`] if config is missing.
    pub fn get_username_fee_wallet(env: Env) -> Address {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::read_username_fee_wallet(&env, &config)
    }

    // -----------------------------------------------------------------------
    // Issue #112 – Artisan Portfolio Verification
    // -----------------------------------------------------------------------

    /// Update an artisan's portfolio CID (Issue #112).
    ///
    /// Allows artisans to update their IPFS portfolio showcase. The CID is
    /// validated using the same rules as escrow metadata — supports CIDv0
    /// (46-char Base58btc starting with `"Qm"`) and CIDv1 with `b`, `f`, or
    /// `z` multibase prefixes. Pass `None` to remove the portfolio link.
    ///
    /// # Parameters
    /// - `user`: `Address` — The artisan updating their portfolio. Must
    ///   authorize this call (`user.require_auth()`).
    /// - `portfolio_cid`: `Option<String>` — IPFS CID of the portfolio, or
    ///   `None` to clear it.
    ///
    /// # Preconditions
    /// - `user` must have an existing profile.
    /// - `user` must have role [`UserRole::Artisan`].
    /// - If `portfolio_cid` is `Some`, it must pass [`validate_ipfs_cid`].
    ///
    /// # Storage Side-Effects
    /// - **Read/Write** [`DataKey::UserProfile(user)`] — `portfolio_cid` updated, TTL extended.
    ///
    /// # Emitted Events
    /// - Topic: `("PortfolioUpdated",)` — Data: `user` address.
    ///
    /// # Errors
    /// - Panics with [`Error::UserNotFound`] if `user` has no profile.
    /// - Panics with `"Only artisans can update portfolio"` if role is not `Artisan`.
    /// - Panics with `"Invalid portfolio CID format"` if CID validation fails.
    ///
    /// # Example
    /// ```ignore
    /// // Set a CIDv0 portfolio link:
    /// client.update_portfolio(
    ///     &artisan,
    ///     &Some(String::from_str(&env, "QmYwAPJzv5CZsnAzt8auVTL3u2M6YvM7NfF4hB9m8C3vM9")),
    /// );
    /// // Clear the portfolio:
    /// client.update_portfolio(&artisan, &None);
    /// ```
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
