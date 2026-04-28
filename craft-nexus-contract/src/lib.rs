#![no_std]
#![allow(clippy::too_many_arguments)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Bytes,
    BytesN, Env, Map, String, Symbol, TryFromVal, Val, Vec,
};

#[cfg(test)]
mod enhanced_features_test;
#[cfg(test)]
mod expired_dispute_fee_test;
#[cfg(test)]
mod min_release_window_test;
#[cfg(test)]
mod reentrancy_test;
#[cfg(test)]
mod scalability_test;
#[cfg(test)]
mod test;
// Onboarding is a separate logical contract; only one `#[contract]` may be linked per WASM
// artifact. Keep it in this crate for host tests (`cargo test`) but omit from guest builds.
#[cfg(not(target_family = "wasm"))]
pub mod onboarding;

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum Error {
    /// Unauthorized operation
    Unauthorized = 1,
    /// Escrow not found
    EscrowNotFound = 2,
    /// Invalid escrow state for operation
    InvalidEscrowState = 3,
    /// Username already exists
    UsernameAlreadyExists = 4,
    /// Token not whitelisted
    TokenNotWhitelisted = 5,
    /// Amount below minimum
    AmountBelowMinimum = 6,
    /// Release window too long
    ReleaseWindowTooLong = 7,
    /// Not in dispute state
    NotInDispute = 8,
    /// User already onboarded
    AlreadyOnboarded = 9,
    /// Invalid fee amount (must be <= MAX_PLATFORM_FEE_BPS)
    InvalidFee = 10,
    /// Buyer and seller cannot be the same
    SameBuyerSeller = 11,
    /// Platform not initialized
    PlatformNotInitialized = 12,
    /// Release window not yet elapsed
    ReleaseWindowNotElapsed = 13,
    /// Batch operation error (deprecated: use BatchLimitExceeded)
    BatchOperationFailed = 14,
    /// Contract is paused
    ContractPaused = 15,
    /// Dispute resolution deadline has not yet expired
    DisputeExpired = 16,
    /// Artisan stake is below the required minimum
    InsufficientStake = 17,
    /// Stake cooldown period is still active
    StakeCooldownActive = 18,
    /// Refund amount is invalid (zero, negative, or exceeds escrow amount)
    InvalidRefundAmount = 19,
    /// Partial refund proposal not found
    ProposalNotFound = 20,
    /// Partial refund proposal already exists for this order
    ProposalAlreadyExists = 21,
    /// Re-entrancy detected
    ReentryDetected = 22,
    /// Release window is zero or negative
    ReleaseWindowTooShort = 23,
    /// Staked funds can only be withdrawn in the original staking token
    StakeTokenMismatch = 24,
    /// Invalid IPFS CID format (must be valid CIDv0 or CIDv1)
    InvalidIpfsHash = 25,
    /// Invalid metadata hash length (must be 32 bytes)
    InvalidMetadataHash = 26,
    /// Batch size exceeds maximum allowed (MAX_BATCH_SIZE)
    BatchLimitExceeded = 27,
    /// Invalid portfolio CID format
    InvalidPortfolioCid = 28,
    /// User is not an artisan
    NotAnArtisan = 29,
    /// Invalid verification level
    InvalidVerificationLevel = 30,
    /// Username change cooldown not elapsed
    UsernameChangeCooldownActive = 31,
    /// Invalid dispute reason (empty or too long)
    InvalidDisputeReason = 32,
    /// Escrow amount below minimum for token
    EscrowAmountBelowMinimum = 33,
    /// Invalid release window (exceeds maximum)
    InvalidReleaseWindow = 34,
    /// Unauthorized admin operation
    UnauthorizedAdmin = 35,
    /// Recurring escrow not found
    RecurringEscrowNotFound = 36,
    /// Escrow cycle not ready for release
    CycleNotReady = 37,
    /// No upgrade proposed
    NoUpgradeProposed = 38,
    /// WASM upgrade grace period not yet elapsed
    UpgradeCooldownActive = 39,
    /// No pending admin address to accept
    NoPendingAdmin = 40,
    /// Batch operation with empty list
    BatchEmpty = 41,
    /// Internal storage data corrupted
    StorageCorrupted = 42,
}

#[cfg(not(target_family = "wasm"))]
impl From<onboarding::Error> for Error {
    fn from(err: onboarding::Error) -> Self {
        match err {
            onboarding::Error::Unauthorized => Error::Unauthorized,
            onboarding::Error::UserNotFound => Error::EscrowNotFound,
            onboarding::Error::AlreadyOnboarded => Error::AlreadyOnboarded,
            _ => Error::InvalidEscrowState,
        }
    }
}

const ESCROW: Symbol = symbol_short!("ESCROW");
const PLATFORM_FEE: Symbol = symbol_short!("PLAT_FEE");
const PLATFORM_WALLET: Symbol = symbol_short!("PLAT_WAL");
const TOTAL_FEES: Symbol = symbol_short!("TOT_FEES");

/// Standard TTL threshold for persistent storage (approx 14 hours at 5s ledger)
const TTL_THRESHOLD: u32 = 10_000;
/// Standard TTL extension for persistent storage (approx 30 days)
const TTL_EXTENSION: u32 = 518_400;

// Default configuration constants (can be overridden via PlatformConfig)
/// Default grace period for WASM upgrades (7 days in seconds)
const DEFAULT_WASM_UPGRADE_COOLDOWN: u32 = 7 * 24 * 60 * 60;

/// Default maximum duration a dispute can remain open before it can be force-resolved (30 days in seconds)
const DEFAULT_MAX_DISPUTE_DURATION: u32 = 30 * 24 * 60 * 60;

/// Default cooldown period after staking before tokens can be unstaked (7 days in seconds)
const DEFAULT_STAKE_COOLDOWN: u32 = 7 * 24 * 60 * 60;

/// Default minimum release window to prevent "flash" auto-releases (1 day in seconds)
const DEFAULT_MIN_RELEASE_WINDOW: u32 = 24 * 60 * 60;
/// Absolute safety ceiling for admin-configurable max release window (365 days).
const ABSOLUTE_MAX_RELEASE_WINDOW: u32 = 365 * 24 * 60 * 60;

/// Maximum platform fee in basis points (10000 = 100%)
const MAX_PLATFORM_FEE_BPS: u32 = 1000; // 10% max
const MAX_TOTAL_RELEASE_WINDOW: u32 = 2592000; // 30 days
const CURRENT_ESCROW_VERSION: u32 = 3;
/// Maximum number of escrows per batch operation (Issue #111)
// Conservative batch size to avoid exceeding instruction/read-write limits
// observed on Soroban testnets. Reduced from 100 to 20 (Issue #198).
const MAX_BATCH_SIZE: u32 = 20;
/// Timeout for unfunded escrows before they can be cancelled (24 hours) (#213)
const UNFUNDED_CANCEL_TIMEOUT: u64 = 24 * 60 * 60;

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum DataKey {
    Escrow(u32),
    /// DEPRECATED: Legacy vector-based storage. Kept for backward compatibility.
    /// New implementations should use BuyerEscrowIndexed instead.
    BuyerEscrows(Address),
    /// DEPRECATED: Legacy vector-based storage. Kept for backward compatibility.
    /// New implementations should use SellerEscrowIndexed instead.
    SellerEscrows(Address),
    MinEscrowAmount(Address),
    TotalFees(Address),
    FeeTokenIndex,
    ContractVersion,
    /// Platform configuration storage key
    PlatformConfig,
    /// Custom fee tier for an artisan (basis points)
    ArtisanFeeTier(Address),
    /// Deprecated referral reward percentage in basis points.
    /// Retained only for storage compatibility; referral payout logic is not implemented.
    ReferralRewardBps,
    /// Staked token amount and asset for an artisan
    ArtisanStake(Address),
    /// Timestamp when the stake cooldown ends for an artisan (DEPRECATED)
    /// Backwards-compatible single timestamp kept for older clients.
    StakeCooldownEnd(Address),
    /// Per-deposit stake queue for an artisan. Each entry represents an
    /// individual deposit and its cooldown end timestamp. This allows
    /// accurate tracking of staking timeframes when multiple deposits
    /// are made at different times.
    ArtisanStakeQueue(Address),
    /// Partial refund proposal for a disputed order
    PartialRefundProposal(u32),
    /// Re-entrancy guard key
    ReentryGuard,
    /// Pending admin address for two-step transfer
    PendingAdmin,
    /// Proposal for contract WASM upgrade
    WasmUpgradeProposal,
    /// Configurable maximum release window (in seconds)
    MaxReleaseWindow,
    /// Address of the deployed onboarding contract for cross-contract reputation calls
    OnboardingContractAddress,
    /// Map of whitelisted token addresses (Address -> bool); enforcement active when non-empty
    WhitelistedTokens,
    /// Ordered list of all escrow order IDs ever created (Vec<u32>); used for off-chain enumeration
    AllEscrowIds,
    /// Total count of escrows ever created; lightweight O(1) alternative to AllEscrowIds.len()
    EscrowCount,
    /// Key for a recurring escrow by its ID
    RecurringEscrow(u64),
    /// ID counter for recurring escrows
    NextRecurringEscrowId,
    /// Count of currently active (non-released, non-refunded) escrows or recurring escrows for a user address.
    /// Used as a barrier for profile deactivation.
    ActiveObligations(Address),
    /// Indexed storage for buyer escrows: (Address, Index) -> EscrowId
    /// Supports unlimited history without 64KB vector limit
    BuyerEscrowIndexed(Address, u32),
    /// Total count of escrows for a buyer
    BuyerEscrowCount(Address),
    /// Indexed storage for seller escrows: (Address, Index) -> EscrowId
    /// Supports unlimited history without 64KB vector limit
    SellerEscrowIndexed(Address, u32),
    /// Total count of escrows for a seller
    SellerEscrowCount(Address),
    /// Total amount of funds locked in active escrows or recurring escrows for a token address.
    /// Used for sweeping unallocated funds (#212).
    TotalLocked(Address),
    /// Total amount of funds currently staked by artisans for a token address.
    TotalStaked(Address),
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ArtisanStakeData {
    pub amount: i128,
    pub token: Address,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct StakeDeposit {
    pub amount: i128,
    pub cooldown_end: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct RecurringEscrow {
    pub id: u64,
    pub buyer: Address,
    pub artisan: Address,
    pub token: Address,
    pub total_amount: i128,
    pub released_amount: i128,
    pub frequency: u64, // in seconds
    pub duration: u32,  // total number of cycles
    pub current_cycle: u32,
    pub last_release_time: u64,
    pub is_active: bool,
}

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum RecurringEscrowAction {
    Created = 1,
    CycleReleased = 2,
    Cancelled = 3,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct RecurringEscrowEvent {
    pub id: u64,
    pub action: RecurringEscrowAction,
    pub buyer: Address,
    pub artisan: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum EscrowStatus {
    Active = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    Resolved = 4,
}

/// Choice of resolution for a disputed escrow.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum Resolution {
    /// Release funds to the seller.
    /// Platform fees ARE collected in this case.
    ReleaseToSeller = 0,
    /// Refund funds to the buyer.
    /// Full amount is returned; platform fees ARE NOT collected.
    RefundToBuyer = 1,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct Escrow {
    pub version: u32,
    pub id: u64,
    pub batch_id: Option<u64>,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub release_window: u32, // Time in seconds before auto-release
    pub created_at: u32,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
    pub dispute_reason: Option<String>,
    pub dispute_initiated_at: Option<u64>,
    pub funded: bool,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
struct LegacyEscrow {
    pub id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub release_window: u32,
    pub created_at: u32,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
    pub dispute_reason: Option<String>,
    pub dispute_initiated_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
struct EscrowWithoutBatch {
    pub version: u32,
    pub id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub release_window: u32,
    pub created_at: u32,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
    pub dispute_reason: Option<String>,
    pub dispute_initiated_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum EscrowAction {
    Created = 0,
    Released = 1,
    Refunded = 2,
    Disputed = 3,
    Resolved = 4,
    Extended = 5,
    BatchCreated = 6,
    BatchReleased = 7,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscrowEvent {
    pub escrow_id: u64,
    pub action: EscrowAction,
    pub buyer: Address,
    pub seller: Address,
    /// Monetary fields are emitted as raw integer types (i128/u64). Avoid
    /// converting integers to strings inside the contract — emit numeric
    /// values and perform human-friendly formatting off-chain (UI/indexer).
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ConfigValue {
    U32(u32),
    I128(i128),
    Address(Address),
    String(String),
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ConfigUpdatedEvent {
    pub field_name: Symbol,
    pub old_value: ConfigValue,
    pub new_value: ConfigValue,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ArtisanFeeTierUpdatedEvent {
    pub artisan: Address,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct TokensStakedEvent {
    pub artisan: Address,
    pub token: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct TokensUnstakedEvent {
    pub artisan: Address,
    pub token: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct MetadataVerifiedEvent {
    pub order_id: u64,
    pub verifier: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PlatformPausedEvent {
    pub initiator: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PlatformUnpausedEvent {
    pub initiator: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscrowMetadata {
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
}

/// Metadata reveal proof for privacy verification (Issue #122)
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct MetadataRevealProof {
    /// The full metadata content (off-chain document)
    pub content: Bytes,
    /// Optional secret key for additional verification
    pub secret: Option<Bytes>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct WasmUpgradeProposal {
    pub wasm_hash: BytesN<32>,
    pub upgrade_at: u64,
}

/// Parameters for batch escrow creation
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscrowCreateParams {
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub order_id: u32,
    pub release_window: Option<u32>,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
}

/// Policy for handling fees when a dispute expires without arbitrator resolution.
/// Determines whether the platform still collects a fee and from whom.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ExpiredDisputeFeePolicy {
    /// Refund buyer in full, platform collects no fee (default, buyer-friendly)
    RefundFullNoPlatformFee = 0,
    /// Refund buyer minus platform fee, platform collects fee from buyer
    RefundMinusPlatformFee = 1,
    /// Refund buyer in full, deduct platform fee from seller's locked amount
    /// (seller loses fee even though they didn't receive payment)
    DeductFeeFromSeller = 2,
    /// Split the platform fee: half from buyer's refund, half from seller
    SplitFee = 3,
}

/// Platform configuration data
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PlatformConfig {
    pub platform_fee_bps: u32,    // Platform fee in basis points (500 = 5%)
    pub platform_wallet: Address, // Wallet address to receive fees
    /// Admin address for management.
    /// This address can be a regular account or a Multisig contract address
    /// to enhance security for sensitive operations like `propose_upgrade_wasm` (#95).
    pub admin: Address,
    pub arbitrator: Address, // Arbitrator for dispute resolution
    pub moderator: Option<Address>,
    pub is_paused: bool,                // Circuit breaker (#96)
    pub min_stake_required: i128, // Minimum stake artisan must hold to create escrows (Issue #99)
    pub pending_admin: Option<Address>, // Pending admin for two-step transfer
    pub wasm_upgrade_cooldown: u32, // Grace period for WASM upgrades in seconds (default: 7 days)
    pub max_dispute_duration: u32, // Maximum duration a dispute can remain open in seconds (default: 30 days)
    pub stake_cooldown: u32, // Cooldown period after staking before tokens can be unstaked in seconds (default: 7 days)
    /// Policy for handling platform fees when disputes expire without arbitrator resolution
    pub expired_dispute_fee_policy: ExpiredDisputeFeePolicy,
    /// Minimum release window to prevent "flash" auto-releases (default: 1 day)
    pub min_release_window: u32,
}

/// Partial refund proposal created during a dispute (Issue #101)
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PartialRefundProposal {
    pub order_id: u32,
    pub refund_amount: i128,
    pub proposed_by: Address,
    pub proposed_at: u64,
}

/// Minimal cross-contract interface for the OnboardingContract.
/// Used by EscrowContract to update user reputation and activity metrics
/// when escrow state changes (release, refund, resolve).
#[soroban_sdk::contractclient(name = "OnboardingClient")]
pub trait OnboardingInterface {
    fn update_reputation(env: Env, address: Address, successful_delta: u32, disputed_delta: u32);
    fn update_user_metrics(
        env: Env,
        address: Address,
        escrow_count_delta: u32,
        volume_delta: i128,
        token_address: Address,
    );
}
#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
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
                if len < 50 || len > 100 {
                    return false;
                }
                // Logic check: CIDv1 base32 ALWAYS starts with 'ba'
                if cid_bytes[1] != b'a' {
                    return false;
                }
                payload
                    .iter()
                    .all(|b| matches!(*b, b'a'..=b'z' | b'2'..=b'7'))
            }
            // base16lower (hex)
            b'f' => {
                // CIDv1 base16 typically ~73 chars
                if len < 60 || len > 120 {
                    return false;
                }
                // Logic check: CIDv1 base16 ALWAYS starts with 'f01'
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

    fn validate_optional_ipfs_hash(env: &Env, ipfs_hash: &Option<String>) {
        if let Some(cid) = ipfs_hash {
            if !Self::validate_ipfs_cid(cid) {
                env.panic_with_error(crate::Error::InvalidIpfsHash);
            }
        }
    }

    fn validate_optional_metadata_hash(env: &Env, metadata_hash: &Option<Bytes>) {
        if let Some(hash) = metadata_hash {
            if hash.len() != 32 {
                env.panic_with_error(crate::Error::InvalidMetadataHash);
            }
        }
    }

    fn get_admin(env: &Env) -> Result<Address, Error> {
        let config: PlatformConfig = env
            .storage()
            .persistent()
            .get(&DataKey::PlatformConfig)
            .ok_or(Error::PlatformNotInitialized)?;
        Self::extend_persistent(env, &DataKey::PlatformConfig);
        Ok(config.admin)
    }

    fn emit_escrow_event(env: &Env, event: EscrowEvent) {
        env.events()
            .publish((Symbol::new(env, "escrow"), event.escrow_id), event);
    }

    fn emit_config_updated(
        env: &Env,
        field_name: &str,
        old_value: ConfigValue,
        new_value: ConfigValue,
    ) {
        env.events().publish(
            (
                Symbol::new(env, "config_updated"),
                Symbol::new(env, field_name),
            ),
            ConfigUpdatedEvent {
                field_name: Symbol::new(env, field_name),
                old_value,
                new_value,
            },
        );
    }

    fn emit_artisan_fee_tier_updated(env: &Env, artisan: Address, fee_bps: u32) {
        env.events().publish(
            (
                Symbol::new(env, "artisan_fee_tier_updated"),
                artisan.clone(),
            ),
            ArtisanFeeTierUpdatedEvent { artisan, fee_bps },
        );
    }

    fn emit_metadata_verified(env: &Env, order_id: u32, verifier: Address) {
        env.events().publish(
            (
                Symbol::new(env, "metadata_verified"),
                (order_id as u64),
            ),
            MetadataVerifiedEvent {
                order_id: order_id as u64,
                verifier,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    fn emit_platform_paused(env: &Env, initiator: Address) {
        env.events().publish(
            (
                Symbol::new(env, "platform_paused"),
                initiator.clone(),
            ),
            PlatformPausedEvent {
                initiator,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    fn emit_platform_unpaused(env: &Env, initiator: Address) {
        env.events().publish(
            (
                Symbol::new(env, "platform_unpaused"),
                initiator.clone(),
            ),
            PlatformUnpausedEvent {
                initiator,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    fn enter_reentry_guard(env: &Env) {
        if env.storage().temporary().has(&DataKey::ReentryGuard) {
            env.panic_with_error(crate::Error::ReentryDetected);
        }
        env.storage().temporary().set(&DataKey::ReentryGuard, &true);
    }

    fn exit_reentry_guard(env: &Env) {
        env.storage().temporary().remove(&DataKey::ReentryGuard);
    }

    // IMPORTANT: this validation is intentionally scoped to escrow creation-time
    // flows only. Do not call from payout/distribution paths, or dynamic minimum
    // changes could trap dust balances in existing escrows.
    fn check_min_amount(env: &Env, token: Address, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::AmountBelowMinimum);
        }

        let min_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MinEscrowAmount(token))
            .unwrap_or(0); // If not set, allow any positive amount

        if amount < min_amount {
            return Err(Error::AmountBelowMinimum);
        }

        Ok(())
    }

    fn update_active_obligations(env: &Env, user: &Address, delta: i32) {
        let key = DataKey::ActiveObligations(user.clone());
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_val = if delta > 0 {
            count.saturating_add(delta as u32)
        } else {
            count.saturating_sub((-delta) as u32)
        };
        env.storage().persistent().set(&key, &new_val);
        Self::extend_persistent(env, &key);
    }

    fn update_total_locked(env: &Env, token: &Address, delta: i128) {
        let key = DataKey::TotalLocked(token.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = current.saturating_add(delta);
        env.storage().persistent().set(&key, &new_total);
        Self::extend_persistent(env, &key);
    }

    fn update_total_staked(env: &Env, token: &Address, delta: i128) {
        let key = DataKey::TotalStaked(token.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = current.saturating_add(delta);
        env.storage().persistent().set(&key, &new_total);
        Self::extend_persistent(env, &key);
    }

    /// Extend the TTL of a persistent storage entry using standardized values.
    fn extend_persistent(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Returns the configured maximum release window (in seconds).
    /// Falls back to MAX_TOTAL_RELEASE_WINDOW (30 days) if not set by admin.
    fn get_max_release_window(env: &Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::MaxReleaseWindow)
            .unwrap_or(MAX_TOTAL_RELEASE_WINDOW)
    }

    /// Returns an OnboardingClient pointed at the registered onboarding contract,
    /// or None if no address has been configured via set_onboarding_contract.
    fn get_onboarding_client(env: &Env) -> Option<OnboardingClient<'_>> {
        env.storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::OnboardingContractAddress)
            .map(|addr| OnboardingClient::new(env, &addr))
    }

    /// Set the configurable maximum release window (admin only).
    ///
    /// # Arguments
    /// * `max_window` - Maximum allowed release window in seconds.
    ///   Must be > 0 and <= ABSOLUTE_MAX_RELEASE_WINDOW.
    pub fn set_max_release_window(env: Env, max_window: u32) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        if max_window == 0 {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }
        if max_window > ABSOLUTE_MAX_RELEASE_WINDOW {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxReleaseWindow, &max_window);
        Self::extend_persistent(&env, &DataKey::MaxReleaseWindow);
    }

    /// Set the minimum release window to prevent "flash" auto-releases (admin only).
    ///
    /// # Arguments
    /// * `min_window` - Minimum allowed release window in seconds
    ///
    /// # Panics
    /// - If min_window is 0
    /// - If min_window exceeds the current max_release_window
    pub fn set_min_release_window(env: Env, min_window: u32) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if min_window == 0 {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }

        let max_window = Self::get_max_release_window(&env);
        if min_window > max_window {
            return Err(Error::ReleaseWindowTooLong);
        }

        let old_min = config.min_release_window;
        config.min_release_window = min_window;

        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        Self::emit_config_updated(
            &env,
            "min_release_window",
            ConfigValue::U32(old_min),
            ConfigValue::U32(min_window),
        );

        Ok(())
    }

    /// Get the current minimum release window
    pub fn get_min_release_window(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.min_release_window
    }

    /// Register the deployed OnboardingContract address so the escrow contract
    /// can make cross-contract reputation / metrics updates (admin only).
    pub fn set_onboarding_contract(env: Env, contract_address: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::OnboardingContractAddress, &contract_address);
        Self::extend_persistent(&env, &DataKey::OnboardingContractAddress);
    }

    /// Add a token to the platform whitelist (admin only).
    ///
    /// Once at least one token is whitelisted, only whitelisted tokens may be
    /// used in escrow creation. The check is skipped when the whitelist is empty,
    /// preserving backward compatibility.
    pub fn whitelist_token(env: Env, token: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let mut whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or(Map::new(&env));
        whitelist.set(token, true);
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedTokens, &whitelist);
        Self::extend_persistent(&env, &DataKey::WhitelistedTokens);
    }

    /// Remove a token from the platform whitelist (admin only).
    ///
    /// If the resulting whitelist is empty, whitelist enforcement is automatically
    /// disabled (all tokens permitted again).
    pub fn remove_token_from_whitelist(env: Env, token: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let mut whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or(Map::new(&env));
        whitelist.remove(token);
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedTokens, &whitelist);
        Self::extend_persistent(&env, &DataKey::WhitelistedTokens);
    }

    /// Check whether a specific token is on the whitelist.
    ///
    /// Returns `true` if the token is explicitly whitelisted, OR if the whitelist
    /// is empty (enforcement not yet active).
    pub fn is_token_whitelisted(env: Env, token: Address) -> bool {
        let whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or(Map::new(&env));
        if whitelist.is_empty() {
            return true;
        }
        whitelist.get(token).unwrap_or(false)
    }

    /// Internal helper: panics with TokenNotWhitelisted when enforcement is active
    /// and the token is not on the whitelist.
    /// NOTE: whitelist enforcement is intentionally performed only during
    /// escrow creation (and related locking operations). State transitions
    /// such as `release`, `refund`, or recurring cycle releases MUST NOT
    /// re-check the whitelist to avoid locking funds for escrows created
    /// before whitelist changes. Keep this helper private and call it only
    /// in creation-time validation paths.
    fn check_token_whitelisted(env: &Env, token: &Address) {
        let whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or(Map::new(env));
        if whitelist.is_empty() {
            return;
        }
        if !whitelist.get(token.clone()).unwrap_or(false) {
            env.panic_with_error(crate::Error::TokenNotWhitelisted);
        }
    }

    pub fn initialize(
        env: Env,
        platform_wallet: Address,
        admin: Address,
        arbitrator: Address,
        platform_fee_bps: u32,
        onboarding_contract: Option<Address>,
    ) {
        admin.require_auth();

        // Validate fee is within bounds
        if platform_fee_bps > MAX_PLATFORM_FEE_BPS {
            env.panic_with_error(crate::Error::InvalidFee);
        }

        let config = PlatformConfig {
            platform_fee_bps,
            platform_wallet: platform_wallet.clone(),
            admin: admin.clone(),
            arbitrator: arbitrator.clone(),
            moderator: None,
            is_paused: false,
            min_stake_required: 0,
            pending_admin: None,
            wasm_upgrade_cooldown: DEFAULT_WASM_UPGRADE_COOLDOWN,
            max_dispute_duration: DEFAULT_MAX_DISPUTE_DURATION,
            stake_cooldown: DEFAULT_STAKE_COOLDOWN,
            expired_dispute_fee_policy: ExpiredDisputeFeePolicy::RefundFullNoPlatformFee,
            min_release_window: DEFAULT_MIN_RELEASE_WINDOW,
        };

        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        env.storage()
            .persistent()
            .set(&PLATFORM_WALLET, &platform_wallet);
        Self::extend_persistent(&env, &PLATFORM_WALLET);

        // Initialize total fees to 0
        let zero: i128 = 0;
        env.storage().persistent().set(&TOTAL_FEES, &zero);
        Self::extend_persistent(&env, &TOTAL_FEES);

        // Initialize contract version to 1
        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &1u32);
        Self::extend_persistent(&env, &DataKey::ContractVersion);

        // Set the onboarding contract address to enable reputation tracking (optional)
        if let Some(ref addr) = onboarding_contract {
            env.storage()
                .persistent()
                .set(&DataKey::OnboardingContractAddress, addr);
            Self::extend_persistent(&env, &DataKey::OnboardingContractAddress);
        }

        Self::emit_config_updated(
            &env,
            "platform_fee_bps",
            ConfigValue::String(String::from_str(&env, "unset")),
            ConfigValue::U32(platform_fee_bps),
        );
        Self::emit_config_updated(
            &env,
            "platform_wallet",
            ConfigValue::String(String::from_str(&env, "unset")),
            ConfigValue::Address(platform_wallet),
        );
        if let Some(addr) = onboarding_contract {
            Self::emit_config_updated(
                &env,
                "onboarding_contract",
                ConfigValue::String(String::from_str(&env, "unset")),
                ConfigValue::Address(addr),
            );
        }
    }

    /// Propose a new administrator for the platform (admin only).
    /// Starts the two-step transfer process (#95).
    pub fn update_admin(env: Env, new_admin: Address) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        config.pending_admin = Some(new_admin);
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
    }

    /// Claim the administrative role (pending admin only).
    /// Completes the two-step transfer process (#95).
    pub fn claim_admin(env: Env) {
        let mut config = Self::get_platform_config_internal(&env);
        let pending = config.pending_admin.as_ref().expect("");
        pending.require_auth();

        config.admin = pending.clone();
        config.pending_admin = None;

        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
    }

    /// Cancel an in-progress two-step admin transfer (current admin only).
    pub fn cancel_admin_transfer(env: Env) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if config.pending_admin.is_none() {
            return Err(Error::NoPendingAdmin);
        }

        config.pending_admin = None;
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
        Ok(())
    }

    /// Migrate a user's escrow list from legacy vector storage to indexed storage.
    /// This is a one-time migration function that should be called for users who have
    /// escrows stored in the old format. Admin only.
    ///
    /// # Arguments
    /// * `user` - Address of the user to migrate
    /// * `is_buyer` - true to migrate buyer escrows, false to migrate seller escrows
    ///
    /// # Returns
    /// Number of escrows migrated
    pub fn migrate_user_escrows(env: Env, user: Address, is_buyer: bool) -> Result<u32, Error> {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let legacy_key = if is_buyer {
            DataKey::BuyerEscrows(user.clone())
        } else {
            DataKey::SellerEscrows(user.clone())
        };

        // Check if legacy data exists
        if !env.storage().persistent().has(&legacy_key) {
            return Ok(0);
        }

        let legacy_escrows: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let count = legacy_escrows.len();

        // Migrate to indexed storage
        for i in 0..count {
            if let Some(escrow_id) = legacy_escrows.get(i) {
                let index_key = if is_buyer {
                    DataKey::BuyerEscrowIndexed(user.clone(), i)
                } else {
                    DataKey::SellerEscrowIndexed(user.clone(), i)
                };
                env.storage().persistent().set(&index_key, &escrow_id);
                Self::extend_persistent(&env, &index_key);
            }
        }

        // Set the count
        let count_key = if is_buyer {
            DataKey::BuyerEscrowCount(user.clone())
        } else {
            DataKey::SellerEscrowCount(user.clone())
        };
        env.storage().persistent().set(&count_key, &count);
        Self::extend_persistent(&env, &count_key);

        // Remove legacy storage to free up space
        env.storage().persistent().remove(&legacy_key);

        Ok(count)
    }

    /// Create a new escrow for an order
    ///
    /// # Arguments
    /// * `buyer` - Address of the buyer
    /// * `seller` - Address of the seller
    /// * `token` - Token contract address (USDC)
    /// * `amount` - Amount to escrow
    /// * `order_id` - Unique order identifier
    /// * `release_window` - Time in seconds before auto-release (default 7 days = 604800)
    pub fn create_escrow(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: u32,
        release_window: Option<u32>,
    ) -> Escrow {
        Self::create_escrow_with_metadata(
            env,
            buyer,
            seller,
            token,
            amount,
            order_id,
            release_window,
            None,
            None,
        )
    }

    /// Create a new escrow for an order and attach off-chain metadata.
    pub fn create_escrow_with_metadata(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: u32,
        release_window: Option<u32>,
        ipfs_hash: Option<String>,
        metadata_hash: Option<Bytes>,
    ) -> Escrow {
        Self::enter_reentry_guard(&env);
        Self::check_not_paused(&env);
        buyer.require_auth();

        // Validate amount is positive and above minimum
        if let Err(e) = Self::check_min_amount(&env, token.clone(), amount) {
            env.panic_with_error(e);
        }

        // Validate buyer and seller are different
        if buyer == seller {
            env.panic_with_error(crate::Error::SameBuyerSeller);
        }

        // Validate token is whitelisted (#103)
        Self::check_token_whitelisted(&env, &token);

        // Check artisan (seller) stake requirement (Issue #99)
        let config = Self::get_platform_config_internal(&env);
        if config.min_stake_required > 0 {
            let artisan_stake: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::ArtisanStake(seller.clone()))
                .map(|stake: ArtisanStakeData| stake.amount)
                .unwrap_or(0);
            if artisan_stake < config.min_stake_required {
                env.panic_with_error(crate::Error::InsufficientStake);
            }
        }

        // Default to 7 days if not specified
        let window = release_window.unwrap_or(604800u32);

        // Validate release window bounds
        let config = Self::get_platform_config_internal(&env);
        let min_window = config.min_release_window;
        let max_window = Self::get_max_release_window(&env);
        
        if window < min_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }
        if window > max_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }

        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;
        Self::validate_optional_ipfs_hash(&env, &ipfs_hash);
        Self::validate_optional_metadata_hash(&env, &metadata_hash);

        let escrow = Escrow {
            version: CURRENT_ESCROW_VERSION,
            id: order_id as u64,
            batch_id: None,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            status: EscrowStatus::Active,
            release_window: window,
            created_at,
            ipfs_hash: ipfs_hash.clone(),
            metadata_hash: metadata_hash.clone(),
            dispute_reason: None,
            dispute_initiated_at: None,
            funded: true,
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        // Track active escrows
        Self::update_active_obligations(&env, &buyer, 1);
        Self::update_active_obligations(&env, &seller, 1);

        // Update global escrow index for off-chain enumeration
        let ids_key = DataKey::AllEscrowIds;
        let mut all_ids: soroban_sdk::Vec<u32> = env
            .storage()
            .persistent()
            .get(&ids_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        all_ids.push_back(order_id);
        env.storage().persistent().set(&ids_key, &all_ids);
        Self::extend_persistent(&env, &ids_key);

        let count_key = DataKey::EscrowCount;
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0u32);
        env.storage().persistent().set(&count_key, &(count + 1));
        Self::extend_persistent(&env, &count_key);

        // Update buyer's escrow list using indexed storage (scalable approach)
        let buyer_count_key = DataKey::BuyerEscrowCount(buyer.clone());
        let buyer_count: u32 = env
            .storage()
            .persistent()
            .get(&buyer_count_key)
            .unwrap_or(0u32);
        let buyer_index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), buyer_count);
        env.storage()
            .persistent()
            .set(&buyer_index_key, &(order_id as u64));
        Self::extend_persistent(&env, &buyer_index_key);
        env.storage()
            .persistent()
            .set(&buyer_count_key, &(buyer_count + 1));
        Self::extend_persistent(&env, &buyer_count_key);

        // Update seller's escrow list using indexed storage (scalable approach)
        let seller_count_key = DataKey::SellerEscrowCount(seller.clone());
        let seller_count: u32 = env
            .storage()
            .persistent()
            .get(&seller_count_key)
            .unwrap_or(0u32);
        let seller_index_key = DataKey::SellerEscrowIndexed(seller.clone(), seller_count);
        env.storage()
            .persistent()
            .set(&seller_index_key, &(order_id as u64));
        Self::extend_persistent(&env, &seller_index_key);
        env.storage()
            .persistent()
            .set(&seller_count_key, &(seller_count + 1));
        Self::extend_persistent(&env, &seller_count_key);

        // Track active escrows for both parties
        Self::update_active_obligations(&env, &buyer, 1);
        Self::update_active_obligations(&env, &seller, 1);

        // Transfer funds from buyer to contract
        let client = token::Client::new(&env, &token);
        client.transfer(&buyer, &env.current_contract_address(), &amount);

        // Track locked funds (#212)
        Self::update_total_locked(&env, &token, amount);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Created,
                buyer: buyer.clone(),
                seller: seller.clone(),
                amount,
                token: token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Self::exit_reentry_guard(&env);
        escrow
    }

    /// Create an escrow without funding it immediately (#213).
    /// The buyer must call `fund_escrow` later to activate it.
    pub fn create_unfunded_escrow(
        env: Env,
        order_id: u32,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        window: u32,
        ipfs_hash: Option<String>,
        metadata_hash: Option<Bytes>,
    ) -> Escrow {
        Self::enter_reentry_guard(&env);

        // Validate release window bounds
        let config = Self::get_platform_config_internal(&env);
        let min_window = config.min_release_window;
        let max_window = Self::get_max_release_window(&env);
        
        if window < min_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }
        if window > max_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }

        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;
        Self::validate_optional_ipfs_hash(&env, &ipfs_hash);
        Self::validate_optional_metadata_hash(&env, &metadata_hash);

        let escrow = Escrow {
            version: CURRENT_ESCROW_VERSION,
            id: order_id as u64,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            status: EscrowStatus::Active,
            release_window: window,
            created_at,
            ipfs_hash: ipfs_hash.clone(),
            metadata_hash: metadata_hash.clone(),
            dispute_reason: None,
            dispute_initiated_at: None,
            funded: false,
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        // Update buyer's escrow list
        let buyer_count_key = DataKey::BuyerEscrowCount(buyer.clone());
        let buyer_count: u32 = env.storage().persistent().get(&buyer_count_key).unwrap_or(0u32);
        let buyer_index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), buyer_count);
        env.storage().persistent().set(&buyer_index_key, &(order_id as u64));
        Self::extend_persistent(&env, &buyer_index_key);
        env.storage().persistent().set(&buyer_count_key, &(buyer_count + 1));
        Self::extend_persistent(&env, &buyer_count_key);

        // Update seller's escrow list
        let seller_count_key = DataKey::SellerEscrowCount(seller.clone());
        let seller_count: u32 = env.storage().persistent().get(&seller_count_key).unwrap_or(0u32);
        let seller_index_key = DataKey::SellerEscrowIndexed(seller.clone(), seller_count);
        env.storage().persistent().set(&seller_index_key, &(order_id as u64));
        Self::extend_persistent(&env, &seller_index_key);
        env.storage().persistent().set(&seller_count_key, &(seller_count + 1));
        Self::extend_persistent(&env, &seller_count_key);

        // Track active escrows (unfunded still count towards active limit to prevent spam)
        Self::update_active_obligations(&env, &buyer, 1);
        Self::update_active_obligations(&env, &seller, 1);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Created,
                buyer: buyer.clone(),
                seller: seller.clone(),
                amount,
                token: token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Self::exit_reentry_guard(&env);
        escrow
    }
    pub fn fund_escrow(env: Env, order_id: u32) -> Result<(), Error> {
        Self::enter_reentry_guard(&env);
        let mut escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.funded {
            return Err(Error::InvalidEscrowState);
        }
        
        escrow.buyer.require_auth();
        
        let client = token::Client::new(&env, &escrow.token);
        client.transfer(&escrow.buyer, &env.current_contract_address(), &escrow.amount);
        
        escrow.funded = true;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));
        
        // Track locked funds (#212)
        Self::update_total_locked(&env, &escrow.token, escrow.amount);
        
        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Created, // Re-emit as created/funded
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        
        Self::exit_reentry_guard(&env);
        Ok(())
    }

    /// Cancel an escrow that has not been funded within the timeout period (#213).
    pub fn cancel_unfunded_escrow(env: Env, order_id: u32) -> Result<(), Error> {
        Self::enter_reentry_guard(&env);
        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.funded {
            return Err(Error::InvalidEscrowState);
        }
        
        let current_time = env.ledger().timestamp();
        if (escrow.created_at as u64) + UNFUNDED_CANCEL_TIMEOUT > current_time {
            return Err(Error::ReleaseWindowNotElapsed);
        }
        
        // Cleanup state
        env.storage().persistent().remove(&(ESCROW, order_id));
        
        // Decrement active obligations
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);
        
        Self::exit_reentry_guard(&env);
        Ok(())
    }

    /// Get escrows for a specific buyer with pagination.
    /// Uses indexed storage for scalability, with fallback to legacy vector storage.
    pub fn get_escrows_by_buyer(
        env: Env,
        buyer: Address,
        page: u32,
        limit: u32,
        reverse: bool,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let mut result = soroban_sdk::Vec::new(&env);

        // Try new indexed storage first
        let count_key = DataKey::BuyerEscrowCount(buyer.clone());
        if env.storage().persistent().has(&count_key) {
            let total_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0u32);
            let start = page * limit;

            if start >= total_count {
                return Ok(result);
            }

            let end = (start + limit).min(total_count);

            for position in start..end {
                let storage_index = if reverse {
                    total_count - 1 - position
                } else {
                    position
                };
                let index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), storage_index);
                if let Some(escrow_id) = env.storage().persistent().get::<_, u64>(&index_key) {
                    result.push_back(escrow_id);
                    env.storage().persistent().extend_ttl(&index_key, 1000, 518400);
                }
            }

            env.storage().persistent().extend_ttl(&count_key, 1000, 518400);
            return Ok(result);
        }

        // Fallback to legacy vector storage for backward compatibility
        let legacy_key = DataKey::BuyerEscrows(buyer);
        let escrow_ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        
        if env.storage().persistent().has(&legacy_key) {
            env.storage().persistent().extend_ttl(&legacy_key, 1000, 518400);
        }

        let start = page * limit;
        let len = escrow_ids.len();

        if start >= len {
            return Ok(result);
        }

        let end = (start + limit).min(len);
        if reverse {
            for position in start..end {
                if let Some(escrow_id) = escrow_ids.get(len - 1 - position) {
                    result.push_back(escrow_id);
                }
            }
            Ok(result)
        } else {
            Ok(escrow_ids.slice(start..end))
        }
    }

    /// Get escrows for a specific seller with pagination.
    /// Uses indexed storage for scalability, with fallback to legacy vector storage.
    pub fn get_escrows_by_seller(
        env: Env,
        seller: Address,
        page: u32,
        limit: u32,
        reverse: bool,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let mut result = soroban_sdk::Vec::new(&env);

        // Try new indexed storage first
        let count_key = DataKey::SellerEscrowCount(seller.clone());
        if env.storage().persistent().has(&count_key) {
            let total_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0u32);
            let start = page * limit;

            if start >= total_count {
                return Ok(result);
            }

            let end = (start + limit).min(total_count);

            for position in start..end {
                let storage_index = if reverse {
                    total_count - 1 - position
                } else {
                    position
                };
                let index_key = DataKey::SellerEscrowIndexed(seller.clone(), storage_index);
                if let Some(escrow_id) = env.storage().persistent().get::<_, u64>(&index_key) {
                    result.push_back(escrow_id);
                    env.storage().persistent().extend_ttl(&index_key, 1000, 518400);
                }
            }

            env.storage().persistent().extend_ttl(&count_key, 1000, 518400);
            return Ok(result);
        }

        // Fallback to legacy vector storage for backward compatibility
        let legacy_key = DataKey::SellerEscrows(seller);
        let escrow_ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        
        if env.storage().persistent().has(&legacy_key) {
            env.storage().persistent().extend_ttl(&legacy_key, 1000, 518400);
        }

        let start = page * limit;
        let len = escrow_ids.len();

        if start >= len {
            return Ok(result);
        }

        let end = (start + limit).min(len);
        if reverse {
            for position in start..end {
                if let Some(escrow_id) = escrow_ids.get(len - 1 - position) {
                    result.push_back(escrow_id);
                }
            }
            Ok(result)
        } else {
            Ok(escrow_ids.slice(start..end))
        }
    }

    /// Get platform configuration
    pub fn get_platform_config(env: Env) -> PlatformConfig {
        Self::get_platform_config_internal(&env)
    }

    fn get_platform_config_internal(env: &Env) -> PlatformConfig {
        env.storage()
            .persistent()
            .get(&DataKey::PlatformConfig)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::PlatformNotInitialized))
    }

    fn try_get_escrow_readonly(env: &Env, order_id: u32) -> Escrow {
        let key = (ESCROW, order_id);
        let stored: Val = env.storage().persistent().get(&key).unwrap_or_else(|| env.panic_with_error(crate::Error::EscrowNotFound));
        let map = Map::<Symbol, Val>::try_from_val(env, &stored).expect("");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let batch_id_key = Symbol::new(env, "batch_id");
            if map.contains_key(batch_id_key) {
                let mut escrow = Escrow::try_from_val(env, &stored).expect("");
                if escrow.version < CURRENT_ESCROW_VERSION {
                    escrow.version = CURRENT_ESCROW_VERSION;
                }
                return escrow;
            }

            let previous = EscrowWithoutBatch::try_from_val(env, &stored).expect("");
            let mut escrow = Self::escrow_from_without_batch(previous);
            if escrow.version < CURRENT_ESCROW_VERSION {
                escrow.version = CURRENT_ESCROW_VERSION;
            }
            return escrow;
        }

        let legacy = LegacyEscrow::try_from_val(env, &stored).expect("");
        Escrow {
            version: CURRENT_ESCROW_VERSION,
            id: legacy.id,
            batch_id: None,
            buyer: legacy.buyer,
            seller: legacy.seller,
            token: legacy.token,
            amount: legacy.amount,
            status: legacy.status,
            release_window: legacy.release_window,
            created_at: legacy.created_at,
            ipfs_hash: legacy.ipfs_hash,
            metadata_hash: legacy.metadata_hash,
            dispute_reason: legacy.dispute_reason,
            dispute_initiated_at: legacy.dispute_initiated_at,
            funded: true,
        }
    }

    fn get_stored_escrow(env: &Env, order_id: u32) -> Escrow {
        let key = (ESCROW, order_id);
        let stored: Val = env.storage().persistent().get(&key).unwrap_or_else(|| env.panic_with_error(crate::Error::EscrowNotFound));
        let map = Map::<Symbol, Val>::try_from_val(env, &stored).expect("");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let batch_id_key = Symbol::new(env, "batch_id");
            let escrow = if map.contains_key(batch_id_key) {
                Escrow::try_from_val(env, &stored).expect("")
            } else {
                let previous = EscrowWithoutBatch::try_from_val(env, &stored).expect("");
                Self::escrow_from_without_batch(previous)
            };
            if escrow.version < CURRENT_ESCROW_VERSION {
                return Self::upgrade_escrow(env, order_id, escrow);
            }
            Self::extend_persistent(env, &key);
            return escrow;
        }

        let legacy = LegacyEscrow::try_from_val(env, &stored).expect("");
        let upgraded = Escrow {
            version: CURRENT_ESCROW_VERSION,
            id: legacy.id,
            batch_id: None,
            buyer: legacy.buyer,
            seller: legacy.seller,
            token: legacy.token,
            amount: legacy.amount,
            status: legacy.status,
            release_window: legacy.release_window,
            created_at: legacy.created_at,
            ipfs_hash: legacy.ipfs_hash,
            metadata_hash: legacy.metadata_hash,
            dispute_reason: legacy.dispute_reason,
            dispute_initiated_at: legacy.dispute_initiated_at,
            funded: true,
        };
        env.storage().persistent().set(&key, &upgraded);
        Self::extend_persistent(env, &key);
        upgraded
    }

    fn upgrade_escrow(env: &Env, order_id: u32, mut escrow: Escrow) -> Escrow {
        if escrow.version < 3 {
            escrow.funded = true;
        }
        escrow.version = CURRENT_ESCROW_VERSION;
        let key = (ESCROW, order_id);
        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(env, &key);
        escrow
    }

    fn escrow_from_without_batch(escrow: EscrowWithoutBatch) -> Escrow {
        Escrow {
            version: escrow.version,
            id: escrow.id,
            batch_id: None,
            buyer: escrow.buyer,
            seller: escrow.seller,
            token: escrow.token,
            amount: escrow.amount,
            status: escrow.status,
            release_window: escrow.release_window,
            created_at: escrow.created_at,
            ipfs_hash: escrow.ipfs_hash,
            metadata_hash: escrow.metadata_hash,
            dispute_reason: escrow.dispute_reason,
            dispute_initiated_at: escrow.dispute_initiated_at,
        }
    }

    /// Calculate platform fee for a given amount
    fn calculate_fee(amount: i128, fee_bps: u32) -> i128 {
        (amount * (fee_bps as i128)) / 10000
    }

    fn add_fee_token_to_index(env: &Env, token: &Address) {
        let key = DataKey::FeeTokenIndex;
        let mut tracked_tokens: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        for index in 0..tracked_tokens.len() {
            if tracked_tokens.get(index) == Some(token.clone()) {
                Self::extend_persistent(env, &key);
                return;
            }
        }

        tracked_tokens.push_back(token.clone());
        env.storage().persistent().set(&key, &tracked_tokens);
        Self::extend_persistent(env, &key);
    }

    fn record_total_fees(env: &Env, token: &Address, fee_amount: i128) {
        if fee_amount <= 0 {
            return;
        }

        let key = DataKey::TotalFees(token.clone());
        let current_total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &(current_total + fee_amount));
        Self::extend_persistent(env, &key);
        Self::add_fee_token_to_index(env, token);
    }

    fn transfer_platform_fee(
        env: &Env,
        token: &Address,
        platform_wallet: &Address,
        fee_amount: i128,
    ) {
        if fee_amount <= 0 {
            return;
        }

        let token_client = token::Client::new(env, token);
        token_client.transfer(
            &env.current_contract_address(),
            platform_wallet,
            &fee_amount,
        );
        Self::record_total_fees(env, token, fee_amount);
    }

    fn get_legacy_total_fees(env: &Env) -> i128 {
        env.storage().persistent().get(&TOTAL_FEES).unwrap_or(0)
    }

    fn get_all_tracked_total_fees(env: &Env) -> i128 {
        let key = DataKey::FeeTokenIndex;
        let tracked_tokens: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        if tracked_tokens.is_empty() {
            return Self::get_legacy_total_fees(env);
        }

        let mut total_fees = 0i128;
        for index in 0..tracked_tokens.len() {
            if let Some(token) = tracked_tokens.get(index) {
                let token_key = DataKey::TotalFees(token);
                let token_total: i128 = env.storage().persistent().get(&token_key).unwrap_or(0);
                total_fees += token_total;
            }
        }

        total_fees
    }

    /// Release funds to seller with platform fee deduction
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn release_funds(env: Env, order_id: u32) {
        Self::enter_reentry_guard(&env);
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            env.panic_with_error(crate::Error::EscrowNotFound);
        }
        Self::extend_persistent(&env, &(ESCROW, order_id));
        let mut escrow: Escrow = escrow_opt.unwrap();

        // Only buyer can release funds
        escrow.buyer.require_auth();

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        // Get platform config
        let config = Self::get_platform_config_internal(&env);

        // Calculate platform fee using effective fee bps for the seller
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let fee_amount = Self::calculate_fee(escrow.amount, fee_bps);
        let seller_amount = escrow.amount - fee_amount;

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        // Transfer platform fee to platform wallet
        if fee_amount > 0 {
            Self::transfer_platform_fee(&env, &escrow.token, &config.platform_wallet, fee_amount);
        }

        // Transfer remaining funds to seller
        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &seller_amount,
        );

        // Track locked funds (#212)
        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Released,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::exit_reentry_guard(&env);

        // Update reputation and activity metrics in onboarding contract (#100, #63)
        if let Some(client) = Self::get_onboarding_client(&env) {
            client.update_reputation(&escrow.seller, &1u32, &0u32);
            client.update_reputation(&escrow.buyer, &1u32, &0u32);
            client.update_user_metrics(&escrow.seller, &1u32, &escrow.amount, &escrow.token);
        }
    }

    /// Auto-release funds after release window (seller can call)
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn auto_release(env: Env, order_id: u32) {
        Self::enter_reentry_guard(&env);
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            env.panic_with_error(crate::Error::EscrowNotFound);
        }
        Self::extend_persistent(&env, &(ESCROW, order_id));
        let mut escrow: Escrow = escrow_opt.unwrap();

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - (escrow.created_at as u64);

        if elapsed < escrow.release_window as u64 {
            env.panic_with_error(crate::Error::ReleaseWindowNotElapsed);
        }

        // Get platform config
        let config = Self::get_platform_config_internal(&env);

        // Calculate platform fee
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let fee_amount = Self::calculate_fee(escrow.amount, fee_bps);
        let seller_amount = escrow.amount - fee_amount;

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        // Transfer platform fee to platform wallet
        if fee_amount > 0 {
            Self::transfer_platform_fee(&env, &escrow.token, &config.platform_wallet, fee_amount);
        }

        // Transfer remaining funds to seller
        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &seller_amount,
        );

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Released,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::exit_reentry_guard(&env);

        // Update reputation and activity metrics in onboarding contract (#100, #63)
        if let Some(client) = Self::get_onboarding_client(&env) {
            client.update_reputation(&escrow.seller, &1u32, &0u32);
            client.update_reputation(&escrow.buyer, &1u32, &0u32);
            client.update_user_metrics(&escrow.seller, &1u32, &escrow.amount, &escrow.token);
        }
    }

    /// Extend the release window for an escrow (only buyer can call)
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    /// * `additional_seconds` - Time in seconds to add to the release window
    pub fn extend_release_window(env: Env, order_id: u32, additional_seconds: u32) {
        Self::enter_reentry_guard(&env);
        let escrow_key = (ESCROW, order_id);
        let escrow_opt = env.storage().persistent().get(&escrow_key);

        if escrow_opt.is_none() {
            env.panic_with_error(crate::Error::EscrowNotFound);
        }

        Self::extend_persistent(&env, &escrow_key);
        let mut escrow: Escrow = escrow_opt.unwrap();

        // Only buyer can extend release window
        escrow.buyer.require_auth();

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let new_window = escrow.release_window.saturating_add(additional_seconds);

        if new_window > MAX_TOTAL_RELEASE_WINDOW {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }

        escrow.release_window = new_window;
        env.storage().persistent().set(&escrow_key, &escrow);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Extended,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Self::exit_reentry_guard(&env);
    }

    /// Propose a new WASM code for the contract (admin only).
    /// Sets a configurable grace period before the upgrade can be executed (#95).
    pub fn propose_upgrade_wasm(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let config = Self::get_platform_config_internal(&env);
        let upgrade_at = env.ledger().timestamp() + config.wasm_upgrade_cooldown as u64;
        let proposal = WasmUpgradeProposal {
            wasm_hash: new_wasm_hash,
            upgrade_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::WasmUpgradeProposal, &proposal);
        Self::extend_persistent(&env, &DataKey::WasmUpgradeProposal);

        Ok(())
    }

    /// Upgrade the contract's WASM code after the grace period has elapsed.
    /// Only the admin can execute the final upgrade once the proposal is ready (#137).
    pub fn execute_upgrade(env: Env) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let proposal: WasmUpgradeProposal = env
            .storage()
            .persistent()
            .get(&DataKey::WasmUpgradeProposal)
            .ok_or(Error::NoUpgradeProposed)?;

        if env.ledger().timestamp() < proposal.upgrade_at {
            return Err(Error::UpgradeCooldownActive);
        }

        env.deployer()
            .update_current_contract_wasm(proposal.wasm_hash);

        // Update version in storage
        let current_version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &(current_version + 1));
        Self::extend_persistent(&env, &DataKey::ContractVersion);

        // Clear proposal
        env.storage()
            .persistent()
            .remove(&DataKey::WasmUpgradeProposal);

        Ok(())
    }

    /// Cancel a proposed WASM upgrade (admin only) (#95).
    pub fn cancel_upgrade_wasm(env: Env) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .remove(&DataKey::WasmUpgradeProposal);

        Ok(())
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0)
    }

    /// Refund funds to buyer (admin only)
    ///
    /// # Arguments
    /// * `escrow_id` - Escrow/Order identifier
    pub fn refund(env: Env, escrow_id: u64) -> Result<(), Error> {
        Self::enter_reentry_guard(&env);
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let order_id = escrow_id as u32;
        let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        let mut escrow: Escrow = escrow_opt.unwrap();

        if escrow.status != EscrowStatus::Active {
            return Err(Error::InvalidEscrowState);
        }

        // Update status
        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        // Refund to buyer
        let client = token::Client::new(&env, &escrow.token);
        client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        // Track locked funds (#212)
        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id,
                action: EscrowAction::Refunded,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::exit_reentry_guard(&env);

        // Buyer wins refund (successful for buyer, disputed for seller) (#100, #63)
        if let Some(client) = Self::get_onboarding_client(&env) {
            client.update_reputation(&escrow.buyer, &1u32, &0u32);
            client.update_reputation(&escrow.seller, &0u32, &1u32);
            client.update_user_metrics(&escrow.seller, &1u32, &escrow.amount, &escrow.token);
        }
        Ok(())
    }

    fn release_funds_to_seller(env: &Env, escrow: &Escrow) {
        let config = Self::get_platform_config_internal(env);
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let fee_amount = Self::calculate_fee(escrow.amount, fee_bps);
        let seller_amount = escrow.amount - fee_amount;

        let token_client = token::Client::new(env, &escrow.token);
        if fee_amount > 0 {
            Self::transfer_platform_fee(env, &escrow.token, &config.platform_wallet, fee_amount);
        }

        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &seller_amount,
        );

        // Track locked funds (#212)
        Self::update_total_locked(env, &escrow.token, -escrow.amount);
    }

    fn refund_funds_to_buyer(env: &Env, escrow: &Escrow) {
        let token_client = token::Client::new(env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        // Track locked funds (#212)
        Self::update_total_locked(env, &escrow.token, -escrow.amount);
    }

    /// Get escrow details
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn get_escrow(env: Env, order_id: u32) -> Escrow {
        Self::get_stored_escrow(&env, order_id)
    }

    /// Get escrow metadata fields only.
    pub fn get_escrow_metadata(env: Env, order_id: u32) -> EscrowMetadata {
        let escrow = Self::get_escrow(env, order_id);
        EscrowMetadata {
            ipfs_hash: escrow.ipfs_hash,
            metadata_hash: escrow.metadata_hash,
        }
    }

    /// Verify that provided metadata matches the stored hash (Issue #122)
    ///
    /// This function allows parties to reveal off-chain metadata and verify it matches
    /// the commitment stored on-chain. Uses SHA-256 hashing for verification.
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    /// * `proof` - MetadataRevealProof containing the full content and optional secret
    ///
    /// # Returns
    /// true if the provided content hashes to the stored metadata_hash, false otherwise
    ///
    /// # Notes
    /// - The metadata_hash must be set on the escrow for verification to succeed
    /// - The secret field is optional and can be used for additional application-level verification
    /// - This function does NOT modify state; it only verifies the commitment
    pub fn verify_metadata_reveal(
        env: Env,
        order_id: u32,
        proof: MetadataRevealProof,
        authorized_address: Address,
    ) -> bool {
        authorized_address.require_auth();

        let escrow = Self::get_escrow(env.clone(), order_id);
        let config = Self::get_platform_config_internal(&env);

        let is_authorized = authorized_address == escrow.buyer
            || authorized_address == escrow.seller
            || authorized_address == config.arbitrator;
        if !is_authorized {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        // If no metadata hash was set, verification fails
        if escrow.metadata_hash.is_none() {
            return false;
        }

        let stored_hash = escrow.metadata_hash.unwrap();

        // Compute SHA-256 hash of the provided content
        let computed_hash = env.crypto().sha256(&proof.content);

        // Convert Hash to Bytes by creating a new Bytes from the hash
        // Hash implements Into<Bytes> in Soroban SDK
        let computed_bytes: Bytes = computed_hash.into();

        // Compare hashes
        computed_bytes == stored_hash
    }

    /// Authorized verification that records successful metadata matching on-chain.
    ///
    /// Only the escrow buyer, seller, or admin may call this method. A successful verification
    /// emits a permanent MetadataVerified event.
    pub fn verify_metadata_reveal_recorded(
        env: Env,
        order_id: u32,
        proof: MetadataRevealProof,
        authorized_address: Address,
    ) -> bool {
        authorized_address.require_auth();

        let escrow = Self::get_escrow(env.clone(), order_id);
        let config = Self::get_platform_config_internal(&env);
        let is_authorized = authorized_address == escrow.buyer
            || authorized_address == escrow.seller
            || authorized_address == config.arbitrator;
        if !is_authorized {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let is_valid = Self::verify_metadata_reveal(env.clone(), order_id, proof, authorized_address.clone());
        if is_valid {
            Self::emit_metadata_verified(&env, order_id, authorized_address);
        }
        is_valid
    }

    /// Check if escrow can be auto-released
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn can_auto_release(env: Env, order_id: u32) -> bool {
        let escrow = Self::try_get_escrow_readonly(&env, order_id);

        if escrow.status != EscrowStatus::Active {
            return false;
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - (escrow.created_at as u64);

        elapsed >= escrow.release_window as u64
    }

    /// Dispute an escrow
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    /// * `dispute_reason` - Reason for dispute
    /// * `authorized_address` - Address authorized to dispute (buyer or seller)
    pub fn dispute_escrow(
        env: Env,
        order_id: u32,
        dispute_reason: String,
        authorized_address: Address,
    ) {
        authorized_address.require_auth();

        let mut escrow = Self::get_stored_escrow(&env, order_id);

        // Allow buyer or seller to dispute
        if !(escrow.buyer == authorized_address || escrow.seller == authorized_address) {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        if !(escrow.status == EscrowStatus::Active) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_reason = Some(dispute_reason.clone());
        escrow.dispute_initiated_at = Some(env.ledger().timestamp());
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Disputed,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Resolve disputed escrow (arbitrator only).
    ///
    /// This function transitions the escrow from `Disputed` to `Resolved`.
    /// Depending on the `resolution` choice:
    /// - `ReleaseToSeller`: Funds are sent to the seller minus the platform fee.
    /// - `RefundToBuyer`: Full original amount is returned to the buyer.
    ///
    /// # Edge Cases
    /// - **Refund Failure**: If the transfer to the buyer fails (e.g. account revoked),
    ///   the entire transaction reverts due to Stellar's atomicity.
    ///   The escrow remains in `Disputed` state for re-investigation.
    /// - **State Logic**: Can ONLY be called if `status` is currently `Disputed`.
    pub fn resolve_dispute(
        env: Env,
        order_id: u32,
        resolution: Resolution,
        authorized_address: Address,
    ) {
        Self::enter_reentry_guard(&env);
        let config = Self::get_platform_config_internal(&env);
        authorized_address.require_auth();
        let is_authorized = authorized_address == config.admin
            || Some(authorized_address.clone()) == config.moderator
            || authorized_address == config.arbitrator;
        if !is_authorized {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let mut escrow = Self::get_stored_escrow(&env, order_id);

        if escrow.status != EscrowStatus::Disputed {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        // CRITICAL: Update status BEFORE external calls (CEI pattern)
        escrow.status = EscrowStatus::Resolved;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        // Now perform token transfers (external calls)
        match resolution {
            Resolution::ReleaseToSeller => {
                Self::release_funds_to_seller(&env, &escrow);
            }
            Resolution::RefundToBuyer => {
                Self::refund_funds_to_buyer(&env, &escrow);
            }
        }

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Resolved,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::exit_reentry_guard(&env);

        // Update reputation based on resolution outcome (#100, #63)
        if let Some(client) = Self::get_onboarding_client(&env) {
            match resolution {
                Resolution::ReleaseToSeller => {
                    // Seller wins dispute: successful for seller, disputed for buyer
                    client.update_reputation(&escrow.seller, &1u32, &0u32);
                    client.update_reputation(&escrow.buyer, &0u32, &1u32);
                    client.update_user_metrics(
                        &escrow.seller,
                        &1u32,
                        &escrow.amount,
                        &escrow.token,
                    );
                }
                Resolution::RefundToBuyer => {
                    // Buyer wins dispute: successful for buyer, disputed for seller
                    client.update_reputation(&escrow.buyer, &1u32, &0u32);
                    client.update_reputation(&escrow.seller, &0u32, &1u32);
                    client.update_user_metrics(
                        &escrow.seller,
                        &1u32,
                        &escrow.amount,
                        &escrow.token,
                    );
                }
            }
        }
    }

    /// Update platform fee percentage (admin only)
    ///
    /// # Arguments
    /// * `new_fee_bps` - New fee in basis points
    pub fn update_platform_fee(env: Env, new_fee_bps: u32) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if new_fee_bps > MAX_PLATFORM_FEE_BPS {
            env.panic_with_error(crate::Error::InvalidFee);
        }

        let new_config = PlatformConfig {
            platform_fee_bps: new_fee_bps,
            platform_wallet: config.platform_wallet,
            admin: config.admin,
            arbitrator: config.arbitrator,
            moderator: config.moderator,
            is_paused: config.is_paused,
            min_stake_required: config.min_stake_required,
            pending_admin: config.pending_admin,
            wasm_upgrade_cooldown: config.wasm_upgrade_cooldown,
            max_dispute_duration: config.max_dispute_duration,
            stake_cooldown: config.stake_cooldown,
            expired_dispute_fee_policy: config.expired_dispute_fee_policy,
            min_release_window: config.min_release_window,
        };

        env.storage().persistent().set(&DataKey::PlatformConfig, &new_config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
        Self::emit_config_updated(
            &env,
            "platform_fee_bps",
            ConfigValue::U32(config.platform_fee_bps),
            ConfigValue::U32(new_fee_bps),
        );
    }

    /// Update platform wallet address (admin only)
    ///
    /// # Arguments
    /// * `new_wallet` - New platform wallet address
    pub fn update_platform_wallet(env: Env, new_wallet: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let new_config = PlatformConfig {
            platform_fee_bps: config.platform_fee_bps,
            platform_wallet: new_wallet,
            admin: config.admin,
            arbitrator: config.arbitrator,
            moderator: config.moderator,
            is_paused: config.is_paused,
            min_stake_required: config.min_stake_required,
            pending_admin: config.pending_admin,
            wasm_upgrade_cooldown: config.wasm_upgrade_cooldown,
            max_dispute_duration: config.max_dispute_duration,
            stake_cooldown: config.stake_cooldown,
            expired_dispute_fee_policy: config.expired_dispute_fee_policy,
            min_release_window: config.min_release_window,
        };

        env.storage().persistent().set(&DataKey::PlatformConfig, &new_config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
        Self::emit_config_updated(
            &env,
            "platform_wallet",
            ConfigValue::Address(config.platform_wallet),
            ConfigValue::Address(new_config.platform_wallet),
        );
    }

    /// Update the expired dispute fee policy (admin only).
    ///
    /// Configures how platform fees are handled when a dispute expires without arbitrator resolution.
    ///
    /// # Arguments
    /// * `policy` - The new fee policy to apply
    ///
    /// # Policies
    /// - RefundFullNoPlatformFee: Buyer gets full refund, platform collects no fee (default)
    /// - RefundMinusPlatformFee: Buyer gets refund minus fee, platform collects fee from buyer
    /// - DeductFeeFromSeller: Buyer gets full refund, seller conceptually loses the fee
    /// - SplitFee: Platform fee split between buyer and seller
    pub fn update_expired_dispute_policy(
        env: Env,
        policy: ExpiredDisputeFeePolicy,
    ) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let old_policy = config.expired_dispute_fee_policy;
        config.expired_dispute_fee_policy = policy;

        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        Self::emit_config_updated(
            &env,
            "expired_dispute_fee_policy",
            ConfigValue::U32(old_policy as u32),
            ConfigValue::U32(policy as u32),
        );

        Ok(())
    }

    /// Get the current expired dispute fee policy
    pub fn get_expired_dispute_policy(env: Env) -> ExpiredDisputeFeePolicy {
        let config = Self::get_platform_config_internal(&env);
        config.expired_dispute_fee_policy
    }

    pub fn set_moderator(env: Env, moderator: Address) {
        let mut config = Self::get_platform_config(env.clone());
        config.admin.require_auth();
        let previous = config
            .moderator
            .clone()
            .map(ConfigValue::Address)
            .unwrap_or_else(|| ConfigValue::String(String::from_str(&env, "unset")));
        config.moderator = Some(moderator.clone());
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
        Self::emit_config_updated(&env, "moderator", previous, ConfigValue::Address(moderator));
    }

    /// Set the minimum escrow amount for a specific token (admin only)
    ///
    /// # Arguments
    /// * `token` - Token address
    /// * `min_amount` - Minimum amount in smallest unit
    pub fn set_min_escrow_amount(env: Env, token: Address, min_amount: i128) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let key = DataKey::MinEscrowAmount(token.clone());
        let old_amount: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        env.storage().persistent().set(&key, &min_amount);
        Self::extend_persistent(&env, &key);
        Self::emit_config_updated(
            &env,
            "min_escrow_amount",
            ConfigValue::I128(old_amount),
            ConfigValue::I128(min_amount),
        );
        Ok(())
    }

    /// Get current platform fee percentage
    pub fn get_platform_fee(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.platform_fee_bps
    }

    /// Get platform wallet address
    pub fn get_platform_wallet(env: Env) -> Address {
        let config = Self::get_platform_config_internal(&env);
        config.platform_wallet
    }

    /// Get total fees collected by platform
    pub fn get_total_fees_collected(env: Env) -> i128 {
        Self::get_all_tracked_total_fees(&env)
    }

    /// Get total fees collected for a specific token.
    pub fn get_total_fees_for_token(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalFees(token))
            .unwrap_or(0)
    }

    /// Calculate the fee for a given amount (for display purposes)
    ///
    /// # Arguments
    /// * `amount` - The escrow amount
    pub fn calculate_fee_for_amount(env: Env, amount: i128) -> i128 {
        let config = Self::get_platform_config_internal(&env);
        Self::calculate_fee(amount, config.platform_fee_bps)
    }

    /// Calculate net amount seller will receive
    ///
    /// # Arguments
    /// * `amount` - The escrow amount
    pub fn calculate_seller_net_amount(env: Env, amount: i128) -> i128 {
        let fee = Self::calculate_fee_for_amount(env, amount);
        amount - fee
    }

    /// Validate escrow parameters for batch creation
    fn validate_escrow_params(env: &Env, params: &EscrowCreateParams) -> Result<(), Error> {
        // Validate amount is positive
        if params.amount <= 0 {
            return Err(Error::AmountBelowMinimum);
        }

        // Check minimum amount
        Self::check_min_amount(env, params.token.clone(), params.amount)?;

        // Validate buyer and seller are different
        if params.buyer == params.seller {
            return Err(Error::SameBuyerSeller);
        }

        // Validate token is whitelisted (#103)
        let whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or(Map::new(env));
        if !whitelist.is_empty() && !whitelist.get(params.token.clone()).unwrap_or(false) {
            return Err(Error::TokenNotWhitelisted);
        }

        // Validate release window bounds (#67)
        let window = params.release_window.unwrap_or(604800u32);
        if window == 0 {
            return Err(Error::ReleaseWindowTooShort);
        }
        let max_window = Self::get_max_release_window(env);
        if window > max_window {
            return Err(Error::ReleaseWindowTooLong);
        }

        // Validate IPFS hash if provided
        Self::validate_optional_ipfs_hash(env, &params.ipfs_hash);

        if let Some(hash) = &params.metadata_hash {
            if hash.len() != 32 {
                return Err(Error::InvalidMetadataHash);
            }
        }

        Ok(())
    }

    /// Create a single escrow from parameters (internal helper)
    /// Note: For batch operations, buyer/seller escrow list updates are consolidated
    /// by the caller to minimize storage writes (Issue #111)
    fn create_single_escrow(
        env: &Env,
        params: EscrowCreateParams,
        batch_id: Option<u64>,
    ) -> Result<u64, Error> {
        // Validate first
        Self::validate_escrow_params(env, &params)?;

        // Default to 7 days if not specified
        let window = params.release_window.unwrap_or(604800u32);
        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;

        // Validate metadata (validate_escrow_params already checked ipfs_hash via validate_optional_ipfs_hash)
        Self::validate_optional_metadata_hash(env, &params.metadata_hash);

        let escrow = Escrow {
            version: CURRENT_ESCROW_VERSION,
            id: params.order_id as u64,
            batch_id,
            buyer: params.buyer.clone(),
            seller: params.seller.clone(),
            token: params.token.clone(),
            amount: params.amount,
            status: EscrowStatus::Active,
            release_window: window,
            created_at,
            ipfs_hash: params.ipfs_hash.clone(),
            metadata_hash: params.metadata_hash.clone(),
            dispute_reason: None,
            dispute_initiated_at: None,
            funded: true,
        };

        env.storage()
            .persistent()
            .set(&(ESCROW, params.order_id), &escrow);
        Self::extend_persistent(env, &(ESCROW, params.order_id));

        // Track active escrows (batch)
        Self::update_active_obligations(env, &params.buyer, 1);
        Self::update_active_obligations(env, &params.seller, 1);

        // Transfer funds from buyer to contract
        let client = token::Client::new(env, &params.token);
        client.transfer(
            &params.buyer,
            &env.current_contract_address(),
            &params.amount,
        );

        // Track locked funds (#212)
        Self::update_total_locked(env, &params.token, params.amount);

        Self::emit_escrow_event(
            env,
            EscrowEvent {
                escrow_id: params.order_id as u64,
                action: EscrowAction::Created,
                buyer: params.buyer.clone(),
                seller: params.seller.clone(),
                amount: params.amount,
                token: params.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(params.order_id as u64)
    }

    /// DevEx: Dry-Run Batch Validation
    /// Validates a batch of escrow creations without modifying state.
    /// Returns a map of index -> Error for any escrow that fails validation.
    pub fn validate_batch_creation(
        env: Env,
        escrows: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Map<u32, Error> {
        let mut errors: Map<u32, Error> = Map::new(&env);

        if escrows.len() > MAX_BATCH_SIZE {
            env.panic_with_error(crate::Error::BatchLimitExceeded);
        }

        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                if let Err(e) = Self::validate_escrow_params(&env, &params) {
                    errors.set(i, e);
                }
            }
        }

        errors
    }

    /// Create multiple escrows in a batch operation (Issue #111: Optimized)
    ///
    /// Validates all escrows first before processing any to ensure atomic behavior.
    /// Optimizations:
    /// - Single authorization check for batch caller
    /// - Consolidated storage updates for buyer/seller escrow lists
    /// - Batch size limit to prevent resource exhaustion
    ///
    /// # Arguments
    /// * `escrows` - Vector of escrow creation parameters (max MAX_BATCH_SIZE items)
    /// * `batch_id` - Unique identifier for this batch operation
    ///
    /// # Returns
    /// Vector of created escrow IDs
    ///
    /// # Errors
    /// - BatchLimitExceeded if batch exceeds MAX_BATCH_SIZE
    /// - Any validation error from individual escrows
    pub fn create_batch_escrow(
        env: Env,
        batch_id: u64,
        escrows: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        Self::enter_reentry_guard(&env);
        Self::check_not_paused(&env);

        // Issue #111: Enforce batch size limit
        if escrows.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchLimitExceeded);
        }

        let mut results = soroban_sdk::Vec::new(&env);

        // Early exit for empty batch
        if escrows.is_empty() {
            Self::exit_reentry_guard(&env);
            return Ok(results);
        }

        // Issue #111: Single authorization check - require auth from first buyer only
        let first_params = escrows.get(0).expect("");
        first_params.buyer.require_auth();

        // Issue #111: Validate all first (single pass)
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                Self::validate_escrow_params(&env, &params)?;
            }
        }

        // Issue #111: Collect buyer/seller updates to consolidate storage writes
        // Using indexed storage for scalability
        let mut buyer_counts: Map<Address, u32> = Map::new(&env);
        let mut seller_counts: Map<Address, u32> = Map::new(&env);

        // Create all escrows
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                match Self::create_single_escrow(&env, params.clone(), Some(batch_id)) {
                    Ok(id) => {
                        let buyer_key = params.buyer.clone();
                        let seller_key = params.seller.clone();

                        // Track buyer counts for indexed storage
                        if !buyer_counts.contains_key(buyer_key.clone()) {
                            let count_key = DataKey::BuyerEscrowCount(buyer_key.clone());
                            let existing_count: u32 = env
                                .storage()
                                .persistent()
                                .get(&count_key)
                                .unwrap_or(0u32);
                            buyer_counts.set(buyer_key.clone(), existing_count);
                        }
                        let buyer_count = buyer_counts.get(buyer_key.clone()).unwrap();
                        
                        // Store escrow ID at indexed position
                        let buyer_index_key = DataKey::BuyerEscrowIndexed(buyer_key.clone(), buyer_count);
                        env.storage().persistent().set(&buyer_index_key, &id);
                        Self::extend_persistent(&env, &buyer_index_key);
                        
                        buyer_counts.set(buyer_key, buyer_count + 1);

                        // Track seller counts for indexed storage
                        if !seller_counts.contains_key(seller_key.clone()) {
                            let count_key = DataKey::SellerEscrowCount(seller_key.clone());
                            let existing_count: u32 = env
                                .storage()
                                .persistent()
                                .get(&count_key)
                                .unwrap_or(0u32);
                            seller_counts.set(seller_key.clone(), existing_count);
                        }
                        let seller_count = seller_counts.get(seller_key.clone()).unwrap();
                        
                        // Store escrow ID at indexed position
                        let seller_index_key = DataKey::SellerEscrowIndexed(seller_key.clone(), seller_count);
                        env.storage().persistent().set(&seller_index_key, &id);
                        Self::extend_persistent(&env, &seller_index_key);
                        
                        seller_counts.set(seller_key, seller_count + 1);

                        // Emit batch event
                        let escrow_opt: Option<Escrow> =
                            env.storage().persistent().get(&(ESCROW, id as u32));
                        if let Some(escrow) = escrow_opt {
                            Self::emit_escrow_event(
                                &env,
                                EscrowEvent {
                                    escrow_id: id,
                                    action: EscrowAction::BatchCreated,
                                    buyer: escrow.buyer,
                                    seller: escrow.seller,
                                    amount: escrow.amount,
                                    token: escrow.token,
                                    timestamp: env.ledger().timestamp(),
                                },
                            );
                        }
                        results.push_back(id);
                    }
                    Err(e) => {
                        Self::exit_reentry_guard(&env);
                        return Err(e);
                    }
                }
            }
        }

        // Issue #111: Consolidate all storage updates at once
        let mut i = 0;
        loop {
            if i >= buyer_counts.len() {
                break;
            }
            if let Some(buyer_addr) = buyer_counts.keys().get(i) {
                if let Some(final_count) = buyer_counts.get(buyer_addr.clone()) {
                    let count_key = DataKey::BuyerEscrowCount(buyer_addr.clone());
                    env.storage()
                        .persistent()
                        .set(&count_key, &final_count);
                    Self::extend_persistent(&env, &count_key);
                }
            }
            i += 1;
        }

        let mut i = 0;
        loop {
            if i >= seller_counts.len() {
                break;
            }
            if let Some(seller_addr) = seller_counts.keys().get(i) {
                if let Some(final_count) = seller_counts.get(seller_addr.clone()) {
                    let count_key = DataKey::SellerEscrowCount(seller_addr.clone());
                    env.storage()
                        .persistent()
                        .set(&count_key, &final_count);
                    Self::extend_persistent(&env, &count_key);
                }
            }
            i += 1;
        }

        // Consolidate global index updates for the entire batch
        if !results.is_empty() {
            let ids_key = DataKey::AllEscrowIds;
            let mut all_ids: soroban_sdk::Vec<u32> = env
                .storage()
                .persistent()
                .get(&ids_key)
                .unwrap_or(soroban_sdk::Vec::new(&env));
            for j in 0..results.len() {
                if let Some(id) = results.get(j) {
                    all_ids.push_back(id as u32);
                }
            }
            env.storage().persistent().set(&ids_key, &all_ids);
            Self::extend_persistent(&env, &ids_key);

            let count_key = DataKey::EscrowCount;
            let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0u32);
            env.storage()
                .persistent()
                .set(&count_key, &(count + results.len()));
            Self::extend_persistent(&env, &count_key);
        }

        Self::exit_reentry_guard(&env);
        Ok(results)
    }

    /// Release multiple escrows in a batch operation
    ///
    /// Validates all escrows first before processing any.
    ///
    /// # Arguments
    /// * `order_ids` - Vector of order IDs to release
    /// * `batch_id` - Unique identifier for this batch operation
    /// * `authorized_address` - Address releasing the funds (buyer)
    pub fn release_batch_funds(
        env: Env,
        _batch_id: u64,
        order_ids: soroban_sdk::Vec<u32>,
        authorized_address: Address,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        Self::enter_reentry_guard(&env);
        authorized_address.require_auth();

        let mut results = soroban_sdk::Vec::new(&env);

        // Validate all escrows first
        for i in 0..order_ids.len() {
            if let Some(order_id) = order_ids.get(i) {
                let escrow_opt = env.storage().persistent().get(&(ESCROW, order_id));

                if escrow_opt.is_none() {
                    return Err(Error::EscrowNotFound);
                }

                let escrow: Escrow = escrow_opt.unwrap();

                // Check status
                if escrow.status != EscrowStatus::Active {
                    return Err(Error::InvalidEscrowState);
                }

                // Check authorization (buyer must match)
                if escrow.buyer != authorized_address {
                    return Err(Error::Unauthorized);
                }
            }
        }

        // Then process all releases
        for i in 0..order_ids.len() {
            if let Some(order_id) = order_ids.get(i) {
                let escrow_opt: Option<Escrow> =
                    env.storage().persistent().get(&(ESCROW, order_id));
                if escrow_opt.is_some() {
                    env.storage()
                        .persistent()
                        .extend_ttl(&(ESCROW, order_id), 1000, 518400);
                }

                if let Some(mut escrow) = escrow_opt {
                    // Get platform config
                    let config = Self::get_platform_config_internal(&env);

                    // Calculate platform fee
                    let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
                    let fee_amount = Self::calculate_fee(escrow.amount, fee_bps);
                    let seller_amount = escrow.amount - fee_amount;

                    // Update status
                    escrow.status = EscrowStatus::Released;
                    env.storage().persistent().set(&(ESCROW, order_id), &escrow);

                    // Transfer platform fee to platform wallet
                    if fee_amount > 0 {
                        Self::transfer_platform_fee(
                            &env,
                            &escrow.token,
                            &config.platform_wallet,
                            fee_amount,
                        );
                    }

                    // Transfer remaining funds to seller
                    let token_client = token::Client::new(&env, &escrow.token);
                    token_client.transfer(
                        &env.current_contract_address(),
                        &escrow.seller,
                        &seller_amount,
                    );

                    // Emit release event
                    Self::emit_escrow_event(
                        &env,
                        EscrowEvent {
                            escrow_id: order_id as u64,
                            action: EscrowAction::BatchReleased,
                            buyer: escrow.buyer.clone(),
                            seller: escrow.seller.clone(),
                            amount: escrow.amount,
                            token: escrow.token.clone(),
                            timestamp: env.ledger().timestamp(),
                        },
                    );
                    results.push_back(order_id as u64);
                }
            }
        }

        Self::exit_reentry_guard(&env);
        Ok(results)
    }

    // NOTE: referral payout support has been removed from the contract. The configuration key is
    // retained only for storage compatibility during upgrades.

    /// Check that the contract is not paused. Panics with ContractPaused if it is.
    fn check_not_paused(env: &Env) {
        if let Some(config) = env
            .storage()
            .persistent()
            .get::<DataKey, PlatformConfig>(&DataKey::PlatformConfig)
        {
            if config.is_paused {
                env.panic_with_error(crate::Error::ContractPaused);
            }
        }
    }

    /// Admin pauses or unpauses the contract.
    pub fn set_paused(env: Env, paused: bool) {
        let admin = Self::get_admin(&env)
            .unwrap_or_else(|_| env.panic_with_error(crate::Error::Unauthorized));
        admin.require_auth();

        let mut config = Self::get_platform_config_internal(&env);
        config.is_paused = paused;
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        if paused {
            Self::emit_platform_paused(&env, admin);
        } else {
            Self::emit_platform_unpaused(&env, admin);
        }
    }

    /// View: check if contract is paused.
    pub fn is_paused(env: Env) -> bool {
        let config = Self::get_platform_config_internal(&env);
        config.is_paused
    }

    // ── Tiered Artisan Fees (#98) ───────────────────────────────────

    /// Admin assigns a custom fee tier (in basis points) for an artisan.
    pub fn set_artisan_fee_tier(env: Env, artisan: Address, fee_bps: u32) {
        let admin = Self::get_admin(&env)
            .unwrap_or_else(|_| env.panic_with_error(crate::Error::Unauthorized));
        admin.require_auth();

        if fee_bps > MAX_PLATFORM_FEE_BPS {
            env.panic_with_error(crate::Error::InvalidFee);
        }

        env.storage()
            .persistent()
            .set(&DataKey::ArtisanFeeTier(artisan.clone()), &fee_bps);
        Self::extend_persistent(&env, &DataKey::ArtisanFeeTier(artisan.clone()));
        Self::emit_artisan_fee_tier_updated(&env, artisan, fee_bps);
    }

    /// Get the effective fee basis points for a seller.
    /// Returns artisan-specific tier if set, otherwise platform default.
    pub fn get_effective_fee_bps(env: Env, seller: Address) -> u32 {
        let key = DataKey::ArtisanFeeTier(seller);
        if let Some(fee) = env.storage().persistent().get::<DataKey, u32>(&key) {
            Self::extend_persistent(&env, &key);
            fee
        } else {
            let config = Self::get_platform_config_internal(&env);
            config.platform_fee_bps
        }
    }

    // ── Referral Rewards (#105) ─────────────────────────────────────

    /// Admin sets the referral reward percentage (basis points of the platform fee).
    pub fn set_referral_reward_bps(env: Env, bps: u32) {
        let admin = Self::get_admin(&env)
            .unwrap_or_else(|_| env.panic_with_error(crate::Error::Unauthorized));
        admin.require_auth();
        if bps > 5000 {
            env.panic_with_error(crate::Error::InvalidFee);
        }
        env.storage()
            .persistent()
            .set(&DataKey::ReferralRewardBps, &bps);
        Self::extend_persistent(&env, &DataKey::ReferralRewardBps);
    }

    /// Get the referral reward basis points.
    pub fn get_referral_reward_bps(env: Env) -> u32 {
        let key = DataKey::ReferralRewardBps;
        let bps = env
            .storage()
            .persistent()
            .get::<DataKey, u32>(&key)
            .unwrap_or(0);
        if env.storage().persistent().has(&key) {
            Self::extend_persistent(&env, &key);
        }
        bps
    }

    // ── Dispute Resolution Deadline (#93) ───────────────────────────

    /// Resolve a dispute that has exceeded the maximum dispute duration.
    ///
    /// If the dispute has been open for longer than the configured max_dispute_duration,
    /// the escrow is resolved according to the configured expired_dispute_fee_policy.
    /// Returns DisputeExpired error if the deadline has not yet passed.
    pub fn resolve_expired_dispute(env: Env, order_id: u32) -> Result<(), Error> {
        let escrow_opt: Option<Escrow> = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        Self::extend_persistent(&env, &(ESCROW, order_id));
        let mut escrow: Escrow = escrow_opt.unwrap();

        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }

        let initiated_at = escrow
            .dispute_initiated_at
            .ok_or(Error::InvalidEscrowState)?;
        let current_time = env.ledger().timestamp();

        let config = Self::get_platform_config_internal(&env);
        if initiated_at + config.max_dispute_duration as u64 > current_time {
            return Err(Error::DisputeExpired);
        }

        // CRITICAL: Update status BEFORE external calls (CEI pattern)
        escrow.status = EscrowStatus::Resolved;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Now perform token transfers (external calls)
        let token_client = token::Client::new(&env, &escrow.token);
        let fee_amount = Self::calculate_fee(escrow.amount, config.platform_fee_bps);

        // Apply the configured fee policy
        match config.expired_dispute_fee_policy {
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee => {
                // Refund buyer in full, platform collects no fee
                token_client.transfer(
                    &env.current_contract_address(),
                    &escrow.buyer,
                    &escrow.amount,
                );
            }
            ExpiredDisputeFeePolicy::RefundMinusPlatformFee => {
                // Refund buyer minus platform fee, platform collects fee
                let buyer_refund = escrow.amount - fee_amount;
                token_client.transfer(
                    &env.current_contract_address(),
                    &escrow.buyer,
                    &buyer_refund,
                );
                token_client.transfer(
                    &env.current_contract_address(),
                    &config.platform_wallet,
                    &fee_amount,
                );
                // Track platform fees
                Self::record_total_fees(&env, &escrow.token, fee_amount);
            }
            ExpiredDisputeFeePolicy::DeductFeeFromSeller => {
                // Refund buyer in full, but conceptually the fee comes from seller's side
                // (seller loses the fee even though they didn't receive payment)
                token_client.transfer(
                    &env.current_contract_address(),
                    &escrow.buyer,
                    &escrow.amount,
                );
                // Note: In this policy, the platform doesn't collect the fee
                // This represents a loss for the seller (they lose the opportunity cost)
                // but protects the buyer from arbitrator failure
            }
            ExpiredDisputeFeePolicy::SplitFee => {
                // Split the platform fee: half from buyer's refund, half conceptually from seller
                let half_fee = fee_amount / 2;
                let buyer_refund = escrow.amount - half_fee;
                
                token_client.transfer(
                    &env.current_contract_address(),
                    &escrow.buyer,
                    &buyer_refund,
                );
                token_client.transfer(
                    &env.current_contract_address(),
                    &config.platform_wallet,
                    &half_fee,
                );
                // Track platform fees (only the collected half)
                Self::record_total_fees(&env, &escrow.token, half_fee);
            }
        }

        // Track locked funds (#212)
        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Resolved,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: current_time,
            },
        );

        Ok(())
    }

    // ── Staking Requirement for Artisans (#99) ───────────────────────

    /// Stake tokens to satisfy the platform's minimum stake requirement.
    ///
    /// The artisan transfers `amount` of `token` to the contract. The stake is stored
    /// and a cooldown timer is set so the tokens cannot be unstaked immediately.
    ///
    /// Staked balances remain owned by the artisan. The contract does not accrue,
    /// distribute, or sweep interest/yield from these reserved funds into platform fees.
    pub fn stake_tokens(env: Env, artisan: Address, token: Address, amount: i128) {
        artisan.require_auth();

        if amount <= 0 {
            env.panic_with_error(crate::Error::AmountBelowMinimum);
        }

        // Transfer from artisan to contract
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&artisan, &env.current_contract_address(), &amount);

        // Track staked funds (#212)
        Self::update_total_staked(&env, &token, amount);

        // Accumulate stake in a single record with token metadata.
        let stake_key = DataKey::ArtisanStake(artisan.clone());
        let current_stake: Option<ArtisanStakeData> = env.storage().persistent().get(&stake_key);
        let new_stake = if let Some(existing_stake) = current_stake {
            if existing_stake.token != token {
                env.panic_with_error(crate::Error::StakeTokenMismatch);
            }
            ArtisanStakeData {
                amount: existing_stake.amount + amount,
                token,
            }
        } else {
            ArtisanStakeData { amount, token }
        };

        env.storage().persistent().set(&stake_key, &new_stake);
        Self::extend_persistent(&env, &stake_key);

        // Per-deposit cooldown queue: push a new deposit entry with its own cooldown_end.
        let config = Self::get_platform_config_internal(&env);
        let cooldown_end = env.ledger().timestamp() + config.stake_cooldown as u64;
        let deposit = StakeDeposit { amount, cooldown_end };
        let queue_key = DataKey::ArtisanStakeQueue(artisan.clone());
        let mut queue: soroban_sdk::Vec<StakeDeposit> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        queue.push_back(deposit);
        env.storage().persistent().set(&queue_key, &queue);
        Self::extend_persistent(&env, &queue_key);

        // Maintain deprecated single timestamp for backwards compatibility: set to
        // the maximum cooldown_end across deposits (i.e., time when all current
        // deposits will be matured). This ensures older clients still see a
        // conservative cooldown value.
        let mut max_end: u64 = 0;
        for i in 0..queue.len() {
            if let Some(d) = queue.get(i) {
                if d.cooldown_end > max_end {
                    max_end = d.cooldown_end;
                }
            }
        }
        let cooldown_key = DataKey::StakeCooldownEnd(artisan.clone());
        env.storage().persistent().set(&cooldown_key, &max_end);
        Self::extend_persistent(&env, &cooldown_key);

        env.events().publish(
            (Symbol::new(&env, "tokens_staked"), artisan.clone()),
            TokensStakedEvent {
                artisan,
                token: new_stake.token.clone(),
                amount,
            },
        );
    }

    /// Unstake previously staked tokens after the cooldown period has elapsed.
    ///
    /// Stakes can only be returned in the exact token originally deposited, which
    /// prevents reserved artisan collateral from being treated as platform-managed fees.
    pub fn unstake_tokens(env: Env, artisan: Address, token: Address) {
        artisan.require_auth();

        // Use per-deposit queue: only matured deposits can be unstaked.
        let queue_key = DataKey::ArtisanStakeQueue(artisan.clone());
        let mut queue: soroban_sdk::Vec<StakeDeposit> = env
            .storage()
            .persistent()
            .get(&queue_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let now = env.ledger().timestamp();
        let mut matured_amount: i128 = 0;

        // Build a new queue with only non-matured deposits preserved.
        let mut remaining: soroban_sdk::Vec<StakeDeposit> = soroban_sdk::Vec::new(&env);
        for i in 0..queue.len() {
            if let Some(d) = queue.get(i) {
                if now >= d.cooldown_end {
                    matured_amount += d.amount;
                } else {
                    remaining.push_back(d);
                }
            }
        }

        if matured_amount <= 0 {
            env.panic_with_error(crate::Error::StakeCooldownActive);
        }

        // Update stored queue and total stake record.
        if remaining.is_empty() {
            // No remaining deposits: remove both queue and stake record
            env.storage().persistent().remove(&queue_key);
            env.storage().persistent().remove(&DataKey::ArtisanStake(artisan.clone()));
            env.storage().persistent().remove(&DataKey::StakeCooldownEnd(artisan.clone()));
        } else {
            env.storage().persistent().set(&queue_key, &remaining);
            Self::extend_persistent(&env, &queue_key);

            // Update deprecated single cooldown to max of remaining
            let mut max_end: u64 = 0;
            for i in 0..remaining.len() {
                if let Some(d) = remaining.get(i) {
                    if d.cooldown_end > max_end {
                        max_end = d.cooldown_end;
                    }
                }
            }
            env.storage().persistent().set(&DataKey::StakeCooldownEnd(artisan.clone()), &max_end);
            Self::extend_persistent(&env, &DataKey::StakeCooldownEnd(artisan.clone()));

            // Update total stake amount (subtract matured)
            let stake_key = DataKey::ArtisanStake(artisan.clone());
            let mut stake_data: ArtisanStakeData = env
                .storage()
                .persistent()
                .get(&stake_key)
                .unwrap_or_else(|| env.panic_with_error(crate::Error::StorageCorrupted));
            stake_data.amount -= matured_amount;
            env.storage().persistent().set(&stake_key, &stake_data);
            Self::extend_persistent(&env, &stake_key);
        }

        // Return matured tokens to artisan
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &artisan, &matured_amount);

        // Track staked funds (#212)
        Self::update_total_staked(&env, &token, -stake_data.amount);

        env.events().publish(
            (Symbol::new(&env, "tokens_unstaked"), artisan.clone()),
            TokensUnstakedEvent {
                artisan,
                token,
                amount: matured_amount,
            },
        );
    }

    /// Return the current staked amount for an artisan.
    pub fn get_stake(env: Env, artisan: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<DataKey, ArtisanStakeData>(&DataKey::ArtisanStake(artisan))
            .map(|stake: ArtisanStakeData| stake.amount)
            .unwrap_or(0)
    }

    /// Admin sets the minimum stake required for artisans to create escrows.
    pub fn set_min_stake_required(env: Env, min_stake: i128) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut config = Self::get_platform_config_internal(&env);
        config.min_stake_required = min_stake;
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);
        Ok(())
    }

    /// Admin sets the WASM upgrade cooldown period (in seconds).
    pub fn set_wasm_upgrade_cooldown(env: Env, cooldown_seconds: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut config = Self::get_platform_config_internal(&env);
        let old_value = config.wasm_upgrade_cooldown;
        config.wasm_upgrade_cooldown = cooldown_seconds;
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        Self::emit_config_updated(
            &env,
            "wasm_upgrade_cooldown",
            ConfigValue::U32(old_value),
            ConfigValue::U32(cooldown_seconds),
        );
        Ok(())
    }

    /// Admin sets the maximum dispute duration (in seconds).
    pub fn set_max_dispute_duration(env: Env, duration_seconds: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut config = Self::get_platform_config_internal(&env);
        let old_value = config.max_dispute_duration;
        config.max_dispute_duration = duration_seconds;
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        Self::emit_config_updated(
            &env,
            "max_dispute_duration",
            ConfigValue::U32(old_value),
            ConfigValue::U32(duration_seconds),
        );
        Ok(())
    }

    /// Admin sets the stake cooldown period (in seconds).
    pub fn set_stake_cooldown(env: Env, cooldown_seconds: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut config = Self::get_platform_config_internal(&env);
        let old_value = config.stake_cooldown;
        config.stake_cooldown = cooldown_seconds;
        env.storage().persistent().set(&DataKey::PlatformConfig, &config);
        Self::extend_persistent(&env, &DataKey::PlatformConfig);

        Self::emit_config_updated(
            &env,
            "stake_cooldown",
            ConfigValue::U32(old_value),
            ConfigValue::U32(cooldown_seconds),
        );
        Ok(())
    }

    // ── Partial Refund Negotiation (#101) ────────────────────────────

    /// Propose a partial refund for a disputed escrow.
    ///
    /// Either the buyer or seller may submit a proposal. Only one proposal may be
    /// active at a time; a second call returns ProposalAlreadyExists.
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    /// * `refund_amount` - Gross amount to refund to the buyer before any
    ///   potential refund-side platform fee is deducted.
    /// * `proposed_by` - Address of the party proposing the refund (must be buyer or seller)
    pub fn propose_partial_refund(
        env: Env,
        order_id: u32,
        refund_amount: i128,
        caller: Address,
    ) -> Result<(), Error> {
        let escrow_opt: Option<Escrow> = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        let escrow: Escrow = escrow_opt.unwrap();

        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }

        // Verify caller is either the buyer or seller
        if caller != escrow.buyer && caller != escrow.seller {
            return Err(Error::Unauthorized);
        }

        // Require auth from the proposing party
        caller.require_auth();

        // `refund_amount` is interpreted as gross; validation includes any
        // configured refund-side fee to ensure transfers remain solvent.
        if !Self::is_valid_partial_refund_gross_amount(&env, &escrow, refund_amount) {
            return Err(Error::InvalidRefundAmount);
        }

        let proposal_key = DataKey::PartialRefundProposal(order_id);
        if env.storage().persistent().has(&proposal_key) {
            return Err(Error::ProposalAlreadyExists);
        }

        let proposal = PartialRefundProposal {
            order_id,
            refund_amount,
            proposed_by: caller,
            proposed_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&proposal_key, &proposal);
        Self::extend_persistent(&env, &proposal_key);

        Ok(())
    }

    // ── Storage Explorer ──────────────────────────────────────

    /// Returns the total number of escrows ever created on this platform.
    ///
    /// This is an O(1) read — safe to call at any scale. Pair with
    /// `get_all_escrow_ids_iterative` to paginate the full ID set without
    /// hitting Soroban CPU/memory resource limits.
    pub fn get_escrow_count(env: Env) -> u32 {
        let key = DataKey::EscrowCount;
        env.storage()
            .persistent()
            .get::<DataKey, u32>(&key)
            .unwrap_or(0)
    }

    /// Returns a page of all escrow order IDs created on the platform, in creation order.
    ///
    /// This is the recommended pattern for frontends to enumerate every escrow without
    /// hitting Soroban resource limits. The function reads a bounded slice of the
    /// globally maintained `AllEscrowIds` index; no on-chain loops proportional to
    /// the total escrow count are performed at call time.
    ///
    /// # Usage pattern (frontend / off-chain)
    /// ```text
    /// total  = get_escrow_count()
    /// pages  = ceil(total / PAGE_SIZE)
    /// for p in 0..pages:
    ///     ids = get_all_escrow_ids_iterative(p, PAGE_SIZE)
    ///     for id in ids:
    ///         escrow = get_escrow(id)
    /// ```
    ///
    /// # Soroban RPC key browsing
    /// To enumerate storage keys directly via the RPC without calling this function,
    /// use the `getLedgerEntries` method or the experimental `getContractData` cursor
    /// endpoint.  Relevant key patterns:
    /// - `DataKey::AllEscrowIds`           – the full ordered ID list (this index)
    /// - `DataKey::EscrowCount`            – u32 total count
    /// - `(ESCROW, order_id: u32)`         – individual escrow struct
    /// - `DataKey::BuyerEscrows(address)`  – DEPRECATED: Legacy Vec<u64> of IDs for a buyer
    /// - `DataKey::SellerEscrows(address)` – DEPRECATED: Legacy Vec<u64> of IDs for a seller
    /// - `DataKey::BuyerEscrowIndexed(address, index)` – Indexed storage: u64 escrow ID at position
    /// - `DataKey::BuyerEscrowCount(address)` – u32 total count of buyer's escrows
    /// - `DataKey::SellerEscrowIndexed(address, index)` – Indexed storage: u64 escrow ID at position
    /// - `DataKey::SellerEscrowCount(address)` – u32 total count of seller's escrows
    ///
    /// # Arguments
    /// * `page`  – Zero-indexed page number
    /// * `limit` – Page size; values above `MAX_BATCH_SIZE` are silently capped
    ///
    /// # Returns
    /// A `Vec<u32>` of escrow IDs for the requested page; empty when `page` is out of range.
    pub fn get_all_escrow_ids_iterative(env: Env, page: u32, limit: u32) -> soroban_sdk::Vec<u32> {
        let limit = limit.min(MAX_BATCH_SIZE);
        if limit == 0 {
            return soroban_sdk::Vec::new(&env);
        }

        let key = DataKey::AllEscrowIds;
        let all_ids: soroban_sdk::Vec<u32> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let start = page * limit;
        let len = all_ids.len();

        if start >= len {
            return soroban_sdk::Vec::new(&env);
        }

        let end = (start + limit).min(len);
        all_ids.slice(start..end)
    }

    /// Accept the outstanding partial refund proposal for a disputed escrow.
    ///
    /// The counterparty (the party that did NOT submit the proposal) calls this function.
    /// Funds are distributed from a gross refund model: buyer receives
    /// `refund_amount - refund_fee`, seller receives the remainder minus seller-side
    /// platform fee. The escrow status is set to Resolved.
    pub fn accept_partial_refund(env: Env, order_id: u32) -> Result<(), Error> {
        let escrow_opt: Option<Escrow> = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        let mut escrow: Escrow = escrow_opt.unwrap();

        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }

        let proposal_key = DataKey::PartialRefundProposal(order_id);
        let proposal_opt: Option<PartialRefundProposal> =
            env.storage().persistent().get(&proposal_key);
        if proposal_opt.is_none() {
            return Err(Error::ProposalNotFound);
        }
        let proposal: PartialRefundProposal = proposal_opt.unwrap();

        // The counterparty is whoever did NOT propose
        if proposal.proposed_by == escrow.buyer {
            escrow.seller.require_auth();
        } else {
            escrow.buyer.require_auth();
        }

        let refund_amount_gross = proposal.refund_amount;
        let refund_fee = Self::calculate_partial_refund_fee(&env, refund_amount_gross);
        let refund_amount_net = refund_amount_gross - refund_fee;
        let seller_gross = escrow.amount - refund_amount_gross;

        // Deduct platform fee from seller's portion using effective fee bps
        let config = Self::get_platform_config_internal(&env);
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let seller_fee = Self::calculate_fee(seller_gross, fee_bps);
        let seller_net = seller_gross - seller_fee;
        let total_platform_fee = refund_fee.saturating_add(seller_fee);

        // CEI Pattern: EFFECTS - Update state BEFORE external calls
        escrow.status = EscrowStatus::Resolved;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Clean up proposal
        env.storage().persistent().remove(&proposal_key);

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        // CEI Pattern: INTERACTIONS - External calls AFTER state updates
        let token_client = token::Client::new(&env, &escrow.token);

        // Refund buyer
        if refund_amount_net > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.buyer,
                &refund_amount_net,
            );
        }

        // Pay platform fee
        if total_platform_fee > 0 {
            Self::transfer_platform_fee(
                &env,
                &escrow.token,
                &config.platform_wallet,
                total_platform_fee,
            );
        }

        // Pay seller
        if seller_net > 0 {
            token_client.transfer(&env.current_contract_address(), &escrow.seller, &seller_net);

            // Track locked funds (#212)
            Self::update_total_locked(&env, &escrow.token, -escrow.amount);
        }

        Self::emit_escrow_event(
            &env,
            EscrowEvent {
                escrow_id: order_id as u64,
                action: EscrowAction::Resolved,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    /// Returns the currently configured refund-side fee basis points.
    ///
    /// Today this is intentionally fixed at 0 bps. A future governance feature
    /// can replace this implementation with configurable storage without changing
    /// partial-refund validation semantics.
    fn get_refund_fee_bps(_env: &Env) -> u32 {
        0
    }

    /// Calculate refund-side fee charged against a proposed gross partial refund.
    fn calculate_partial_refund_fee(env: &Env, gross_refund_amount: i128) -> i128 {
        let refund_fee_bps = Self::get_refund_fee_bps(env);
        Self::calculate_fee(gross_refund_amount, refund_fee_bps)
    }

    /// Validate gross partial refund amount against escrow solvency including any
    /// potential refund-side fee that may apply.
    fn is_valid_partial_refund_gross_amount(env: &Env, escrow: &Escrow, gross_refund: i128) -> bool {
        if gross_refund <= 0 || gross_refund > escrow.amount {
            return false;
        }
        let potential_refund_fee = Self::calculate_partial_refund_fee(env, gross_refund);
        gross_refund.saturating_add(potential_refund_fee) <= escrow.amount
    }

    /// Check if a user has any active traditional or recurring escrows.
    pub fn has_active_escrows(env: Env, user: Address) -> bool {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveObligations(user))
            .unwrap_or(0);
        count > 0
    }

    /// Create a new recurring escrow for recurring payments/subscriptions.
    pub fn create_recurring_escrow(
        env: Env,
        buyer: Address,
        artisan: Address,
        token: Address,
        total_amount: i128,
        frequency: u64,
        duration: u32,
    ) -> RecurringEscrow {
        Self::enter_reentry_guard(&env);
        Self::check_not_paused(&env);
        buyer.require_auth();

        if duration == 0 || frequency == 0 || total_amount <= 0 {
            env.panic_with_error(crate::Error::AmountBelowMinimum);
        }
        if buyer == artisan {
            env.panic_with_error(crate::Error::SameBuyerSeller);
        }

        // Validate token whitelist
        Self::check_token_whitelisted(&env, &token);

        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRecurringEscrowId)
            .unwrap_or(1);
        env.storage()
            .persistent()
            .set(&DataKey::NextRecurringEscrowId, &(id + 1));
        Self::extend_persistent(&env, &DataKey::NextRecurringEscrowId);

        let now = env.ledger().timestamp();

        let escrow = RecurringEscrow {
            id,
            buyer: buyer.clone(),
            artisan: artisan.clone(),
            token: token.clone(),
            total_amount,
            released_amount: 0,
            frequency,
            duration,
            current_cycle: 0,
            last_release_time: now,
            is_active: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey::RecurringEscrow(id), &escrow);
        Self::extend_persistent(&env, &DataKey::RecurringEscrow(id));

        // Track active recurring escrows
        Self::update_active_obligations(&env, &buyer, 1);
        Self::update_active_obligations(&env, &artisan, 1);

        // Lock funds upfront
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &total_amount);

        // Track locked funds (#212)
        Self::update_total_locked(&env, &token, total_amount);

        env.events().publish(
            (Symbol::new(&env, "recurring_escrow"), id),
            RecurringEscrowEvent {
                id,
                action: RecurringEscrowAction::Created,
                buyer,
                artisan,
                amount: total_amount,
                timestamp: now,
            },
        );

        Self::exit_reentry_guard(&env);
        escrow
    }

    /// Release funds for the next cycle in a recurring escrow.
    pub fn release_next_cycle(env: Env, id: u64) {
        Self::enter_reentry_guard(&env);
        let key = DataKey::RecurringEscrow(id);
        let mut escrow: RecurringEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::RecurringEscrowNotFound));

        if !escrow.is_active {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }
        if escrow.current_cycle >= escrow.duration {
            env.panic_with_error(crate::Error::CycleNotReady);
        }

        let now = env.ledger().timestamp();
        if now < escrow.last_release_time + escrow.frequency {
            env.panic_with_error(crate::Error::CycleNotReady);
        }

        let cycle_amount = if escrow.current_cycle == escrow.duration - 1 {
            // Last cycle: handle remainder
            escrow.total_amount - escrow.released_amount
        } else {
            escrow.total_amount / (escrow.duration as i128)
        };

        // Calculate and transfer platform fee
        let config = Self::get_platform_config_internal(&env);
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.artisan.clone());
        let fee_amount = Self::calculate_fee(cycle_amount, fee_bps);
        let artisan_amount = cycle_amount - fee_amount;

        if fee_amount > 0 {
            Self::transfer_platform_fee(&env, &escrow.token, &config.platform_wallet, fee_amount);
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.artisan,
            &artisan_amount,
        );

        // Track locked funds (#212)
        Self::update_total_locked(&env, &escrow.token, -cycle_amount);

        // Update escrow state
        escrow.released_amount += cycle_amount;
        escrow.current_cycle += 1;
        escrow.last_release_time = now;

        if escrow.current_cycle == escrow.duration {
            escrow.is_active = false;
            // Decrement active recurring counts
            Self::update_active_obligations(&env, &escrow.buyer, -1);
            Self::update_active_obligations(&env, &escrow.artisan, -1);
        }

        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(&env, &key);

        env.events().publish(
            (Symbol::new(&env, "recurring_escrow"), id),
            RecurringEscrowEvent {
                id,
                action: RecurringEscrowAction::CycleReleased,
                buyer: escrow.buyer.clone(),
                artisan: escrow.artisan.clone(),
                amount: cycle_amount,
                timestamp: now,
            },
        );

        // Update reputation
        if let Some(client) = Self::get_onboarding_client(&env) {
            client.update_user_metrics(&escrow.artisan, &1u32, &cycle_amount, &escrow.token);
            if !escrow.is_active {
                client.update_reputation(&escrow.artisan, &1u32, &0u32);
                client.update_reputation(&escrow.buyer, &1u32, &0u32);
            }
        }

        Self::exit_reentry_guard(&env);
    }

    /// Cancel a recurring escrow and refund remaining funds to the buyer.
    pub fn cancel_recurring_escrow(env: Env, id: u64) {
        Self::enter_reentry_guard(&env);
        let key = DataKey::RecurringEscrow(id);
        let mut escrow: RecurringEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::RecurringEscrowNotFound));

        escrow.buyer.require_auth();
        if !escrow.is_active {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let remaining = escrow.total_amount - escrow.released_amount;

        // CEI Pattern: EFFECTS - Update state BEFORE external calls
        escrow.is_active = false;
        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(&env, &key);

        // Decrement active recurring counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.artisan, -1);

        // CEI Pattern: INTERACTIONS - External calls AFTER state updates
        if remaining > 0 {
            let token_client = token::Client::new(&env, &escrow.token);
            token_client.transfer(&env.current_contract_address(), &escrow.buyer, &remaining);

            // Track locked funds (#212)
            Self::update_total_locked(&env, &escrow.token, -remaining);
        }

        env.events().publish(
            (Symbol::new(&env, "recurring_escrow"), id),
            RecurringEscrowEvent {
                id,
                action: RecurringEscrowAction::Cancelled,
                buyer: escrow.buyer.clone(),
                artisan: escrow.artisan.clone(),
                amount: remaining,
                timestamp: env.ledger().timestamp(),
            },
        );

        Self::exit_reentry_guard(&env);
    }

    /// Get details of a recurring escrow.
    pub fn get_recurring_escrow(env: Env, id: u64) -> RecurringEscrow {
        env.storage()
            .persistent()
            .get(&DataKey::RecurringEscrow(id))
            .expect("")
    }

    /// Recovery function to sweep unallocated tokens from the contract (admin only).
    /// Unallocated funds = current_balance - (total_locked_in_escrows + total_staked_by_artisans).
    pub fn sweep_unallocated_funds(env: Env, token: Address, destination: Address) -> Result<i128, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let token_client = token::Client::new(&env, &token);
        let balance = token_client.balance(&env.current_contract_address());
        
        let locked: i128 = env.storage().persistent().get(&DataKey::TotalLocked(token.clone())).unwrap_or(0);
        let staked: i128 = env.storage().persistent().get(&DataKey::TotalStaked(token.clone())).unwrap_or(0);
        
        let unallocated = balance - (locked + staked);
        
        if unallocated > 0 {
            token_client.transfer(&env.current_contract_address(), &destination, &unallocated);
        }
        
        Ok(unallocated)
    }
}
