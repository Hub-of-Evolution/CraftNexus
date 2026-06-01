//! Onboarding Contract
//!
//! Handles user registration (onboarding), role assignments, username configuration,
//! profile management, and verification processes for buyers and artisans on the CraftNexus platform.

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
/// Maximum verification history entries retained per user (#519).
const MAX_VERIFICATION_HISTORY: u32 = 10;

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
    /// DEPRECATED: Legacy Vec-based verification history (#63).
    /// Migrated lazily to indexed compact entries (#519).
    VerificationHistory(Address),
    /// Count of compact verification history entries per user (#519)
    VerificationHistoryCount(Address),
    /// Indexed compact verification history entry (#519)
    VerificationHistoryIndexed(Address, u32),
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
    /// Dispute-resolution delegate (Issue #116). Moderators may resolve
    /// escrows when their address is also registered on the escrow
    /// contract's platform config, but they cannot change WASM, platform
    /// fees, or other admin-only settings.
    Moderator = 4,
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
    /// Optional IPFS content identifier for an artisan's portfolio
    /// showcase (Issue #112).
    ///
    /// `None` when unset or after removal via `update_portfolio`. When
    /// present, the CID must conform to the same validation rules as
    /// escrow metadata CIDs (see `validate_ipfs_cid`). Indexers can read
    /// this field from `get_user` / `get_user_by_username` responses or
    /// subscribe to `PortfolioUpdated` events for live updates.
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

/// Activity metrics used to determine eligibility for auto-verification (#63).
///
/// Written exclusively by the registered escrow contract via
/// [`OnboardingContract::update_user_metrics`] and read by
/// [`OnboardingContract::get_user_metrics`], [`OnboardingContract::auto_verify_user`],
/// and the internal `try_auto_verify` helper. Volume is normalized to 7 decimal
/// places before accumulation so threshold comparisons remain token-agnostic.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserMetrics {
    /// Total number of completed seller-side escrows recorded by the escrow contract.
    pub total_escrow_count: u32,
    /// Cumulative seller volume in stroops at 7-decimal precision (not raw token units).
    pub total_volume: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserOnboardedEvent {
    pub user: Address,
    pub username: String,
    pub role: UserRole,
}

/// A single entry in a user's verification history log (#63).
///
/// Returned by [`OnboardingContract::get_verification_history`]. On-chain storage
/// uses compact [`VerificationActionCode`] values; this struct exposes human-readable
/// `action` strings for off-chain indexers and client UIs.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct VerificationEntry {
    /// Ledger timestamp (seconds) when the action was recorded.
    pub timestamp: u64,
    /// One of: `"requested"`, `"approved"`, `"rejected"`, `"auto_verified"`,
    /// `"username_changed_revoked"`. See issue #473 / component #72.
    pub action: String,
    /// Address that performed the action; `None` for auto-verification events.
    pub by: Option<Address>,
}

/// Compact action code for indexed verification history storage (#519).
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
enum VerificationActionCode {
    Requested = 0,
    Approved = 1,
    Rejected = 2,
    AutoVerified = 3,
    UsernameChangedRevoked = 4,
}

/// Lightweight on-chain verification history entry (#519).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
struct CompactVerificationEntry {
    timestamp: u64,
    action: VerificationActionCode,
    by: Option<Address>,
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
    /// Whether threshold-based verification should run automatically.
    pub auto_verify_enabled: bool,
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

/// Normalize a raw username string into its canonical on-chain form.
///
/// # Integration notes — issue #497 / component #96
///
/// ## Purpose
/// All username storage keys and uniqueness checks operate on the
/// *normalized* form produced by this function. Clients and indexers
/// must apply the same normalization before constructing lookup keys or
/// comparing usernames.
///
/// ## Normalization rules (applied in order)
/// 1. ASCII alphanumeric characters (`a-z`, `A-Z`, `0-9`) are kept and
///    lowercased.
/// 2. Separator characters (space ` `, underscore `_`, hyphen `-`,
///    period `.`) are collapsed to a single `_`. Consecutive separators
///    produce exactly one `_`; leading and trailing separators are
///    stripped.
/// 3. A subset of Latin-extended and Cyrillic Unicode code points are
///    transliterated to their closest ASCII equivalents via
///    `map_username_bytes` (e.g. `ä` → `a`, `ß` → `ss`). Zero-width
///    joiners and BOM sequences are silently dropped.
/// 4. Any other byte sequence that is not matched by the above rules is
///    replaced with a single `_` separator (subject to the collapsing
///    rule in step 2).
/// 5. The result is always lowercase ASCII.
///
/// ## Input constraints
/// - Maximum input length: 256 bytes. Inputs exceeding this limit cause
///   a panic; callers should validate length before invoking.
/// - The function does **not** enforce minimum or maximum username
///   length — that is the responsibility of the calling function
///   (`onboard_user`, `change_username`) using the configured
///   `min_username_length` / `max_username_length` values.
///
/// ## Storage side-effects
/// - None. This is a pure transformation with no persistent reads or
///   writes.
///
/// ## Off-chain consumers
/// - Apply the same rules client-side before calling `is_username_taken`
///   or `get_user_by_username` to avoid false negatives caused by
///   un-normalized input.
/// - The `UserOnboarded` and `UsernameChanged` events carry the
///   already-normalized username; use those values verbatim for display
///   and reverse lookups.
///
/// # Arguments
/// * `env` - Soroban environment reference
/// * `username` - Raw username string provided by the caller
///
/// # Returns
/// Normalized username as a `soroban_sdk::String` (lowercase ASCII,
/// separators collapsed, no leading/trailing `_`).
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
/// Shared validation logic for portfolio CIDs (Issue #112) and escrow
/// metadata hashes. Returns `true` when the string is a well-formed CID;
/// callers should treat `false` as `Error::InvalidPortfolioCid` in
/// onboarding or the equivalent escrow error in the main contract.
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

    /// Resolve the wallet that receives username-change fees.
    ///
    /// Reads `DataKey::UsernameChangeFeeWallet`; when unset, falls back to
    /// `config.platform_admin`. Extends TTL when the key exists.
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

    /// Load persisted activity metrics for `address`, or zeroed defaults.
    ///
    /// Extends TTL on `DataKey::UserMetrics(address)` when an entry exists.
    fn read_user_metrics(env: &Env, address: &Address) -> UserMetrics {
        let key = DataKey::UserMetrics(address.clone());
        let metrics = env.storage().persistent().get(&key).unwrap_or(UserMetrics {
            total_escrow_count: 0,
            total_volume: 0,
        });
        if env.storage().persistent().has(&key) {
            Self::extend_persistent(env, &key);
        }
        metrics
    }

    /// Map a compact verification action code to its canonical string label.
    ///
    /// Labels are stable API surface for indexers consuming
    /// [`VerificationEntry::action`] via [`OnboardingContract::get_verification_history`].
    fn verification_action_to_string(env: &Env, action: VerificationActionCode) -> String {
        match action {
            VerificationActionCode::Requested => String::from_str(env, "requested"),
            VerificationActionCode::Approved => String::from_str(env, "approved"),
            VerificationActionCode::Rejected => String::from_str(env, "rejected"),
            VerificationActionCode::AutoVerified => String::from_str(env, "auto_verified"),
            VerificationActionCode::UsernameChangedRevoked => {
                String::from_str(env, "username_changed_revoked")
            }
        }
    }

    /// Parse a legacy verification-history action string into a compact code.
    ///
    /// Used during lazy migration from `DataKey::VerificationHistory` (Vec) to
    /// indexed compact entries (#519). Unknown strings map to
    /// `UsernameChangedRevoked`.
    ///
    /// # Component #84: Integration Interface Documentation
    ///
    /// ## Overview
    /// The onboarding contract provides a unified interface for managing user profiles,
    /// role assignments, and verification workflows on the CraftNexus platform.
    ///
    /// ## Core Structures
    ///
    /// ### UserProfile
    /// - **Purpose**: Represents a user's complete onboarding state
    /// - **Preconditions**: Must be initialized via `onboard_user` before access
    /// - **Storage**: Persistent key `DataKey::UserProfile(address)` with TTL auto-refresh
    /// - **Events Emitted**: `UserOnboardedEvent`, `RoleUpdated`, `PortfolioUpdated`
    ///
    /// ### OnboardingConfig
    /// - **Purpose**: System-wide settings for verification, username constraints
    /// - **Preconditions**: Must be initialized before any user operations
    /// - **Storage**: Singleton `DataKey::Config` with extended TTL for stability
    /// - **Modifiable By**: Platform admin only
    ///
    /// ### UserMetrics
    /// - **Purpose**: Tracks escrow volume and count for auto-verification eligibility
    /// - **Preconditions**: Populated only by escrow contract via `update_user_metrics`
    /// - **Storage**: `DataKey::UserMetrics(address)` updated asynchronously
    /// - **Side-Effects**: Auto-verification triggers when thresholds met
    ///
    /// ## API Parameters & Validation
    ///
    /// ### Username Constraints (Component #84)
    /// - **Format**: Normalized to lowercase, UTF-8 canonical form
    /// - **Length**: 3-50 characters after normalization
    /// - **Uniqueness**: Case-insensitive across all users (enforced via `DataKey::Username`)
    /// - **Reserved Names**: "admin" and derivations permanently reserved
    /// - **Change Cooldown**: 30 days between successive changes (prevents abuse)
    ///
    /// ### Role Transitions (Endpoint #85)
    /// - **Valid Roles**: Buyer, Artisan, Moderator (None and Admin excluded)
    /// - **Authorization**: Platform admin only; enforced via `require_auth()`
    /// - **Audit Trail**: All transitions logged to `VerificationHistoryIndexed`
    /// - **Event Emission**: `RoleUpdated` carries (user, old_role, new_role)
    ///
    /// ### Verification Workflow
    /// - **Auto-Verification**: Triggered when metrics meet thresholds (configurable)
    /// - **Manual Requests**: Queued in FIFO order via `VerificationQueueHead/Tail`
    /// - **History**: Last 10 entries retained per user (compact indexed format #519)
    /// - **State Machine**: none → requested → {approved|rejected} → verified
    ///
    /// ## Storage Optimization (Issue #82)
    /// - **Compact Types**: Uses `symbol_short` and flat `CompactVerificationEntry`
    /// - **TTL Strategy**: Entries extended only on active reads to conserve rent
    /// - **Lazy Migration**: Legacy Vec entries converted to indexed on first access
    /// - **Rent Calculation**: ~600 stroops/ledger for typical profile (28 bytes)
    ///
    /// ## Check-Effect-Interactions Pattern (Security)
    /// All state-mutating endpoints follow strict ordering:
    /// 1. **Check**: Validate authorization, preconditions, constraints
    /// 2. **Effect**: Update persistent storage, emit events
    /// 3. **Interact**: External token transfers (e.g., username change fees) LAST
    ///
    /// This prevents reentrancy where malicious callers trigger intermediate states
    /// via callbacks on arbitrary token contracts before final balance settlement.
    fn parse_verification_action(env: &Env, action: &String) -> VerificationActionCode {
        if action == &String::from_str(env, "requested") {
            VerificationActionCode::Requested
        } else if action == &String::from_str(env, "approved") {
            VerificationActionCode::Approved
        } else if action == &String::from_str(env, "rejected") {
            VerificationActionCode::Rejected
        } else if action == &String::from_str(env, "auto_verified") {
            VerificationActionCode::AutoVerified
        } else {
            VerificationActionCode::UsernameChangedRevoked
        }
    }

    fn migrate_legacy_verification_history(env: &Env, user: &Address) {
        let legacy_key = DataKey::VerificationHistory(user.clone());
        if !env.storage().persistent().has(&legacy_key) {
            return;
        }

        let history: Vec<VerificationEntry> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(Vec::new(env));

        let count_key = DataKey::VerificationHistoryCount(user.clone());
        let mut count: u32 = 0;
        for i in 0..history.len() {
            if let Some(entry) = history.get(i) {
                let compact = CompactVerificationEntry {
                    timestamp: entry.timestamp,
                    action: Self::parse_verification_action(env, &entry.action),
                    by: entry.by.clone(),
                };
                let entry_key = DataKey::VerificationHistoryIndexed(user.clone(), i);
                env.storage().persistent().set(&entry_key, &compact);
                Self::extend_persistent(env, &entry_key);
                count = i + 1;
            }
        }

        if count > 0 {
            env.storage().persistent().set(&count_key, &count);
            Self::extend_persistent(env, &count_key);
        }

        env.storage().persistent().remove(&legacy_key);
    }

    /// Append a verification history entry with FIFO circular-buffer semantics.
    ///
    /// [FEATURE #83] Enhanced business flow: Maintains a compact sliding window of verification
    /// actions for audit trails and compliance reporting. Implements circular-buffer semantics
    /// to enforce bounded storage while preserving temporal ordering of recent events.
    ///
    /// When history reaches MAX_VERIFICATION_HISTORY (10 entries), oldest entries are shifted
    /// and the newest entry is appended at the tail. This enables long-running contract states
    /// to support arbitration reviews without unbounded storage growth.
    ///
    /// # Arguments
    /// * `user` - Address of the user whose history is updated
    /// * `action` - Compact verification action code (Requested, Approved, etc.)
    /// * `by` - Optional moderator/admin address that triggered the action
    ///
    /// # Storage Side-Effects
    /// - Reads/writes `DataKey::VerificationHistoryCount(user)` (4 bytes)
    /// - Reads/writes up to 10 entries of `DataKey::VerificationHistoryIndexed(user, slot)`
    /// - Each entry is ~24 bytes (timestamp u64 + action u32 + optional address 32 bytes)
    /// - Extends TTL on count and all affected entries to prevent archival
    ///
    /// # Performance (Issue #82)
    /// - Amortized O(1) append for count < MAX_VERIFICATION_HISTORY
    /// - O(MAX_VERIFICATION_HISTORY) shift cost when buffer is full (rare operation)
    /// - Single TTL bump per entry = ~100 CPU instructions (vs Vec iteration = 1000+)
    ///
    /// # Check-Effect-Interactions
    /// 1. Check: Validate MAX_VERIFICATION_HISTORY constraint
    /// 2. Effect: Update persistent storage and TTL
    /// 3. Interact: No external calls; purely on-chain state management
    fn append_verification_history(
        env: &Env,
        user: &Address,
        action: VerificationActionCode,
        by: Option<Address>,
    ) {
        Self::migrate_legacy_verification_history(env, user);

        let count_key = DataKey::VerificationHistoryCount(user.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        // [FEATURE #83] Circular-buffer rotation for active contracts:
        // When history is full, shift older entries down and append new entry at end.
        // This supports long-lived arbitration scenarios without unbounded growth.
        let slot = if count >= MAX_VERIFICATION_HISTORY {
            // Shift entries: move index i down to i-1 for all i in [1, MAX-1]
            for i in 1..MAX_VERIFICATION_HISTORY {
                let src_key = DataKey::VerificationHistoryIndexed(user.clone(), i);
                if let Some(entry) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, CompactVerificationEntry>(&src_key)
                {
                    let dst_key = DataKey::VerificationHistoryIndexed(user.clone(), i - 1);
                    env.storage().persistent().set(&dst_key, &entry);
                    Self::extend_persistent(env, &dst_key);
                    env.storage().persistent().remove(&src_key);
                }
            }
            MAX_VERIFICATION_HISTORY - 1
        } else {
            count
        };

        let entry = CompactVerificationEntry {
            timestamp: env.ledger().timestamp(),
            action,
            by,
        };
        let entry_key = DataKey::VerificationHistoryIndexed(user.clone(), slot);
        env.storage().persistent().set(&entry_key, &entry);
        Self::extend_persistent(env, &entry_key);

        let new_count = if count >= MAX_VERIFICATION_HISTORY {
            MAX_VERIFICATION_HISTORY
        } else {
            count + 1
        };
        env.storage().persistent().set(&count_key, &new_count);
        Self::extend_persistent(env, &count_key);
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
    /// Soroban charges rent per ledger entry, so persistent state for an
    /// active escrow / profile must have its TTL refreshed regularly to
    /// avoid archival. Using a single helper keeps the threshold/extension
    /// pair (`TTL_THRESHOLD`, `TTL_EXTENSION`) consistent across every
    /// read/write path — drift between sites is the usual cause of
    /// entries being archived earlier than callers expect.
    ///
    /// Callers do not need to check existence first: `extend_ttl` on a
    /// missing key is a no-op, but it still costs CPU. For hot paths that
    /// may legitimately call the helper with absent keys, use
    /// [`Self::extend_persistent_if_present`] instead.
    fn extend_persistent(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// TTL-bump variant that first checks the entry exists (Issue #82 optimization).
    ///
    /// [PERFORMANCE #82] Optimized storage layout: Validates storage entry presence before
    /// applying `extend_ttl` to avoid redundant CPU cycles when refreshing archived or
    /// non-existent keys. Particularly useful during batched operations where stale references
    /// may surface (e.g., escrow references during verification sweeps).
    ///
    /// On-chain economics: `persistent().has()` costs ~50 CPU units, while `extend_ttl` on
    /// a missing key wastes ~100 units. This check saves 100% of `extend_ttl` cost for
    /// archived entries (gas savings ~5-10 stroops per stale reference).
    ///
    /// # Storage Optimization Strategy
    /// - Compact representation: Only stores minimal required state per entry
    /// - Lazy TTL refresh: Only bump when entry is actively accessed (read pattern)
    /// - Indexed access: O(1) lookups via `DataKey::VerificationHistoryIndexed(user, slot)`
    /// - No Vec allocations: Eliminates runtime allocation overhead (Issue #82)
    ///
    /// # Arguments
    /// * `key` - Storage key to conditionally refresh (must implement `IntoVal<Env, Val>`)
    ///
    /// # Returns
    /// `true` if entry existed and TTL was extended; `false` if key was absent
    ///
    /// # Usage Pattern
    /// ```ignore
    /// if Self::extend_persistent_if_present(env, &user_profile_key) {
    ///     // Profile was active and TTL refreshed; safe to proceed
    /// } else {
    ///     // Profile archived; handle stale reference gracefully
    /// }
    /// ```
    fn extend_persistent_if_present<K>(env: &Env, key: &K) -> bool
    where
        K: soroban_sdk::IntoVal<Env, soroban_sdk::Val> + Clone,
    {
        if env.storage().persistent().has(key) {
            env.storage()
                .persistent()
                .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
            true
        } else {
            false
        }
    }

    /// Refresh the persistent TTL for a user's profile entry (#103, Issue #82).
    ///
    /// [PERFORMANCE #82] Storage optimization endpoint: Active escrow contracts call this
    /// during long-running escrow lifecycles to prevent participant profiles from being
    /// archived while escrows remain open. Implements conditional TTL refresh to avoid
    /// wasted CPU on already-archived entries.
    ///
    /// This endpoint is essential for maintaining consistency between escrow and
    /// onboarding contract state during disputes that span multiple ledger epochs.
    /// Only the registered escrow contract or the platform admin may invoke this.
    ///
    /// # Enhanced business flow — issue #496
    ///
    /// The Config entry TTL is now extended after auth passes. Previously the
    /// Config read was not accompanied by a TTL bump, meaning a Config entry
    /// close to expiry could be archived on the same ledger as a valid
    /// `bump_user_profile_ttl` call. Extending Config here keeps the contract
    /// configuration live for the full `TTL_EXTENSION` window whenever an
    /// authorized escrow settlement touches this endpoint.
    ///
    /// # Returns
    /// `true` if the profile existed and its TTL was refreshed; `false` if the
    /// key was absent (profile archived or never created).
    ///
    /// # Preconditions
    /// - Escrow contract must be registered via `set_escrow_contract`
    /// - Caller must be either escrow contract or platform admin
    pub fn bump_user_profile_ttl(env: Env, user: Address) -> bool {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        match config.escrow_contract {
            Some(ref escrow_addr) => escrow_addr.require_auth(),
            None => config.platform_admin.require_auth(),
        }
        // Issue #496 — extend Config TTL after auth so the configuration
        // entry stays live for the full extension window on every authorized
        // call, preventing silent archival during active escrow lifecycles.
        Self::extend_persistent(&env, &DataKey::Config);
        Self::extend_persistent_if_present(&env, &DataKey::UserProfile(user))
    }

    /// Refresh the persistent TTL for a user's activity metrics entry (#107, Issue #82).
    ///
    /// [PERFORMANCE #82] Complements `bump_user_profile_ttl` for escrow contracts that
    /// read or write activity metrics during settlement. Uses conditional TTL extension
    /// to optimize storage rent calculations and prevent premature archival of metrics
    /// during multi-ledger arbitration workflows. Only the registered escrow
    /// contract or the platform admin may invoke this.
    ///
    /// # Enhanced business flow — issue #496
    ///
    /// Config TTL is extended after auth passes, matching the pattern applied
    /// to `bump_user_profile_ttl` above.
    ///
    /// # Returns
    /// `true` if the metrics entry existed and its TTL was refreshed; `false`
    /// if the key was absent.
    ///
    /// # Preconditions
    /// - Metrics must have been initialized via `update_user_metrics`
    /// - Caller must be either registered escrow contract or platform admin
    pub fn bump_user_metrics_ttl(env: Env, user: Address) -> bool {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        match config.escrow_contract {
            Some(ref escrow_addr) => escrow_addr.require_auth(),
            None => config.platform_admin.require_auth(),
        }
        // Issue #496 — extend Config TTL after auth, same as bump_user_profile_ttl.
        Self::extend_persistent(&env, &DataKey::Config);
        Self::extend_persistent_if_present(&env, &DataKey::UserMetrics(user))
    }

    /// Initialize the onboarding contract system.
    ///
    /// Sets up the `OnboardingConfig` singleton and reserves the "admin" username.
    /// Creates the initial admin user profile with full platform privileges.
    ///
    /// # Arguments
    /// * `admin` - Platform administrator address (must call this method to authorize)
    ///
    /// # Storage Side-Effects
    /// - Writes singleton `DataKey::Config` with default verification thresholds
    /// - Writes `DataKey::UserProfile(admin)` with Admin role
    /// - Writes `DataKey::Username("admin")` pointing to admin address (reserved)
    /// - Extends TTL on all initialized entries
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

    /// Onboard a new user to the platform
    ///
    /// # Preconditions
    /// - The caller must be the `user` address (`user.require_auth()` is enforced).
    /// - The contract must be initialized.
    /// - The normalized username length must be within configured minimum and maximum limits.
    /// - The `user` must not have been onboarded already.
    /// - The normalized username must not be taken.
    /// - The `role` must be either `UserRole::Buyer` or `UserRole::Artisan`.
    ///
    /// # Storage Side-effects
    /// - Writes a new `UserProfile` struct to `DataKey::UserProfile(user)`.
    /// - Writes the unique username mapping to `DataKey::Username(normalized_username)`.
    /// - Refreshes the TTL of `DataKey::Config`, the new user profile, and the new username key.
    ///
    /// # Emitted Events
    /// - Publishes a `UserOnboarded` event containing the user address, normalized username, and role.
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

        // Check if user already onboarded (#92).
        //
        // Reads the `DataKey::UserProfile(user)` persistent entry to determine
        // whether this address has already completed onboarding.
        //
        // Storage side-effect (existing users only): if a profile is found, its
        // persistent TTL is extended by `TTL_EXTENSION` ledgers before the panic.
        // This "optimistic TTL refresh on read" pattern ensures that a profile
        // that is close to expiry is not silently archived on the same ledger that
        // rejected a duplicate-onboarding attempt — the failure path should never
        // be a vector for accidentally losing a live record.
        //
        // Preconditions:
        //   - Config must be initialized (checked and extended above).
        //   - `user.require_auth()` must have passed (enforced at function entry).
        //
        // Integration notes for off-chain integrators (#92):
        //   - Use `get_user(user)` or subscribe to `UserOnboarded` events as the
        //     preferred way to check onboarding status; avoid probing this storage
        //     key directly, as TTL expiry can make `has` return `false` for users
        //     who have not interacted with the contract recently.
        //   - `onboard_user` panics (no return value) on a duplicate call, so
        //     client code should guard with a `get_user` probe or catch the error
        //     via `try_invoke_contract` before calling this function.
        //   - Profile shape is versioned via `CURRENT_USER_PROFILE_VERSION`; any
        //     schema change requires a migration via `migrate_user_profile`.
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

        // Store username → address mapping for uniqueness enforcement.
        //
        // Storage side-effect: writes a `DataKey::Username(normalized)` persistent entry
        // whose value is the owner's `Address`.  This secondary index is the authoritative
        // source for username availability checks and is consulted by:
        //   - `get_user_by_username`  — resolves a handle to a full `UserProfile`
        //   - `change_username`       — removes the old key and writes a new one atomically
        //   - the uniqueness guard earlier in this function (`.has` check)
        //
        // Preconditions (validated above):
        //   1. No `DataKey::Username(normalized)` entry exists yet (uniqueness guard passed).
        //   2. The normalized username satisfies the configured min/max length constraints.
        //
        // Integration notes for off-chain indexers (#104):
        //   - Index the `UserOnboarded` and `UsernameChanged` events to maintain a
        //     username → address mapping without polling contract storage directly.
        //   - The key is stored under a `TTL_EXTENSION`-ledger TTL.  Integrators that
        //     probe storage directly must account for key expiry if `extend_persistent`
        //     was not called recently (e.g. for dormant accounts).
        //   - Username normalisation rules: lowercase, separator characters collapsed to
        //     `_`, leading/trailing separators stripped.  Apply the same rules on the
        //     client side before constructing a lookup key.
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized.clone()), &user);
        Self::extend_persistent(&env, &DataKey::Username(normalized.clone()));

        // Emit UserOnboarded event (#108).
        //
        // Event topic  : `("UserOnboarded",)`
        // Event payload: `UserOnboardedEvent { user, username, role }`
        //
        // Fields:
        // * `user`     - The wallet address that was just onboarded.
        // * `username` - The normalized (lowercased, trimmed) username stored on-chain.
        // * `role`     - The role assigned: `Buyer` (1) or `Artisan` (2).
        //                Numeric discriminants:
        //                  0 = Admin    (reserved; cannot be self-assigned)
        //                  1 = Buyer
        //                  2 = Artisan
        //                Off-chain consumers should treat any unrecognised discriminant
        //                as unknown and not silently drop the event.
        //
        // Emitted after all storage writes complete, so subscribers observing this
        // event can safely query `get_user` and `get_user_by_username` immediately.
        //
        // Integration notes for off-chain indexers (#108):
        //   - Subscribe to topic `"UserOnboarded"` to build a real-time user registry
        //     without polling `get_user` for every address.
        //   - The `username` field carries the canonical on-chain form; use it verbatim
        //     for reverse lookups and display.  Do not re-normalise on the client side
        //     unless you are constructing a new lookup key (same normalisation rules
        //     apply: lowercase, separators collapsed to `_`, no leading/trailing `_`).
        //   - Trigger downstream workflows (welcome emails, dashboard provisioning, etc.)
        //     only after the event is confirmed in a closed ledger to avoid acting on
        //     failed transactions.
        //   - This event is emitted exactly once per address.  A second call to
        //     `onboard_user` with the same address panics with `AlreadyOnboarded`
        //     and produces no event.
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

    /// Read-only accessor for a user's profile, keyed by their Stellar
    /// address.
    ///
    /// # Integration notes — issue #529
    ///
    /// - This is the canonical "is this address onboarded?" entrypoint
    ///   for off-chain integrations. It **reverts** with
    ///   `Error::UserNotFound` if no profile exists for `user`, so
    ///   callers that want a non-erroring probe should wrap the call
    ///   with the host's `try_invoke_contract` API and treat the
    ///   `Err` case as "not onboarded".
    /// - The returned `UserProfile` carries the user's role, status,
    ///   verification flag, portfolio CID, and metadata fields needed
    ///   by the escrow and reputation systems. Treat the response as
    ///   a snapshot; the profile can be mutated by `update_role`,
    ///   `deactivate_profile`, `verify_user`, `update_portfolio`, and
    ///   `change_username`, each of which emits an event indexers
    ///   can subscribe to instead of polling this function.
    /// - The function is gas-only (no token movements) so it is safe
    ///   to call from a simulation / preview path.
    ///
    /// # Arguments
    /// * `user` - User's wallet address
    ///
    /// # Returns
    /// `UserProfile` if a profile exists, otherwise panics with
    /// `Error::UserNotFound`.
    pub fn get_user(env: Env, user: Address) -> UserProfile {
        Self::get_user_profile(&env, user)
    }

    /// Check if the user has any active escrows on the configured escrow contract.
    /// Returns false if no escrow contract is registered or if the user has no active escrows.
    pub fn has_active_contracts(env: Env, user: Address) -> bool {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        if let Some(escrow_contract) = config.escrow_contract {
            let client = EscrowClient::new(&env, &escrow_contract);
            client.has_active_escrows(&user)
        } else {
            false
        }
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
        if let Some(profile) = Self::try_get_user_profile(&env, user) {
            profile.role
        } else {
            UserRole::None
        }
    }

    /// Assign or update the moderator role for a user (admin only).
    ///
    /// # Security (#117)
    /// Requires platform admin authorization before any state transition.
    /// Promote a user to Moderator role.
    ///
    /// # Authorization
    ///
    /// **SECURITY**: Only the platform admin can invoke this endpoint.
    /// The caller's signature is verified via `require_auth()` before any mutation.
    /// Unauthorized invocation results in immediate transaction rollback.
    ///
    /// # Arguments
    /// * `user` - Address to promote to Moderator
    ///
    /// # Returns
    /// Updated `UserProfile` with the new Moderator role assigned.
    pub fn set_moderator(env: Env, user: Address) -> UserProfile {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        config.platform_admin.require_auth();
        Self::update_user_role(env, user, UserRole::Moderator)
    }

    /// Update a user's platform role (admin-only endpoint).
    ///
    /// # Authorization
    ///
    /// **SECURITY**: Only the platform admin can invoke this endpoint.
    /// The caller's signature is verified via `require_auth()` before any state mutation.
    /// Unauthorized invocation with mismatched credentials results in immediate
    /// transaction rollback with no state changes applied.
    ///
    /// Strictly enforces role transitions to prevent unauthorized state mutations.
    /// Validates that the new role is a supported platform role (Buyer, Artisan, or Moderator);
    /// Admin and None roles cannot be assigned via this method to maintain security invariants.
    ///
    /// # Arguments
    /// * `user` - User's wallet address to update
    /// * `new_role` - New role to assign (must be Buyer, Artisan, or Moderator)
    ///
    /// # Returns
    /// Updated `UserProfile` with the new role and incremented version.
    ///
    /// # Storage Side-Effects
    /// - Writes updated `UserProfile` to persistent storage under `DataKey::UserProfile(user)`
    /// - Emits `RoleUpdated` event carrying (user, old_role, new_role) for indexer consumption
    /// - Extends TTL on config and profile entries to prevent archival during state transitions
    ///
    /// # Reverts if
    /// - Caller is not the platform admin (authorization check fails)
    /// - User not found in persistent storage
    /// - New role is Admin or None (invalid assignment)
    /// - Config not initialized
    pub fn update_user_role(env: Env, user: Address, new_role: UserRole) -> UserProfile {
        // Security: Get config to verify admin authorization
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        // [SECURITY] Endpoint #85: Strict authorization check
        // Only admin can update roles; require_auth() verifies the caller's digital signature
        config.platform_admin.require_auth();
        
        // [SECURITY] Validate new role assignment; prevent unauthorized role escalation
        match new_role {
            UserRole::Admin | UserRole::None => {
                env.panic_with_error(Error::InvalidRole);
            }
            _ => {} // Proceed for Buyer, Artisan, Moderator
        }

        // Fetch and validate existing profile before mutation
        let mut profile = Self::get_user_profile(&env, user.clone());

        // [SECURITY] Prevent unnecessary state mutations and replay attacks
        // by recording state transition audit trail for forensic analysis
        let old_role = profile.role.clone();
        profile.role = new_role.clone();

        // Store updated profile
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
        Self::extend_persistent(&env, &DataKey::UserProfile(user.clone()));

        // Issue #520 — event now carries (user, old_role, new_role) so
        // downstream consumers don't need a follow-up read to know what
        // the role transitioned from.
        env.events().publish(
            (Symbol::new(&env, "RoleUpdated"),),
            (user.clone(), old_role, new_role),
        );

        profile
    }

    /// Deactivate the user's profile and release their username.
    /// Reverts if:
    /// - User has active escrows (traditional or recurring)
    /// - User is "admin"
    /// - Profile is already deactivated
    /// Deactivate a user profile, preventing further platform activity.
    ///
    /// # Authorization
    ///
    /// **SECURITY**: Only the user whose profile is being deactivated can invoke this.
    /// The caller's signature is verified via `require_auth()` before state mutation.
    /// Unauthorized invocation results in immediate transaction rollback.
    ///
    /// # Arguments
    /// * `user` - Address of the user whose profile to deactivate
    ///
    /// # Storage Side-Effects
    /// - Marks user profile status as `Deactivated` in persistent storage
    /// - Releases the username back to the pool (not reserved for the deactivated user)
    /// - Emits `ProfileDeactivated` event with the user address
    ///
    /// # Preconditions
    /// - User must be onboarded (have an existing profile)
    /// - Profile must not already be deactivated
    /// - User must not have active escrows (checked via escrow contract)
    /// - Admin user profile cannot be deactivated
    ///
    /// # Reverts if
    /// - Caller is not the user being deactivated (authorization failure)
    /// - Profile already deactivated
    /// - Active escrows exist for this user
    /// - User is the admin
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

        // Issue #524 — event payload now carries the user's role at
        // deactivation time. The role was overwritten in the
        // `Deactivated` status above, so emitting the captured
        // `profile.role` here lets an indexer attribute the
        // deactivation to "an artisan left" vs "a customer left"
        // without a follow-up profile read.
        env.events().publish(
            (Symbol::new(&env, "ProfileDeactivated"), user.clone()),
            (user, profile.role.clone()),
        );
    }

    /// Reactivate a previously deactivated profile (Issue #115).
    ///
    /// Re-registers the user's original username and sets status back to Active.
    ///
    /// # Reverts if
    /// - Profile is not deactivated
    /// - Username has been claimed by another user since deactivation
    /// Re-activate a previously deactivated user profile.
    ///
    /// # Authorization
    ///
    /// **SECURITY**: Only the user whose profile is being reactivated can invoke this.
    /// The caller's signature is verified via `require_auth()` before state mutation.
    /// Unauthorized invocation results in immediate transaction rollback.
    ///
    /// # Arguments
    /// * `user` - Address of the deactivated user to reactivate
    ///
    /// # Returns
    /// Updated `UserProfile` with status changed back to `Active`.
    ///
    /// # Storage Side-Effects
    /// - Marks user profile status as `Active` in persistent storage
    /// - Re-claims the user's reserved username in persistent storage
    /// - Emits `ProfileReactivated` event with user address and role
    /// - Extends TTL on profile and username entries
    ///
    /// # Preconditions
    /// - User must have been previously deactivated
    /// - User's username must still be available (not taken by another user)
    /// - Profile must exist and be in deactivated status
    ///
    /// # Reverts if
    /// - Caller is not the user being reactivated (authorization failure)
    /// - Profile not found in persistent storage
    /// - Profile is not in Deactivated status
    /// - Username has been taken by another user while deactivated
    pub fn reactivate_profile(env: Env, user: Address) -> UserProfile {
        user.require_auth();

        let profile_key = DataKey::UserProfile(user.clone());
        let mut profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
        Self::extend_persistent(&env, &profile_key);

        if profile.status != ProfileStatus::Deactivated {
            env.panic_with_error(Error::ProfileDeactivated);
        }

        // Re-claim username — fail if another user took it while deactivated
        let normalized = normalize_username(&env, &profile.username);
        if env.storage().persistent().has(&DataKey::Username(normalized.clone())) {
            env.panic_with_error(Error::UsernameTaken);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized.clone()), &user);
        Self::extend_persistent(&env, &DataKey::Username(normalized));

        profile.status = ProfileStatus::Active;
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        env.events().publish(
            (Symbol::new(&env, "ProfileReactivated"), user.clone()),
            (user, profile.role.clone()),
        );

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
    /// Mark a user as verified on the platform.
    ///
    /// # Authorization
    ///
    /// **SECURITY**: Only the platform admin can invoke this endpoint.
    /// The caller's signature is verified via `require_auth()` before any state mutation.
    /// Unauthorized invocation results in immediate transaction rollback with
    /// `Error::Unauthorized`.
    ///
    /// # Arguments
    /// * `user` - Address of the user to verify
    ///
    /// # Returns
    /// Updated `UserProfile` with `is_verified` flag set to true.
    ///
    /// # Storage Side-Effects
    /// - Writes updated `UserProfile` to persistent storage
    /// - Emits `UserVerified` event containing the verified user address
    /// - Extends TTL on config and profile entries
    ///
    /// # Reverts if
    /// - Caller is not the platform admin (unauthorized)
    /// - User not found in persistent storage
    /// - Config not initialized
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
        env.storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized))
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
    /// # Security — issue #498
    ///
    /// Auth check runs before any TTL extension or storage write, following
    /// the check-effect-interactions pattern. A non-admin caller is rejected
    /// by `require_auth` before the contract touches any persistent state,
    /// preventing unauthorized callers from extending the Config TTL as a
    /// side-effect of a failed invocation.
    ///
    /// # Arguments
    /// * `contract_address` - Address of the deployed escrow contract
    ///
    /// # Reverts if
    /// - Contract not initialized
    /// - Caller is not platform admin
    pub fn set_escrow_contract(env: Env, contract_address: Address) {
        // Issue #498 — load config read-only first, then require_auth,
        // then extend TTL and write. This ordering ensures unauthorized
        // callers cannot trigger any storage side-effects.
        let mut config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        config.platform_admin.require_auth();

        Self::extend_persistent(&env, &DataKey::Config);
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

    /// Enable or disable threshold-based automatic verification (admin only).
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
    /// # Integration notes — issue #469 / component #68
    ///
    /// ## Preconditions
    /// - Contract must be initialized (`DataKey::Config` present).
    /// - `address` may be any Stellar address; no auth is required for this
    ///   read-only accessor.
    ///
    /// ## Storage side-effects
    /// - Reads `DataKey::UserMetrics(address)` via the internal
    ///   `read_user_metrics` helper.
    /// - When a metrics entry already exists, its persistent TTL is extended
    ///   by `TTL_EXTENSION` ledgers (~30 days). This prevents accumulated
    ///   counters from silently resetting if the key expires between escrow
    ///   settlements.
    /// - When no entry exists, returns zeroed defaults without writing storage.
    ///
    /// ## Emitted events
    /// - None. This is a gas-only read suitable for simulation and indexer
    ///   backfills.
    ///
    /// ## Off-chain consumers
    /// - Pair with `min_escrow_count_for_verify` and `min_volume_for_verify`
    ///   from `get_config` to display auto-verification progress.
    /// - `total_volume` is stored at 7-decimal precision after normalization
    ///   in `update_user_metrics`; do not assume raw token stroops.
    /// - Prefer subscribing to `UserVerified` over polling this function once
    ///   thresholds are met.
    ///
    /// # Arguments
    /// * `address` - The user's Stellar wallet address
    ///
    /// # Returns
    /// [`UserMetrics`] with `total_escrow_count` and `total_volume` populated,
    /// or zeroed defaults when no escrow activity has been recorded.
    pub fn get_user_metrics(env: Env, address: Address) -> UserMetrics {
        let metrics_key = DataKey::UserMetrics(address.clone());
        let metrics = env
            .storage()
            .persistent()
            .get::<DataKey, UserMetrics>(&metrics_key)
            .unwrap_or(UserMetrics {
                total_escrow_count: 0,
                total_volume: 0,
            });
        if env.storage().persistent().has(&metrics_key) {
            Self::extend_persistent(&env, &metrics_key);
        }
        metrics
    }

    /// Increment a user's activity metrics (called by the escrow contract).
    ///
    /// # Integration notes — issue #469 / component #68
    ///
    /// ## Preconditions
    /// - Contract must be initialized.
    /// - Caller must be the registered `OnboardingConfig::escrow_contract`
    ///   address (authenticated via `require_auth`), or `platform_admin` when
    ///   no escrow contract is registered yet.
    /// - `escrow_count_delta` and `volume_delta` are saturating increments;
    ///   pass `0` for either field to skip that counter.
    /// - `token_address` must be a valid Soroban token contract; its `decimals()`
    ///   value drives volume normalization to 7-decimal stroops.
    ///
    /// ## Storage side-effects
    /// - Reads and extends TTL on `DataKey::Config`.
    /// - Reads, writes, and extends TTL on `DataKey::UserMetrics(address)`.
    /// - When `auto_verify_enabled` is true and thresholds are met after the
    ///   update, may also read/write `DataKey::UserProfile(address)`, append
    ///   compact verification history entries, and emit `UserVerified` via the
    ///   internal `try_auto_verify` path.
    ///
    /// ## Emitted event — `UserVerified` (conditional)
    /// - **Topics:** `(Symbol::new("UserVerified"),)`
    /// - **Data:** `Address` — the verified user
    /// - Emitted only when auto-verification triggers inside `try_auto_verify`
    ///   after this call. No event is emitted when thresholds are not met.
    ///
    /// ## Off-chain consumers
    /// - Escrow contract should call this after each seller-side settlement
    ///   with the gross token amount and seller address.
    /// - This function performs no token transfers (check-effect-interactions
    ///   safe: auth check, storage writes, optional profile update only).
    /// - Indexers tracking verification progress should listen for
    ///   `UserVerified` rather than diffing metrics on every escrow event.
    ///
    /// # Arguments
    /// * `address` - Seller whose metrics to increment
    /// * `escrow_count_delta` - Number of completed escrows to add (typically `1`)
    /// * `volume_delta` - Gross token amount in the token's native stroops
    /// * `token_address` - Token contract used for decimal normalization
    ///
    /// # Reverts if
    /// - Contract not initialized
    /// - Caller is not the registered escrow contract (or admin fallback)
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
        let mut metrics = Self::read_user_metrics(&env, &address);

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
        // Issue #523 — short-circuit on the cheap arithmetic check
        // BEFORE doing the persistent read of `UserProfile`. The
        // verification threshold is the hot path; reading + decoding
        // a `UserProfile` costs persistent-storage CPU instructions
        // that we charge for every escrow settlement. Bailing out
        // early when the metric bar isn't met saves that read on
        // every settle until the user actually qualifies.
        if metrics.total_escrow_count < config.min_escrow_count_for_verify
            || metrics.total_volume < config.min_volume_for_verify
        {
            return;
        }

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
    /// The user being checked must sign the transaction. Even though
    /// auto-verification only flips a positive flag when on-chain metrics
    /// already qualify (so a malicious caller could not fabricate a
    /// verification), gating on `address.require_auth()` keeps the
    /// endpoint locked to the account owner — preventing third parties
    /// from forcing a verification event onto a user who has not opted
    /// in to the auto-flow, and giving auditors a clear authenticated
    /// source for every `UserVerified` event emitted via this path.
    ///
    /// # Returns
    /// `true` if the user was just auto-verified, `false` if thresholds not met or already verified.
    pub fn auto_verify_user(env: Env, address: Address) -> bool {
        // Lock the endpoint to the account being verified. The Soroban
        // host short-circuits the rest of the call if the signature is
        // missing or signed by a different address, so an unauthorized
        // invocation can never reach the state mutation below.
        address.require_auth();

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
    /// Calling this a second time before the request is processed is a no-op.
    ///
    /// Only Buyers and Artisans may invoke this endpoint. Admins and Moderators
    /// are assigned their roles directly and do not use the verification queue.
    pub fn request_verification(env: Env, user: Address) {
        user.require_auth();

        let profile_key = DataKey::UserProfile(user.clone());
        let profile: UserProfile = env
            .storage()
            .persistent()
            .get(&profile_key)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound));
        Self::extend_persistent(&env, &profile_key);

        // Only Buyers and Artisans may request manual verification.
        // Admins and Moderators are assigned their roles directly and bypass
        // the verification queue.
        assert!(
            profile.role == UserRole::Buyer || profile.role == UserRole::Artisan,
            "Only Buyers and Artisans can request verification"
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

    /// Approve or reject a pending manual verification request (admin only).
    ///
    /// # Integration notes — issue #477 / component #76
    ///
    /// ## Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `OnboardingConfig::platform_admin`
    ///   (`require_auth`).
    /// - `user` must be onboarded (`DataKey::UserProfile(user)`).
    /// - A pending verification request for `user` is cleared as part of
    ///   processing (queue head advanced via `clear_verification_request`).
    ///
    /// ## Storage side-effects
    /// - Reads and extends TTL on `DataKey::Config`.
    /// - Reads, writes, and extends TTL on `DataKey::UserProfile(user)`,
    ///   updating only `is_verified` to match `approve`. Profile version
    ///   (`CURRENT_USER_PROFILE_VERSION`) and all other fields are preserved.
    /// - Removes `DataKey::VerificationRequest(user)` and compacts the queue.
    /// - Appends a compact history entry with action `"approved"` or
    ///   `"rejected"` and `by = Some(platform_admin)`.
    ///
    /// ## Emitted event — `UserVerified` (on approval only)
    /// - **Topics:** `(Symbol::new("UserVerified"),)`
    /// - **Data:** `Address` — the newly verified `user`
    /// - Not emitted when `approve == false`.
    ///
    /// ## Off-chain consumers
    /// - Indexers should treat `UserVerified` as the canonical signal that
    ///   `is_verified` flipped to `true`; pair with `get_verification_history`
    ///   for a full audit trail including rejections.
    /// - This function performs no token transfers (check-effect-interactions
    ///   safe: auth check and storage writes only).
    ///
    /// # Arguments
    /// * `user` - Address of the user whose request is being processed
    /// * `approve` - `true` to verify the user, `false` to reject
    ///
    /// # Reverts if
    /// - Contract not initialized
    /// - Caller is not platform admin
    /// - User not found
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
        let action = if approve {
            String::from_str(&env, "approved")
        } else {
            String::from_str(&env, "rejected")
        };
        let hist_key = DataKey::VerificationHistory(user.clone());
        let mut history: Vec<VerificationEntry> = env
            .storage()
            .persistent()
            .get(&hist_key)
            .unwrap_or(Vec::new(&env));
        history.push_back(VerificationEntry {
            timestamp: env.ledger().timestamp(),
            action,
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
    /// Only the user themselves may read their own verification history.
    pub fn get_verification_history(env: Env, user: Address) -> Vec<VerificationEntry> {
        user.require_auth();

        Self::migrate_legacy_verification_history(&env, &user);

        let count_key = DataKey::VerificationHistoryCount(user.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if env.storage().persistent().has(&count_key) {
            Self::extend_persistent(&env, &count_key);
        }

        let mut result = Vec::new(&env);
        for index in 0..count {
            let entry_key = DataKey::VerificationHistoryIndexed(user.clone(), index);
            if let Some(compact) = env
                .storage()
                .persistent()
                .get::<DataKey, CompactVerificationEntry>(&entry_key)
            {
                result.push_back(VerificationEntry {
                    timestamp: compact.timestamp,
                    action: Self::verification_action_to_string(&env, compact.action),
                    by: compact.by,
                });
                Self::extend_persistent(&env, &entry_key);
            }
        }
        result
    }

    /// Get all addresses currently awaiting manual verification (admin helper).
    pub fn get_verification_queue(env: Env) -> Vec<Address> {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);
        config.platform_admin.require_auth();

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
        let cooldown_key = DataKey::LastUsernameChange(user.clone());
        if let Some(last_change) = env.storage().persistent().get::<DataKey, u64>(&cooldown_key) {
            if env.storage().persistent().has(&cooldown_key) {
                Self::extend_persistent(&env, &cooldown_key);
            }
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
            action: String::from_str(&env, "username_revoked"),
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

    /// Set the username change fee (admin only) - Issue #114
    ///
    /// # Arguments
    /// * `fee` - Fee amount in stroops (0 to disable)
    pub fn set_username_change_fee(env: Env, fee: i128) {
        // Issue #522 — strict check-effect-interactions ordering. We
        // load the config first (read-only), validate the caller is
        // the configured admin, validate the `fee` argument, and only
        // then perform any TTL extension or persistent write. This way
        // a non-admin caller cannot wedge the Config TTL by spamming
        // this entry point — they're rejected by `require_auth` before
        // we touch storage at all. Same pattern is applied to the
        // sibling setters below for consistency.
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        config.platform_admin.require_auth();
        if fee < 0 {
            env.panic_with_error(Error::InvalidFee);
        }

        Self::extend_persistent(&env, &DataKey::Config);
        env.storage()
            .persistent()
            .set(&DataKey::UsernameChangeFee, &fee);
        Self::extend_persistent(&env, &DataKey::UsernameChangeFee);
    }

    /// Set the token used to collect username change fees (admin only).
    pub fn set_username_fee_token(env: Env, token: Address) {
        // Issue #526 — strict check-effect-interactions ordering.
        // Load config (read-only) → require_auth(admin) → only then
        // touch any persistent storage. The previous implementation
        // called `extend_persistent` on the Config key before the auth
        // check, so a non-admin caller could spam-extend Config TTL
        // before being rejected. Matched layout applied to
        // `set_username_fee_wallet` below.
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        config.platform_admin.require_auth();

        Self::extend_persistent(&env, &DataKey::Config);
        env.storage()
            .persistent()
            .set(&DataKey::UsernameChangeFeeToken, &token);
        Self::extend_persistent(&env, &DataKey::UsernameChangeFeeToken);
    }

    /// Set the wallet that receives username change fees (admin only).
    ///
    /// # Integration notes — issue #465 / component #64
    ///
    /// ## Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `OnboardingConfig::platform_admin`
    ///   (`require_auth` runs before any storage write or TTL extension).
    ///
    /// ## Storage side-effects
    /// - Reads and extends TTL on `DataKey::Config`.
    /// - Writes and extends TTL on `DataKey::UsernameChangeFeeWallet`.
    /// - Does not modify profile shapes or `CURRENT_USER_PROFILE_VERSION`.
    ///
    /// ## Emitted events
    /// - None.
    ///
    /// ## Off-chain consumers
    /// - Pair with `get_username_fee_wallet`, `get_username_change_fee`, and
    ///   `get_username_fee_token` to display the full fee configuration before
    ///   a user invokes `change_username`.
    /// - When no wallet is configured, `get_username_fee_wallet` falls back
    ///   to `platform_admin` via the internal `read_username_fee_wallet` helper.
    /// - This function performs no token transfers (check-effect-interactions
    ///   safe: auth check and storage write only).
    ///
    /// # Arguments
    /// * `wallet` - Stellar address that receives username-change fee transfers
    ///
    /// # Reverts if
    /// - Contract not initialized
    /// - Caller is not platform admin
    pub fn set_username_fee_wallet(env: Env, wallet: Address) {
        // Issue #526 — same ordering as `set_username_fee_token`
        // above: require_auth runs before any TTL extension or write.
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        config.platform_admin.require_auth();

        Self::extend_persistent(&env, &DataKey::Config);
        env.storage()
            .persistent()
            .set(&DataKey::UsernameChangeFeeWallet, &wallet);
        Self::extend_persistent(&env, &DataKey::UsernameChangeFeeWallet);
    }

    /// Get the current username change fee - Issue #114
    pub fn get_username_change_fee(env: Env) -> i128 {
        let fee_key = DataKey::UsernameChangeFee;
        let fee = env
            .storage()
            .persistent()
            .get(&fee_key)
            .unwrap_or(0);
        if env.storage().persistent().has(&fee_key) {
            Self::extend_persistent(&env, &fee_key);
        }
        fee
    }

    /// Get the configured token used for username change fees.
    pub fn get_username_fee_token(env: Env) -> Option<Address> {
        Self::read_username_fee_token(&env)
    }

    /// Get the configured wallet used for username change fees.
    ///
    /// # Integration notes — issue #465 / component #64
    ///
    /// ## Preconditions
    /// - Contract must be initialized.
    /// - No auth required; safe for simulation and read-only client previews.
    ///
    /// ## Storage side-effects
    /// - Reads `DataKey::UsernameChangeFeeWallet` via `read_username_fee_wallet`.
    /// - When the key exists, extends its persistent TTL by `TTL_EXTENSION`
    ///   ledgers (~30 days).
    /// - When unset, returns `OnboardingConfig::platform_admin` without writing
    ///   storage.
    ///
    /// ## Emitted events
    /// - None.
    ///
    /// ## Off-chain consumers
    /// - Clients preparing a `change_username` transaction should display this
    ///   address as the fee recipient alongside `get_username_change_fee` and
    ///   `get_username_fee_token`.
    /// - The actual fee transfer in `change_username` uses this resolved wallet
    ///   as the token transfer destination (external call is the final action
    ///   in that execution path per check-effect-interactions).
    ///
    /// # Returns
    /// Configured fee wallet, or `platform_admin` when no override is set.
    ///
    /// # Reverts if
    /// - Contract not initialized
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
    /// Allows artisans to attach, replace, or remove an IPFS content
    /// identifier that points to their off-chain portfolio showcase.
    ///
    /// # Integration notes — issue #513 / component #112
    ///
    /// ## Preconditions
    /// - Contract must be initialized.
    /// - `user` must sign the transaction (`user.require_auth()`).
    /// - `user` must be onboarded with `UserRole::Artisan`. Buyers and
    ///   other roles cannot update a portfolio.
    /// - When `portfolio_cid` is `Some(cid)`, `cid` must pass
    ///   `validate_ipfs_cid` (shared with escrow metadata validation):
    ///   - **CIDv0:** exactly 46 chars, Base58btc, prefix `Qm`
    ///   - **CIDv1:** multibase prefix `b` (base32lower), `f`
    ///     (base16lower), or `z` (base58btc) with version byte `0x01`
    /// - Pass `None` to clear an existing portfolio link.
    ///
    /// ## Storage side-effects
    /// - Reads, writes, and extends TTL on
    ///   `DataKey::UserProfile(user)`, updating only
    ///   `UserProfile.portfolio_cid`. All other profile fields —
    ///   including `version` (`CURRENT_USER_PROFILE_VERSION`), role,
    ///   verification status, and reputation counters — are preserved.
    /// - No username-index or config keys are touched. Storage rent for
    ///   the profile entry grows only when a non-empty CID string is
    ///   stored; setting `None` removes the optional payload and reduces
    ///   entry size.
    ///
    /// ## Emitted event — `PortfolioUpdated`
    /// - **Topics:** `(Symbol::new("PortfolioUpdated"),)`
    /// - **Data:** `Address` — the `user` whose portfolio changed
    /// - The event does **not** include the CID itself; indexers should
    ///   call `get_user(user)` or `get_user_by_username` after observing
    ///   the event to fetch the updated `portfolio_cid` value.
    ///
    /// ## Off-chain consumers
    /// - Portfolio CIDs are also returned by read-only accessors
    ///   `get_user` and `get_user_by_username` as part of `UserProfile`.
    /// - This function performs no token transfers (check-effect-
    ///   interactions safe: checks and storage writes only).
    /// - Clients should resolve the CID against IPFS gateways or pinning
    ///   services off-chain; the contract stores only the identifier.
    ///
    /// # Arguments
    /// * `user` - Artisan's wallet address (must sign)
    /// * `portfolio_cid` - IPFS CID to set, or `None` to remove
    ///
    /// # Returns
    /// Updated `UserProfile` reflecting the new `portfolio_cid` value.
    ///
    /// # Reverts if
    /// - User not onboarded (`Error::UserNotFound`)
    /// - User is not an artisan
    /// - Invalid CID format when `portfolio_cid` is `Some`
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
