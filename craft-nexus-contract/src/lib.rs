#![no_std]
#![allow(clippy::too_many_arguments)]
#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, xdr::ToXdr,
    Address, Bytes, BytesN, Env, IntoVal, Map, String, Symbol, TryFromVal, Val, Vec,
};
extern crate alloc;

/// Centralised time-boundary policy for the contract.
pub mod time_policy;

#[cfg(test)]
mod arbitration_escalation_test;
#[cfg(test)]
mod enhanced_features_test;
#[cfg(test)]
mod event_snapshot_test;
#[cfg(test)]
mod expired_dispute_fee_test;
#[cfg(test)]
mod min_release_window_test;
#[cfg(test)]
mod reentrancy_test;
#[cfg(test)]
mod sweep_allowance_test;
#[cfg(test)]
mod scalability_test;
#[cfg(test)]
mod time_boundary_test;
#[cfg(test)]
mod test;
#[cfg(test)]
mod pagination_boundary_test;
#[cfg(test)]
mod prop_test;

// Onboarding is a separate logical contract; only one `#[contract]` may be linked per WASM
// artifact. Keep it in this crate for host tests (`cargo test`) but omit from guest builds.
#[cfg(not(target_family = "wasm"))]
pub mod onboarding;

/// Centralized pagination input validation (Issue #1022).
pub mod pagination_validation;

/// Error codes grouped by category for off-chain triage.
///
/// # Categories
///
/// | Range   | Category     | Meaning                                         | Triage                    |
/// |---------|-------------|-------------------------------------------------|---------------------------|
/// | 1–9     | Auth/Access | Authorization, ownership, or existence failures | Rollback immediately      |
/// | 10–19   | State       | Invalid state transitions or preconditions      | Retry after state change  |
/// | 20–29   | Config      | Operator-configurable limits or misconfig       | Operator must act         |
/// | 30–39   | Operational | System or cooldown gates                        | Retry after cooldown      |
/// | 40–42   | Validation  | Input validation failures                       | Fix caller input          |
///
/// Use [`is_retryable`] to determine whether an error may succeed on retry.
#[contracterror(export = false)]
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum Error {
    // ── Auth / Access (1–9): rollback immediately ──
    /// The caller is not authorized for this operation. Ensure you are using
    /// the correct admin, arbitrator, moderator, buyer, or seller address.
    Unauthorized = 1,
    /// No escrow exists with the given order ID. Verify the order_id is
    /// correct and the escrow has not already been cleaned up.
    EscrowNotFound = 2,
    /// The escrow is not in the required state for this operation. For example,
    /// you cannot release a Disputed escrow or re-fund an already-funded escrow.
    /// Call get_escrow to inspect the current status before retrying.
    InvalidEscrowState = 3,
    /// DEPRECATED: Handled by onboarding contract. Retained for ABI compatibility.
    UsernameAlreadyExists = 4,
    /// The token is not on the platform whitelist. An admin must call
    /// whitelist_token before this token can be used in escrows.
    TokenNotWhitelisted = 5,
    /// The escrow amount is below the configured per-token minimum. Call
    /// get_fee_token_config to check the minimum, then increase the amount.
    AmountBelowMinimum = 6,
    /// The requested release window exceeds the platform-configured maximum.
    /// Call get_max_release_window to check the current ceiling.
    ReleaseWindowTooLong = 7,
    /// The escrow is not in the Disputed state; dispute resolution cannot
    /// proceed. The escrow must be in Disputed status before resolve_dispute
    /// can be called.
    NotInDispute = 8,
    /// DEPRECATED: Handled by onboarding contract. Retained for ABI compatibility.
    AlreadyOnboarded = 9,
    // ── State / Transition (10–19): retry after state change ──
    /// The fee exceeds the maximum allowed platform fee (MAX_PLATFORM_FEE_BPS,
    /// currently 10%). Reduce fee_bps and retry.
    InvalidFee = 10,
    /// The buyer and seller addresses are identical; self-escrow is not
    /// permitted. Use distinct buyer and seller addresses.
    SameBuyerSeller = 11,
    /// The platform has not been initialized. Call initialize before
    /// invoking any escrow operations.
    PlatformNotInitialized = 12,
    /// The escrow release window has not yet elapsed; auto-release is
    /// premature. Wait until created_at + release_window seconds have passed.
    ReleaseWindowNotElapsed = 13,
    /// Batch operation error (deprecated: use BatchLimitExceeded)
    BatchOperationFailed = 14,
    /// The contract is currently paused by an admin. Wait for the platform
    /// to be unpaused (is_paused returns false) before retrying.
    ContractPaused = 15,
    /// The dispute deadline (max_dispute_duration) has not yet elapsed;
    /// resolve_expired_dispute cannot be called yet. Wait until
    /// dispute_initiated_at + max_dispute_duration seconds have passed.
    DisputeExpired = 16,
    /// The artisan's staked collateral is below the required minimum. The
    /// artisan must call stake_tokens to top up before this operation proceeds.
    InsufficientStake = 17,
    /// The stake cooldown period has not yet elapsed. Wait until the
    /// cooldown_end timestamp has passed before attempting to unstake.
    StakeCooldownActive = 18,
    /// The partial refund amount is invalid: it must be positive and not
    /// exceed the escrow amount. Adjust refund_amount and retry.
    InvalidRefundAmount = 19,
    // ── Config / Resource (20–29): operator must act ──
    /// Partial refund proposal not found
    ProposalNotFound = 20,
    /// Partial refund proposal already exists for this order
    ProposalAlreadyExists = 21,
    /// A re-entrant call was detected and blocked. Do not call guarded
    /// functions recursively. Retry the operation as a standalone call.
    ReentryDetected = 22,
    /// The release window is below the platform-configured minimum
    /// (min_release_window). Call get_min_release_window to check the floor,
    /// then increase the window value.
    ReleaseWindowTooShort = 23,
    /// Staked funds can only be withdrawn in the original staking token
    StakeTokenMismatch = 24,
    /// Invalid admin address provided (zero address, invalid format, etc.)
    InvalidAdminAddress = 25,
    /// Platform configuration storage is corrupted or missing required fields
    CorruptedPlatformConfig = 26,
    /// Stake history queue is at capacity; requires pruning before new entries
    StakeQueueFull = 27,
    /// Admin recovery failed due to time lock or invalid conditions
    AdminRecoveryFailed = 28,
    /// Batch operation limit exceeded
    BatchLimitExceeded = 29,
    // ── Operational / Gates (30–39): retry after cooldown ──
    /// Deprecated function called (no-op for ABI compatibility)
    DeprecatedFunction = 30,
    /// No pending admin transfer to accept or cancel
    NoPendingAdmin = 31,
    /// No WASM upgrade has been proposed
    NoUpgradeProposed = 32,
    /// WASM upgrade cooldown period is still active
    UpgradeCooldownActive = 33,
    /// A WASM upgrade proposal already exists
    UpgradeProposalExists = 34,
    /// Invalid WASM upgrade hash provided
    InvalidUpgradeHash = 35,
    /// Recurring escrow not found
    RecurringEscrowNotFound = 36,
    /// Escrow cycle not ready for release
    CycleNotReady = 37,
    /// Recurring escrow ID counter has reached its maximum safe value
    RecurringEscrowIdExhausted = 38,
    /// Onboarding contract address has not been configured
    OnboardingContractNotSet = 39,
    /// The configured onboarding contract rejected the participant state proof
    OnboardingAuthorizationFailed = 56,
    // ── Validation (40+): fix caller input ──
    /// Provided metadata hash is invalid
    InvalidMetadataHash = 40,
    /// Provided IPFS hash is invalid
    InvalidIpfsHash = 41,
    /// Caller is not an authorized upgrade signer
    NotAnUpgradeSigner = 42,
    /// The same signer already approved this WASM upgrade hash
    AlreadyApproved = 43,
    /// Token decimal places are outside the supported range (0–18)
    InvalidTokenDecimals = 44,
    /// No compatibility manifest has been submitted for the upgrade
    UpgradeCompatibilityMissing = 45,
    /// The compatibility manifest does not describe the current state/version
    UpgradeCompatibilityInvalid = 46,
    /// The migration report contains records that require manual handling
    UpgradeMigrationIncomplete = 47,
    /// Persisted storage is on a legacy layout that must be migrated first.
    StorageLayoutMismatch = 48,
    /// Admin action is in a terminal state (executed or cancelled)
    AdminActionTerminal = 49,
    /// Admin action does not yet have enough approvals
    AdminActionNeedsApprovals = 50,
    /// Admin action timelock is still active
    AdminActionTimelockActive = 51,
    /// Caller is not an authorized admin action signer
    NotAnAdminActionSigner = 52,
    /// Evidence retention window has expired or is invalid (#927)
    EvidenceExpired = 53,
    /// Evidence payload has already been used in a previous dispute (#927)
    EvidenceAlreadyUsed = 54,
    /// Invalid dispute session for evidence submission (#927)
    InvalidDisputeSession = 55,
    /// Contract does not implement the supported token interface.
    UnsupportedToken = 560,
    /// The requested continuation size is outside the scheduler bound.
    InvalidBatchWorkLimit = 57,
    /// The scheduled batch was cancelled.
    BatchJobCancelled = 58,
    /// The requested scheduled batch does not exist.
    BatchJobNotFound = 59,
    /// The caller is not the account that scheduled the batch.
    BatchJobUnauthorized = 60,
    /// The scheduled batch has already reached a terminal state.
    BatchJobCompleted = 61,
    /// Platform wallet cannot be the contract address.
    InvalidPlatformWallet = 62,
    /// Provided service-agreement hash is invalid
    InvalidServiceAgreementHash = 63,
    /// Evidence challenge window has not elapsed; arbitrator resolution is blocked.
    ChallengeWindowActive = 64,
    /// The arbitrator address is blacklisted.
    ArbitratorBlacklisted = 65,
    /// Dispute action is not valid in the current session (duplicate escalate, bad parent evidence).
    InvalidDisputeAction = 66,
    /// Dispute escalation window has not elapsed.
    EscalationWindowActive = 67,
    /// Arbitrator resolution deadline (`max_dispute_duration`) has elapsed.
    ArbitratorDeadlineExceeded = 68,
    /// This escrow was already settled; a second settlement path cannot run.
    SettlementAlreadyFinalized = 69,
    /// Tracked obligations exceed the token balance held by the contract.
    EmergencyAccountingInvariant = 70,
    /// A reconciliation report has unresolved customer or collateral liabilities.
    ReconciliationRequired = 71,
    /// The requested reconciliation repair plan does not exist.
    RepairPlanNotFound = 72,
    /// The reconciliation repair plan has already reached a terminal state.
    RepairPlanTerminal = 73,
    /// The live token state no longer matches the reviewed repair plan.
    RepairPlanPreconditionFailed = 74,
    /// The user does not have an onboarding profile registered with the
    /// configured onboarding contract.
    OnboardingProfileNotFound = 75,
    /// The user's onboarding profile is not in an active state (deactivated,
    /// under review, or flagged).
    OnboardingProfileInactive = 76,
    /// The user's onboarding role does not permit the requested marketplace
    /// operation.
    OnboardingRoleMismatch = 77,
    /// The user's onboarding profile state version does not match the expected
    /// current version — stale onboarding state detected.
    OnboardingProfileStale = 78,
    /// The user's verification status has been revoked or is not current.
    OnboardingVerificationRevoked = 79,
    /// An escrow with this order ID already exists. Duplicate escrow
    /// identifiers are rejected so a retry (or a conflicting external
    /// reference) can never overwrite an existing escrow's state.
    EscrowAlreadyExists = 80,
    /// Pagination limit is zero; caller must request at least one item (#1022).
    PaginationLimitZero = 81,
    /// Pagination cursor is invalid (past end of dataset or empty dataset) (#1022).
    PaginationCursorInvalid = 82,
    /// Requested WASM upgrade cooldown is below `MIN_WASM_UPGRADE_COOLDOWN`,
    /// which would let the mandatory review window be bypassed (#1062).
    UpgradeCooldownTooShort = 83,
}

/// Returns `true` if the error is transient and the operation may succeed on retry.
///
/// Retryable errors are those that depend on time, state change, or operator
/// action that is expected to resolve. Non-retryable errors (auth, not-found,
/// validation, permanent config) will **never** succeed on retry without
/// a different input or caller.
#[must_use]
pub fn is_retryable(error: Error) -> bool {
    matches!(
        error,
        Error::InvalidEscrowState
            | Error::ReleaseWindowNotElapsed
            | Error::ContractPaused
            | Error::DisputeExpired
            | Error::StakeCooldownActive
            | Error::ReentryDetected
            | Error::StakeQueueFull
            | Error::UpgradeCooldownActive
            | Error::CycleNotReady
            | Error::BatchLimitExceeded
            | Error::ChallengeWindowActive
            | Error::EscalationWindowActive
            | Error::ArbitratorDeadlineExceeded
    )
}

const ESCROW: Symbol = symbol_short!("ESCROW");
const PLATFORM_FEE: Symbol = symbol_short!("PLAT_FEE");
const PLATFORM_WALLET: Symbol = symbol_short!("PLAT_WAL");
const ONBOARD_CALL_FAILED: Symbol = symbol_short!("OB_FAIL");

const BASE58_BTC_CHARSET: [bool; 256] = {
    let mut chars = [false; 256];

    let mut i = b'1' as usize;
    while i <= b'9' as usize {
        chars[i] = true;
        i += 1;
    }

    i = b'A' as usize;
    while i <= b'H' as usize {
        chars[i] = true;
        i += 1;
    }

    i = b'J' as usize;
    while i <= b'N' as usize {
        chars[i] = true;
        i += 1;
    }

    i = b'P' as usize;
    while i <= b'Z' as usize {
        chars[i] = true;
        i += 1;
    }

    i = b'a' as usize;
    while i <= b'k' as usize {
        chars[i] = true;
        i += 1;
    }

    i = b'm' as usize;
    while i <= b'z' as usize {
        chars[i] = true;
        i += 1;
    }

    chars
};
const TOTAL_FEES: Symbol = symbol_short!("TOT_FEES");

/// Standard TTL threshold for persistent storage (approx 14 hours at 5s ledger)
const TTL_THRESHOLD: u32 = 10_000;
/// Lower TTL threshold used for hot index reads to reduce the cost of frequent
/// TTL refresh calls (Issue #533).
const READ_TTL_THRESHOLD: u32 = 1_000;
/// Standard TTL extension for persistent storage (approx 30 days)
const TTL_EXTENSION: u32 = 518_400;

// Default configuration constants (can be overridden via PlatformConfig)
// Re-exported from the centralised time_policy module for single source of truth.
/// Default grace period for WASM upgrades (7 days in seconds)
const DEFAULT_WASM_UPGRADE_COOLDOWN: u32 = time_policy::WASM_UPGRADE_COOLDOWN as u32;
/// Minimum enforceable WASM upgrade cooldown (1 day in seconds) (#1062).
///
/// `execute_upgrade` correctly rejects execution before `upgrade_at`, but that
/// review window is only meaningful if it cannot be trivially shortened. Without
/// a floor here, a single admin call to `set_wasm_upgrade_cooldown(0)` right
/// before proposing an upgrade would let it execute immediately, defeating the
/// whole point of the timelock.
const MIN_WASM_UPGRADE_COOLDOWN: u32 = 24 * 60 * 60;
/// Minimum time (seconds) that must elapse after a cancel_upgrade_wasm call
/// before propose_upgrade_wasm is accepted again (Issue #618).
/// Prevents the cancel-and-repropose pattern that resets the review window.
const CANCEL_REPROPOSE_COOLDOWN: u64 = time_policy::CANCEL_REPROPOSE_COOLDOWN;

/// Default maximum duration a dispute can remain open before it can be force-resolved (30 days in seconds)
const DEFAULT_MAX_DISPUTE_DURATION: u32 = time_policy::MAX_DISPUTE_DURATION as u32;

/// Default cooldown period after staking before tokens can be unstaked (7 days in seconds)
const DEFAULT_STAKE_COOLDOWN: u32 = time_policy::STAKE_COOLDOWN as u32;

/// Default minimum release window to prevent "flash" auto-releases (1 day in seconds)
const DEFAULT_MIN_RELEASE_WINDOW: u32 = time_policy::MIN_RELEASE_WINDOW as u32;
/// Absolute safety ceiling for admin-configurable max release window (365 days).
const ABSOLUTE_MAX_RELEASE_WINDOW: u32 = time_policy::ABSOLUTE_MAX_RELEASE_WINDOW as u32;

/// Default evidence expiry / retention window (7 days in seconds) (#927)
const DEFAULT_EVIDENCE_EXPIRY_WINDOW: u64 = time_policy::EVIDENCE_EXPIRY_WINDOW;
/// Default challenge period window before a dispute can be resolved (1 day in seconds) (#942)
const DEFAULT_EVIDENCE_CHALLENGE_WINDOW: u32 = time_policy::EVIDENCE_CHALLENGE_WINDOW as u32;
/// Default dispute escalation window (3 days in seconds) (#941)
const DEFAULT_DISPUTE_ESCALATION_WINDOW: u32 = time_policy::DISPUTE_ESCALATION_WINDOW as u32;
/// Default rate limit max calls per window (#943)
const DEFAULT_RATE_LIMIT_MAX_CALLS: u32 = 5;
/// Default rate limit window (1 hour in seconds) (#943)
const DEFAULT_RATE_LIMIT_WINDOW: u32 = time_policy::RATE_LIMIT_WINDOW as u32;

/// Maximum platform fee in basis points (10000 = 100%)
const MAX_PLATFORM_FEE_BPS: u32 = 1000; // 10% max
const MAX_TOTAL_RELEASE_WINDOW: u32 = time_policy::MAX_TOTAL_RELEASE_WINDOW as u32;
const CURRENT_ESCROW_VERSION: u32 = 4;
/// Explicit storage layout version for persisted contract state.
///
/// New deployments initialize this to `CURRENT_STORAGE_LAYOUT_VERSION`; legacy
/// deployments without the key must run `migrate_storage_layout` before any
/// WASM upgrade can be executed.
const CURRENT_STORAGE_LAYOUT_VERSION: u32 = 1;
/// Maximum number of escrows per batch operation (Issue #111)
// Conservative batch size to avoid exceeding instruction/read-write limits
// observed on Soroban testnets. Reduced from 100 to 20 (Issue #198).
const MAX_BATCH_SIZE: u32 = 20;
/// Maximum number of escrows a scheduled continuation may process.
const MAX_SCHEDULED_BATCH_WORK: u32 = 5;
const MAX_PAGE_SIZE: u32 = 100;
/// Timeout for unfunded escrows before they can be cancelled (24 hours) (#213)
const UNFUNDED_CANCEL_TIMEOUT: u64 = time_policy::UNFUNDED_CANCEL_TIMEOUT;
/// Hard ceiling for `NextRecurringEscrowId` (Issue #233).
///
/// `u64::MAX` is reserved as a sentinel so the allocator can detect an
/// exhausted ID space without wrapping. At the realistic peak rate of
/// one new recurring escrow per ledger this cap is far beyond any
/// practical deployment lifetime, but the explicit bound lets us fail
/// fast with `Error::RecurringEscrowIdExhausted` instead of silently
/// colliding with an existing entry.
const MAX_RECURRING_ESCROW_ID: u64 = u64::MAX - 1;
/// Deterministic fee policy version. Bump when fee allocation formulas change.
const FEE_POLICY_VERSION: u32 = 1;
/// Maximum number of upgrade records retained in `UpgradeHistory`. Older
/// records are dropped FIFO once the cap is reached. Sized so a contract
/// upgraded twice a year for ~16 years still has full visibility.
const MAX_UPGRADE_HISTORY: u32 = 32;

/// Symbol topics emitted alongside `UpgradeProposalEvent`.
const UPGRADE_PROPOSED: Symbol = symbol_short!("UPG_PROP");
const UPGRADE_CANCELLED: Symbol = symbol_short!("UPG_CANC");
const UPGRADE_EXECUTED: Symbol = symbol_short!("UPG_EXEC");
/// Maximum number of stake history entries per artisan (bounded queue to prevent storage bloat) (#237)
const MAX_STAKE_HISTORY_SIZE: u32 = 100;
/// Threshold at which to trigger automatic pruning of old stake history entries (#237)
const STAKE_HISTORY_PRUNE_THRESHOLD: u32 = 80;
/// Maximum number of stake deposits per artisan queue (bounded to prevent storage bloat)
const MAX_STAKE_QUEUE_SIZE: u32 = 50;
/// Threshold at which to trigger automatic pruning of matured stake deposits
const STAKE_QUEUE_PRUNE_THRESHOLD: u32 = 40;
/// Time lock period before admin recovery is allowed (7 days) (#240)
const ADMIN_RECOVERY_DELAY: u64 = time_policy::ADMIN_RECOVERY_DELAY;
/// Minimum allowed admin recovery cooldown. Deploys attempting to set a
/// shorter window (including zero) will be rejected during recovery.
const MIN_ADMIN_RECOVERY_COOLDOWN: u64 = time_policy::MIN_ADMIN_RECOVERY_COOLDOWN;
/// Default timelock delay for pending critical admin actions (24 hours).
const DEFAULT_ADMIN_ACTION_TIMELOCK_DELAY: u64 = time_policy::ADMIN_ACTION_TIMELOCK_DELAY;

#[contracttype(export = false)]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum DisputeTransition {
    Initiate,
    SubmitEvidence,
    Escalate,
    ProposeRefund,
    AcceptRefund(Address), // address of the proposer
    CancelRefund(Address), // address of the proposer
    ResolveArbitrated,
}

/// The kind of critical admin action that requires multi-sig approval
/// and timelock enforcement.
#[contracttype(export = false)]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum AdminActionKind {
    PausePlatform(bool),
    SetPlatformFee(u32),
    SetPlatformWallet(Address),
    SetWasmUpgradeCooldown(u32),
    SetMinStakeRequired(i128),
    SweepUnallocatedFunds(Address, Address),
    ExecuteUpgrade(BytesN<32>),
    SetMaxDisputeDuration(u32),
    SetStakeCooldown(u32),
    SetArtisanFeeTier(Address, u32),
    SetModerator(Address),
    SetMinEscrowAmount(Address, i128),
    SetMaxReleaseWindow(u32),
    SetMinReleaseWindow(u32),
    SetOnboardingContract(Address),
    SetExpiredDisputePolicy(ExpiredDisputeFeePolicy),
    ApplyReconciliationRepair(u64),
}

/// A pending critical admin action proposal that requires multi-sig
/// approvals and a timelock before execution.
#[contracttype(export = false)]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct AdminActionProposal {
    pub id: u64,
    pub kind: AdminActionKind,
    pub proposer: Address,
    pub approvals: Vec<Address>,
    pub threshold: u32,
    pub signers: Vec<Address>,
    pub created_at: u64,
    pub ready_at: u64,
    pub executed: bool,
    pub cancelled: bool,
}

/// Storage keys for the admin action proposal system.
#[contracttype(export = false)]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum AdminActionDataKey {
    NextAdminActionId,
    AdminAction(u64),
    AdminActionSigners,
    AdminActionThreshold,
    AdminActionTimelockDelay,
}

#[contracttype(export = false)]
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
    FeeTokenConfig(Address),
    ContractVersion,
    /// Platform configuration storage key
    PlatformConfig,
    /// Explicit storage layout version for persisted state.
    StorageLayoutVersion,
    /// Custom fee tier for an artisan (basis points)
    ArtisanFeeTier(Address),
    /// Staked token amount and asset for an artisan
    ArtisanStake(Address),
    /// DEPRECATED legacy storage: Token address backing an artisan's staked balance.
    ///
    /// Replaced by [`ArtisanStake::token`] in [`ArtisanStakeData`]. Kept for
    /// lazy migration of pre-v2 stake records during contract upgrades.
    ArtisanStakeToken(Address),
    StakeCooldownEnd(Address),
    /// DEPRECATED single-cooldown timestamp for an artisan.
    ///
    /// Active stake/unstake logic uses [`DataKey::ArtisanStakeQueue`]; this
    /// key is **never read** by any code path in the live contract and
    /// cannot influence cooldown decisions. It is updated alongside the
    /// queue (set to the latest `cooldown_end`) purely so older read-only
    /// clients still see a meaningful value. Once a queue is fully
    /// drained the key is removed in `unstake_tokens`.
    ///
    ///
    /// Per-deposit stake queue for an artisan. Each entry represents an
    /// individual deposit and its cooldown end timestamp. This allows
    /// accurate tracking of staking timeframes when multiple deposits
    /// are made at different times.
    ArtisanStakeQueue(Address),
    /// Count of entries in the artisan stake queue (for bounds checking)
    ArtisanStakeQueueCount(Address),
    /// Indexed storage of stake deposits (Address, index) -> StakeDeposit
    ArtisanStakeQueueIndexed(Address, u32),
    /// Partial refund proposal for a disputed order
    PartialRefundProposal(u32),
    /// Monotonic revision counter for partial refund proposals
    PartialRefundNonce(u32),
    /// Terminal settlement receipt; presence means the dispute is finalized.
    SettlementReceipt(u32),
    /// Blacklisted arbitrator address
    ArbitratorBlacklist(Address),
    /// Count of currently open disputes
    ActiveDisputeCount,
    /// Cumulative funded escrow volume
    TotalVolume,
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
    /// DEPRECATED legacy storage: Map of whitelisted token addresses (Address -> bool).
    /// New code stores each token as an individual key-value pair.
    WhitelistedTokens,
    /// Individual whitelisted token entry (Address -> bool)
    WhitelistedTokenIndexed(Address),
    /// Count of whitelisted tokens for efficient enumeration.
    WhitelistedTokenCount,
    /// DEPRECATED: Legacy monolithic Vec of all escrow order IDs.
    /// New writes use [`DataKey::GlobalEscrowIdIndexed`] (#515). Kept for
    /// lazy migration on the next index update or paginated read.
    AllEscrowIds,
    /// Total count of escrows ever created; O(1) length for indexed enumeration
    EscrowCount,
    /// Indexed global escrow order ID by creation sequence (#515).
    /// Each entry stores one `u32` order ID, avoiding Vec rewrites on batch create.
    GlobalEscrowIdIndexed(u32),
    /// Fallback admin address for recovery if primary admin storage is corrupted (#240)
    FallbackAdmin,
    /// Timestamp when admin recovery mechanism becomes available (time-lock safety).
    /// Stored as a compact `u64` ledger timestamp (#431 / key index #30).
    AdminRecoveryTime,
    /// The configured delay (seconds) that was recorded when the recovery time
    /// was initiated. Used to validate that a minimum cooldown was respected.
    AdminRecoveryDelay,
    /// Historical record of stake changes per artisan (bounded queue for audit trail) (#237)
    StakeHistory(Address),
    /// Count of entries in the stake history queue (bounds checking)
    StakeHistoryCount(Address),
    /// Timestamp when an artisan's stake was last modified (for maintenance checks)
    StakeLastModified(Address),
    /// Number of fund-movement audit entries for a given actor/account.
    FundAuditCount(Address),
    /// Indexed fund-movement audit entry for an actor/account.
    FundAuditIndexed(Address, u32),
    /// Indexed storage of a buyer's escrow ID by position
    BuyerEscrowIndexed(Address, u32),
    /// Indexed storage of a seller's escrow ID by position
    SellerEscrowIndexed(Address, u32),
    /// Count of a buyer's escrows
    BuyerEscrowCount(Address),
    /// Count of a seller's escrows
    SellerEscrowCount(Address),
    /// Total locked funds across all active escrows for a given token address.
    TotalLocked(Address),
    /// Total amount of funds currently staked by artisans for a token address.
    TotalStaked(Address),
    /// Indexed artisan address with a persisted stake record.
    StakedArtisanIndexed(u32),
    /// Number of indexed artisan stake records.
    StakedArtisanCount,
    /// Latest completed reconciliation result for a token address.
    ReconciliationReport(Address),
    /// In-progress reconciliation accumulator for a token address.
    ReconciliationProgress(Address),
    /// Repair plan awaiting explicit admin-action approval.
    ReconciliationRepairPlan(u64),
    /// Monotonic repair-plan identifier.
    NextReconciliationRepairPlanId,
    /// Bounded log of completed WASM upgrades. Capped at MAX_UPGRADE_HISTORY
    UpgradeHistory,
    /// Compatibility evidence for completed WASM upgrades.
    UpgradeCompatibilityHistory,
    /// Key for a recurring escrow by its ID
    RecurringEscrow(u64),
    /// ID counter for recurring escrows
    NextRecurringEscrowId,
    /// Number of recurring escrow records created.
    RecurringEscrowCount,
    /// Persisted resource-aware batch escrow job.
    BatchEscrowJob(u64),
    /// Count of currently active (non-released, non-refunded) escrows or recurring escrows for a user address.
    ActiveObligations(Address),
    /// Required number of distinct signer approvals before a WASM upgrade proposal is committed.
    UpgradeThreshold,
    /// Canonical per-round approval state (signers snapshot, threshold snapshot,
    /// round nonce, and accumulated approvals).  Replaces the old hash-keyed
    /// `UpgradeApprovals(BytesN<32>)` to prevent cross-round replay.
    /// Always stored at index 0; the nonce lives inside the struct.
    UpgradeApprovalState(u32),
    /// Ordered list of addresses authorized to co-sign WASM upgrade proposals.
    UpgradeSigners,
    /// Ledger timestamp (u64) recorded when the last upgrade proposal was
    /// cancelled. Used to enforce CANCEL_REPROPOSE_COOLDOWN (Issue #618).
    LastUpgradeCancelledAt,
    /// Differential compatibility manifest keyed by the proposed WASM hash.
    UpgradeCompatibilityManifest(BytesN<32>),
    /// Structured evidence log for a disputed escrow order (#927)
    EvidenceLog(u32),
    /// Submitted evidence hash to prevent reuse across disputes (#927)
    UsedEvidenceHash(BytesN<32>),
    /// Escalation record for a dispute (#941)
    DisputeEscalation(u32),
    /// Configurable dispute escalation window in seconds (#941)
    DisputeEscalationWindow,
    /// Counter for rate-limited calls per address per window (#943)
    RateLimitCount(Address, u64),
    /// Platform rate limit configuration (max_calls, window) (#943)
    RateLimitConfig,
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
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum RecurringEscrowAction {
    Created = 0,
    CycleReleased = 1,
    Cancelled = 2,
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
    pub frequency: u64,
    pub duration: u32,
    pub current_cycle: u64,
    pub last_release_time: u64,
    pub is_active: bool,
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

/// Lifecycle status of an escrow order.
///
/// # Live variants
/// - `Active` — funded (or created) and open for release / refund / dispute
/// - `Released` — funds sent to the seller
/// - `Refunded` — funds returned to the buyer
/// - `Disputed` — dispute opened; awaiting arbitrator resolution
/// - `Resolved` — dispute resolved (release or refund completed)
/// - `ReleasePending` / `RefundPending` / `DisputePending` — in-flight
///  CEI transitions claimed while an external call is outstanding
///
/// # Removed legacy variants (issue #706)
/// `Draft` and `UnderReview` were deprecated in contract version 1.2 and are
/// **not** part of this enum. Do not reintroduce them — they caused confusion
/// with the live lifecycle and are unused by every transition path.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PlatformStats {
    pub total_volume: i128,
    pub total_escrows: u32,
    pub active_users: u32,
    pub whitelist_count: u32,
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
    ReleasePending = 5,
    RefundPending = 6,
    DisputePending = 7,
    /// In-flight exclusive claim while a dispute settlement path executes.
    SettlementPending = 8,
}

#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum EscrowStateIssue {
    None = 0,
    EscrowNotFound = 1,
    PendingTransitionUnfinished = 2,
    MissingDisputeTimestamp = 3,
    InvalidTerminalState = 4,
    SettlementReceiptConflict = 5,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscrowStateDiagnostic {
    pub order_id: u32,
    pub status: EscrowStatus,
    pub is_consistent: bool,
    pub issue: EscrowStateIssue,
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

/// Describes which settlement formula to apply when computing a `FeeAllocation`.
///
/// Every terminal settlement path must supply one of these variants so that
/// `compute_fee_allocation` can deterministically decide how the escrow pot is
/// split among platform, seller, and buyer.  Adding a new path means adding a
/// new variant here; all existing invariant tests will catch regressions.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum SettlementKind {
    /// Normal release (buyer-approved or auto-release).
    /// Platform fee deducted from the seller's portion; buyer pays nothing.
    ReleaseFunds,
    /// Full refund with no fee (admin-initiated or dispute RefundToBuyer).
    /// Buyer receives the entire escrow amount; platform collects nothing.
    FullRefundNoFee,
    /// Expired-dispute resolution: buyer receives full amount, platform fee
    /// comes only from the seller's locked pot.
    ExpiredDisputeDeductFromSeller,
    /// Expired-dispute resolution: platform fee deducted from the buyer's
    /// refund; seller receives nothing additional.
    ExpiredDisputeDeductFromBuyer,
    /// Expired-dispute resolution: fee split equally between buyer and seller.
    ExpiredDisputeSplitFee,
    /// Partial-refund settlement. `refund_gross` and `seller_gross` are the
    /// gross portions *before* fees, supplied as context fields.
    PartialRefund(i128, i128),
}

/// Output of `compute_fee_allocation`.
///
/// Every value is non-negative and the three amounts sum exactly to the
/// original `escrow.amount`, guaranteeing the contract never leaks or
/// over-pays:
///
/// ```text
/// platform_fee + seller_amount + buyer_amount == escrow_amount
/// ```
///
/// Callers **must** use these three values — and only these three values —
/// when performing token transfers in any settlement path.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FeeAllocation {
    /// Amount transferred to the platform wallet.
    pub platform_fee: i128,
    /// Net amount transferred to the seller (artisan).
    pub seller_amount: i128,
    /// Net amount transferred back to the buyer.
    pub buyer_amount: i128,
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
    pub dispute_reason: Option<Symbol>,
    pub dispute_initiated_at: Option<u64>,
    pub funded: bool,
    /// Ledger timestamp after which any party (or admin) may cancel this escrow
    /// if it has not yet been funded. Set to created_at + UNFUNDED_CANCEL_TIMEOUT
    /// for unfunded escrows; None for escrows that were funded at creation (#656).
    pub funding_deadline: Option<u64>,
    pub service_agreement_hash: Option<Bytes>,
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

/// Escrow format before service_agreement_hash was added (#708).
/// Used for backward-compatible deserialization during v4→v5 migration.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
struct EscrowV4 {
    pub version: u32,
    pub id: u64,
    pub batch_id: Option<u64>,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub status: EscrowStatus,
    pub release_window: u32,
    pub created_at: u32,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
    pub dispute_reason: Option<Symbol>,
    pub dispute_initiated_at: Option<u64>,
    pub funded: bool,
    pub funding_deadline: Option<u64>,
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
pub struct FundMovementAuditEntry {
    pub actor: Address,
    pub amount: i128,
    pub reason: Symbol,
    pub timestamp: u64,
    pub balance_impact: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FundAllocation {
    pub balance: i128,
    pub total_locked: i128,
    pub total_staked: i128,
    pub unallocated: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ReconciliationReport {
    pub token: Address,
    pub balance: i128,
    pub expected_locked: i128,
    pub expected_staked: i128,
    pub tracked_locked: i128,
    pub tracked_staked: i128,
    pub scanned_escrows: u32,
    pub next_cursor: u32,
    pub complete: bool,
    pub unresolved: bool,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ReconciliationRepairPlan {
    pub id: u64,
    pub token: Address,
    pub expected_locked: i128,
    pub expected_staked: i128,
    pub observed_balance: i128,
    pub observed_tracked_locked: i128,
    pub observed_tracked_staked: i128,
    pub created_at: u64,
    pub applied: bool,
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscrowEvent {
    /// Schema version for this event payload. Increment when fields are added
    /// or reordered so off-chain indexers can handle multiple schema generations
    /// without breaking across upgrades. Current version: 1.
    pub schema_version: u32,
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
pub struct EscrowResolvedEvent {
    /// Schema version for this event payload. Increment when fields are added
    /// or reordered so off-chain indexers can handle multiple schema generations
    /// without breaking across upgrades. Current version: 1.
    pub schema_version: u32,
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub arbitrator: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ReputationUpdateEvent {
    pub address: Address,
    pub successful_delta: u32,
    pub disputed_delta: u32,
    pub metrics_sales_delta: u32,
    pub metrics_amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

/// Tagged union used to carry a single configuration value inside
/// [`ConfigUpdatedEvent`].
///
/// Soroban events must be self-describing for off-chain indexers, but
/// `PlatformConfig` fields are heterogeneous (counts, monetary amounts,
/// addresses, and free-form strings). Rather than emit a separate event type
/// per field — which would bloat the contract's event ABI — every admin
/// configuration change is normalized into one of these four variants. Indexers
/// match on the variant tag to recover the underlying Rust type without any
/// loss of precision (in particular, `I128` monetary values are never
/// stringified on-chain; see the note on [`EscrowEvent::amount`]).
///
/// # Variant mapping
///
/// * `U32`     — bounded counters and basis-point fees (e.g. `platform_fee_bps`).
/// * `I128`    — monetary thresholds such as `min_escrow_amount`.
/// * `Address` — role and token addresses (e.g. `fee_collector`).
/// * `String`  — human-readable identifiers that have no compact encoding.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ConfigValue {
    U32(u32),
    I128(i128),
    Address(Address),
    String(String),
}

/// Emitted whenever an admin mutates a single field of the on-chain
/// `PlatformConfig`.
///
/// # Topics
///
/// Published under `(symbol "config_updated", symbol field_name)` so indexers
/// can subscribe to changes of a specific field cheaply. The `field_name` topic
/// mirrors the `field_name` payload member.
///
/// # Preconditions
///
/// * The caller must be the current platform admin; the emitting function
///   asserts `admin.require_auth()` before the storage write, so this event is
///   only ever observed for an authorized change.
///
/// # Storage side-effects
///
/// * The corresponding `PlatformConfig` field has already been persisted by the
///   time this event fires. The event is emitted *after* the storage write,
///   in keeping with the check-effects-interactions ordering used throughout
///   the contract.
///
/// # Payload
///
/// * `field_name` — symbolic name of the mutated field.
/// * `old_value`  — value held immediately before the write.
/// * `new_value`  — value persisted by this update.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ConfigUpdatedEvent {
    pub field_name: Symbol,
    pub old_value: ConfigValue,
    pub new_value: ConfigValue,
}

/// Emitted when an artisan's negotiated platform-fee tier is set or changed.
///
/// Per-artisan fee tiers let the platform reward high-reputation sellers with a
/// reduced `fee_bps` (basis points, where `10_000` == 100%). The persisted tier
/// overrides the global `platform_fee_bps` for that artisan's future escrows.
///
/// # Topics
///
/// Published under `(symbol "artisan_fee_tier_updated", address artisan)` so a
/// client can stream the fee history of a single artisan.
///
/// # Preconditions
///
/// * The caller must be the platform admin (`require_auth`).
/// * `fee_bps` is validated against `MAX_PLATFORM_FEE_BPS`; an out-of-range
///   value aborts with [`Error::InvalidFee`] and no event is emitted.
///
/// # Storage side-effects
///
/// * The artisan's fee-tier ledger entry is written (and its TTL extended)
///   before this event fires.
///
/// # Payload
///
/// * `artisan` — address whose fee tier was updated.
/// * `fee_bps` — the new fee in basis points applied to future escrows.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct ArtisanFeeTierUpdatedEvent {
    pub artisan: Address,
    pub fee_bps: u32,
}

/// Emitted when an artisan stakes collateral tokens into the platform.
///
/// Staking is a precondition for accepting high-value escrows; the staked
/// balance backs the artisan's dispute exposure. Token movement obeys the
/// check-effects-interactions pattern: the artisan's persistent stake balance
/// is increased and committed *before* the external `token.transfer` callback,
/// so a malicious token contract cannot re-enter and observe a stale balance.
///
/// # Topics
///
/// Published under `(symbol "tokens_staked", address artisan)`.
///
/// # Preconditions
///
/// * `artisan.require_auth()` — only the staker may stake on their own behalf.
/// * `amount` must be positive and `token` whitelisted.
///
/// # Storage side-effects
///
/// * The artisan's staked-balance entry is incremented and its TTL extended.
///
/// # Payload
///
/// * `artisan` — the staking address.
/// * `token`   — the staked token's contract address.
/// * `amount`  — raw token amount staked (never stringified on-chain).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct TokensStakedEvent {
    pub artisan: Address,
    pub token: Address,
    pub amount: i128,
}

/// Emitted when an artisan withdraws previously staked collateral.
///
/// # Topics
///
/// Published under `(symbol "tokens_unstaked", address artisan)`.
///
/// # Preconditions
///
/// * `artisan.require_auth()`.
/// * The stake cooldown must have elapsed, otherwise the call aborts with
///   [`Error::StakeCooldownActive`] and no event is emitted.
/// * The withdrawal token must match the original staking token
///   ([`Error::StakeTokenMismatch`]).
///
/// # Storage side-effects
///
/// * The artisan's staked-balance entry is decremented *before* the outbound
///   `token.transfer`, preserving reentrancy safety (the transfer is the final
///   interaction in the call path).
///
/// # Payload
///
/// * `artisan` — the withdrawing address.
/// * `token`   — the unstaked token's contract address.
/// * `amount`  — raw token amount returned to the artisan.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct TokensUnstakedEvent {
    pub artisan: Address,
    pub token: Address,
    pub amount: i128,
}

/// Emitted when an order's off-chain metadata is verified against its on-chain
/// commitment.
///
/// The contract stores only a compact hash of an order's metadata (see
/// [`EscrowMetadata`]); the full document lives off-chain (e.g. IPFS). When a
/// verifier reveals the document and the contract confirms its hash matches the
/// stored commitment, this event records the successful verification so
/// indexers can mark the order's metadata as trusted.
///
/// # Topics
///
/// Published under `(symbol "metadata_verified", u64 order_id)`.
///
/// # Preconditions
///
/// * The stored commitment must exist and the revealed content must hash to it,
///   otherwise the call aborts with [`Error::InvalidMetadataHash`].
///
/// # Storage side-effects
///
/// * None beyond TTL refresh of the order entry; verification is a read-and-
///   compare operation that emits this audit event.
///
/// # Payload
///
/// * `order_id`  — the escrow/order whose metadata was verified.
/// * `verifier`  — address that submitted the reveal proof.
/// * `timestamp` — ledger timestamp at verification time.
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
    pub service_agreement_hash: Option<Bytes>,
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

/// Test-only metadata structure for simplified testing
#[cfg(test)]
#[derive(Clone, Eq, PartialEq)]
pub struct Metadata {
    pub title: String,
    pub description: String,
    pub category: String,
}

/// Proposal record for a pending WASM upgrade.
///
/// `upgrade_at` is the earliest ledger timestamp at which `execute_upgrade` may
/// run; it equals `proposed_at + wasm_upgrade_cooldown` from `PlatformConfig`.
/// `proposed_by` records the admin that submitted the proposal — note that the
/// admin role can rotate via the two-step transfer (`update_admin` /
/// `claim_admin`), so the value reflects the admin at proposal time, not at
/// execution time. `execute_upgrade` re-checks the *current* admin's auth, so
/// rotating admins cannot bypass authorization.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct WasmUpgradeProposal {
    pub wasm_hash: BytesN<32>,
    pub upgrade_at: u64,
    pub proposed_by: Address,
    pub proposed_at: u64,
}

/// Lifecycle event emitted whenever a WASM upgrade proposal is created,
/// replaced, cancelled, or executed. Indexers can use the `action` symbol to
/// reconstruct the upgrade audit trail without scanning storage.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeProposalEvent {
    pub action: Symbol,
    pub wasm_hash: BytesN<32>,
    pub admin: Address,
    pub timestamp: u64,
    pub upgrade_at: u64,
}

/// On-chain record of a completed WASM upgrade.
///
/// One entry is appended to `UpgradeHistory` per successful `execute_upgrade`
/// call, providing operators and auditors visibility into how the contract
/// reached its current `ContractVersion`.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeRecord {
    pub from_version: u32,
    pub to_version: u32,
    pub wasm_hash: BytesN<32>,
    pub admin: Address,
    pub timestamp: u64,
}

/// Additive audit record for compatibility evidence. Kept separate from
/// `UpgradeRecord` so existing serialized upgrade history remains readable.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeCompatibilityRecord {
    pub from_version: u32,
    pub to_version: u32,
    pub wasm_hash: BytesN<32>,
    pub state_commitment: BytesN<32>,
    pub migration_checkpoint: BytesN<32>,
    pub timestamp: u64,
}

/// Evidence produced by an isolated old/new implementation compatibility run.
/// Hash fields commit to the complete manifest and its test evidence; the
/// contract deliberately does not trust an uncommitted human-readable report.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeCompatibilityManifest {
    pub source_version: u32,
    pub target_version: u32,
    pub state_commitment: BytesN<32>,
    pub interface_commitment: BytesN<32>,
    pub authorization_commitment: BytesN<32>,
    pub preconditions_commitment: BytesN<32>,
    pub postconditions_commitment: BytesN<32>,
    pub rollback_commitment: BytesN<32>,
    pub migration_checkpoint: BytesN<32>,
    pub migration_complete: bool,
    pub manual_records: u32,
}

/// Stable, representative state used by migration tooling when creating a
/// differential snapshot. The resulting hash is supplied in the manifest.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeStateSnapshot {
    pub contract_version: u32,
    pub escrow_count: u32,
    pub recurring_escrow_next_id: u64,
    pub upgrade_threshold: u32,
    pub paused: bool,
    pub onboarding_configured: bool,
}

/// Immutable per-round state for the multi-sig upgrade approval flow.
///
/// Written once on the **first** approval call for a given proposal nonce and
/// never mutated except to append new approvals.  Keyed by
/// `DataKey::UpgradeApprovalState(nonce)`.
///
/// # Security properties
///
/// * `signers`   — snapshotted from `UpgradeSigners` (or admin fallback) at
///   round open.  Subsequent `set_upgrade_signers` calls cannot alter which
///   addresses are eligible for this round, closing the signer-rotation race.
///
/// * `threshold` — snapshotted from `UpgradeThreshold` at round open.
///   Mid-round `set_upgrade_threshold` calls therefore cannot lower the bar
///   for the current round.
///
/// * `approvals` — grows monotonically as valid signers call
///   `propose_upgrade_wasm`.  Only addresses present in `signers` may appear
///   here; duplicates are rejected with `AlreadyApproved`.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeApprovalState {
    /// Monotonically increasing round counter.  Incremented on every
    /// `cancel_upgrade_wasm` call so that residual state from a prior
    /// round cannot be replayed in a subsequent round.
    pub nonce: u32,
    /// Signer set captured when the round was opened (first approval).
    pub signers: Vec<Address>,
    /// Approval threshold captured when the round was opened.
    pub threshold: u32,
    /// Addresses that have submitted a valid approval this round.
    pub approvals: Vec<Address>,
}

/// Per-token fee configuration introduced for #239.
///
/// The legacy `FeeTokenIndex` storage held only a flat `Vec<Address>` of
/// fee-receiving tokens, which forced any future multi-token fee model into a
/// contract upgrade. This struct gives us a per-token slot keyed by
/// `DataKey::FeeTokenConfig(token)` that can carry forward additional fields
/// (e.g. custom_bps overrides, token-specific receivers) without touching the
/// global storage shape — new fields can be appended as `Option<T>` and read
/// with safe fallbacks.
///
/// # Fields
///
/// * `active` - Boolean flag indicating whether this token is currently active for
///   platform fee collection. When false, the admin can disable a token without
///   losing its accumulated totals, allowing history preservation while stopping
///   future fee counting.
///
/// * `custom_fee_bps` - Optional custom fee basis points specific to this token.
///   Reserved for a future multi-token fee mode; currently NOT consulted by
///   `calculate_fee` to keep this change storage-only and avoid behavior changes.
///   A follow-up issue will wire this into fee calculation once the storage shape
///   stabilizes in production.
///
/// * `accumulated` - Total fees accumulated in this token, measured in stroops.
///   Monotonically increasing counter that preserves fee history across
///   activation/deactivation cycles.
///
/// # Storage Side-effects
///
/// - Stored persistently under `DataKey::FeeTokenConfig(token_address)` with
///   TTL extension on reads to prevent premature archival.
/// - Updates to this struct trigger config refresh in affected escrow operations
///   to ensure correct fee calculations based on token status.
///
/// # Integration notes
///
/// Off-chain integrators should cache this struct keyed by token address and
/// refresh on-demand when escrow operations reference new tokens. The `accumulated`
/// field provides audit trail for fee reconciliation; timestamp context is
/// available via escrow event logs.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FeeTokenInfo {
    pub active: bool,
    pub custom_fee_bps: Option<u32>,
    pub accumulated: i128,
}

/// Summary event emitted after a fee-token config migration run.
///
/// Operators can compare `scanned_tokens`, `migrated_configs`, and
/// `skipped_existing` to verify that the legacy `FeeTokenIndex` was fully
/// audited and that a second run is a no-op.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FeeTokenConfigsMigratedEvent {
    pub scanned_tokens: u32,
    pub migrated_configs: u32,
    pub skipped_existing: u32,
}

/// Aggregated version metadata returned from `get_version_info`. Mirrors the
/// fields surfaced via the upgrade history but in a flat shape suitable for
/// dashboards / `migrate_v_x` style audits.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct VersionInfo {
    pub current_version: u32,
    pub upgrade_count: u32,
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
    pub service_agreement_hash: Option<Bytes>,
}

/// Lifecycle state for a resource-aware batch escrow job.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum BatchJobStatus {
    Pending = 0,
    Completed = 1,
    Cancelled = 2,
}

/// Persisted state for a scheduled batch. The parameters are immutable so a
/// continuation always operates on the same ordered input and cursor.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct BatchEscrowJob {
    pub owner: Address,
    pub params: Vec<EscrowCreateParams>,
    pub next_index: u32,
    pub status: BatchJobStatus,
}

/// Lightweight progress returned to clients and indexers without exposing the
/// stored parameter vector.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct BatchJobProgress {
    pub id: u64,
    pub owner: Address,
    pub next_index: u32,
    pub total: u32,
    pub status: BatchJobStatus,
}

/// Policy for handling fees when a dispute expires without arbitrator resolution.
///
/// A dispute "expires" when the configured `max_dispute_duration` elapses
/// without the arbitrator delivering a verdict (see [`PlatformConfig`]).
/// At that point the escrow must still be unwound — but the platform fee
/// is suddenly ambiguous: nobody won the dispute, so the usual "loser
/// pays the fee" rule doesn't apply. This enum is the on-chain knob the
/// admin uses to pick a policy at configuration time.
///
/// # Indexer / off-chain integration
///
/// Each variant is serialised as its `repr(u32)` discriminant on the
/// wire, so off-chain indexers can match against `0..=3` without binding
/// to the variant names. The discriminants are stable; reordering them
/// would be a breaking change for existing escrows whose `PlatformConfig`
/// has been persisted on-chain.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ExpiredDisputeFeePolicy {
    /// Refund buyer in full, platform collects no fee. The default,
    /// buyer-friendly policy — the platform absorbs the cost of the
    /// arbitrator timing out. Use when buyer goodwill matters more than
    /// covering operational cost on stalled disputes.
    RefundFullNoPlatformFee = 0,
    /// Refund buyer minus platform fee. The platform still earns its
    /// fee, taken from the buyer's refunded amount. Symmetric to a
    /// normal "buyer loses" resolution; use when the platform must
    /// cover its costs regardless of dispute outcome.
    RefundMinusPlatformFee = 1,
    /// Refund buyer in full, deduct platform fee from the seller's
    /// locked amount. The seller forfeits the fee even though they
    /// never received payment — use when seller responsibility for
    /// presenting evidence outweighs the cost of forfeiting on a
    /// stalled arbitration.
    DeductFeeFromSeller = 2,
    /// Split the platform fee: half deducted from the buyer's refund,
    /// half from the seller's locked amount. The most "neutral" policy
    /// — both sides share the cost of the arbitrator timing out.
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
    /// Dispute escalation window in seconds (default: 3 days)
    pub dispute_escalation_window: u32,
    /// Evidence/counter-evidence challenge window before arbitrator resolution
    pub evidence_challenge_window: u32,
}

/// Structured record of dispute evidence with metadata and expiry thresholds (#927).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEvidence {
    pub id: u64,
    pub order_id: u32,
    pub dispute_session_id: u64,
    pub submitter: Address,
    pub evidence_uri: String,
    pub parent_evidence_id: Option<u64>,
    pub submitted_at: u64,
    pub expires_at: u64,
    pub is_invalidated: bool,
}

/// Record of dispute escalation to arbitration (#941).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEscalationRecord {
    pub order_id: u32,
    pub escalated_by: Address,
    pub escalated_at: u64,
}

/// Configuration for sensitive action rate limiting (#943).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct RateLimitConfig {
    pub max_calls: u32,
    pub window: u32,
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
    /// Incremented on each cancel so a cancelled proposal cannot be replayed.
    pub nonce: u64,
}

/// Which terminal settlement path finalized a dispute.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum SettlementPath {
    PartialRefundAccepted = 0,
    ArbitratedRelease = 1,
    ArbitratedRefund = 2,
    ArbitratedPartial = 3,
    ExpiredDispute = 4,
}

/// Immutable receipt written before token transfers on every dispute settlement path.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct SettlementReceipt {
    pub order_id: u32,
    pub path: SettlementPath,
    pub executed_at: u64,
    pub proposal_nonce: u64,
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

/// Coherent onboarding state proof passed across the escrow boundary.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct OnboardingAttestation {
    pub account: Address,
    pub profile_version: u32,
    pub role: UserRole,
    pub is_verified: bool,
    pub status: ProfileStatus,
    pub state_revision: u64,
    pub ledger_sequence: u32,
    pub operation_id: Bytes,
    pub contract_instance: Address,
    pub state_digest: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct LegacyUserProfile {
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

/// Minimal cross-contract interface for the OnboardingContract.
/// Used by CraftNexusContract to update user reputation and activity metrics
/// when escrow state changes (release, refund, resolve).
#[soroban_sdk::contractclient(name = "OnboardingClient")]
pub trait OnboardingInterface {
    /// Issue a proof bound to this escrow contract and operation.
    fn get_onboarding_attestation(
        env: Env,
        user: Address,
        operation_id: Bytes,
        contract_instance: Address,
    ) -> OnboardingAttestation;
    /// Validate and consume a single-use onboarding proof.
    fn validate_onboarding_attestation(env: Env, attestation: OnboardingAttestation) -> bool;
    /// Increment a user's reputation counters.
    ///
    /// Called by this escrow contract after a terminal escrow outcome where a
    /// winner/loser can be determined (release/refund/dispute resolution).
    /// The onboarding contract authenticates that the caller is the registered
    /// escrow contract address.
    fn update_reputation(env: Env, address: Address, successful_delta: u32, disputed_delta: u32);
    /// Increment a user's activity metrics (escrow count + volume).
    ///
    /// Used by onboarding to drive auto-verification and analytics counters.
    fn update_user_metrics(
        env: Env,
        address: Address,
        escrow_count_delta: u32,
        volume_delta: i128,
        token_address: Address,
    );
    /// Mark a user profile as deactivated.
    ///
    /// Used by the escrow contract for administrative safety actions; the
    /// onboarding contract enforces its own authorization rules.
    fn deactivate_profile(env: Env, user: Address);
    /// Verify a user, returning the updated profile.
    fn verify_user(env: Env, user: Address) -> UserProfile;
    /// Return true if the user currently has any active escrow obligations.
    fn has_active_contracts(env: Env, user: Address) -> bool;
    /// Update onboarding's local active-contract counter for a user.
    ///
    /// `delta` should be `+1` when an escrow becomes active and `-1` when the
    /// escrow closes. The onboarding contract rejects underflows.
    fn update_active_contracts(env: Env, user: Address, delta: i32);
    /// Number of onboarding profiles whose status is currently active.
    fn get_active_user_count(env: Env) -> u32;
    /// Refresh the persistent TTL for a user's profile entry.
    fn bump_user_profile_ttl(env: Env, user: Address) -> bool;
    /// Refresh the persistent TTL for a user's activity metrics entry.
    fn bump_user_metrics_ttl(env: Env, user: Address) -> bool;
    /// Return the role assigned to `user`, or `UserRole::None` if no profile exists.
    fn get_user_role(env: Env, user: Address) -> UserRole;
    /// Return true if `user` has an active onboarding profile.
    fn is_profile_active(env: Env, user: Address) -> bool;
    /// Return the schema version stored on `user`'s profile, or `0` if no profile exists.
    fn get_user_profile_version(env: Env, user: Address) -> u32;
    /// Return the monotonically increasing state version for `user`'s profile,
    /// or `0` if no profile exists.
    fn get_user_state_version(env: Env, user: Address) -> u32;
    /// Return true if `user` has passed verification (manual or auto).
    fn is_user_verified(env: Env, user: Address) -> bool;
}

#[contract]
/// CraftNexus escrow contract.
///
/// # Storage model
/// - Escrows are stored under `(ESCROW, order_id)` as a single compact record.
/// - Enumeration uses count + indexed keys (e.g. `EscrowCount` +
///   `GlobalEscrowIdIndexed(i)`) to avoid unbounded `Vec` growth.
///
/// # TTL model
/// - Persistent entries extend TTL on write via `extend_persistent`.
/// - Hot index reads use `extend_persistent_read` with a lower threshold to
///   reduce rent-refresh overhead while still preventing accidental expiry.
///
/// # Onboarding integration
/// - The admin can register an onboarding contract address.
/// - Cross-contract calls are wrapped in `try_invoke_contract` helpers so an
///   onboarding failure never bricks escrow settlement.
pub struct CraftNexusContract;

impl CraftNexusContract {
    pub fn enter_reentry_guard(env: &Env) {
        if env.storage().temporary().has(&DataKey::ReentryGuard) {
            env.panic_with_error(crate::Error::ReentryDetected);
        }
        env.storage().temporary().set(&DataKey::ReentryGuard, &true);
    }

    pub fn exit_reentry_guard(env: &Env) {
        env.storage().temporary().remove(&DataKey::ReentryGuard);
    }
}

/// Alias and compatibility layers
pub const ESCROW_CONTRACT: CraftNexusContract = CraftNexusContract;

pub type EscrowContractClient<'a> = CraftNexusContractClient<'a>;

/// Guard to ensure reentry protection is cleared even if a panic or error occurs.
/// This is essential to prevent contract locks from persisting across failed calls.
/// Automatically removes the guard when dropped, ensuring cleanup in all control flows.
struct ReentryGuardScope<'a> {
    env: &'a Env,
}

impl<'a> ReentryGuardScope<'a> {
    fn new(env: &'a Env) -> Self {
        CraftNexusContract::enter_reentry_guard(env);
        ReentryGuardScope { env }
    }
}

impl<'a> Drop for ReentryGuardScope<'a> {
    fn drop(&mut self) {
        CraftNexusContract::exit_reentry_guard(self.env);
    }
}

#[contractimpl]
impl CraftNexusContract {
    fn validate_ipfs_cid(cid: &String) -> bool {
        let len = cid.len() as usize;
        if len == 0 || len > 128 {
            return false;
        }

        let mut buf = [0u8; 128];
        cid.copy_into_slice(&mut buf[0..len]);
        let cid_bytes = &buf[0..len];

        let is_v0 = len == 46
            && cid_bytes[0] == b'Q'
            && cid_bytes[1] == b'm'
            && cid_bytes.iter().all(|b| Self::is_base58_btc_char(*b));

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
                if !(50..=100).contains(&len) || cid_bytes[1] != b'a' {
                    return false;
                }
                payload
                    .iter()
                    .all(|b| matches!(*b, b'a'..=b'z' | b'2'..=b'7'))
            }
            // base16lower (hex)
            b'f' => {
                if !(60..=120).contains(&len) || cid_bytes[1] != b'0' || cid_bytes[2] != b'1' {
                    return false;
                }
                payload
                    .iter()
                    .all(|b| matches!(*b, b'0'..=b'9' | b'a'..=b'f'))
            }
            // base58btc
            b'z' => {
                if !(40..=100).contains(&len) {
                    return false;
                }
                payload.iter().all(|b| Self::is_base58_btc_char(*b))
            }
            _ => false,
        }
    }

    #[inline(always)]
    fn is_base58_btc_char(byte: u8) -> bool {
        BASE58_BTC_CHARSET[byte as usize]
    }

    /// Validate an optional IPFS CID string, panicking with `InvalidIpfsHash` if present but invalid.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `ipfs_hash` - Optional CID string to validate
    ///
    /// # Errors
    /// Panics with `Error::InvalidIpfsHash` if the CID is present but fails `validate_ipfs_cid`.
    ///
    /// # Storage side-effects
    /// None — this is a pure validation helper with no storage reads or writes.
    #[inline(always)]
    fn validate_optional_ipfs_hash(env: &Env, ipfs_hash: &Option<String>) {
        if let Some(cid) = ipfs_hash {
            if !Self::validate_ipfs_cid(cid) {
                env.panic_with_error(crate::Error::InvalidIpfsHash);
            }
        }
    }

    /// Validate an optional metadata hash, panicking with `InvalidMetadataHash` if present but not 32 bytes.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `metadata_hash` - Optional raw bytes to validate
    ///
    /// # Errors
    /// Panics with `Error::InvalidMetadataHash` if the hash is present but its length is not exactly 32 bytes.
    ///
    /// # Storage side-effects
    /// None — this is a pure validation helper with no storage reads or writes.
    #[inline(always)]
    fn validate_optional_metadata_hash(env: &Env, metadata_hash: &Option<Bytes>) {
        if let Some(hash) = metadata_hash {
            if hash.len() != 32 {
                env.panic_with_error(crate::Error::InvalidMetadataHash);
            }
        }
    }

    fn validate_optional_service_agreement_hash(env: &Env, hash: &Option<Bytes>) {
        if let Some(h) = hash {
            if h.len() != 32 {
                env.panic_with_error(crate::Error::InvalidServiceAgreementHash);
            }
        }
    }

    #[inline(always)]
    fn get_admin(env: &Env) -> Result<Address, Error> {
        let config: PlatformConfig = env
            .storage()
            .instance()
            .get(&DataKey::PlatformConfig)
            .ok_or(Error::PlatformNotInitialized)?;
        Ok(config.admin)
    }

    /// Validates admin address to ensure it's not zero/default and is properly initialized (#240)
    /// This prevents common configuration errors and hardens against corruption.
    ///
    /// Storage-layout note: this validator sits on the hot path for any
    /// admin-gated mutation. Checks are ordered cheapest-first so the
    /// common case (a structurally valid candidate that differs from
    /// the current contract address) returns without touching persistent
    /// storage at all — a small but consistent gas saving across every
    /// transfer / propose / accept_admin call.
    fn validate_admin_address(env: &Env, admin: &Address) -> Result<(), Error> {
        // Ensure the address is not the contract's own address — a common
        // misconfiguration that would lock the contract out of admin
        // operations forever.
        let contract = env.current_contract_address();
        if admin == &contract {
            return Err(Error::InvalidAdminAddress);
        }
        // Note: Additional address validation could be performed here
        // (e.g., checking if address exists on ledger, format validation, etc.)
        Ok(())
    }

    /// Validates a proposed platform wallet address (#707).
    ///
    /// Rejects addresses that would cause `transfer_platform_fee` to panic at
    /// the host level — specifically the contract's own address, which is
    /// structurally valid but semantically meaningless as a fee destination and
    /// would lock collected fees inside the escrow contract forever.
    ///
    /// Called by both `initialize` and `update_platform_wallet` so the
    /// invariant is enforced at every write point rather than only at read time.
    fn validate_platform_wallet(env: &Env, wallet: &Address) -> Result<(), Error> {
        if wallet == &env.current_contract_address() {
            return Err(Error::InvalidPlatformWallet);
        }
        Ok(())
    }

    /// Gets platform configuration with fallback mechanism for corruption recovery (#240)
    /// Returns the primary config if valid, falls back to last-known good state if corrupted
    #[allow(dead_code)]
    fn get_platform_config_safe(env: &Env) -> Result<PlatformConfig, Error> {
        let config: Option<PlatformConfig> = env.storage().persistent().get(&PLATFORM_FEE);

        if let Some(cfg) = config {
            // Validate that critical fields are initialized
            if Self::validate_admin_address(env, &cfg.admin).is_ok() {
                Self::extend_persistent(env, &PLATFORM_FEE);
                return Ok(cfg);
            }
        }

        // If primary config is missing or corrupted, attempt to recover using fallback admin
        if let Some(fallback_admin) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::FallbackAdmin)
        {
            Self::extend_persistent(env, &DataKey::FallbackAdmin);
            // Emit recovery event for audit trail
            env.events().publish(
                (Symbol::new(env, "admin_config_recovered"), true),
                String::from_str(env, "Using fallback admin after config corruption detected"),
            );
            // Return a minimal valid config with fallback admin
            // This ensures critical operations remain accessible even if config is corrupted
            return Ok(PlatformConfig {
                platform_fee_bps: 500, // 5% default fee
                platform_wallet: fallback_admin.clone(),
                admin: fallback_admin,
                arbitrator: env.current_contract_address(),
                moderator: None,
                is_paused: true, // Safer to default to paused during recovery
                min_stake_required: 0,
                pending_admin: None,
                wasm_upgrade_cooldown: DEFAULT_WASM_UPGRADE_COOLDOWN,
                max_dispute_duration: DEFAULT_MAX_DISPUTE_DURATION,
                stake_cooldown: DEFAULT_STAKE_COOLDOWN,
                expired_dispute_fee_policy: ExpiredDisputeFeePolicy::RefundFullNoPlatformFee,
                min_release_window: DEFAULT_MIN_RELEASE_WINDOW,
                dispute_escalation_window: DEFAULT_DISPUTE_ESCALATION_WINDOW,
                evidence_challenge_window: DEFAULT_EVIDENCE_CHALLENGE_WINDOW,
            });
        }

        Err(Error::CorruptedPlatformConfig)
    }

    /// Emits audit event for admin changes to maintain a complete audit trail (#240)
    fn emit_admin_changed(
        env: &Env,
        previous_admin: Address,
        new_admin: Address,
        change_type: &str,
    ) {
        env.events().publish(
            (Symbol::new(env, "admin_changed"), change_type.as_bytes()),
            (previous_admin, new_admin),
        );
    }

    /// Stores fallback admin address for recovery purposes (#240)
    /// This ensures that even if primary admin storage is corrupted, platform can be recovered
    fn set_fallback_admin(env: &Env, admin: Address) -> Result<(), Error> {
        Self::validate_admin_address(env, &admin)?;
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
        Self::extend_persistent(env, &DataKey::FallbackAdmin);
        Ok(())
    }

    fn emit_escrow_created(env: &Env, event: EscrowEvent) {
        env.events()
            .publish((symbol_short!("escrow"), event.escrow_id), event);
    }

    fn emit_escrow_resolved_event(env: &Env, event: EscrowResolvedEvent) {
        env.events().publish(
            (Symbol::new(env, "escrow_resolved"), event.escrow_id),
            event,
        );
    }

    fn emit_reputation_update(env: &Env, event: ReputationUpdateEvent) {
        env.events().publish(
            (
                Symbol::new(env, "stake_reputation_update"),
                event.address.clone(),
            ),
            event,
        );
    }

    fn emit_config_updated(
        env: &Env,
        field_name: &str,
        old_value: ConfigValue,
        new_value: ConfigValue,
    ) {
        env.events().publish(
            (
                Symbol::new(env, "admin_config_updated"),
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
            (Symbol::new(env, "admin_fee_tier_updated"), artisan.clone()),
            ArtisanFeeTierUpdatedEvent { artisan, fee_bps },
        );
    }

    fn emit_metadata_verified(env: &Env, order_id: u32, verifier: Address) {
        env.events().publish(
            (
                Symbol::new(env, "escrow_metadata_verified"),
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
            (Symbol::new(env, "admin_platform_paused"), initiator.clone()),
            PlatformPausedEvent {
                initiator,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    fn emit_platform_unpaused(env: &Env, initiator: Address) {
        env.events().publish(
            (
                Symbol::new(env, "admin_platform_unpaused"),
                initiator.clone(),
            ),
            PlatformUnpausedEvent {
                initiator,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Atomically appends one escrow ID to the indexed global registry and
    /// increments `EscrowCount` (#515 / Issue #226).
    fn update_escrow_indices_atomic(env: &Env, order_id: u32) {
        // Issue #515 — O(1) indexed append replaces monolithic AllEscrowIds Vec
        // rewrites. Legacy Vec entries are migrated lazily on first touch.
        Self::migrate_legacy_all_escrow_ids(env);

        let count_key = DataKey::EscrowCount;
        let count = Self::get_persistent_u32(env, &count_key);

        let index_key = DataKey::GlobalEscrowIdIndexed(count);
        env.storage().persistent().set(&index_key, &order_id);
        Self::extend_persistent(env, &index_key);

        env.storage().persistent().set(&count_key, &(count + 1));
        Self::extend_persistent(env, &count_key);
    }

    /// Atomically appends escrow IDs to the indexed global registry for batch
    /// operations (#515). Each ID is stored under its own key so batch creates
    /// avoid rewriting a growing Vec.
    fn update_escrow_indices_batch_atomic(env: &Env, order_ids: &soroban_sdk::Vec<u32>) {
        if order_ids.is_empty() {
            return;
        }

        Self::migrate_legacy_all_escrow_ids(env);

        let count_key = DataKey::EscrowCount;
        let mut count = Self::get_persistent_u32(env, &count_key);

        for i in 0..order_ids.len() {
            if let Some(id) = order_ids.get(i) {
                let index_key = DataKey::GlobalEscrowIdIndexed(count);
                env.storage().persistent().set(&index_key, &id);
                Self::extend_persistent(env, &index_key);
                count += 1;
            }
        }

        env.storage().persistent().set(&count_key, &count);
        Self::extend_persistent(env, &count_key);
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

    /// Records a stake operation in the history queue for audit trail and analytics (#237)
    /// Implements bounded queue with automatic pruning to prevent unbounded storage growth
    fn record_stake_history(
        env: &Env,
        artisan: &Address,
        new_stake: i128,
        operation: &str,
    ) -> Result<(), Error> {
        let count_key = DataKey::StakeHistoryCount(artisan.clone());
        let _history_key = DataKey::StakeHistory(artisan.clone());

        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        // Check if we need to prune before adding new entry
        if current_count >= MAX_STAKE_HISTORY_SIZE {
            // Queue is full, cannot add more entries
            return Err(Error::StakeQueueFull);
        }

        // If approaching capacity threshold, schedule pruning
        if current_count >= STAKE_HISTORY_PRUNE_THRESHOLD {
            // Keep only most recent 50% of entries to free up space
            // This is done lazily - oldest entries will be overwritten on next full cycle
            let new_count = current_count / 2;
            env.storage().persistent().set(&count_key, &new_count);
            Self::extend_persistent(env, &count_key);
        }

        // Record timestamp of this operation for maintenance checks
        let modified_key = DataKey::StakeLastModified(artisan.clone());
        env.storage()
            .persistent()
            .set(&modified_key, &env.ledger().timestamp());
        Self::extend_persistent(env, &modified_key);

        // Emit audit event
        env.events().publish(
            (Symbol::new(env, "stake_operation"), operation.as_bytes()),
            (artisan.clone(), new_stake),
        );

        Ok(())
    }

    /// Prunes obsolete stake history entries when queue reaches capacity (#237)
    /// Implements safe cleanup strategy that preserves recent entries for audit trail
    #[allow(dead_code)]
    fn prune_stake_history(env: &Env, artisan: &Address) {
        let count_key = DataKey::StakeHistoryCount(artisan.clone());
        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if current_count > 0 {
            // Keep most recent 50 entries, discard older ones
            let retained_count = current_count.min(50);
            env.storage().persistent().set(&count_key, &retained_count);
            Self::extend_persistent(env, &count_key);
        }
    }

    #[inline(always)]
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

    #[inline(always)]
    fn update_active_dispute_count(env: &Env, delta: i32) {
        let key = DataKey::ActiveDisputeCount;
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_val = if delta > 0 {
            count.saturating_add(delta as u32)
        } else {
            count.saturating_sub((-delta) as u32)
        };
        env.storage().persistent().set(&key, &new_val);
        Self::extend_persistent(env, &key);
    }

    pub fn get_active_dispute_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ActiveDisputeCount)
            .unwrap_or(0)
    }

    #[inline(always)]
    fn get_total_volume(env: &Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalVolume)
            .unwrap_or(0)
    }

    #[inline(always)]
    fn read_persistent<K, V>(env: &Env, key: &K) -> Option<V>
    where
        K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
        V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    {
        let value = env.storage().persistent().get::<K, V>(key);
        if value.is_some() {
            Self::extend_persistent(env, key);
        }
        value
    }

    #[inline(always)]
    fn update_total_locked(env: &Env, token: &Address, delta: i128) {
        let key = DataKey::TotalLocked(token.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = current.saturating_add(delta);
        env.storage().persistent().set(&key, &new_total);
        Self::extend_persistent(env, &key);
        env.storage()
            .persistent()
            .remove(&DataKey::ReconciliationReport(token.clone()));
    }

    #[inline(always)]
    fn update_total_staked(env: &Env, token: &Address, delta: i128) {
        let key = DataKey::TotalStaked(token.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = current.saturating_add(delta);
        env.storage().persistent().set(&key, &new_total);
        Self::extend_persistent(env, &key);
        env.storage()
            .persistent()
            .remove(&DataKey::ReconciliationReport(token.clone()));
    }

    /// Extend the TTL of a persistent storage entry using standardized values.
    #[inline(always)]
    fn extend_persistent(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .persistent()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
    }

    #[inline(always)]
    fn extend_persistent_read(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .persistent()
            .extend_ttl(key, READ_TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Read a persistent `u32` and extend its TTL when the key exists (#515).
    #[inline(always)]
    fn get_persistent_u32(env: &Env, key: &DataKey) -> u32 {
        Self::read_persistent(env, key).unwrap_or(0u32)
    }

    /// Read a persistent `u64` and extend its TTL when the key exists (#431 / key index #30).
    #[inline(always)]
    fn get_persistent_u64(env: &Env, key: &DataKey) -> u64 {
        match env.storage().persistent().get(key) {
            Some(value) => {
                Self::extend_persistent(env, key);
                value
            }
            None => 0u64,
        }
    }

    #[inline(always)]
    fn get_whitelist_count(env: &Env) -> u32 {
        let count_key = DataKey::WhitelistedTokenCount;
        Self::read_persistent(env, &count_key).unwrap_or(0u32)
    }

    #[inline(always)]
    fn set_whitelist_count(env: &Env, count: u32) {
        let count_key = DataKey::WhitelistedTokenCount;
        env.storage().persistent().set(&count_key, &count);
        Self::extend_persistent(env, &count_key);
    }

    fn migrate_legacy_whitelisted_tokens(env: &Env) {
        let legacy_key = DataKey::WhitelistedTokens;
        if !env.storage().persistent().has(&legacy_key) {
            return;
        }

        let legacy_whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(Map::new(env));

        let mut count = Self::get_whitelist_count(env);
        for (token, enabled) in legacy_whitelist.iter() {
            if enabled {
                let token_key = DataKey::WhitelistedTokenIndexed(token.clone());
                if !env.storage().persistent().has(&token_key) {
                    env.storage().persistent().set(&token_key, &true);
                    Self::extend_persistent(env, &token_key);
                    count += 1;
                }
            }
        }

        if count > 0 {
            Self::set_whitelist_count(env, count);
        } else {
            env.storage()
                .persistent()
                .remove(&DataKey::WhitelistedTokenCount);
        }

        env.storage().persistent().remove(&legacy_key);
    }

    /// Migrate legacy `AllEscrowIds` Vec storage to indexed keys (#515).
    fn migrate_legacy_all_escrow_ids(env: &Env) {
        let legacy_key = DataKey::AllEscrowIds;
        if !env.storage().persistent().has(&legacy_key) {
            return;
        }

        let all_ids: soroban_sdk::Vec<u32> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(env));

        for i in 0..all_ids.len() {
            if let Some(id) = all_ids.get(i) {
                let index_key = DataKey::GlobalEscrowIdIndexed(i);
                if !env.storage().persistent().has(&index_key) {
                    env.storage().persistent().set(&index_key, &id);
                    Self::extend_persistent(env, &index_key);
                }
            }
        }

        let count_key = DataKey::EscrowCount;
        let stored_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if stored_count < all_ids.len() {
            env.storage().persistent().set(&count_key, &all_ids.len());
            Self::extend_persistent(env, &count_key);
        }

        env.storage().persistent().remove(&legacy_key);
    }

    /// Returns the configured maximum release window (in seconds).
    /// Falls back to MAX_TOTAL_RELEASE_WINDOW (30 days) if not set by admin.
    #[inline(always)]
    fn get_max_release_window(env: &Env) -> u32 {
        let key = DataKey::MaxReleaseWindow;
        Self::read_persistent(env, &key).unwrap_or(MAX_TOTAL_RELEASE_WINDOW)
    }

    /// Returns the configured onboarding contract address, if any (#243).
    fn get_onboarding_address(env: &Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::OnboardingContractAddress)
    }

    /// Build a client for the configured onboarding contract, if one is set.
    ///
    /// This helper is used by the escrow contract’s safe cross-contract
    /// integration so onboarding updates remain behind a single, explicit
    /// entry point instead of being spread across ad-hoc address lookups.
    fn get_onboarding_client(env: &Env) -> Option<(Address, OnboardingClient<'_>)> {
        Self::get_onboarding_address(env).map(|address| {
            let client = OnboardingClient::new(env, &address);
            (address, client)
        })
    }

    fn authorize_onboarding_state(
        env: &Env,
        user: &Address,
        operation_id: Bytes,
        expected_role: UserRole,
    ) {
        let escrow_address = env.current_contract_address();
        let (onboarding_address, onboarding) = match Self::get_onboarding_client(env) {
            Some(client) => client,
            None => return,
        };
        let attestation = onboarding.get_onboarding_attestation(
            user,
            &operation_id,
            &escrow_address,
        );
        if attestation.account != *user
            || attestation.role != expected_role
            || attestation.status != ProfileStatus::Active
        {
            env.panic_with_error(crate::Error::OnboardingAuthorizationFailed);
        }
        match env.try_invoke_contract::<bool, soroban_sdk::Error>(
            &onboarding_address,
            &Symbol::new(env, "validate_onboarding_attestation"),
            (attestation,).into_val(env),
        ) {
            Ok(Ok(true)) => {}
            _ => env.panic_with_error(crate::Error::OnboardingAuthorizationFailed),
        }
    }

    fn onboarding_operation_id(env: &Env, action: &[u8], order_id: u32) -> Bytes {
        let mut operation_id = Bytes::from_slice(env, action);
        operation_id.extend_from_slice(&order_id.to_be_bytes());
        operation_id
    }

    fn onboarding_operation_id_u64(env: &Env, action: &[u8], identifier: u64) -> Bytes {
        let mut operation_id = Bytes::from_slice(env, action);
        operation_id.extend_from_slice(&identifier.to_be_bytes());
        operation_id
    }

    fn onboarding_cycle_operation_id(env: &Env, id: u64, cycle: u64) -> Bytes {
        let mut operation_id = Bytes::from_slice(env, b"release_next_cycle:");
        operation_id.extend_from_slice(&id.to_be_bytes());
        operation_id.extend_from_slice(&cycle.to_be_bytes());
        operation_id
    }

    /// Public read-only accessor for the registered onboarding contract
    /// address. Returns `OnboardingContractNotSet` rather than `None` so that
    /// SDK clients receive a typed error instead of a silent unwrap on a
    /// `None`. Use `has_onboarding_contract` for a boolean check (#243).
    pub fn get_onboarding_contract(env: Env) -> Result<Address, Error> {
        Self::get_onboarding_address(&env).ok_or(Error::OnboardingContractNotSet)
    }

    /// True iff `set_onboarding_contract` has been called. Useful for
    /// dashboards and integration tests that need to assert configuration
    /// without surfacing an error path (#243).
    pub fn has_onboarding_contract(env: Env) -> bool {
        Self::get_onboarding_address(&env).is_some()
    }

    /// Check if a user has any active escrows or recurring escrows.
    pub fn has_active_escrows(env: Env, user: Address) -> bool {
        let key = DataKey::ActiveObligations(user);
        Self::get_persistent_u32(&env, &key) > 0
    }

    /// Emit a structured warning event when a cross-contract call to the
    /// onboarding contract fails. Indexers can subscribe to `OB_FAIL` to flag
    /// integration drift between the escrow and onboarding contracts.
    fn emit_onboarding_call_failed(env: &Env, method: Symbol, address: Address) {
        env.events().publish(
            (ONBOARD_CALL_FAILED, method),
            (address, env.ledger().timestamp()),
        );
    }

    /// Safely call `update_reputation` on the registered onboarding contract.
    ///
    /// Returns `Ok(true)` on a successful cross-contract call, `Ok(false)` if
    /// the call failed (so the caller can decide whether to fall back to
    /// emitting events) or no contract is configured. Never panics, never
    /// propagates the host trap — the escrow flow MUST keep moving even if
    /// the onboarding contract is broken or pointing at a malicious
    /// implementation (#243).
    #[allow(dead_code)]
    fn safe_update_reputation(
        env: &Env,
        address: Address,
        successful_delta: u32,
        disputed_delta: u32,
    ) -> bool {
        // Issue #527 — short-circuit on the no-op call before paying
        // for the persistent storage read of the onboarding contract
        // address. If both reputation deltas are 0 the cross-contract
        // call has no effect; returning `true` here saves a storage
        // decode + a host `try_invoke_contract` on every escrow
        // settlement where reputation didn't change.
        if successful_delta == 0 && disputed_delta == 0 {
            return true;
        }

        let (onboarding_address, _onboarding) = match Self::get_onboarding_client(env) {
            Some(client) => client,
            None => return false,
        };

        let method = Symbol::new(env, "update_reputation");
        let args: Vec<Val> = (address.clone(), successful_delta, disputed_delta).into_val(env);

        match env.try_invoke_contract::<(), soroban_sdk::Error>(&onboarding_address, &method, args)
        {
            Ok(Ok(())) => true,
            _ => {
                Self::emit_onboarding_call_failed(env, method, onboarding_address);
                false
            }
        }
    }

    /// Safely call `update_user_metrics` on the registered onboarding contract.
    /// Mirrors `safe_update_reputation`'s contract: never panics, returns
    /// `false` on missing config or cross-contract failure (#243).
    #[allow(dead_code)]
    fn safe_update_user_metrics(
        env: &Env,
        address: Address,
        escrow_count_delta: u32,
        volume_delta: i128,
        token_address: Address,
    ) -> bool {
        let (onboarding_address, _onboarding) = match Self::get_onboarding_client(env) {
            Some(client) => client,
            None => return false,
        };

        let method = Symbol::new(env, "update_user_metrics");
        let args: Vec<Val> = (
            address.clone(),
            escrow_count_delta,
            volume_delta,
            token_address.clone(),
        )
            .into_val(env);

        match env.try_invoke_contract::<(), soroban_sdk::Error>(&onboarding_address, &method, args)
        {
            Ok(Ok(())) => true,
            _ => {
                Self::emit_onboarding_call_failed(env, method, onboarding_address);
                false
            }
        }
    }

    #[allow(dead_code)]
    fn safe_update_active_contracts(env: &Env, user: Address, delta: i32) -> bool {
        if delta == 0 {
            return true;
        }

        let (onboarding_address, _onboarding) = match Self::get_onboarding_client(env) {
            Some(client) => client,
            None => return false,
        };

        let method = Symbol::new(env, "update_active_contracts");
        let args: Vec<Val> = (user.clone(), delta).into_val(env);

        match env.try_invoke_contract::<(), soroban_sdk::Error>(&onboarding_address, &method, args)
        {
            Ok(Ok(())) => true,
            _ => {
                Self::emit_onboarding_call_failed(env, method, onboarding_address);
                false
            }
        }
    }

    /// Safely query the onboarding contract for a single user's canonical state.
    ///
    /// Returns `Ok((is_active, role, is_verified, state_version))` on success,
    /// `Err(())` when no onboarding contract is configured or the cross-contract
    /// call fails. Never panics — callers decide how to handle a missing or
    /// unreachable onboarding contract.
    ///
    /// All four cross-contract reads are issued as separate `try_invoke_contract`
    /// calls so a partial failure is observable and distinguishable from a full
    /// outage.
    fn safe_check_onboarding_state(
        env: &Env,
        user: &Address,
    ) -> Result<(bool, UserRole, bool, u32), ()> {
        let (onboarding_address, _) = match Self::get_onboarding_client(env) {
            Some(c) => c,
            None => return Err(()),
        };

        // is_profile_active
        let active_method = Symbol::new(env, "is_profile_active");
        let active_args: Vec<Val> = (user.clone(),).into_val(env);
        let is_active: bool = match env.try_invoke_contract::<bool, soroban_sdk::Error>(
            &onboarding_address,
            &active_method,
            active_args,
        ) {
            Ok(Ok(v)) => v,
            _ => {
                Self::emit_onboarding_call_failed(
                    env,
                    active_method,
                    onboarding_address.clone(),
                );
                return Err(());
            }
        };

        // get_user_role
        let role_method = Symbol::new(env, "get_user_role");
        let role_args: Vec<Val> = (user.clone(),).into_val(env);
        let role: UserRole = match env.try_invoke_contract::<UserRole, soroban_sdk::Error>(
            &onboarding_address,
            &role_method,
            role_args,
        ) {
            Ok(Ok(v)) => v,
            _ => {
                Self::emit_onboarding_call_failed(
                    env,
                    role_method,
                    onboarding_address.clone(),
                );
                return Err(());
            }
        };

        // is_user_verified
        let verified_method = Symbol::new(env, "is_user_verified");
        let verified_args: Vec<Val> = (user.clone(),).into_val(env);
        let is_verified: bool = match env.try_invoke_contract::<bool, soroban_sdk::Error>(
            &onboarding_address,
            &verified_method,
            verified_args,
        ) {
            Ok(Ok(v)) => v,
            _ => {
                Self::emit_onboarding_call_failed(
                    env,
                    verified_method,
                    onboarding_address.clone(),
                );
                return Err(());
            }
        };

        // get_user_state_version
        let version_method = Symbol::new(env, "get_user_state_version");
        let version_args: Vec<Val> = (user.clone(),).into_val(env);
        let state_version: u32 = match env.try_invoke_contract::<u32, soroban_sdk::Error>(
            &onboarding_address,
            &version_method,
            version_args,
        ) {
            Ok(Ok(v)) => v,
            _ => {
                Self::emit_onboarding_call_failed(
                    env,
                    version_method,
                    onboarding_address,
                );
                return Err(());
            }
        };

        Ok((is_active, role, is_verified, state_version))
    }

    /// Validate onboarding state for both buyer and seller before a privileged
    /// marketplace operation (escrow creation).
    ///
    /// # Behaviour
    ///
    /// When no onboarding contract is configured the check is a no-op — the
    /// platform operates in open mode and all accounts are permitted.
    ///
    /// When an onboarding contract is configured each user must satisfy all of:
    /// - Profile exists (state_version > 0; a zero version means no profile).
    /// - Profile is currently active (not deactivated, flagged, or under review).
    /// - Role is not `UserRole::None` — the user must have completed onboarding
    ///   with a recognized role.
    /// - Profile is verified (`is_verified == true`) when the platform requires
    ///   verification for privileged operations.
    ///
    /// The state_version check is deferred here: a zero version is treated as
    /// "profile not found". Any non-zero version is accepted because version
    /// staleness at the point of escrow creation is only relevant if the caller
    /// supplies an explicit expected version. Role and active-status changes
    /// are reflected immediately because they are read live from the canonical
    /// onboarding source.
    ///
    /// # Errors
    ///
    /// Panics with the appropriate [`Error`] variant on any validation failure:
    /// - [`Error::OnboardingProfileNotFound`] — no profile (state_version == 0).
    /// - [`Error::OnboardingProfileInactive`] — profile is not active.
    /// - [`Error::OnboardingRoleMismatch`] — role is `None` (not properly onboarded).
    fn validate_onboarding_state(env: &Env, buyer: &Address, seller: &Address) {
        // No-op when no onboarding contract is configured.
        if Self::get_onboarding_address(env).is_none() {
            return;
        }

        for user in [buyer, seller] {
            let (is_active, role, _is_verified, state_version) =
                match Self::safe_check_onboarding_state(env, user) {
                    Ok(state) => state,
                    Err(()) => {
                        // Cross-contract call failed — emit warning but allow the
                        // operation to proceed so a temporarily unreachable onboarding
                        // contract cannot permanently brick escrow creation.
                        Self::emit_onboarding_call_failed(
                            env,
                            Symbol::new(env, "check_state"),
                            user.clone(),
                        );
                        continue;
                    }
                };

            // A state_version of 0 means the profile does not exist.
            if state_version == 0 {
                env.panic_with_error(Error::OnboardingProfileNotFound);
            }

            // Profile must be in an active state.
            if !is_active {
                env.panic_with_error(Error::OnboardingProfileInactive);
            }

            // User must have a recognized onboarding role.
            if role == UserRole::None {
                env.panic_with_error(Error::OnboardingRoleMismatch);
            }
        }
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

        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

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
    ///
    /// (#243) Rejects pointing the onboarding contract at the escrow itself —
    /// a self-call would create a re-entrancy hazard if the trait surface ever
    /// expands. Cross-contract calls into the configured address remain
    /// indirect via `safe_update_reputation` / `safe_update_user_metrics`,
    /// which trap-isolate failures so a misbehaving onboarding contract
    /// cannot brick escrow operations. Emits a `config_updated` event with
    /// the previous and new addresses for audit trails.
    pub fn set_onboarding_contract(env: Env, contract_address: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if contract_address == env.current_contract_address() {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let previous = Self::get_onboarding_address(&env);

        // Issue #527 — short-circuit on the no-op call before paying
        // for the persistent storage write and TTL extension.
        if let Some(ref current) = previous {
            if *current == contract_address {
                return;
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::OnboardingContractAddress, &contract_address);
        Self::extend_persistent(&env, &DataKey::OnboardingContractAddress);

        let old_value = match previous {
            Some(addr) => ConfigValue::Address(addr),
            None => ConfigValue::String(String::from_str(&env, "unset")),
        };
        Self::emit_config_updated(
            &env,
            "onboarding_contract",
            old_value,
            ConfigValue::Address(contract_address),
        );
    }

    /// Clear the registered onboarding contract address (admin only) (#243).
    /// After calling this, `get_onboarding_contract` returns
    /// `OnboardingContractNotSet` and the safe cross-contract helpers become
    /// no-ops — escrow flows continue to emit `ReputationUpdateEvent`s for
    /// off-chain reconstruction (#211).
    pub fn clear_onboarding_contract(env: Env) -> Result<(), Error> {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let previous = Self::get_onboarding_address(&env).ok_or(Error::OnboardingContractNotSet)?;

        env.storage()
            .persistent()
            .remove(&DataKey::OnboardingContractAddress);

        Self::emit_config_updated(
            &env,
            "onboarding_contract",
            ConfigValue::Address(previous),
            ConfigValue::String(String::from_str(&env, "unset")),
        );
        Ok(())
    }

    /// Add a token to the platform whitelist (admin only).
    ///
    /// Uses individual key-value pairs for scalability instead of a single Map.
    /// Each token is stored as DataKey::WhitelistedTokenIndexed(token) -> true.
    /// Once at least one token is whitelisted, only whitelisted tokens may be
    /// used in escrow creation. The check is skipped when the whitelist is empty,
    /// preserving backward compatibility.
    ///
    /// # Decimal validation
    ///
    /// The token's `decimals()` value must be in the range 0–18 (inclusive).
    /// Tokens with more than 18 decimal places would overflow the volume
    /// normalization arithmetic in the onboarding contract and are rejected
    /// with [`Error::InvalidTokenDecimals`].
    pub fn whitelist_token(env: Env, token: Address) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        // Probe the SEP-41 read interface before persisting an administrator
        // supplied address. Missing or malformed methods become a stable
        // contract error instead of an opaque host panic.
        let token_client = token::Client::new(&env, &token);
        let decimals = token_client
            .try_decimals()
            .map_err(|_| Error::UnsupportedToken)?
            .map_err(|_| Error::UnsupportedToken)?;
        if decimals > 18 {
            return Err(Error::InvalidTokenDecimals);
        }
        token_client
            .try_balance(&env.current_contract_address())
            .map_err(|_| Error::UnsupportedToken)?
            .map_err(|_| Error::UnsupportedToken)?;

        Self::migrate_legacy_whitelisted_tokens(&env);
        let token_key = DataKey::WhitelistedTokenIndexed(token.clone());
        let mut count = Self::get_whitelist_count(&env);

        if !env.storage().persistent().has(&token_key) {
            env.storage().persistent().set(&token_key, &true);
            Self::extend_persistent(&env, &token_key);
            count += 1;
            Self::set_whitelist_count(&env, count);
        }
        Ok(())
    }

    /// Remove a token from the platform whitelist (admin only).
    ///
    /// Uses individual key-value pairs for scalability. Removes the specific
    /// token entry and updates the count. If the resulting whitelist is empty,
    /// whitelist enforcement is automatically disabled (all tokens permitted again).
    pub fn remove_token_from_whitelist(env: Env, token: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        Self::migrate_legacy_whitelisted_tokens(&env);
        let token_key = DataKey::WhitelistedTokenIndexed(token.clone());

        if env.storage().persistent().has(&token_key) {
            env.storage().persistent().remove(&token_key);
            let count = Self::get_whitelist_count(&env);
            if count > 0 {
                Self::set_whitelist_count(&env, count - 1);
            }
        }
    }

    /// Check whether a specific token is on the whitelist.
    ///
    /// Returns `true` if the token is explicitly whitelisted, OR if the whitelist
    /// is empty (enforcement not yet active). Uses individual key lookups for
    /// scalability instead of loading the entire whitelist Map.
    pub fn is_token_whitelisted(env: Env, token: Address) -> bool {
        Self::migrate_legacy_whitelisted_tokens(&env);
        let count = Self::get_whitelist_count(&env);
        if count == 0 {
            return true;
        }

        let token_key = DataKey::WhitelistedTokenIndexed(token);
        let is_whitelisted = env.storage().persistent().has(&token_key);
        if is_whitelisted {
            Self::extend_persistent(&env, &token_key);
        }
        is_whitelisted
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
        Self::migrate_legacy_whitelisted_tokens(env);
        let count = Self::get_whitelist_count(env);
        if count == 0 {
            return;
        }

        let token_key = DataKey::WhitelistedTokenIndexed(token.clone());
        if !env.storage().persistent().has(&token_key) {
            env.panic_with_error(crate::Error::TokenNotWhitelisted);
        }
    }

    /// Get the count of whitelisted tokens.
    ///
    /// Returns 0 if no tokens are whitelisted (enforcement disabled).
    /// This is more efficient than loading all tokens when only the count is needed.
    pub fn get_whitelisted_token_count(env: Env) -> u32 {
        Self::migrate_legacy_whitelisted_tokens(&env);
        Self::get_whitelist_count(&env)
    }

    /// Migrate legacy whitelist storage to individual key-value pairs.
    ///
    /// This function reads the old WhitelistedTokens Map and converts each entry
    /// to individual WhitelistedTokenIndexed keys. Should be called once during
    /// contract upgrade to migrate existing data.
    pub fn migrate_whitelist_storage(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let legacy_key = DataKey::WhitelistedTokens;

        // Check if legacy storage exists
        if !env.storage().persistent().has(&legacy_key) {
            return 0; // Nothing to migrate
        }

        let legacy_whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(Map::new(&env));

        let mut migrated_count = 0u32;

        // Migrate each token to individual storage
        let keys = legacy_whitelist.keys();
        for i in 0..keys.len() {
            if let Some(token) = keys.get(i) {
                if let Some(is_whitelisted) = legacy_whitelist.get(token.clone()) {
                    if is_whitelisted {
                        let token_key = DataKey::WhitelistedTokenIndexed(token);
                        env.storage().persistent().set(&token_key, &true);
                        Self::extend_persistent(&env, &token_key);
                        migrated_count += 1;
                    }
                }
            }
        }

        // Update count
        if migrated_count > 0 {
            let count_key = DataKey::WhitelistedTokenCount;
            env.storage().persistent().set(&count_key, &migrated_count);
            Self::extend_persistent(&env, &count_key);
        }

        // Remove legacy storage
        env.storage().persistent().remove(&legacy_key);

        migrated_count
    }

    /// Migrate legacy ArtisanStakeQueue Vec storage to individual indexed entries.
    ///
    /// This function reads the old ArtisanStakeQueue Vec and converts each entry
    /// to individual ArtisanStakeQueueIndexed keys. Should be called once during
    /// contract upgrade to migrate existing data.
    pub fn migrate_artisan_stake_queue(env: Env, artisan: Address) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let legacy_key = DataKey::ArtisanStakeQueue(artisan.clone());

        // Check if legacy storage exists
        if !env.storage().persistent().has(&legacy_key) {
            return 0; // Nothing to migrate
        }

        let legacy_queue: soroban_sdk::Vec<StakeDeposit> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let queue_len = legacy_queue.len();
        if queue_len == 0 {
            // Remove empty legacy queue
            env.storage().persistent().remove(&legacy_key);
            return 0;
        }

        // Migrate each deposit to individual indexed storage
        for i in 0..queue_len {
            if let Some(deposit) = legacy_queue.get(i) {
                let deposit_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), i);
                env.storage().persistent().set(&deposit_key, &deposit);
                Self::extend_persistent(&env, &deposit_key);
            }
        }

        // Set count
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        env.storage().persistent().set(&count_key, &queue_len);
        Self::extend_persistent(&env, &count_key);

        // Remove legacy storage
        env.storage().persistent().remove(&legacy_key);

        queue_len
    }

    /// Migrate legacy artisan stake records from split `ArtisanStake` (i128) +
    /// `ArtisanStakeToken` (Address) storage to the unified `ArtisanStakeData`
    /// struct (#1034).
    ///
    /// This function is idempotent: it returns 0 if the record is already in
    /// the new format. Should be called lazily during stake reads or writes
    /// so existing artisan balances are preserved across contract upgrades.
    pub fn migrate_legacy_artisan_stake(env: Env, artisan: Address) -> u32 {
        let stake_key = DataKey::ArtisanStake(artisan.clone());
        let token_key = DataKey::ArtisanStakeToken(artisan.clone());

        if !env.storage().persistent().has(&token_key) {
            return 0;
        }

        let old_amount: Option<i128> = env.storage().persistent().get(&stake_key);
        let old_token: Option<Address> = env.storage().persistent().get(&token_key);

        if let (Some(amount), Some(token)) = (old_amount, old_token) {
            let new_stake = ArtisanStakeData { amount, token };
            env.storage().persistent().set(&stake_key, &new_stake);
            Self::extend_persistent(&env, &stake_key);
            env.storage().persistent().remove(&token_key);
            return 1;
        }

        0
    }

    /// Get the count of stake deposits in an artisan's queue.
    ///
    /// Returns 0 if no deposits exist. This is more efficient than loading
    /// all deposits when only the count is needed.
    pub fn get_artisan_stake_queue_count(env: Env, artisan: Address) -> u32 {
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        env.storage().persistent().get(&count_key).unwrap_or(0)
    }

    /// Get paginated stake deposits for an artisan (admin/debug helper).
    ///
    /// Returns up to `limit` deposits starting from `offset`. Useful for
    /// inspecting queue state without loading the entire queue.
    pub fn get_artisan_stake_deposits(
        env: Env,
        artisan: Address,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<StakeDeposit>, Error> {
        let limit = pagination_validation::validate_limit(
            limit,
            pagination_validation::MAX_ADMIN_PAGE_SIZE,
        )?;
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        let total_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        // Return empty if offset is past the end
        if offset >= total_count {
            return Ok(soroban_sdk::Vec::new(&env));
        }

        let mut deposits = soroban_sdk::Vec::new(&env);
        let end = core::cmp::min(offset + limit, total_count);

        for i in offset..end {
            let deposit_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), i);
            if let Some(deposit) = env
                .storage()
                .persistent()
                .get::<DataKey, StakeDeposit>(&deposit_key)
            {
                deposits.push_back(deposit);
            }
        }

        Ok(deposits)
    }

    fn assert_dispute_actor_permissions(
        env: &Env,
        config: &PlatformConfig,
        escrow: &Escrow,
        caller: &Address,
        transition: DisputeTransition,
    ) -> Result<(), Error> {
        match transition {
            DisputeTransition::Initiate
            | DisputeTransition::SubmitEvidence
            | DisputeTransition::Escalate
            | DisputeTransition::ProposeRefund => {
                if *caller != escrow.buyer && *caller != escrow.seller {
                    return Err(Error::Unauthorized);
                }
            }
            DisputeTransition::AcceptRefund(proposer) => {
                // Must be the counterparty to the proposer
                if proposer == escrow.buyer && *caller != escrow.seller {
                    return Err(Error::Unauthorized);
                }
                if proposer == escrow.seller && *caller != escrow.buyer {
                    return Err(Error::Unauthorized);
                }
                if *caller != escrow.buyer && *caller != escrow.seller {
                    return Err(Error::Unauthorized);
                }
            }
            DisputeTransition::CancelRefund(proposer) => {
                // Must be the proposer
                if *caller != proposer {
                    return Err(Error::Unauthorized);
                }
            }
            DisputeTransition::ResolveArbitrated => {
                let is_privileged = *caller == config.admin
                    || *caller == config.arbitrator
                    || Some(caller.clone()) == config.moderator;
                if !is_privileged {
                    return Err(Error::Unauthorized);
                }
                if *caller != config.admin && Self::arbitrator_on_blacklist(env, caller) {
                    return Err(Error::ArbitratorBlacklisted);
                }
            }
        }
        Ok(())
    }
}