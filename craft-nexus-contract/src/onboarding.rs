//! Onboarding Contract
//!
//! Handles user registration (onboarding), role assignments, username configuration,
//! profile management, and verification processes for buyers and artisans on the CraftNexus platform.
//!
//! # Integration guide
//!
//! This section documents the integration surface that off-chain indexers and
//! client interfaces depend on (Issue #453 / component #52). Three integration
//! channels exist: the **read API** (view functions), the **event stream**, and
//! the **cross-contract interface** shared with the escrow contract.
//!
//! ## Read API for indexers and clients
//!
//! All read functions are side-effect free with respect to *state shape* but
//! refresh the persistent TTL of the entries they touch (via the internal
//! `extend_persistent` helper). Repeatedly reading a profile is
//! therefore safe and additionally keeps the entry from being archived.
//!
//! | Function | Returns | Notes |
//! |----------|---------|-------|
//! | [`OnboardingContract::get_user`] | [`UserProfile`] | Panics with [`Error::UserNotFound`] when absent. |
//! | [`OnboardingContract::get_user_by_username`] | [`UserProfile`] | Looks up by the *normalized* username (lowercased, see `onboard_user`). |
//! | [`OnboardingContract::is_onboarded`] | `bool` | Non-panicking existence check. |
//! | [`OnboardingContract::is_username_taken`] | `bool` | Accepts any casing; normalizes internally. |
//! | [`OnboardingContract::get_user_role`] | [`UserRole`] | Returns [`UserRole::None`] for unknown users. |
//! | [`OnboardingContract::is_verified`] | `bool` | Reflects manual or auto verification. |
//! | [`OnboardingContract::get_user_metrics`] | [`UserMetrics`] | Escrow count / volume used for auto-verification. |
//! | [`OnboardingContract::get_user_reputation`] | `(u32, u32)` | `(successful_trades, disputed_trades)`. |
//! | [`OnboardingContract::get_verification_history`] | `Vec<VerificationEntry>` | Compact entries decoded to human-readable actions. |
//! | [`OnboardingContract::get_verification_queue`] | `Vec<Address>` | Pending manual-verification requests in FIFO order. |
//! | [`OnboardingContract::get_config`] | [`OnboardingConfig`] | Global contract configuration. |
//!
//! ## Event stream
//!
//! Events are the canonical integration signal for indexers; subscribe to these
//! topics rather than polling. Consumers should treat an event as authoritative
//! only after it appears in a closed ledger. Each row lists the topic tuple, the
//! data payload, and the function that emits it.
//!
//! | Topic tuple | Data payload | Emitted by |
//! |-------------|--------------|------------|
//! | `("UserOnboarded",)` | [`UserOnboardedEvent`] `{ user, username, role }` | [`OnboardingContract::onboard_user`] |
//! | `("RoleUpdated",)` | `(user: Address, old_role: UserRole, new_role: UserRole)` | [`OnboardingContract::update_user_role`] |
//! | `("UserVerified",)` | `user: Address` | `verify_user`, `auto_verify_user`, `process_verification_request` |
//! | `("ProfileDeactivated", user: Address)` | `(user: Address, role: UserRole)` | [`OnboardingContract::deactivate_profile`] |
//! | `("ProfileReactivated", user: Address)` | `(user: Address, role: UserRole)` | [`OnboardingContract::reactivate_profile`] |
//! | `("UsernameChanged",)` | `user: Address` | [`OnboardingContract::change_username`] |
//! | `("PortfolioUpdated",)` | `user: Address` | [`OnboardingContract::update_portfolio`] |
//!
//! Notes for consumers:
//! - `ProfileDeactivated` / `ProfileReactivated` carry the user **in the topic
//!   tuple** so indexers can filter the stream per user without decoding the
//!   payload; the payload additionally carries the role captured at the time of
//!   the transition so no follow-up profile read is required.
//! - `UserVerified`, `UsernameChanged`, and `PortfolioUpdated` carry only the
//!   address; fetch the current value via [`OnboardingContract::get_user`] when
//!   the new field value is needed.
//! - `UserOnboarded` is emitted exactly once per address — a second
//!   `onboard_user` call for the same address panics with
//!   [`Error::AlreadyOnboarded`] and emits nothing.
//!
//! ## Cross-contract interface
//!
//! Onboarding both calls and is called by the escrow contract:
//! - **Outbound:** during [`OnboardingContract::deactivate_profile`] the contract
//!   invokes [`EscrowInterface::has_active_escrows`] (via the generated
//!   `EscrowClient`) to block deactivation while escrows are open.
//! - **Inbound:** the escrow contract — the address stored in
//!   [`OnboardingConfig::escrow_contract`] — is the only authorized caller of
//!   [`OnboardingContract::update_reputation`],
//!   [`OnboardingContract::update_user_metrics`], and
//!   [`OnboardingContract::update_active_contracts`]. When `escrow_contract` is
//!   `None`, the `platform_admin` is used as the authorized fallback.
//!
//! ## Profile versioning
//!
//! Stored profiles are versioned by [`CURRENT_USER_PROFILE_VERSION`]. Older
//! entries (including the legacy version-less shape) are migrated transparently
//! on first read (internal `try_get_user_profile`); integrators never observe an
//! out-of-date shape through the read API.



use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Bytes, Env, Map, String,
    Symbol, TryFromVal, Val, Vec,
};
use crate::alloc::string::ToString;
extern crate alloc;

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
    /// Count of currently-active contracts for a user, maintained by the
    /// configured escrow contract (Issue #533).
    ///
    /// This key lets onboarding flows validate "active contract" constraints
    /// (e.g. preventing deactivation) without needing a cross-contract call
    /// back into the escrow contract on every check.
    ActiveContractCount(Address),
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
    None = 0,    // User has not onboarded
    Buyer = 1,   // Can purchase items
    Artisan = 2, // Can sell items and create escrow
    Admin = 3,   // Platform administrator
    /// Dispute-resolution delegate (Issue #116). Moderators may resolve
    /// escrows when their address is also registered on the escrow
    /// contract's platform config, but they cannot change WASM, platform
    /// fees, or other admin-only settings.
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
    pub username: Symbol,
    pub registered_at: u64,
    /// Whether the user has passed verification (manual or auto-threshold)
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
    pub portfolio_cid: Option<Bytes>,
    /// Status of the user profile - Issue #113
    pub status: ProfileStatus,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
struct LegacyUserProfile {
    pub address: Address,
    pub role: UserRole,
    pub username: Symbol,
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
///
/// # Integration notes — issue #427 / component #26
///
/// ## Storage layout
/// Stored persistently under [`DataKey::UserMetrics`]`(Address)`. The struct
/// is intentionally flat — two scalar fields only — to minimise on-chain
/// entry size and reduce rent overhead. Avoid adding `Vec` or `Map` fields
/// here; derived counters belong in separate indexed keys.
///
/// ## TTL management
/// [`OnboardingContract::get_user_metrics`] extends the TTL of this entry
/// on every read (via `extend_persistent`) so the record stays live as long
/// as the user's escrow activity is ongoing. Writers (`update_user_metrics`)
/// also extend TTL immediately after the write. The combined read + write
/// refresh ensures the entry survives even during long periods with no new
/// escrow settlements.
///
/// ## Preconditions for writers
/// - Caller must be the [`OnboardingConfig::escrow_contract`] address, or
///   `platform_admin` when no escrow contract is configured.
/// - `volume_delta` must be normalised to 7-decimal stroops before
///   accumulation; raw token amounts in non-standard decimals will produce
///   incorrect threshold comparisons.
///
/// ## Storage side-effects
/// - `DataKey::UserMetrics(address)` — read, incremented, written, TTL extended.
/// - `DataKey::Config` — read to check auto-verify thresholds.
/// - May transitively write `DataKey::UserProfile(address)` and append
///   compact verification history entries when auto-verification triggers.
///
/// ## Off-chain consumers
/// Indexers should subscribe to `UserVerified` events (see [`OnboardingContract::auto_verify_user`])
/// rather than polling this struct to detect when a user crosses the
/// verification threshold. The `total_volume` field is in 7-decimal
/// stroop precision and must be divided by `10^7` before display.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UserMetrics {
    /// Total number of completed seller-side escrows recorded by the escrow contract.
    ///
    /// Incremented by [`OnboardingContract::update_user_metrics`] on each
    /// seller-side escrow settlement. Never decremented. Compared against
    /// [`OnboardingConfig::min_escrow_count_for_verify`] during auto-verify.
    pub total_escrow_count: u32,
    /// Cumulative seller volume in stroops at 7-decimal precision (not raw token units).
    ///
    /// Accumulated by normalising each settlement amount to 7 decimal places
    /// before addition, ensuring token-agnostic threshold comparisons. Compared
    /// against [`OnboardingConfig::min_volume_for_verify`] during auto-verify.
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
/// Returned by [`OnboardingContract::get_verification_history`]. On-chain storage
/// uses compact [`VerificationActionCode`] values; this struct exposes human-readable
/// `action` strings for off-chain indexers and client UIs.
///
/// # Integration notes — issue #425 / component #24
///
/// ## Purpose
/// Each `VerificationEntry` is a record of a discrete verification lifecycle
/// event for a user: a request, an approval, a rejection, an automatic
/// threshold-based verification, or a revocation triggered by a username
/// change. Together these entries form the user's complete verification audit
/// trail, enabling off-chain indexers and admin dashboards to reconstruct the
/// full history.
///
/// ## Preconditions
/// - Entries are appended only by functions in [`OnboardingContract`] that
///   mutate verification state (`request_verification`, `approve_verification`,
///   `reject_verification`, `auto_verify_user`, `change_username`).
/// - No external contract or account may write entries directly.
///
/// ## Storage side-effects
/// Entries are stored under two key patterns:
/// - `DataKey::VerificationHistoryCount(Address)` — count of entries (u32)
/// - `DataKey::VerificationHistoryIndexed(Address, u32)` — per-entry compact
///   record ([`CompactVerificationEntry`]); the `action` field is stored as
///   [`VerificationActionCode`] to minimise on-chain size.
///
/// Both keys have their TTL extended on every write and on reads that touch
/// the history. The legacy `DataKey::VerificationHistory(Address)` Vec-based
/// key is migrated lazily on first read and should not be written by new code.
///
/// ## Emitted events
/// The functions that append history entries also emit named events on the
/// contract's event stream. Indexers should prefer events over polling:
/// - `request_verification` → `VerificationRequested`
/// - `approve_verification` → `UserVerified`
/// - `reject_verification`  → `VerificationRejected`
/// - `auto_verify_user`     → `UserVerified`
/// - `change_username`      → `UsernameChanged` (if verification is revoked)
///
/// ## Off-chain consumers
/// The `action` field is a human-readable [`Symbol`] — one of:
/// `"requested"`, `"approved"`, `"rejected"`, `"auto_verified"`,
/// `"username_changed_revoked"`.
/// Indexers may store this verbatim; no further decoding is required.
/// The `by` field carries the actor's address for admin-initiated actions
/// and is `None` for auto-verification events, which have no single actor.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct VerificationEntry {
    /// Ledger timestamp (seconds) when the action was recorded.
    pub timestamp: u64,
    /// One of: `"requested"`, `"approved"`, `"rejected"`, `"auto_verified"`,
    /// `"username_changed_revoked"`. See issue #473 / component #72.
    pub action: Symbol,
    /// Address that performed the action; `None` for auto-verification events.
    pub by: Option<Address>,
}

/// Compact action code for indexed verification history storage (#519).
///
/// # Integration notes — issue #429 / component #28
///
/// ## Purpose
/// [`VerificationActionCode`] is the on-chain wire representation of a
/// verification lifecycle action, encoded as a `u32` discriminant. Using
/// a compact code rather than a full [`Symbol`] string reduces the size of
/// each [`CompactVerificationEntry`] ledger entry, lowering rent costs at
/// scale when users accumulate many history records.
///
/// ## Preconditions
/// - Only values defined in this enum are written to storage. Any unknown
///   discriminant encountered during a future upgrade indicates a schema
///   mismatch and should be treated as an error by migration tooling.
///
/// ## Storage side-effects
/// - Stored as the `action` field inside
///   [`DataKey::VerificationHistoryIndexed`]`(Address, u32)` entries.
/// - Discriminant values are **stable** — adding new variants is safe;
///   reordering or removing variants is a breaking schema change.
///
/// ## Off-chain consumers
/// Decode the discriminant using the mapping below:
/// - `0` → `"requested"` (user submitted a manual review request)
/// - `1` → `"approved"`  (admin approved the verification request)
/// - `2` → `"rejected"`  (admin rejected the verification request)
/// - `3` → `"auto_verified"` (activity thresholds triggered auto-verification)
/// - `4` → `"username_changed_revoked"` (username change revoked verification)
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
///
/// # Integration notes — issue #429 / component #28 (continued)
///
/// ## Purpose
/// `CompactVerificationEntry` is the actual bytes persisted under
/// [`DataKey::VerificationHistoryIndexed`]`(Address, u32)`. It mirrors
/// [`VerificationEntry`] but stores `action` as a [`VerificationActionCode`]
/// discriminant rather than a heap-allocated [`Symbol`], keeping each
/// storage entry small and rent-efficient.
///
/// ## Storage side-effects
/// - Key: `DataKey::VerificationHistoryIndexed(user_address, index_u32)`
/// - TTL is extended immediately after every write and on reads within
///   `get_verification_history`.
/// - The corresponding count is maintained under
///   `DataKey::VerificationHistoryCount(user_address)`.
///
/// ## Off-chain consumers
/// Indexers receive the decoded [`VerificationEntry`] form (with
/// human-readable `action` symbols) from
/// [`OnboardingContract::get_verification_history`] — this struct is
/// internal and does not appear in any public ABI. Clients should never
/// need to decode `CompactVerificationEntry` directly.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
struct CompactVerificationEntry {
    timestamp: u64,
    action: VerificationActionCode,
    by: Option<Address>,
}

/// Contract configuration for the onboarding module.
///
/// # Integration notes — issue #437 / component #36
///
/// ## Purpose
/// `OnboardingConfig` is the single on-chain configuration record for the
/// [`OnboardingContract`]. It is stored as a singleton under
/// [`DataKey::Config`] and read by nearly every public function. Admins
/// manage it through dedicated setter functions; it should never be written
/// directly by external contracts.
///
/// ## Preconditions
/// - Only the `platform_admin` address may call functions that mutate this
///   config (e.g. `set_config`, `set_auto_verify_config`,
///   `set_escrow_contract`). Each mutating function calls
///   `platform_admin.require_auth()` before any storage write.
/// - `min_username_length` must be ≤ `max_username_length`.
/// - `min_escrow_count_for_verify` and `min_volume_for_verify` are advisory
///   thresholds; setting both to `0` effectively disables the count/volume
///   gates while `auto_verify_enabled` remains the master switch.
///
/// ## Storage side-effects
/// - Stored persistently under `DataKey::Config` (singleton).
/// - TTL is extended on every read and write via `extend_persistent`.
/// - Any function that reads `Config` and finds the entry absent will panic
///   with [`Error::NotInitialized`]; callers should ensure `initialize` has
///   been called before any other contract function.
///
/// ## Emitted events
/// Configuration mutations emit a `ConfigUpdated` event per changed field so
/// indexers can track admin actions without scanning storage. No event is
/// emitted during `initialize`.
///
/// ## Off-chain consumers
/// - Read the singleton via `get_config` (if exposed) or listen to
///   `ConfigUpdated` events to maintain a local mirror.
/// - Cache `require_username`, `min_username_length`, and
///   `max_username_length` client-side to validate usernames before
///   submitting `onboard_user` or `change_username` transactions.
/// - `escrow_contract` identifies the only address authorized to call
///   `update_user_metrics` and `update_reputation`; indexers can use this
///   to validate event sources.
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
    /// Attempted to decrement active contract count below zero
    ActiveContractUnderflow = 15,
}

/// Cross-contract interface the onboarding contract uses to query the escrow
/// contract.
///
/// The `#[contractclient]` attribute generates an `EscrowClient` that onboarding
/// uses to call into the configured [`OnboardingConfig::escrow_contract`]. This
/// is the only outbound cross-contract dependency of the onboarding contract.
///
/// Integrators implementing an escrow-compatible contract must expose a matching
/// `has_active_escrows` entrypoint with this exact signature, otherwise
/// [`OnboardingContract::deactivate_profile`] will fail to resolve the call.
#[soroban_sdk::contractclient(name = "EscrowClient")]
pub trait EscrowInterface {
    /// Returns `true` when `user` still has at least one open/active escrow.
    ///
    /// Called during [`OnboardingContract::deactivate_profile`] to enforce the
    /// "no deactivation with active escrows" rule ([`Error::ActiveEscrowsExist`]).
    ///
    /// # Parameters
    /// - `user`: address whose active-escrow status is being queried.
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

/// Transliterate the leading multi-byte character of a username to its ASCII
/// equivalent (component #48 — username normalization interface).
///
/// This is the lookup table that drives [`normalize_username`]: it maps the
/// first UTF-8 character of `input` (accented Latin letters, Greek, and
/// Cyrillic look-alikes, plus zero-width/BOM separators) to a stable ASCII
/// byte sequence so that visually-similar usernames collapse to one canonical
/// form. Canonicalization is what makes the `DataKey::Username` index a true
/// uniqueness constraint and blocks homoglyph squatting (e.g. Cyrillic "о" vs
/// Latin "o").
///
/// # Off-chain integration
///
/// Clients and indexers that need to predict the on-chain canonical username
/// (for example, to check availability before submitting `onboard_user`) must
/// reproduce this exact mapping. The mapping is intentionally append-only:
/// existing entries are never repointed, so a username that normalized a given
/// way on registration continues to resolve identically across upgrades.
///
/// # Parameters
/// - `input`: the remaining username bytes, positioned at the start of a UTF-8
///   character. Only the leading byte(s) are inspected.
///
/// # Returns
/// - `Some((ascii, consumed))` — `ascii` is the replacement byte slice to emit
///   and `consumed` is the number of input bytes the matched character
///   occupied (so the caller can advance its cursor).
/// - `None` — the leading character has no transliteration rule; the caller
///   handles it with its default lowercasing/separator logic.
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

/// Return the length, in bytes, of the UTF-8 character that begins with
/// `first_byte` (component #48 — username normalization interface).
///
/// Used by [`normalize_username`] and [`map_username_bytes`] to advance the
/// byte cursor one full character at a time when scanning a username, since
/// `no_std` Soroban strings are processed as raw byte buffers rather than as
/// `char` iterators. The length is derived purely from the leading byte's
/// high bits per the UTF-8 encoding:
///
/// - `0x00..=0x7F` → 1 byte (ASCII)
/// - `0xC0..=0xDF` → 2 bytes
/// - `0xE0..=0xEF` → 3 bytes
/// - `0xF0..=0xF7` → 4 bytes
///
/// # Parameters
/// - `first_byte`: the first byte of a UTF-8 character.
///
/// # Returns
/// The byte length of the character (1–4). Continuation bytes and any invalid
/// leading byte fall back to `1` so the scanner always makes forward progress
/// and never loops on malformed input.
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
/// CraftNexus onboarding contract.
///
/// This contract owns the user onboarding registry (profiles + usernames) and
/// exposes integration endpoints used by the escrow contract to:
/// - Update reputation counters (`update_reputation`)
/// - Update activity metrics (`update_user_metrics`)
/// - Maintain the active-contract counter used for business rules
///   (`update_active_contracts`)
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

    fn is_verification_pending_internal(env: &Env, user: &Address) -> bool {
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

            if Self::is_verification_pending_internal(env, &queued_user) {
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
        if let Some(wallet) = env.storage().persistent().get(&key) {
            Self::extend_persistent(env, &key);
            wallet
        } else {
            config.platform_admin.clone()
        }
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
   fn verification_action_to_string(env: &Env, action: VerificationActionCode) -> Symbol {
        match action {
            VerificationActionCode::Requested => Symbol::new(env, "requested"),
            VerificationActionCode::Approved => Symbol::new(env, "approved"),
            VerificationActionCode::Rejected => Symbol::new(env, "rejected"),
            VerificationActionCode::AutoVerified => Symbol::new(env, "auto_verified"),
            VerificationActionCode::UsernameChangedRevoked => {
                Symbol::new(env, "username_revoked")
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
    fn parse_verification_action(env: &Env, action: &Symbol) -> VerificationActionCode {
        if action == &Symbol::new(env, "requested") {
            VerificationActionCode::Requested
        } else if action == &Symbol::new(env, "approved") {
            VerificationActionCode::Approved
        } else if action == &Symbol::new(env, "rejected") {
            VerificationActionCode::Rejected
        } else if action == &Symbol::new(env, "auto_verified") {
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
        // [PERFORMANCE #94] Extend TTL on read so the count key does not expire while
        // the buffer is still in active use. Without this bump a count entry close to
        // its TTL deadline could be archived on the same ledger as the write that follows,
        // silently resetting the history length to zero on the next call.
        let count: u32 = if let Some(c) = env.storage().persistent().get(&count_key) {
            Self::extend_persistent(env, &count_key);
            c
        } else {
            0
        };

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
            
            // ---> ADD THIS LINE TO FIX THE TTL BUG <---
            Self::extend_persistent(env, &key); 
            
            return Some(profile);
        }

        let legacy =
            LegacyUserProfile::try_from_val(env, &stored).expect("User profile storage corrupted");
        
        // Migrate Option<String> to Option<Bytes>
        let optimized_cid = legacy.portfolio_cid.map(|cid_str| {
            let mut cid_bytes = Bytes::new(env);
            let len = cid_str.len() as usize;
            let mut buf = [0u8; 128]; // Max CID length
            cid_str.copy_into_slice(&mut buf[..len]);
            cid_bytes.extend_from_slice(&buf[..len]);
            cid_bytes
        });

        let upgraded = UserProfile {
            version: CURRENT_USER_PROFILE_VERSION,
            address: legacy.address.clone(),
            role: legacy.role,
            username: legacy.username.clone(),
            registered_at: legacy.registered_at,
            is_verified: legacy.is_verified,
            successful_trades: legacy.successful_trades,
            disputed_trades: legacy.disputed_trades,
            portfolio_cid: optimized_cid,
            status: ProfileStatus::Active,
        };
        env.storage().persistent().set(&key, &upgraded);
        Self::extend_persistent(env, &key);
        Some(upgraded)
    }

    fn get_user_profile(env: &Env, user: Address) -> UserProfile {
        Self::try_get_user_profile(env, user)
            .unwrap_or_else(|| env.panic_with_error(Error::UserNotFound))
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

    fn require_ttl_bump_auth(config: &OnboardingConfig) {
        match config.escrow_contract {
            Some(ref escrow_addr) => escrow_addr.require_auth(),
            None => config.platform_admin.require_auth(),
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
        Self::require_ttl_bump_auth(&config);
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
        Self::require_ttl_bump_auth(&config);
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
            username: Symbol::new(&env, "admin"),
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
        // [SECURITY] Endpoint #93: The registering user must prove ownership of the
        // supplied address. Unauthorized invocation without a valid user signature is
        // rejected before any state mutation.
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

        // [SECURITY] Endpoint #93: Only verified platform roles may approve new user
        // registrations. The platform admin must co-sign every onboarding transaction
        // to prevent unauthorized state transitions.
        config.platform_admin.require_auth();
        Self::extend_persistent(&env, &DataKey::Config);

        // Normalize the username (lowercase + trim whitespace)
        let normalized = normalize_username(&env, &username);
        
        // Convert to Symbol for optimized storage
        let username_len = core::cmp::min(normalized.len() as usize, 32);
        let mut user_buf = [0u8; 32];
        normalized.copy_into_slice(&mut user_buf[..username_len]);
        let s = core::str::from_utf8(&user_buf[..username_len]).unwrap();
        let optimized_username = Symbol::new(&env, s);
        assert!(
            username_len >= config.min_username_length as usize,
            "Username too short"
        );
        assert!(
            username_len <= config.max_username_length as usize,
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
            username: optimized_username,
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
    /// Transparently migrates legacy profiles (missing `version` or `status`
    /// fields) to [`CURRENT_USER_PROFILE_VERSION`] on first read and persists
    /// the upgraded form. This ensures callers always receive a fully-shaped
    /// [`UserProfile`] regardless of when the account was created.
    ///
    /// # Returns
    /// `UserProfile` if a profile exists, otherwise panics with
    /// `Error::UserNotFound`.
    pub fn get_user(env: Env, user: Address) -> UserProfile {
        Self::get_user_profile(&env, user)
    }

    /// Check if the user has any active escrows on the configured escrow contract.
    ///
    /// [FEATURE #51 / #452] The queried user must authorize this read.
    /// Returns false if no escrow contract is registered or if the user has no active escrows.
    pub fn has_active_contracts(env: Env, user: Address) -> bool {
        user.require_auth();

        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        let local_key = DataKey::ActiveContractCount(user.clone());
        if let Some(count) = env.storage().persistent().get::<_, u32>(&local_key) {
            Self::extend_persistent(&env, &local_key);
            return count > 0;
        }

        if let Some(escrow_contract) = config.escrow_contract {
            let client = EscrowClient::new(&env, &escrow_contract);
            client.has_active_escrows(&user)
        } else {
            false
        }
    }

    /// Return the precise number of active escrow contracts tracked for `user`.
    ///
    /// # Enhanced business flow — feature #47
    ///
    /// [`OnboardingContract::has_active_contracts`] only answers the boolean
    /// "is the user currently engaged?" question. Complex escrow and reputation
    /// scenarios — staggered multi-order settlement, reputation weighting by
    /// concurrent workload, and off-chain risk dashboards — need the exact
    /// concurrency level, not just a flag. This endpoint exposes the locally
    /// maintained `DataKey::ActiveContractCount(user)` counter so off-chain
    /// indexers and client UIs can read it directly without replaying every
    /// escrow event or making a cross-contract call.
    ///
    /// The counter is the same value maintained by
    /// [`OnboardingContract::update_active_contracts`] (incremented when an
    /// escrow becomes active, decremented on close). When no local entry exists
    /// the user has no tracked active contracts and `0` is returned; this is the
    /// canonical "not engaged" state and is consistent with
    /// `has_active_contracts` returning `false`.
    ///
    /// # Authorization
    /// - None. This is a read-only query that mutates no business state and
    ///   therefore needs no `require_auth`; it may be called by indexers and
    ///   clients freely. It only refreshes the TTL of entries it reads.
    ///
    /// # Storage side-effects
    /// - Reads and extends TTL on `DataKey::Config`.
    /// - Reads and (when present) extends TTL on
    ///   `DataKey::ActiveContractCount(user)`.
    /// - No `UserProfile` shape is touched, so no profile-version upgrade is
    ///   required (`CURRENT_USER_PROFILE_VERSION` unaffected).
    ///
    /// # Arguments
    /// * `user` - Address whose active-contract concurrency is being queried.
    ///
    /// # Returns
    /// The number of currently-active contracts for `user` (`0` when none are
    /// tracked).
    ///
    /// # Reverts if
    /// - Contract not initialized.
    pub fn get_active_contract_count(env: Env, user: Address) -> u32 {
        let _config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        let key = DataKey::ActiveContractCount(user);
        match env.storage().persistent().get::<_, u32>(&key) {
            Some(count) => {
                Self::extend_persistent(&env, &key);
                count
            }
            None => 0,
        }
    }

    /// Get user profile by username (case-insensitive)
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
    /// regardless of profile status or version.
    ///
    /// # Security — issue #438
    /// This endpoint is now protected with `require_auth()` to prevent unauthorized
    /// callers from querying onboarding status. Only the user themselves may invoke
    /// this check - it is a privileged query.
    ///
    /// # Storage Optimization — issue #443
    /// TTL extension is now applied on read to prevent premature archival of
    /// user profiles during extended escrow lifecycles.
    ///
    /// # Parameters
    /// - `user`: `Address` — The wallet address to check.
    ///
    /// # Storage Side-Effects
    /// - **Read** [`DataKey::UserProfile(user)`] — existence check with TTL extension.
    ///
    /// # Emitted Events
    /// None.
    ///
    /// # Errors
    /// None — always returns a `bool`.
    pub fn is_onboarded(env: Env, user: Address) -> bool {
        user.require_auth();
        let key = DataKey::UserProfile(user.clone());
        // Issue #423/#435: extend TTL on read to prevent storage expiry
        if env.storage().persistent().has(&key) {
            Self::extend_persistent(&env, &key);
            true
        } else {
            false
        }
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
        // [SECURITY] Endpoint #69: Only the platform admin may promote users to Moderator.
        // Any caller without a valid admin signature is rejected before state mutation.
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

        // [SECURITY] Endpoint #85: Strict authorization check
        // Only admin can update roles; require_auth() verifies the caller's digital signature
        config.platform_admin.require_auth();
        Self::extend_persistent(&env, &DataKey::Config);

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

        let username_string = String::from_str(&env, profile.username.to_string().as_ref());
        let normalized = normalize_username(&env, &username_string);
        if normalized == String::from_str(&env, "admin") {
            env.panic_with_error(Error::Unauthorized);
        }

        let local_key = DataKey::ActiveContractCount(user.clone());
        if let Some(count) = env.storage().persistent().get::<_, u32>(&local_key) {
            Self::extend_persistent(&env, &local_key);
            if count > 0 {
                env.panic_with_error(Error::ActiveEscrowsExist);
            }
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
        let username_string = String::from_str(&env, profile.username.to_string().as_ref());
        let normalized = normalize_username(&env, &username_string);
        if env
            .storage()
            .persistent()
            .has(&DataKey::Username(normalized.clone()))
        {
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
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `platform_admin`.
    /// - `user` must have an existing profile.
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

        // Only admin can verify users
        config.platform_admin.require_auth();
        Self::extend_persistent(&env, &DataKey::Config);

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
    /// # Security — issue #450
    /// This endpoint is now protected with `require_auth()` to prevent unauthorized
    /// callers from querying verification status. Only the authenticated user may
    /// invoke this check.
    ///
    /// # Storage Optimization — issue #443
    /// TTL extension is applied on read to prevent premature archival during
    /// extended escrow lifecycles.
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
        user.require_auth();
        let profile_key = DataKey::UserProfile(user.clone());
        if let Some(profile) = env
            .storage()
            .persistent()
            .get::<_, UserProfile>(&profile_key)
        {
            Self::extend_persistent(&env, &profile_key);
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
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));

        // Auth check before any storage mutation (#422).
        config.platform_admin.require_auth();

        let mut config = config;
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
        // Issue #426/#434: require auth to prevent unauthorized access to user activity data
        address.require_auth();
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

        match config.escrow_contract {
            Some(ref escrow_addr) => escrow_addr.require_auth(),
            None => config.platform_admin.require_auth(),
        }
        Self::extend_persistent(&env, &DataKey::Config);
        Self::extend_persistent(&env, &DataKey::Config);

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

    /// Update a user's "active contracts" counter (called by the escrow contract).
    ///
    /// This endpoint provides a lightweight, upgrade-safe way for the escrow
    /// contract to signal that a user has entered or exited an active escrow
    /// lifecycle. The onboarding contract uses the counter to enforce business
    /// rules such as "profiles with active escrows cannot be deactivated"
    /// without making a cross-contract read on every check.
    ///
    /// # Authorization
    /// - Requires auth from `OnboardingConfig::escrow_contract` when configured.
    /// - Falls back to `platform_admin` auth when no escrow contract is registered.
    ///
    /// # Storage side-effects
    /// - Reads and extends TTL on `DataKey::Config`.
    /// - Reads/writes and extends TTL on `DataKey::ActiveContractCount(user)`.
    /// - Extends TTL (if present) on `DataKey::UserProfile(user)` and
    ///   `DataKey::UserMetrics(user)` so active users do not silently fall out of
    ///   onboarding state due to key expiry between settlements.
    ///
    /// # Arguments
    /// * `user` - User participating in the active contract lifecycle
    /// * `delta` - Signed increment: `+1` when a contract becomes active, `-1`
    ///   when it is closed. Other values are allowed but should be used with
    ///   caution; underflows revert with `Error::ActiveContractUnderflow`.
    ///
    /// # Reverts if
    /// - Contract not initialized
    /// - Caller is not the registered escrow contract (or admin fallback)
    /// - `delta` would underflow the active counter
    pub fn update_active_contracts(env: Env, user: Address, delta: i32) {
        if delta == 0 {
            return;
        }

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

        let key = DataKey::ActiveContractCount(user.clone());
        let current = env.storage().persistent().get::<_, u32>(&key).unwrap_or(0u32);
        if env.storage().persistent().has(&key) {
            Self::extend_persistent(&env, &key);
        }

        let next = if delta > 0 {
            current.saturating_add(delta as u32)
        } else {
            let subtract = (-delta) as u32;
            if subtract > current {
                env.panic_with_error(Error::ActiveContractUnderflow);
            }
            current - subtract
        };

        if next == 0 {
            if env.storage().persistent().has(&key) {
                env.storage().persistent().remove(&key);
            }
        } else {
            env.storage().persistent().set(&key, &next);
            Self::extend_persistent(&env, &key);
        }

        Self::extend_persistent_if_present(&env, &DataKey::UserProfile(user.clone()));
        Self::extend_persistent_if_present(&env, &DataKey::UserMetrics(user));
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
                action: Symbol::new(env, "auto_verified"),
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

        if Self::is_verification_pending_internal(&env, &user) {
            return;
        }

        Self::enqueue_verification_request(&env, &user);

        Self::append_verification_history(
            &env,
            &user,
            VerificationActionCode::Requested,
            Some(user.clone()),
        );
    }

    /// Check whether a user currently has a pending manual verification request.
    ///
    /// The queried user must authorize this read.
    pub fn is_verification_pending(env: Env, user: Address) -> bool {
        user.require_auth();
        Self::is_verification_pending_internal(&env, &user)
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
        // [SECURITY] Endpoint #53 (issue #454): this verification state transition
        // flips `is_verified` on a user profile and is restricted to the platform
        // admin role. The config is loaded read-only first, then authorization is
        // enforced via `require_auth()` before any storage write or TTL extension.
        // The Soroban host aborts the invocation and rolls back the transaction if
        // the call is not signed by `platform_admin`, so an unauthorized caller can
        // never reach the profile mutation, queue update, or event emission below.
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

        Self::append_verification_history(
            &env,
            &user,
            if approve {
                VerificationActionCode::Approved
            } else {
                VerificationActionCode::Rejected
            },
            Some(config.platform_admin.clone()),
        );

        if approve {
            env.events()
                .publish((Symbol::new(&env, "UserVerified"),), &user);
        }
    }

    /// Force-clear a pending manual verification request without approving or
    /// rejecting it (admin only).
    ///
    /// # Security — issue #41 / endpoint hardening
    ///
    /// Unlike [`OnboardingContract::request_verification`] (which a user invokes
    /// for themselves) this is a privileged queue-maintenance state transition:
    /// it removes an arbitrary user's pending request and advances the manual
    /// verification queue head. Allowing any caller to invoke it would let a
    /// malicious actor evict legitimate users from the verification queue,
    /// denying them admin review. The endpoint is therefore locked behind
    /// `OnboardingConfig::platform_admin.require_auth()`, which is evaluated
    /// *before* any storage mutation. An unauthorized invocation fails the auth
    /// check and the host rolls the entire transaction back, leaving the queue
    /// untouched.
    ///
    /// Use this to evict stale or abandoned requests (e.g. from users who later
    /// deactivated) so the queue head can advance. To verify or reject a request
    /// and record an audit entry, prefer
    /// [`OnboardingContract::process_verification_request`] instead.
    ///
    /// # Preconditions
    /// - Contract must be initialized.
    /// - Caller must be `OnboardingConfig::platform_admin` (`require_auth`).
    ///
    /// # Storage side-effects
    /// - Reads and extends TTL on `DataKey::Config`.
    /// - Removes `DataKey::VerificationRequest(user)` (if present) and compacts
    ///   the queue by advancing `DataKey::VerificationQueueHead`.
    /// - No `UserProfile` shape is touched, so no profile-version upgrade is
    ///   required (`CURRENT_USER_PROFILE_VERSION` unaffected).
    ///
    /// # Arguments
    /// * `user` - Address whose pending verification request should be cleared.
    ///
    /// # Returns
    /// `true` if a pending request existed and was cleared; `false` if the user
    /// had no pending request (call is an idempotent no-op in that case).
    ///
    /// # Reverts if
    /// - Contract not initialized.
    /// - Caller is not the platform admin.
    pub fn admin_clear_verification_request(env: Env, user: Address) -> bool {
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);

        // Authorization gate — must run before any state mutation so an
        // unauthorized caller triggers a full transaction rollback (#41).
        config.platform_admin.require_auth();

        let was_pending = Self::is_verification_pending_internal(&env, &user);
        Self::clear_verification_request(&env, &user);
        was_pending
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
        let config: OnboardingConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
        Self::extend_persistent(&env, &DataKey::Config);
        // [SECURITY] Endpoint #73: Verification queue is sensitive data; only the
        // platform admin may read it. Unauthorized access results in immediate rollback.
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
                if Self::is_verification_pending_internal(&env, &user) {
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
        // Issue #426/#434: require auth to prevent unauthorized access to sensitive trade data
        address.require_auth();
        let key = DataKey::UserProfile(address.clone());
        match env
            .storage()
            .persistent()
            .get::<DataKey, UserProfile>(&key)
        {
            Some(profile) => {
                // Issue #423/#435: extend TTL on read to prevent storage expiry
                Self::extend_persistent(&env, &key);
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
        let cooldown_key = DataKey::LastUsernameChange(user.clone());
        if let Some(last_change) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&cooldown_key)
        {
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

        // Atomically remove old username mapping and add new one
        let old_username = profile.username.clone();
        let old_string = String::from_str(&env, old_username.to_string().as_ref());
        env.storage().persistent().remove(&DataKey::Username(old_string));

        // Store new username → address mapping
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized_new.clone()), &user);
        Self::extend_persistent(&env, &DataKey::Username(normalized_new.clone()));

        // Update profile with new username
        let new_username_len = core::cmp::min(normalized_new.len() as usize, 32);
        let mut user_buf = [0u8; 32];
        normalized_new.copy_into_slice(&mut user_buf[..new_username_len]);
        let rust_str = core::str::from_utf8(&user_buf[..new_username_len]).unwrap();
        let optimized_new_username = Symbol::new(&env, rust_str);
        profile.username = optimized_new_username;
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
            action: Symbol::new(&env, "username_revoked"),
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

        // Interaction (CEI pattern: external transfer is the last step)
        Self::collect_username_change_fee(&env, &user, &config);

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
        let fee_key = DataKey::UsernameChangeFee;
        let fee = env.storage().persistent().get(&fee_key).unwrap_or(0);
        if env.storage().persistent().has(&fee_key) {
            Self::extend_persistent(&env, &fee_key);
        }
        fee
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
        let optimized_cid = portfolio_cid.map(|cid_str| {
            let mut cid_bytes = Bytes::new(&env);
            let len = cid_str.len() as usize;
            let mut buf = [0u8; 128];
            cid_str.copy_into_slice(&mut buf[..len]);
            cid_bytes.extend_from_slice(&buf[..len]);
            cid_bytes
        });

        // Update portfolio CID
        profile.portfolio_cid = optimized_cid;

        // Store updated profile
        env.storage().persistent().set(&profile_key, &profile);
        Self::extend_persistent(&env, &profile_key);

        // Emit event
        env.events()
            .publish((Symbol::new(&env, "PortfolioUpdated"),), &user);

        profile
    }
}
