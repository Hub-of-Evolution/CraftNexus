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

/// Bounded, overflow-safe oracle-price conversion (Issue #1088).
pub mod conversion;

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
#[contracterror(export = false)]
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum Error {
    // ── Auth / Access (1–9): rollback immediately ──
    Unauthorized = 1,
    EscrowNotFound = 2,
    InvalidEscrowState = 3,
    UsernameAlreadyExists = 4,
    TokenNotWhitelisted = 5,
    AmountBelowMinimum = 6,
    ReleaseWindowTooLong = 7,
    NotInDispute = 8,
    AlreadyOnboarded = 9,
    // ── State / Transition (10–19): retry after state change ──
    InvalidFee = 10,
    SameBuyerSeller = 11,
    PlatformNotInitialized = 12,
    ReleaseWindowNotElapsed = 13,
    BatchOperationFailed = 14,
    ContractPaused = 15,
    DisputeExpired = 16,
    InsufficientStake = 17,
    StakeCooldownActive = 18,
    InvalidRefundAmount = 19,
    // ── Config / Resource (20–29): operator must act ──
    ProposalNotFound = 20,
    ProposalAlreadyExists = 21,
    ReentryDetected = 22,
    ReleaseWindowTooShort = 23,
    StakeTokenMismatch = 24,
    InvalidAdminAddress = 25,
    CorruptedPlatformConfig = 26,
    StakeQueueFull = 27,
    AdminRecoveryFailed = 28,
    BatchLimitExceeded = 29,
    // ── Operational / Gates (30–39): retry after cooldown ──
    DeprecatedFunction = 30,
    NoPendingAdmin = 31,
    NoUpgradeProposed = 32,
    UpgradeCooldownActive = 33,
    UpgradeProposalExists = 34,
    InvalidUpgradeHash = 35,
    RecurringEscrowNotFound = 36,
    CycleNotReady = 37,
    RecurringEscrowIdExhausted = 38,
    OnboardingContractNotSet = 39,
    // ── Validation (40+): fix caller input ──
    /// The configured onboarding contract rejected the participant state proof
    OnboardingAuthorizationFailed = 56,
    // â”€â”€ Validation (40+): fix caller input â”€â”€
    /// Provided metadata hash is invalid
    InvalidMetadataHash = 40,
    InvalidIpfsHash = 41,
    NotAnUpgradeSigner = 42,
    AlreadyApproved = 43,
    InvalidTokenDecimals = 44,
    UpgradeCompatibilityMissing = 45,
    UpgradeCompatibilityInvalid = 46,
    UpgradeMigrationIncomplete = 47,
    StorageLayoutMismatch = 48,
    AdminActionTerminal = 49,
    AdminActionNeedsApprovals = 50,
    AdminActionTimelockActive = 51,
    NotAnAdminActionSigner = 52,
    EvidenceExpired = 53,
    EvidenceAlreadyUsed = 54,
    InvalidDisputeSession = 55,
    UnsupportedToken = 56,
    InvalidBatchWorkLimit = 57,
    BatchJobCancelled = 58,
    BatchJobNotFound = 59,
    BatchJobUnauthorized = 60,
    BatchJobCompleted = 61,
    PaginationLimitZero = 80,
    PaginationCursorInvalid = 81,
    /// Platform wallet cannot be the contract address.
    InvalidPlatformWallet = 62,
    InvalidServiceAgreementHash = 63,
    ChallengeWindowActive = 64,
    ArbitratorBlacklisted = 65,
    InvalidDisputeAction = 66,
    EscalationWindowActive = 67,
    ArbitratorDeadlineExceeded = 68,
    SettlementAlreadyFinalized = 69,
    EmergencyAccountingInvariant = 70,
    ReconciliationRequired = 71,
    RepairPlanNotFound = 72,
    RepairPlanTerminal = 73,
    RepairPlanPreconditionFailed = 74,
    OnboardingProfileNotFound = 75,
    OnboardingProfileInactive = 76,
    OnboardingRoleMismatch = 77,
    OnboardingProfileStale = 78,
    OnboardingVerificationRevoked = 79,
    EscrowAlreadyExists = 80,
    /// Pagination limit is zero; caller must request at least one item (#1022).
    PaginationLimitZero = 81,
    /// Pagination cursor is invalid (past end of dataset or empty dataset) (#1022).
    PaginationCursorInvalid = 82,
    /// Requested WASM upgrade cooldown is below `MIN_WASM_UPGRADE_COOLDOWN`,
    /// which would let the mandatory review window be bypassed (#1062).
    UpgradeCooldownTooShort = 83,
    /// An oracle-driven currency conversion produced a negative amount,
    /// price, or liquidity input (#1088).
    ConversionNegativeInput = 84,
    /// An oracle-driven currency conversion used a decimals value outside
    /// the supported range (#1088).
    ConversionUnsupportedDecimals = 85,
    /// An oracle-driven currency conversion overflowed `i128` arithmetic
    /// (#1088).
    ConversionOverflow = 86,
    /// The oracle quote's reported liquidity is below the configured
    /// minimum; the conversion is rejected rather than settled against a
    /// thin book (#1088).
    ConversionInsufficientLiquidity = 87,
    /// The oracle quote moved further from the trusted reference price than
    /// the configured maximum movement allows (#1088).
    ConversionExcessiveMovement = 88,
    /// A strictly positive conversion input produced a zero output, which
    /// would silently destroy value; rejected instead of settling for zero
    /// (#1088).
    ConversionOutputUnderflow = 89,
}

/// Maps a [`conversion::ConversionError`] onto the contract's own [`Error`]
/// enum so settlement paths that call into [`conversion::convert_amount`] or
/// [`conversion::convert_amount_ceiling`] can propagate a single, ABI-stable
/// error type to callers.
impl From<conversion::ConversionError> for Error {
    fn from(err: conversion::ConversionError) -> Self {
        match err {
            conversion::ConversionError::NegativeInput => Error::ConversionNegativeInput,
            conversion::ConversionError::UnsupportedDecimals => {
                Error::ConversionUnsupportedDecimals
            }
            conversion::ConversionError::Overflow => Error::ConversionOverflow,
            conversion::ConversionError::InsufficientLiquidity => {
                Error::ConversionInsufficientLiquidity
            }
            conversion::ConversionError::ExcessiveMovement => Error::ConversionExcessiveMovement,
            conversion::ConversionError::OutputUnderflow => Error::ConversionOutputUnderflow,
        }
    }
}

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
//! CraftNexus escrow, staking, and onboarding contracts.
//!
//! This crate hosts the main `CraftNexusContract` (escrow) plus the
//! storage-lifecycle / TTL-management framework introduced in #920.

#![no_std]

pub mod storage_lifecycle;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// Storage lifecycle, compaction, and TTL-management framework (#920).
pub use storage_lifecycle::{
    CompactionReport, StorageRetentionPolicy, DEFAULT_RETAINED_AUDIT_ENTRIES,
    DEFAULT_RETAINED_EMERGENCY_HISTORY, DEFAULT_RETAINED_STAKE_HISTORY,
    DEFAULT_RETAINED_UPGRADE_HISTORY,
};

const TOTAL_FEES: Symbol = symbol_short!("TOT_FEES");

const TTL_THRESHOLD: u32 = 10_000;
const READ_TTL_THRESHOLD: u32 = 1_000;
const TTL_EXTENSION: u32 = 518_400;

const DEFAULT_WASM_UPGRADE_COOLDOWN: u32 = time_policy::WASM_UPGRADE_COOLDOWN as u32;
const CANCEL_REPROPOSE_COOLDOWN: u64 = time_policy::CANCEL_REPROPOSE_COOLDOWN;
const DEFAULT_MAX_DISPUTE_DURATION: u32 = time_policy::MAX_DISPUTE_DURATION as u32;
const DEFAULT_STAKE_COOLDOWN: u32 = time_policy::STAKE_COOLDOWN as u32;
const DEFAULT_MIN_RELEASE_WINDOW: u32 = time_policy::MIN_RELEASE_WINDOW as u32;
const ABSOLUTE_MAX_RELEASE_WINDOW: u32 = time_policy::ABSOLUTE_MAX_RELEASE_WINDOW as u32;
const DEFAULT_EVIDENCE_EXPIRY_WINDOW: u64 = time_policy::EVIDENCE_EXPIRY_WINDOW;
const DEFAULT_EVIDENCE_CHALLENGE_WINDOW: u32 = time_policy::EVIDENCE_CHALLENGE_WINDOW as u32;
const DEFAULT_DISPUTE_ESCALATION_WINDOW: u32 = time_policy::DISPUTE_ESCALATION_WINDOW as u32;
const DEFAULT_RATE_LIMIT_MAX_CALLS: u32 = 5;
const DEFAULT_RATE_LIMIT_WINDOW: u32 = time_policy::RATE_LIMIT_WINDOW as u32;

const MAX_PLATFORM_FEE_BPS: u32 = 1000;
const MAX_TOTAL_RELEASE_WINDOW: u32 = time_policy::MAX_TOTAL_RELEASE_WINDOW as u32;
const CURRENT_ESCROW_VERSION: u32 = 4;
const CURRENT_STORAGE_LAYOUT_VERSION: u32 = 1;
const MAX_BATCH_SIZE: u32 = 20;
const MAX_SCHEDULED_BATCH_WORK: u32 = 5;
const MAX_PAGE_SIZE: u32 = 100;
const UNFUNDED_CANCEL_TIMEOUT: u64 = time_policy::UNFUNDED_CANCEL_TIMEOUT;
const MAX_RECURRING_ESCROW_ID: u64 = u64::MAX - 1;
const FEE_POLICY_VERSION: u32 = 1;
const MAX_UPGRADE_HISTORY: u32 = 32;

const UPGRADE_PROPOSED: Symbol = symbol_short!("UPG_PROP");
const UPGRADE_CANCELLED: Symbol = symbol_short!("UPG_CANC");
const UPGRADE_EXECUTED: Symbol = symbol_short!("UPG_EXEC");

const MAX_STAKE_HISTORY_SIZE: u32 = 100;
const STAKE_HISTORY_PRUNE_THRESHOLD: u32 = 80;
const MAX_STAKE_QUEUE_SIZE: u32 = 50;
const STAKE_QUEUE_PRUNE_THRESHOLD: u32 = 40;
const ADMIN_RECOVERY_DELAY: u64 = time_policy::ADMIN_RECOVERY_DELAY;
const MIN_ADMIN_RECOVERY_COOLDOWN: u64 = time_policy::MIN_ADMIN_RECOVERY_COOLDOWN;
const DEFAULT_ADMIN_ACTION_TIMELOCK_DELAY: u64 = time_policy::ADMIN_ACTION_TIMELOCK_DELAY;

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
    BuyerEscrows(Address),
    SellerEscrows(Address),
    MinEscrowAmount(Address),
    TotalFees(Address),
    FeeTokenIndex,
    FeeTokenConfig(Address),
    ContractVersion,
    PlatformConfig,
    StorageLayoutVersion,
    ArtisanFeeTier(Address),
    ArtisanStake(Address),
    ArtisanStakeToken(Address),
    StakeCooldownEnd(Address),
    ArtisanStakeQueue(Address),
    ArtisanStakeQueueCount(Address),
    ArtisanStakeQueueIndexed(Address, u32),
    PartialRefundProposal(u32),
    SettlementReceipt(u32),
    ArbitratorBlacklist(Address),
    ActiveDisputeCount,
    TotalVolume,
    ReentryGuard,
    PendingAdmin,
    WasmUpgradeProposal,
    MaxReleaseWindow,
    OnboardingContractAddress,
    WhitelistedTokens,
    WhitelistedTokenIndexed(Address),
    WhitelistedTokenCount,
    AllEscrowIds,
    EscrowCount,
    GlobalEscrowIdIndexed(u32),
    FallbackAdmin,
    AdminRecoveryTime,
    AdminRecoveryDelay,
    StakeHistory(Address),
    StakeHistoryCount(Address),
    StakeLastModified(Address),
    FundAuditCount(Address),
    FundAuditIndexed(Address, u32),
    BuyerEscrowIndexed(Address, u32),
    SellerEscrowIndexed(Address, u32),
    BuyerEscrowCount(Address),
    SellerEscrowCount(Address),
    TotalLocked(Address),
    TotalStaked(Address),
    StakedArtisanIndexed(u32),
    StakedArtisanCount,
    ReconciliationReport(Address),
    ReconciliationProgress(Address),
    ReconciliationRepairPlan(u64),
    NextReconciliationRepairPlanId,
    UpgradeHistory,
    UpgradeCompatibilityHistory,
    RecurringEscrow(u64),
    NextRecurringEscrowId,
    RecurringEscrowCount,
    BatchEscrowJob(u64),
    ActiveObligations(Address),
    UpgradeThreshold,
    UpgradeApprovalState(u32),
    UpgradeSigners,
    LastUpgradeCancelledAt,
    UpgradeCompatibilityManifest(BytesN<32>),
    EvidenceLog(u32),
    UsedEvidenceHash(BytesN<32>),
    DisputeEscalation(u32),
    DisputeEscalationWindow,
    RateLimitCount(Address, u64),
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

#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum Resolution {
    ReleaseToSeller = 0,
    RefundToBuyer = 1,
}

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum SettlementKind {
    ReleaseFunds,
    FullRefundNoFee,
    ExpiredDisputeDeductFromSeller,
    ExpiredDisputeDeductFromBuyer,
    ExpiredDisputeSplitFee,
    PartialRefund(i128, i128),
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FeeAllocation {
    pub platform_fee: i128,
    pub seller_amount: i128,
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
    pub release_window: u32,
    pub created_at: u32,
    pub ipfs_hash: Option<String>,
    pub metadata_hash: Option<Bytes>,
    pub dispute_reason: Option<Symbol>,
    pub dispute_initiated_at: Option<u64>,
    pub funded: bool,
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
    pub schema_version: u32,
    pub escrow_id: u64,
    pub action: EscrowAction,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscrowResolvedEvent {
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
    pub service_agreement_hash: Option<Bytes>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct MetadataRevealProof {
    pub content: Bytes,
    pub secret: Option<Bytes>,
}

#[cfg(test)]
#[derive(Clone, Eq, PartialEq)]
pub struct Metadata {
    pub title: String,
    pub description: String,
    pub category: String,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct WasmUpgradeProposal {
    pub wasm_hash: BytesN<32>,
    pub upgrade_at: u64,
    pub proposed_by: Address,
    pub proposed_at: u64,
}

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

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeApprovalState {
    pub nonce: u32,
    pub signers: Vec<Address>,
    pub threshold: u32,
    pub approvals: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FeeTokenInfo {
    pub active: bool,
    pub custom_fee_bps: Option<u32>,
    pub accumulated: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct FeeTokenConfigsMigratedEvent {
    pub scanned_tokens: u32,
    pub migrated_configs: u32,
    pub skipped_existing: u32,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct VersionInfo {
    pub current_version: u32,
    pub upgrade_count: u32,
}

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

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum BatchJobStatus {
    Pending = 0,
    Completed = 1,
    Cancelled = 2,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct BatchEscrowJob {
    pub owner: Address,
    pub params: Vec<EscrowCreateParams>,
    pub next_index: u32,
    pub status: BatchJobStatus,
}

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

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ExpiredDisputeFeePolicy {
    RefundFullNoPlatformFee = 0,
    RefundMinusPlatformFee = 1,
    DeductFeeFromSeller = 2,
    SplitFee = 3,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PlatformConfig {
    pub platform_fee_bps: u32,
    pub platform_wallet: Address,
    pub admin: Address,
    pub arbitrator: Address,
    pub moderator: Option<Address>,
    pub is_paused: bool,
    pub min_stake_required: i128,
    pub pending_admin: Option<Address>,
    pub wasm_upgrade_cooldown: u32,
    pub max_dispute_duration: u32,
    pub stake_cooldown: u32,
    pub expired_dispute_fee_policy: ExpiredDisputeFeePolicy,
    pub min_release_window: u32,
    pub dispute_escalation_window: u32,
    pub evidence_challenge_window: u32,
}

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

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEscalationRecord {
    pub order_id: u32,
    pub escalated_by: Address,
    pub escalated_at: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct RateLimitConfig {
    pub max_calls: u32,
    pub window: u32,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PartialRefundProposal {
    pub order_id: u32,
    pub refund_amount: i128,
    pub proposed_by: Address,
    pub proposed_at: u64,
    pub nonce: u64,
}

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

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct SettlementReceipt {
    pub order_id: u32,
    pub path: SettlementPath,
    pub executed_at: u64,
    pub proposal_nonce: u64,
}

#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum UserRole {
    None = 0,
    Buyer = 1,
    Artisan = 2,
    Admin = 3,
    Moderator = 4,
}

#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ProfileStatus {
    Active = 0,
    Deactivated = 1,
}

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
    pub successful_trades: u32,
    pub disputed_trades: u32,
    pub portfolio_cid: Option<String>,
    pub status: ProfileStatus,
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
    pub successful_trades: u32,
    pub disputed_trades: u32,
    pub portfolio_cid: Option<String>,
}

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
    fn deactivate_profile(env: Env, user: Address);
    fn verify_user(env: Env, user: Address) -> UserProfile;
    fn has_active_contracts(env: Env, user: Address) -> bool;
    fn update_active_contracts(env: Env, user: Address, delta: i32);
    fn get_active_user_count(env: Env) -> u32;
    fn bump_user_profile_ttl(env: Env, user: Address) -> bool;
    fn bump_user_metrics_ttl(env: Env, user: Address) -> bool;
    fn get_user_role(env: Env, user: Address) -> UserRole;
    fn is_profile_active(env: Env, user: Address) -> bool;
    fn get_user_profile_version(env: Env, user: Address) -> u32;
    fn get_user_state_version(env: Env, user: Address) -> u32;
    fn is_user_verified(env: Env, user: Address) -> bool;
}

#[contract]
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

pub const ESCROW_CONTRACT: CraftNexusContract = CraftNexusContract;

pub type EscrowContractClient<'a> = CraftNexusContractClient<'a>;

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

        if len < 3 {
            return false;
        }

        let prefix = cid_bytes[0];
        let payload = &cid_bytes[1..];

        match prefix {
            b'b' => {
                if !(50..=100).contains(&len) || cid_bytes[1] != b'a' {
                    return false;
                }
                payload
                    .iter()
                    .all(|b| matches!(*b, b'a'..=b'z' | b'2'..=b'7'))
            }
            b'f' => {
                if !(60..=120).contains(&len) || cid_bytes[1] != b'0' || cid_bytes[2] != b'1' {
                    return false;
                }
                payload
                    .iter()
                    .all(|b| matches!(*b, b'0'..=b'9' | b'a'..=b'f'))
            }
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

    #[inline(always)]
    fn validate_optional_ipfs_hash(env: &Env, ipfs_hash: &Option<String>) {
        if let Some(cid) = ipfs_hash {
            if !Self::validate_ipfs_cid(cid) {
                env.panic_with_error(crate::Error::InvalidIpfsHash);
            }
        }
    }

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

    fn validate_admin_address(env: &Env, admin: &Address) -> Result<(), Error> {
        let contract = env.current_contract_address();
        if admin == &contract {
            return Err(Error::InvalidAdminAddress);
        }
        Ok(())
    }

    fn validate_platform_wallet(env: &Env, wallet: &Address) -> Result<(), Error> {
        if wallet == &env.current_contract_address() {
            return Err(Error::InvalidPlatformWallet);
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn get_platform_config_safe(env: &Env) -> Result<PlatformConfig, Error> {
        let config: Option<PlatformConfig> = env.storage().persistent().get(&PLATFORM_FEE);

        if let Some(cfg) = config {
            if Self::validate_admin_address(env, &cfg.admin).is_ok() {
                Self::extend_persistent(env, &PLATFORM_FEE);
                return Ok(cfg);
            }
        }

        if let Some(fallback_admin) = env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::FallbackAdmin)
        {
            Self::extend_persistent(env, &DataKey::FallbackAdmin);
            env.events().publish(
                (Symbol::new(env, "admin_config_recovered"), true),
                String::from_str(env, "Using fallback admin after config corruption detected"),
            );
            return Ok(PlatformConfig {
                platform_fee_bps: 500,
                platform_wallet: fallback_admin.clone(),
                admin: fallback_admin,
                arbitrator: env.current_contract_address(),
                moderator: None,
                is_paused: true,
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

    fn update_escrow_indices_atomic(env: &Env, order_id: u32) {
        Self::migrate_legacy_all_escrow_ids(env);

        let count_key = DataKey::EscrowCount;
        let count = Self::get_persistent_u32(env, &count_key);

        let index_key = DataKey::GlobalEscrowIdIndexed(count);
        env.storage().persistent().set(&index_key, &order_id);
        Self::extend_persistent(env, &index_key);

        env.storage().persistent().set(&count_key, &(count + 1));
        Self::extend_persistent(env, &count_key);
    }

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

    fn check_min_amount(env: &Env, token: Address, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::AmountBelowMinimum);
        }

        let min_amount: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MinEscrowAmount(token))
            .unwrap_or(0);

        if amount < min_amount {
            return Err(Error::AmountBelowMinimum);
        }

        Ok(())
    }

    fn record_stake_history(
        env: &Env,
        artisan: &Address,
        new_stake: i128,
        operation: &str,
    ) -> Result<(), Error> {
        let count_key = DataKey::StakeHistoryCount(artisan.clone());
        let _history_key = DataKey::StakeHistory(artisan.clone());

        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if current_count >= MAX_STAKE_HISTORY_SIZE {
            return Err(Error::StakeQueueFull);
        }

        if current_count >= STAKE_HISTORY_PRUNE_THRESHOLD {
            let new_count = current_count / 2;
            env.storage().persistent().set(&count_key, &new_count);
            Self::extend_persistent(env, &count_key);
        }

        let modified_key = DataKey::StakeLastModified(artisan.clone());
        env.storage()
            .persistent()
            .set(&modified_key, &env.ledger().timestamp());
        Self::extend_persistent(env, &modified_key);

        env.events().publish(
            (Symbol::new(env, "stake_operation"), operation.as_bytes()),
            (artisan.clone(), new_stake),
        );

        Ok(())
    }

    #[allow(dead_code)]
    fn prune_stake_history(env: &Env, artisan: &Address) {
        let count_key = DataKey::StakeHistoryCount(artisan.clone());
        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if current_count > 0 {
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

    #[inline(always)]
    fn get_persistent_u32(env: &Env, key: &DataKey) -> u32 {
        Self::read_persistent(env, key).unwrap_or(0u32)
    }

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

    #[inline(always)]
    fn get_max_release_window(env: &Env) -> u32 {
        let key = DataKey::MaxReleaseWindow;
        Self::read_persistent(env, &key).unwrap_or(MAX_TOTAL_RELEASE_WINDOW)
    }

    fn get_onboarding_address(env: &Env) -> Option<Address> {
        env.storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::OnboardingContractAddress)
    }

    fn get_onboarding_client(env: &Env) -> Option<(Address, OnboardingClient<'_>)> {
        Self::get_onboarding_address(env).map(|address| {
            let client = OnboardingClient::new(env, &address);
            (address, client)
        })
    }

    pub fn get_onboarding_contract(env: Env) -> Result<Address, Error> {
        Self::get_onboarding_address(&env).ok_or(Error::OnboardingContractNotSet)
    }

    pub fn has_onboarding_contract(env: Env) -> bool {
        Self::get_onboarding_address(&env).is_some()
    }

    pub fn has_active_escrows(env: Env, user: Address) -> bool {
        let key = DataKey::ActiveObligations(user);
        Self::get_persistent_u32(&env, &key) > 0
    }

    fn emit_onboarding_call_failed(env: &Env, method: Symbol, address: Address) {
        env.events().publish(
            (ONBOARD_CALL_FAILED, method),
            (address, env.ledger().timestamp()),
        );
    }

    #[allow(dead_code)]
    fn safe_update_reputation(
        env: &Env,
        address: Address,
        successful_delta: u32,
        disputed_delta: u32,
    ) -> bool {
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

    fn safe_check_onboarding_state(
        env: &Env,
        user: &Address,
    ) -> Result<(bool, UserRole, bool, u32), ()> {
        let (onboarding_address, _) = match Self::get_onboarding_client(env) {
            Some(c) => c,
            None => return Err(()),
        };

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

    fn validate_onboarding_state(env: &Env, buyer: &Address, seller: &Address) {
        if Self::get_onboarding_address(env).is_none() {
            return;
        }

        for user in [buyer, seller] {
            let (is_active, role, _is_verified, state_version) =
                match Self::safe_check_onboarding_state(env, user) {
                    Ok(state) => state,
                    Err(()) => {
                        Self::emit_onboarding_call_failed(
                            env,
                            Symbol::new(env, "check_state"),
                            user.clone(),
                        );
                        continue;
                    }
                };

            if state_version == 0 {
                env.panic_with_error(Error::OnboardingProfileNotFound);
            }

            if !is_active {
                env.panic_with_error(Error::OnboardingProfileInactive);
            }

            if role == UserRole::None {
                env.panic_with_error(Error::OnboardingRoleMismatch);
            }
        }
    }

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

    pub fn get_min_release_window(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.min_release_window
    }

    pub fn set_onboarding_contract(env: Env, contract_address: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if contract_address == env.current_contract_address() {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let previous = Self::get_onboarding_address(&env);

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

    pub fn whitelist_token(env: Env, token: Address) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

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

    pub fn get_whitelisted_token_count(env: Env) -> u32 {
        Self::migrate_legacy_whitelisted_tokens(&env);
        Self::get_whitelist_count(&env)
    }

    pub fn migrate_whitelist_storage(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let legacy_key = DataKey::WhitelistedTokens;

        if !env.storage().persistent().has(&legacy_key) {
            return 0; 
        }

        let legacy_whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(Map::new(&env));

        let mut migrated_count = 0u32;

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

        if migrated_count > 0 {
            let count_key = DataKey::WhitelistedTokenCount;
            env.storage().persistent().set(&count_key, &migrated_count);
            Self::extend_persistent(&env, &count_key);
        }

        env.storage().persistent().remove(&legacy_key);

        migrated_count
    }

    pub fn migrate_artisan_stake_queue(env: Env, artisan: Address) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let legacy_key = DataKey::ArtisanStakeQueue(artisan.clone());

        if !env.storage().persistent().has(&legacy_key) {
            return 0; 
        }

        let legacy_queue: soroban_sdk::Vec<StakeDeposit> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let queue_len = legacy_queue.len();
        if queue_len == 0 {
            env.storage().persistent().remove(&legacy_key);
            return 0;
        }

        for i in 0..queue_len {
            if let Some(deposit) = legacy_queue.get(i) {
                let deposit_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), i);
                env.storage().persistent().set(&deposit_key, &deposit);
                Self::extend_persistent(&env, &deposit_key);
            }
        }

        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        env.storage().persistent().set(&count_key, &queue_len);
        Self::extend_persistent(&env, &count_key);

        env.storage().persistent().remove(&legacy_key);

        queue_len
    }

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

    pub fn get_artisan_stake_queue_count(env: Env, artisan: Address) -> u32 {
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        env.storage().persistent().get(&count_key).unwrap_or(0)
    }

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

    pub fn initialize(
        env: Env,
        platform_wallet: Address,
        admin: Address,
        arbitrator: Address,
        platform_fee_bps: u32,
        onboarding_contract: Option<Address>,
    ) {
        admin.require_auth();

        if platform_fee_bps > MAX_PLATFORM_FEE_BPS {
            env.panic_with_error(crate::Error::InvalidFee);
        }

        if let Err(e) = Self::validate_platform_wallet(&env, &platform_wallet) {
            env.panic_with_error(e);
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
            dispute_escalation_window: DEFAULT_DISPUTE_ESCALATION_WINDOW,
            evidence_challenge_window: DEFAULT_EVIDENCE_CHALLENGE_WINDOW,
        };

        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        if let Err(e) = Self::set_fallback_admin(&env, admin.clone()) {
            env.panic_with_error(e);
        }

        env.storage()
            .persistent()
            .set(&PLATFORM_WALLET, &platform_wallet);
        Self::extend_persistent(&env, &PLATFORM_WALLET);

        let zero: i128 = 0;
        env.storage().persistent().set(&TOTAL_FEES, &zero);
        Self::extend_persistent(&env, &TOTAL_FEES);

        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &1u32);
        Self::extend_persistent(&env, &DataKey::ContractVersion);

        env.storage().persistent().set(
            &DataKey::StorageLayoutVersion,
            &CURRENT_STORAGE_LAYOUT_VERSION,
        );
        Self::extend_persistent(&env, &DataKey::StorageLayoutVersion);

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

    pub fn update_admin(env: Env, new_admin: Address) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if Self::validate_admin_address(&env, &new_admin).is_err() {
            env.panic_with_error(Error::InvalidAdminAddress);
        }

        new_admin.require_auth();

        let previous_admin = config.admin.clone();
        config.pending_admin = Some(new_admin.clone());
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        Self::emit_admin_changed(&env, previous_admin, new_admin, "admin_proposed");
    }

    pub fn claim_admin(env: Env) {
        let mut config = Self::get_platform_config_internal(&env);
        let pending = config.pending_admin.as_ref().expect("");
        pending.require_auth();

        if Self::validate_admin_address(&env, pending).is_err() {
            env.panic_with_error(Error::InvalidAdminAddress);
        }

        let previous_admin = config.admin.clone();
        let new_admin = pending.clone();
        config.admin = new_admin.clone();
        config.pending_admin = None;

        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        Self::emit_admin_changed(&env, previous_admin, new_admin, "admin_claimed");
    }

    pub fn cancel_admin_transfer(env: Env) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if config.pending_admin.is_none() {
            return Err(Error::NoPendingAdmin);
        }

        config.pending_admin = None;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        Ok(())
    }

    fn get_admin_action_signers(env: &Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&AdminActionDataKey::AdminActionSigners)
            .unwrap_or_else(|| {
                let mut signers = Vec::new(env);
                if let Ok(admin) = Self::get_admin(env) {
                    signers.push_back(admin);
                }
                signers
            })
    }

    fn get_admin_action_threshold(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&AdminActionDataKey::AdminActionThreshold)
            .unwrap_or(1u32)
    }

    fn get_admin_action_timelock_delay(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&AdminActionDataKey::AdminActionTimelockDelay)
            .unwrap_or(DEFAULT_ADMIN_ACTION_TIMELOCK_DELAY)
    }

    fn get_next_admin_action_id(env: &Env) -> u64 {
        env.storage()
            .persistent()
            .get(&AdminActionDataKey::NextAdminActionId)
            .unwrap_or(1u64)
    }

    fn get_admin_action(env: &Env, action_id: u64) -> Option<AdminActionProposal> {
        env.storage()
            .persistent()
            .get::<AdminActionDataKey, AdminActionProposal>(&AdminActionDataKey::AdminAction(
                action_id,
            ))
    }

    fn persist_admin_action(env: &Env, action: &AdminActionProposal) {
        env.storage()
            .persistent()
            .set(&AdminActionDataKey::AdminAction(action.id), action);
        Self::extend_persistent(env, &AdminActionDataKey::AdminAction(action.id));
    }

    pub fn set_admin_action_signers(env: Env, signers: Vec<Address>) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        if signers.is_empty() {
            env.storage()
                .persistent()
                .remove(&AdminActionDataKey::AdminActionSigners);
        } else {
            env.storage()
                .persistent()
                .set(&AdminActionDataKey::AdminActionSigners, &signers);
            Self::extend_persistent(&env, &AdminActionDataKey::AdminActionSigners);
        }
        Ok(())
    }

    pub fn set_admin_action_threshold(env: Env, threshold: u32) -> Result<(), Error> {
        if threshold == 0 {
            return Err(Error::InvalidFee);
        }
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&AdminActionDataKey::AdminActionThreshold, &threshold);
        Ok(())
    }

    pub fn set_admin_action_timelock_delay(env: Env, delay_seconds: u64) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        env.storage().instance().set(
            &AdminActionDataKey::AdminActionTimelockDelay,
            &delay_seconds,
        );
        Ok(())
    }

    pub fn propose_admin_action(
        env: Env,
        proposer: Address,
        action: AdminActionKind,
    ) -> Result<AdminActionProposal, Error> {
        proposer.require_auth();

        let signers = Self::get_admin_action_signers(&env);
        if !signers.iter().any(|signer| signer == proposer) {
            return Err(Error::NotAnAdminActionSigner);
        }

        let threshold = Self::get_admin_action_threshold(&env);
        let delay = Self::get_admin_action_timelock_delay(&env);
        let created_at = env.ledger().timestamp();
        let next_id = Self::get_next_admin_action_id(&env);

        let mut approvals = Vec::new(&env);
        approvals.push_back(proposer.clone());

        let proposal = AdminActionProposal {
            id: next_id,
            kind: action,
            proposer: proposer.clone(),
            approvals,
            threshold,
            signers: signers.clone(),
            created_at,
            ready_at: created_at + delay,
            executed: false,
            cancelled: false,
        };

        env.storage()
            .persistent()
            .set(&AdminActionDataKey::NextAdminActionId, &(next_id + 1));
        Self::extend_persistent(&env, &AdminActionDataKey::NextAdminActionId);
        Self::persist_admin_action(&env, &proposal);

        Ok(proposal)
    }

    pub fn approve_admin_action(
        env: Env,
        action_id: u64,
        signer: Address,
    ) -> Result<AdminActionProposal, Error> {
        signer.require_auth();

        let mut action =
            Self::get_admin_action(&env, action_id).ok_or(Error::AdminActionTerminal)?;
        if action.cancelled {
            return Err(Error::AdminActionTerminal);
        }
        if action.executed {
            return Err(Error::AdminActionTerminal);
        }
        if !action.signers.iter().any(|existing| existing == signer) {
            return Err(Error::NotAnAdminActionSigner);
        }
        if action.approvals.iter().any(|existing| existing == signer) {
            return Err(Error::AlreadyApproved);
        }

        action.approvals.push_back(signer);
        Self::persist_admin_action(&env, &action);
        Ok(action)
    }

    pub fn cancel_admin_action(env: Env, action_id: u64) -> Result<AdminActionProposal, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut action =
            Self::get_admin_action(&env, action_id).ok_or(Error::AdminActionTerminal)?;
        if action.cancelled {
            return Err(Error::AdminActionTerminal);
        }
        if action.executed {
            return Err(Error::AdminActionTerminal);
        }

        action.cancelled = true;
        Self::persist_admin_action(&env, &action);
        Ok(action)
    }

    pub fn execute_admin_action(env: Env, action_id: u64) -> Result<(), Error> {
        let action = Self::get_admin_action(&env, action_id).ok_or(Error::AdminActionTerminal)?;
        if action.cancelled {
            return Err(Error::AdminActionTerminal);
        }
        if action.executed {
            return Err(Error::AdminActionTerminal);
        }
        if action.approvals.len() < action.threshold {
            return Err(Error::AdminActionNeedsApprovals);
        }
        let now = env.ledger().timestamp();
        if now < action.ready_at {
            return Err(Error::AdminActionTimelockActive);
        }

        let mut persisted = action.clone();
        Self::apply_admin_action(&env, &persisted)?;
        persisted.executed = true;
        Self::persist_admin_action(&env, &persisted);
        Ok(())
    }

    pub fn get_pending_admin_actions(env: Env) -> Vec<AdminActionProposal> {
        let mut actions = Vec::new(&env);
        let next_id = Self::get_next_admin_action_id(&env);
        for action_id in 1..next_id {
            if let Some(action) = Self::get_admin_action(&env, action_id) {
                if !action.executed && !action.cancelled {
                    actions.push_back(action);
                }
            }
        }
        actions
    }

    fn apply_admin_action(env: &Env, action: &AdminActionProposal) -> Result<(), Error> {
        match &action.kind {
            AdminActionKind::PausePlatform(paused) => Self::set_paused_internal(env, *paused),
            AdminActionKind::SetPlatformFee(new_fee_bps) => {
                let mut config = Self::get_platform_config_internal(env);
                if *new_fee_bps > MAX_PLATFORM_FEE_BPS {
                    return Err(Error::InvalidFee);
                }
                let old_fee = config.platform_fee_bps;
                config.platform_fee_bps = *new_fee_bps;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "platform_fee_bps",
                    ConfigValue::U32(old_fee),
                    ConfigValue::U32(*new_fee_bps),
                );
                Ok(())
            }
            AdminActionKind::SetPlatformWallet(new_wallet) => {
                let mut config = Self::get_platform_config_internal(env);
                let old_wallet = config.platform_wallet.clone();
                config.platform_wallet = new_wallet.clone();
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "platform_wallet",
                    ConfigValue::Address(old_wallet),
                    ConfigValue::Address(new_wallet.clone()),
                );
                Ok(())
            }
            AdminActionKind::SetWasmUpgradeCooldown(cooldown_seconds) => {
                let mut config = Self::get_platform_config_internal(env);
                let old_value = config.wasm_upgrade_cooldown;
                config.wasm_upgrade_cooldown = *cooldown_seconds;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "wasm_upgrade_cooldown",
                    ConfigValue::U32(old_value),
                    ConfigValue::U32(*cooldown_seconds),
                );
                Ok(())
            }
            AdminActionKind::SetMinStakeRequired(min_stake) => {
                let mut config = Self::get_platform_config_internal(env);
                config.min_stake_required = *min_stake;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Ok(())
            }
            AdminActionKind::SweepUnallocatedFunds(token, destination) => {
                if env
                    .storage()
                    .persistent()
                    .get::<DataKey, ReconciliationReport>(&DataKey::ReconciliationReport(token.clone()))
                    .is_some_and(|report| report.unresolved)
                {
                    return Err(Error::ReconciliationRequired);
                }
                let allocation = Self::fund_allocation(env, token);
                if allocation.unallocated < 0 {
                    return Err(Error::EmergencyAccountingInvariant);
                }
                let unallocated = allocation.unallocated;
                if unallocated > 0 {
                    Self::transfer_tokens_and_record_audit(
                        env,
                        token,
                        &env.current_contract_address(),
                        destination,
                        unallocated,
                        destination,
                        Symbol::new(env, "sweep_unallocated"),
                        unallocated,
                    );
                }
                Ok(())
            }
            AdminActionKind::ExecuteUpgrade(expected_wasm_hash) => {
                Self::execute_upgrade(env.clone(), expected_wasm_hash.clone())
            }
            AdminActionKind::SetMaxDisputeDuration(duration) => {
                let mut config = Self::get_platform_config_internal(env);
                let old_value = config.max_dispute_duration;
                config.max_dispute_duration = *duration;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "max_dispute_duration",
                    ConfigValue::U32(old_value),
                    ConfigValue::U32(*duration),
                );
                Ok(())
            }
            AdminActionKind::SetStakeCooldown(cooldown) => {
                let mut config = Self::get_platform_config_internal(env);
                let old_value = config.stake_cooldown;
                config.stake_cooldown = *cooldown;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "stake_cooldown",
                    ConfigValue::U32(old_value),
                    ConfigValue::U32(*cooldown),
                );
                Ok(())
            }
            AdminActionKind::SetArtisanFeeTier(artisan, fee_bps) => {
                let config = Self::get_platform_config_internal(env);
                if *fee_bps > MAX_PLATFORM_FEE_BPS {
                    return Err(Error::InvalidFee);
                }
                config.admin.require_auth();
                env.storage()
                    .persistent()
                    .set(&DataKey::ArtisanFeeTier(artisan.clone()), fee_bps);
                Self::extend_persistent(env, &DataKey::ArtisanFeeTier(artisan.clone()));
                Self::emit_artisan_fee_tier_updated(env, artisan.clone(), *fee_bps);
                Ok(())
            }
            AdminActionKind::SetModerator(moderator) => {
                let mut config = Self::get_platform_config_internal(env);
                let previous = config
                    .moderator
                    .clone()
                    .map(ConfigValue::Address)
                    .unwrap_or_else(|| ConfigValue::String(String::from_str(env, "unset")));
                config.moderator = Some(moderator.clone());
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "moderator",
                    previous,
                    ConfigValue::Address(moderator.clone()),
                );
                Ok(())
            }
            AdminActionKind::SetMinEscrowAmount(token, min_amount) => {
                let admin = Self::get_admin(env)?;
                admin.require_auth();
                let key = DataKey::MinEscrowAmount(token.clone());
                let old_amount: i128 = env.storage().persistent().get(&key).unwrap_or(0);
                env.storage().persistent().set(&key, min_amount);
                Self::extend_persistent(env, &key);
                Self::emit_config_updated(
                    env,
                    "min_escrow_amount",
                    ConfigValue::I128(old_amount),
                    ConfigValue::I128(*min_amount),
                );
                Ok(())
            }
            AdminActionKind::SetMaxReleaseWindow(window) => {
                let old_value: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::MaxReleaseWindow)
                    .unwrap_or(MAX_TOTAL_RELEASE_WINDOW);
                env.storage()
                    .persistent()
                    .set(&DataKey::MaxReleaseWindow, window);
                Self::extend_persistent(env, &DataKey::MaxReleaseWindow);
                Self::emit_config_updated(
                    env,
                    "max_release_window",
                    ConfigValue::U32(old_value),
                    ConfigValue::U32(*window),
                );
                Ok(())
            }
            AdminActionKind::SetMinReleaseWindow(window) => {
                let mut config = Self::get_platform_config_internal(env);
                let old_value = config.min_release_window;
                config.min_release_window = *window;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "min_release_window",
                    ConfigValue::U32(old_value),
                    ConfigValue::U32(*window),
                );
                Ok(())
            }
            AdminActionKind::SetOnboardingContract(address) => {
                let admin = Self::get_admin(env)?;
                admin.require_auth();
                env.storage()
                    .instance()
                    .set(&DataKey::OnboardingContractAddress, address);
                Self::extend_persistent(env, &DataKey::OnboardingContractAddress);
                Ok(())
            }
            AdminActionKind::SetExpiredDisputePolicy(policy) => {
                let mut config = Self::get_platform_config_internal(env);
                let old_policy = config.expired_dispute_fee_policy;
                config.expired_dispute_fee_policy = *policy;
                env.storage()
                    .instance()
                    .set(&DataKey::PlatformConfig, &config);
                Self::emit_config_updated(
                    env,
                    "expired_dispute_fee_policy",
                    ConfigValue::U32(old_policy as u32),
                    ConfigValue::U32(*policy as u32),
                );
                Ok(())
            }
            AdminActionKind::ApplyReconciliationRepair(plan_id) => {
                Self::apply_reconciliation_repair(env, *plan_id)
            }
        }
    }

    fn apply_reconciliation_repair(env: &Env, plan_id: u64) -> Result<(), Error> {
        let key = DataKey::ReconciliationRepairPlan(plan_id);
        let mut plan: ReconciliationRepairPlan = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::RepairPlanNotFound)?;
        if plan.applied || plan.cancelled {
            return Err(Error::RepairPlanTerminal);
        }

        let allocation = Self::fund_allocation(env, &plan.token);
        if allocation.balance != plan.observed_balance
            || allocation.total_locked != plan.observed_tracked_locked
            || allocation.total_staked != plan.observed_tracked_staked
            || allocation.balance < plan.expected_locked + plan.expected_staked
        {
            return Err(Error::RepairPlanPreconditionFailed);
        }

        env.storage().persistent().set(
            &DataKey::TotalLocked(plan.token.clone()),
            &plan.expected_locked,
        );
        env.storage().persistent().set(
            &DataKey::TotalStaked(plan.token.clone()),
            &plan.expected_staked,
        );
        env.storage()
            .persistent()
            .remove(&DataKey::ReconciliationReport(plan.token.clone()));
        plan.applied = true;
        env.storage().persistent().set(&key, &plan);
        Self::extend_persistent(env, &key);
        Ok(())
    }

    pub fn recover_admin_access(env: Env, recovered_admin: Address) -> Result<(), Error> {
        let fallback = match env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::FallbackAdmin)
        {
            Some(fallback) => fallback,
            None => return Err(Error::AdminRecoveryFailed),
        };

        fallback.require_auth();

        if Self::validate_admin_address(&env, &recovered_admin).is_err() {
            return Err(Error::AdminRecoveryFailed);
        }

        if let Ok(current_admin) = Self::get_admin(&env) {
            if recovered_admin == current_admin {
                return Err(Error::AdminRecoveryFailed);
            }
        }

        let recovery_time = Self::get_persistent_u64(&env, &DataKey::AdminRecoveryTime);

        let current_time = env.ledger().timestamp();

        if recovery_time == 0 {
            let new_recovery_time = current_time + ADMIN_RECOVERY_DELAY;
            let recovery_time_key = DataKey::AdminRecoveryTime;
            env.storage()
                .persistent()
                .set(&recovery_time_key, &new_recovery_time);
            Self::extend_persistent(&env, &recovery_time_key);

            let delay_key = DataKey::AdminRecoveryDelay;
            env.storage()
                .persistent()
                .set(&delay_key, &ADMIN_RECOVERY_DELAY);
            Self::extend_persistent(&env, &delay_key);

            env.events().publish(
                (Symbol::new(&env, "admin_recovery_initiated"), true),
                String::from_str(&env, "7-day time lock initiated for admin recovery"),
            );
            return Err(Error::AdminRecoveryFailed);
        }

        if current_time < recovery_time {
            return Err(Error::AdminRecoveryFailed);
        }

        let recorded_delay = Self::get_persistent_u64(&env, &DataKey::AdminRecoveryDelay);
        if recorded_delay == 0 || recorded_delay < MIN_ADMIN_RECOVERY_COOLDOWN {
            return Err(Error::AdminRecoveryFailed);
        }

        let mut config = Self::get_platform_config_internal(&env);
        let previous_admin = config.admin.clone();

        config.admin = recovered_admin.clone();
        config.pending_admin = None;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        env.storage().persistent().set(&PLATFORM_FEE, &config);

        env.storage()
            .persistent()
            .remove(&DataKey::AdminRecoveryTime);
        env.storage()
            .persistent()
            .remove(&DataKey::AdminRecoveryDelay);

        Self::emit_admin_changed(&env, previous_admin, recovered_admin, "admin_recovered");

        Ok(())
    }

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
            None,
        )
    }

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
        service_agreement_hash: Option<Bytes>,
    ) -> Escrow {
        let _guard = ReentryGuardScope::new(&env);
        Self::check_not_paused(&env);
        buyer.require_auth();

        if let Err(e) = Self::check_min_amount(&env, token.clone(), amount) {
            env.panic_with_error(e);
        }

        if buyer == seller {
            env.panic_with_error(crate::Error::SameBuyerSeller);
        }

        Self::check_token_whitelisted(&env, &token);

        let config = Self::get_platform_config_internal(&env);
        if config.min_stake_required > 0 {
            Self::migrate_legacy_artisan_stake(env.clone(), seller.clone());
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

        let window = release_window.unwrap_or(604800u32);

        let min_window = config.min_release_window;
        let max_window = Self::get_max_release_window(&env);

        if window < min_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }
        if window > max_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }

        Self::validate_onboarding_state(&env, &buyer, &seller);

        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;
        Self::validate_optional_ipfs_hash(&env, &ipfs_hash);
        Self::validate_optional_metadata_hash(&env, &metadata_hash);
        Self::validate_optional_service_agreement_hash(&env, &service_agreement_hash);

        Self::assert_escrow_not_exists(&env, order_id);

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
            funding_deadline: None,
            service_agreement_hash: service_agreement_hash.clone(),
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        Self::update_active_obligations(&env, &buyer, 1);
        Self::update_active_obligations(&env, &seller, 1);

        Self::update_escrow_indices_atomic(&env, order_id);

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

        Self::safe_update_active_contracts(&env, buyer.clone(), 1);
        Self::safe_update_active_contracts(&env, seller.clone(), 1);

        Self::update_total_locked(&env, &token, amount);
        Self::transfer_tokens_and_record_audit(
            &env,
            &token,
            &buyer,
            &env.current_contract_address(),
            amount,
            &buyer,
            Symbol::new(&env, "escrow_funded"),
            -amount,
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Created,
                buyer: buyer.clone(),
                seller: seller.clone(),
                amount,
                token: token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        escrow
    }

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
        service_agreement_hash: Option<Bytes>,
    ) -> Escrow {
        let _guard = ReentryGuardScope::new(&env);

        let config = Self::get_platform_config_internal(&env);
        let min_window = config.min_release_window;
        let max_window = Self::get_max_release_window(&env);

        if window < min_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }
        if window > max_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }

        Self::validate_onboarding_state(&env, &buyer, &seller);

        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;
        Self::validate_optional_ipfs_hash(&env, &ipfs_hash);
        Self::validate_optional_metadata_hash(&env, &metadata_hash);
        Self::validate_optional_service_agreement_hash(&env, &service_agreement_hash);

        let funding_deadline = created_at_u64 + UNFUNDED_CANCEL_TIMEOUT;

        Self::assert_escrow_not_exists(&env, order_id);

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
            funded: false,
            funding_deadline: Some(funding_deadline),
            service_agreement_hash: service_agreement_hash.clone(),
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

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

        Self::update_active_obligations(&env, &buyer, 1);
        Self::update_active_obligations(&env, &seller, 1);

        Self::safe_update_active_contracts(&env, buyer.clone(), 1);
        Self::safe_update_active_contracts(&env, seller.clone(), 1);

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Created,
                buyer: buyer.clone(),
                seller: seller.clone(),
                amount,
                token: token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        escrow
    }

    pub fn fund_escrow(env: Env, order_id: u32) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let mut escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.funded {
            return Err(Error::InvalidEscrowState);
        }

        escrow.buyer.require_auth();

        escrow.funded = true;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));
        Self::update_total_locked(&env, &escrow.token, escrow.amount);

        Self::transfer_tokens_and_record_audit(
            &env,
            &escrow.token,
            &escrow.buyer,
            &env.current_contract_address(),
            escrow.amount,
            &escrow.buyer,
            Symbol::new(&env, "escrow_funded"),
            -escrow.amount,
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Created, 
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn cancel_unfunded_escrow(env: Env, order_id: u32, caller: Address) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.funded {
            return Err(Error::InvalidEscrowState);
        }

        let current_time = env.ledger().timestamp();
        let deadline = escrow
            .funding_deadline
            .unwrap_or((escrow.created_at as u64) + UNFUNDED_CANCEL_TIMEOUT);

        if time_policy::is_deadline_reached(current_time, deadline) {
            let admin = Self::get_admin(&env).unwrap_or(escrow.buyer.clone());
            if caller != escrow.buyer && caller != escrow.seller && caller != admin {
                return Err(Error::Unauthorized);
            }
            caller.require_auth();
        } else {
            if caller != escrow.buyer {
                return Err(Error::Unauthorized);
            }
            caller.require_auth();
        }

        env.storage().persistent().remove(&(ESCROW, order_id));

        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Ok(())
    }

    pub fn auto_cancel_unfunded(
        env: Env,
        admin: Address,
        order_ids: soroban_sdk::Vec<u32>,
    ) -> Result<u32, Error> {
        let _guard = ReentryGuardScope::new(&env);

        let stored_admin = Self::get_admin(&env)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let current_time = env.ledger().timestamp();
        let mut cancelled_count: u32 = 0;

        for order_id in order_ids.iter() {
            let key = (ESCROW, order_id);

            let escrow: Escrow = match env.storage().persistent().get(&key) {
                Some(e) => e,
                None => continue,
            };

            if escrow.funded {
                continue;
            }

            let deadline = escrow
                .funding_deadline
                .unwrap_or((escrow.created_at as u64) + UNFUNDED_CANCEL_TIMEOUT);
            if current_time < deadline {
                continue;
            }

            env.storage().persistent().remove(&key);
            Self::update_active_obligations(&env, &escrow.buyer, -1);
            Self::update_active_obligations(&env, &escrow.seller, -1);
            Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
            Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

            cancelled_count += 1;
        }

        Ok(cancelled_count)
    }

    pub fn get_escrows_by_buyer(
        env: Env,
        buyer: Address,
        page: u32,
        page_size: u32,
        reverse: bool,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        buyer.require_auth();
        let mut result = soroban_sdk::Vec::new(&env);

        let page_size =
            pagination_validation::validate_limit(page_size, pagination_validation::MAX_PAGE_SIZE)?;

        let count_key = DataKey::BuyerEscrowCount(buyer.clone());
        if env.storage().persistent().has(&count_key) {
            let total_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0u32);
            let start = page * page_size;

            if start >= total_count {
                return Ok(result);
            }

            let end = (start + page_size).min(total_count);

            for position in start..end {
                let storage_index = if reverse {
                    total_count - 1 - position
                } else {
                    position
                };
                let index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), storage_index);
                if let Some(escrow_id) = env.storage().persistent().get::<_, u64>(&index_key) {
                    result.push_back(escrow_id);
                    Self::extend_persistent_read(&env, &index_key);
                }
            }

            Self::extend_persistent_read(&env, &count_key);
            return Ok(result);
        }

        let legacy_key = DataKey::BuyerEscrows(buyer);
        let escrow_ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        if env.storage().persistent().has(&legacy_key) {
            Self::extend_persistent_read(&env, &legacy_key);
        }

        let start = page * page_size;
        let len = escrow_ids.len();

        if start >= len {
            return Ok(result);
        }

        let end = (start + page_size).min(len);
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

    pub fn get_escrows_by_seller(
        env: Env,
        seller: Address,
        page: u32,
        page_size: u32,
        reverse: bool,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        seller.require_auth();
        let mut result = soroban_sdk::Vec::new(&env);

        let page_size =
            pagination_validation::validate_limit(page_size, pagination_validation::MAX_PAGE_SIZE)?;

        let count_key = DataKey::SellerEscrowCount(seller.clone());
        if env.storage().persistent().has(&count_key) {
            let total_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0u32);
            let start = page * page_size;

            if start >= total_count {
                return Ok(result);
            }

            let end = (start + page_size).min(total_count);

            for position in start..end {
                let storage_index = if reverse {
                    total_count - 1 - position
                } else {
                    position
                };
                let index_key = DataKey::SellerEscrowIndexed(seller.clone(), storage_index);
                if let Some(escrow_id) = env.storage().persistent().get::<_, u64>(&index_key) {
                    result.push_back(escrow_id);
                    Self::extend_persistent_read(&env, &index_key);
                }
            }

            Self::extend_persistent_read(&env, &count_key);
            return Ok(result);
        }

        let legacy_key = DataKey::SellerEscrows(seller);
        let escrow_ids: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&legacy_key)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        if env.storage().persistent().has(&legacy_key) {
            Self::extend_persistent_read(&env, &legacy_key);
        }

        let start = page * page_size;
        let len = escrow_ids.len();

        if start >= len {
            return Ok(result);
        }

        let end = (start + page_size).min(len);
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

    pub fn get_platform_config(env: Env) -> PlatformConfig {
        Self::get_platform_config_internal(&env)
    }

    fn get_platform_config_internal(env: &Env) -> PlatformConfig {
        let key = DataKey::PlatformConfig;
        let stored: Val = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::PlatformNotInitialized));

        let config = PlatformConfig::try_from_val(env, &stored).expect("Corrupted PlatformConfig");
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTENSION);
        config
    }

    fn set_paused_internal(env: &Env, paused: bool) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(env);
        config.is_paused = paused;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        if paused {
            Self::emit_platform_paused(env, config.admin.clone());
        } else {
            Self::emit_platform_unpaused(env, config.admin.clone());
        }
        Ok(())
    }

    fn try_get_escrow_readonly(env: &Env, order_id: u32) -> Escrow {
        let key = (ESCROW, order_id);
        let stored: Val = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::EscrowNotFound));
        let map = Map::<Symbol, Val>::try_from_val(env, &stored).expect("");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let batch_id_key = Symbol::new(env, "batch_id");
            if map.contains_key(batch_id_key) {
                let sah_key = Symbol::new(env, "service_agreement_hash");
                let mut escrow = if map.contains_key(sah_key) {
                    Escrow::try_from_val(env, &stored).expect("")
                } else {
                    let v4 = EscrowV4::try_from_val(env, &stored).expect("");
                    Self::escrow_from_v4(v4)
                };
                if escrow.version < CURRENT_ESCROW_VERSION {
                    escrow.version = CURRENT_ESCROW_VERSION;
                }
                Self::extend_persistent(env, &key);
                return escrow;
            }

            let previous = EscrowWithoutBatch::try_from_val(env, &stored).expect("");
            let mut escrow = Self::escrow_from_without_batch(env, previous);
            if escrow.version < CURRENT_ESCROW_VERSION {
                escrow.version = CURRENT_ESCROW_VERSION;
            }
            Self::extend_persistent(env, &key);
            return escrow;
        }

        let legacy = LegacyEscrow::try_from_val(env, &stored).expect("");

        let dispute_symbol = legacy.dispute_reason.map(|r| {
            let len = r.len() as usize;
            let slice_len = core::cmp::min(len, 32);
            let mut buf = [0u8; 32];
            r.copy_into_slice(&mut buf[..slice_len]);
            let s = core::str::from_utf8(&buf[..slice_len]).unwrap();
            Symbol::new(env, s)
        });

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
            dispute_reason: dispute_symbol,
            dispute_initiated_at: legacy.dispute_initiated_at,
            funded: true,
            funding_deadline: None,
            service_agreement_hash: None,
        };
        Self::extend_persistent(env, &key);
        upgraded
    }

    fn get_stored_escrow(env: &Env, order_id: u32) -> Escrow {
        let key = (ESCROW, order_id);
        let stored: Val = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::EscrowNotFound));
        let map = Map::<Symbol, Val>::try_from_val(env, &stored).expect("");
        let version_key = Symbol::new(env, "version");

        if map.contains_key(version_key) {
            let batch_id_key = Symbol::new(env, "batch_id");
            let escrow = if map.contains_key(batch_id_key) {
                let sah_key = Symbol::new(env, "service_agreement_hash");
                if map.contains_key(sah_key) {
                    Escrow::try_from_val(env, &stored).expect("")
                } else {
                    let v4 = EscrowV4::try_from_val(env, &stored).expect("");
                    Self::escrow_from_v4(v4)
                }
            } else {
                let previous = EscrowWithoutBatch::try_from_val(env, &stored).expect("");
                Self::escrow_from_without_batch(env, previous)
            };
            if escrow.version < CURRENT_ESCROW_VERSION {
                return Self::upgrade_escrow(env, order_id, escrow);
            }
            Self::extend_persistent(env, &key);
            return escrow;
        }

        let legacy = LegacyEscrow::try_from_val(env, &stored).expect("");
        let dispute_symbol = legacy.dispute_reason.map(|r| {
            let len = r.len() as usize;
            let slice_len = core::cmp::min(len, 32);
            let mut buf = [0u8; 32];
            r.copy_into_slice(&mut buf[..slice_len]);
            let s = core::str::from_utf8(&buf[..slice_len]).unwrap();
            Symbol::new(env, s)
        });
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
            dispute_reason: dispute_symbol,
            dispute_initiated_at: legacy.dispute_initiated_at,
            funded: true,
            funding_deadline: None,
            service_agreement_hash: None,
        };
        env.storage().persistent().set(&key, &upgraded);
        Self::extend_persistent(env, &key);
        upgraded
    }

    fn assert_escrow_not_exists(env: &Env, order_id: u32) {
        if env.storage().persistent().has(&(ESCROW, order_id)) {
            env.panic_with_error(crate::Error::EscrowAlreadyExists);
        }
    }

    fn assert_valid_transition(
        previous_status: EscrowStatus,
        next_status: EscrowStatus,
    ) -> Result<(), Error> {
        let is_allowed = matches!(
            (previous_status, next_status),
            (EscrowStatus::Active, EscrowStatus::DisputePending)
                | (EscrowStatus::Active, EscrowStatus::ReleasePending)
                | (EscrowStatus::Active, EscrowStatus::RefundPending)
                | (EscrowStatus::DisputePending, EscrowStatus::Disputed)
                | (EscrowStatus::Disputed, EscrowStatus::SettlementPending)
                | (EscrowStatus::SettlementPending, EscrowStatus::Resolved)
                | (EscrowStatus::ReleasePending, EscrowStatus::Released)
                | (EscrowStatus::RefundPending, EscrowStatus::Refunded)
        );

        if is_allowed {
            Ok(())
        } else {
            Err(Error::InvalidEscrowState)
        }
    }

    fn claim_active_escrow_transition(
        env: &Env,
        order_id: u32,
        pending_status: EscrowStatus,
    ) -> Result<Escrow, Error> {
        let mut escrow = Self::get_stored_escrow(env, order_id);
        Self::assert_valid_transition(escrow.status, pending_status)?;

        escrow.status = pending_status;
        let key = (ESCROW, order_id);
        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(env, &key);
        Ok(escrow)
    }

    pub fn diagnose_escrow_state(env: Env, order_id: u32) -> EscrowStateDiagnostic {
        Self::inspect_escrow_state(&env, order_id)
    }

    fn inspect_escrow_state(env: &Env, order_id: u32) -> EscrowStateDiagnostic {
        let key = (ESCROW, order_id);
        let escrow_opt = env.storage().persistent().get::<(Symbol, u32), Escrow>(&key);
        if escrow_opt.is_none() {
            return EscrowStateDiagnostic {
                order_id,
                status: EscrowStatus::Active,
                is_consistent: false,
                issue: EscrowStateIssue::EscrowNotFound,
            };
        }

        let escrow = escrow_opt.unwrap();
        let pending_incomplete = matches!(
            escrow.status,
            EscrowStatus::ReleasePending
                | EscrowStatus::RefundPending
                | EscrowStatus::DisputePending
                | EscrowStatus::SettlementPending
        );
        let missing_dispute_timestamp = escrow.status == EscrowStatus::Disputed
            && escrow.dispute_initiated_at.is_none();
        let terminal_without_receipt = matches!(
            escrow.status,
            EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved
        ) && !Self::has_settlement_receipt(env, order_id)
            && escrow.status == EscrowStatus::Resolved;

        let issue = if pending_incomplete {
            EscrowStateIssue::PendingTransitionUnfinished
        } else if missing_dispute_timestamp {
            EscrowStateIssue::MissingDisputeTimestamp
        } else if terminal_without_receipt {
            EscrowStateIssue::SettlementReceiptConflict
        } else {
            EscrowStateIssue::None
        };

        EscrowStateDiagnostic {
            order_id,
            status: escrow.status,
            is_consistent: matches!(issue, EscrowStateIssue::None),
            issue,
        }
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

    fn escrow_from_without_batch(env: &Env, escrow: EscrowWithoutBatch) -> Escrow {
        let dispute_symbol = escrow.dispute_reason.map(|r| {
            let len = r.len() as usize;
            let slice_len = core::cmp::min(len, 32);
            let mut buf = [0u8; 32];
            r.copy_into_slice(&mut buf[..slice_len]);
            let s = core::str::from_utf8(&buf[..slice_len]).unwrap();
            Symbol::new(env, s)
        });

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
            dispute_reason: dispute_symbol,
            dispute_initiated_at: escrow.dispute_initiated_at,
            funded: true,
            funding_deadline: None,
            service_agreement_hash: None,
        }
    }

    fn escrow_from_v4(escrow: EscrowV4) -> Escrow {
        Escrow {
            version: escrow.version,
            id: escrow.id,
            batch_id: escrow.batch_id,
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
            funded: escrow.funded,
            funding_deadline: escrow.funding_deadline,
            service_agreement_hash: None,
        }
    }

    #[inline(always)]
    fn try_calculate_fee(amount: i128, fee_bps: u32) -> Result<i128, Error> {
        if amount < 0 {
            return Err(Error::InvalidFee);
        }

        amount
            .checked_mul(fee_bps as i128)
            .and_then(|product| product.checked_div(10_000))
            .ok_or(Error::InvalidFee)
    }

    #[inline(always)]
    fn calculate_fee(env: &Env, amount: i128, fee_bps: u32) -> i128 {
        Self::try_calculate_fee(amount, fee_bps).unwrap_or_else(|err| env.panic_with_error(err))
    }

    fn compute_fee_allocation(
        env: &Env,
        escrow_amount: i128,
        fee_bps: u32,
        kind: SettlementKind,
    ) -> FeeAllocation {
        let allocation = match kind {
            SettlementKind::ReleaseFunds => {
                let platform_fee = Self::calculate_fee(env, escrow_amount, fee_bps);
                let seller_amount = escrow_amount - platform_fee;
                FeeAllocation {
                    platform_fee,
                    seller_amount,
                    buyer_amount: 0,
                }
            }

            SettlementKind::FullRefundNoFee => FeeAllocation {
                platform_fee: 0,
                seller_amount: 0,
                buyer_amount: escrow_amount,
            },

            SettlementKind::ExpiredDisputeDeductFromSeller => FeeAllocation {
                platform_fee: 0,
                seller_amount: 0,
                buyer_amount: escrow_amount,
            },

            SettlementKind::ExpiredDisputeDeductFromBuyer => {
                let platform_fee = Self::calculate_fee(env, escrow_amount, fee_bps);
                let buyer_amount = escrow_amount - platform_fee;
                FeeAllocation {
                    platform_fee,
                    seller_amount: 0,
                    buyer_amount,
                }
            }

            SettlementKind::ExpiredDisputeSplitFee => {
                let full_fee = Self::calculate_fee(env, escrow_amount, fee_bps);
                let platform_fee = full_fee / 2;
                let buyer_amount = escrow_amount - platform_fee;
                FeeAllocation {
                    platform_fee,
                    seller_amount: 0,
                    buyer_amount,
                }
            }

            SettlementKind::PartialRefund(refund_gross, seller_gross) => {
                if refund_gross < 0 || seller_gross < 0 {
                    env.panic_with_error(crate::Error::InvalidRefundAmount);
                }
                if refund_gross.checked_add(seller_gross) != Some(escrow_amount) {
                    env.panic_with_error(crate::Error::InvalidRefundAmount);
                }

                let platform_fee = Self::calculate_fee(env, seller_gross, fee_bps);
                FeeAllocation {
                    platform_fee,
                    seller_amount: seller_gross - platform_fee,
                    buyer_amount: refund_gross,
                }
            }
        };

        let sum = allocation
            .platform_fee
            .checked_add(allocation.seller_amount)
            .and_then(|s| s.checked_add(allocation.buyer_amount));
        if sum != Some(escrow_amount) {
            env.panic_with_error(crate::Error::InvalidFee);
        }

        allocation
    }

    fn proposal_key(order_id: u32) -> DataKey {
        DataKey::PartialRefundProposal(order_id)
    }

    fn settlement_receipt_key(order_id: u32) -> DataKey {
        DataKey::SettlementReceipt(order_id)
    }

    fn has_settlement_receipt(env: &Env, order_id: u32) -> bool {
        env.storage()
            .persistent()
            .has(&Self::settlement_receipt_key(order_id))
    }

    fn load_partial_refund_proposal(env: &Env, order_id: u32) -> Option<PartialRefundProposal> {
        env.storage()
            .persistent()
            .get(&Self::proposal_key(order_id))
    }

    fn is_privileged_resolver(config: &PlatformConfig, caller: &Address) -> bool {
        *caller == config.admin
            || *caller == config.arbitrator
            || Some(caller.clone()) == config.moderator
    }

    fn arbitrator_on_blacklist(env: &Env, caller: &Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::ArbitratorBlacklist(caller.clone()))
            .unwrap_or(false)
    }

    fn assert_privileged_settlement_caller(
        env: &Env,
        config: &PlatformConfig,
        caller: &Address,
    ) -> Result<(), Error> {
        if !Self::is_privileged_resolver(config, caller) {
            return Err(Error::Unauthorized);
        }
        if *caller != config.admin && Self::arbitrator_on_blacklist(env, caller) {
            return Err(Error::ArbitratorBlacklisted);
        }
        Ok(())
    }

    fn is_escrow_party(escrow: &Escrow, caller: &Address) -> bool {
        *caller == escrow.buyer || *caller == escrow.seller
    }

    fn assert_open_for_settlement(env: &Env, escrow: &Escrow, order_id: u32) -> Result<(), Error> {
        Self::assert_no_prior_settlement(env, order_id)?;
        Self::assert_disputed_for_policy(escrow)
    }

    fn dispute_clock(escrow: &Escrow) -> Result<u64, Error> {
        escrow.dispute_initiated_at.ok_or(Error::InvalidEscrowState)
    }

    fn assert_no_prior_settlement(env: &Env, order_id: u32) -> Result<(), Error> {
        if Self::has_settlement_receipt(env, order_id) {
            return Err(Error::SettlementAlreadyFinalized);
        }
        Ok(())
    }

    fn assert_disputed_for_policy(escrow: &Escrow) -> Result<(), Error> {
        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }
        Ok(())
    }

    fn validate_partial_refund_solvency(
        env: &Env,
        escrow: &Escrow,
        refund_gross: i128,
    ) -> Result<(i128, FeeAllocation), Error> {
        if refund_gross <= 0 || refund_gross > escrow.amount {
            return Err(Error::InvalidRefundAmount);
        }
        let seller_gross = escrow
            .amount
            .checked_sub(refund_gross)
            .ok_or(Error::InvalidRefundAmount)?;
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let allocation = Self::compute_fee_allocation(
            env,
            escrow.amount,
            fee_bps,
            SettlementKind::PartialRefund(refund_gross, seller_gross),
        );
        let sum = allocation
            .platform_fee
            .checked_add(allocation.seller_amount)
            .and_then(|s| s.checked_add(allocation.buyer_amount));
        if sum != Some(escrow.amount)
            || allocation.platform_fee < 0
            || allocation.seller_amount < 0
            || allocation.buyer_amount < 0
        {
            return Err(Error::InvalidRefundAmount);
        }
        Ok((seller_gross, allocation))
    }

    fn assert_arbitrator_resolution_window(
        env: &Env,
        escrow: &Escrow,
        config: &PlatformConfig,
    ) -> Result<(), Error> {
        let initiated_at = Self::dispute_clock(escrow)?;
        let now = env.ledger().timestamp();
        let challenge = config.evidence_challenge_window as u64;
        if time_policy::is_window_active(now, initiated_at, challenge) {
            return Err(Error::ChallengeWindowActive);
        }
        if time_policy::is_window_elapsed(now, initiated_at, config.max_dispute_duration as u64) {
            return Err(Error::ArbitratorDeadlineExceeded);
        }
        Ok(())
    }

    fn assert_expired_dispute_window(
        env: &Env,
        escrow: &Escrow,
        config: &PlatformConfig,
    ) -> Result<(), Error> {
        let initiated_at = Self::dispute_clock(escrow)?;
        let now = env.ledger().timestamp();
        if time_policy::is_window_active(now, initiated_at, config.max_dispute_duration as u64) {
            return Err(Error::DisputeExpired);
        }
        Ok(())
    }

    fn claim_disputed_settlement(env: &Env, order_id: u32) -> Result<Escrow, Error> {
        Self::assert_no_prior_settlement(env, order_id)?;
        let mut escrow = Self::get_stored_escrow(env, order_id);
        Self::assert_disputed_for_policy(&escrow)?;
        escrow.status = EscrowStatus::SettlementPending;
        let key = (ESCROW, order_id);
        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(env, &key);
        Ok(escrow)
    }

    fn write_settlement_receipt(
        env: &Env,
        order_id: u32,
        path: SettlementPath,
        proposal_nonce: u64,
    ) {
        let key = Self::settlement_receipt_key(order_id);
        env.storage().persistent().set(
            &key,
            &SettlementReceipt {
                order_id,
                path,
                executed_at: env.ledger().timestamp(),
                proposal_nonce,
            },
        );
        Self::extend_persistent(env, &key);
    }

    fn clear_partial_refund_proposal(env: &Env, order_id: u32) {
        env.storage()
            .persistent()
            .remove(&Self::proposal_key(order_id));
    }

    fn commit_resolved_escrow(
        env: &Env,
        order_id: u32,
        mut escrow: Escrow,
        path: SettlementPath,
        proposal_nonce: u64,
    ) -> Escrow {
        escrow.status = EscrowStatus::Resolved;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::write_settlement_receipt(env, order_id, path, proposal_nonce);
        Self::clear_partial_refund_proposal(env, order_id);
        Self::update_active_dispute_count(env, -1);
        Self::update_active_obligations(env, &escrow.buyer, -1);
        Self::update_active_obligations(env, &escrow.seller, -1);
        Self::safe_update_active_contracts(env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(env, escrow.seller.clone(), -1);
        Self::update_total_locked(env, &escrow.token, -escrow.amount);
        escrow
    }

    fn apply_fee_allocation_transfers(
        env: &Env,
        escrow: &Escrow,
        allocation: &FeeAllocation,
        platform_wallet: &Address,
        buyer_audit: &str,
        seller_audit: &str,
    ) {
        if allocation.buyer_amount > 0 {
            Self::transfer_tokens_and_record_audit(
                env,
                &escrow.token,
                &env.current_contract_address(),
                &escrow.buyer,
                allocation.buyer_amount,
                &escrow.buyer,
                Symbol::new(env, buyer_audit),
                allocation.buyer_amount,
            );
        }
        if allocation.platform_fee > 0 {
            Self::transfer_platform_fee(
                env,
                &escrow.token,
                platform_wallet,
                allocation.platform_fee,
            );
        }
        if allocation.seller_amount > 0 {
            Self::transfer_tokens_and_record_audit(
                env,
                &escrow.token,
                &env.current_contract_address(),
                &escrow.seller,
                allocation.seller_amount,
                &escrow.seller,
                Symbol::new(env, seller_audit),
                allocation.seller_amount,
            );
        }
    }

    fn add_fee_token_to_index(env: &Env, token: &Address) {
        let key = DataKey::FeeTokenIndex;
        let mut tracked_tokens: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));

        let mut already_tracked = false;
        for index in 0..tracked_tokens.len() {
            if tracked_tokens.get(index) == Some(token.clone()) {
                already_tracked = true;
                break;
            }
        }

        if !already_tracked {
            tracked_tokens.push_back(token.clone());
            env.storage().persistent().set(&key, &tracked_tokens);
        }
        Self::extend_persistent(env, &key);

        Self::ensure_fee_token_config(env, token);
    }

    fn ensure_fee_token_config(env: &Env, token: &Address) {
        let cfg_key = DataKey::FeeTokenConfig(token.clone());
        if !env.storage().persistent().has(&cfg_key) {
            let info = FeeTokenInfo {
                active: true,
                custom_fee_bps: None,
                accumulated: 0,
            };
            env.storage().persistent().set(&cfg_key, &info);
        }
        Self::extend_persistent(env, &cfg_key);
    }

    fn bump_fee_token_accumulator(env: &Env, token: &Address, amount: i128) {
        if amount <= 0 {
            return;
        }
        Self::ensure_fee_token_config(env, token);
        let cfg_key = DataKey::FeeTokenConfig(token.clone());
        let mut info: FeeTokenInfo =
            env.storage()
                .persistent()
                .get(&cfg_key)
                .unwrap_or(FeeTokenInfo {
                    active: true,
                    custom_fee_bps: None,
                    accumulated: 0,
                });
        info.accumulated = info.accumulated.saturating_add(amount);
        env.storage().persistent().set(&cfg_key, &info);
        Self::extend_persistent(env, &cfg_key);
    }

    pub fn get_fee_token_config(env: Env, token: Address) -> Option<FeeTokenInfo> {
        env.storage()
            .persistent()
            .get(&DataKey::FeeTokenConfig(token))
    }

    pub fn get_fee_tokens(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::FeeTokenIndex)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn set_fee_token_config(
        env: Env,
        token: Address,
        active: bool,
        custom_fee_bps: Option<u32>,
    ) -> Result<(), Error> {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if let Some(bps) = custom_fee_bps {
            if bps > MAX_PLATFORM_FEE_BPS {
                return Err(Error::InvalidFee);
            }
        }

        let cfg_key = DataKey::FeeTokenConfig(token.clone());
        let existing: FeeTokenInfo =
            env.storage()
                .persistent()
                .get(&cfg_key)
                .unwrap_or(FeeTokenInfo {
                    active: true,
                    custom_fee_bps: None,
                    accumulated: 0,
                });

        let info = FeeTokenInfo {
            active,
            custom_fee_bps,
            accumulated: existing.accumulated,
        };
        env.storage().persistent().set(&cfg_key, &info);
        Self::extend_persistent(&env, &cfg_key);
        Ok(())
    }

    pub fn migrate_fee_token_configs(env: Env) -> Result<u32, Error> {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let tokens: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::FeeTokenIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let scanned_tokens = tokens.len();
        let mut migrated: u32 = 0;
        let mut skipped_existing: u32 = 0;
        for index in 0..tokens.len() {
            if let Some(token) = tokens.get(index) {
                let cfg_key = DataKey::FeeTokenConfig(token.clone());
                if !env.storage().persistent().has(&cfg_key) {
                    let info = FeeTokenInfo {
                        active: true,
                        custom_fee_bps: None,
                        accumulated: env
                            .storage()
                            .persistent()
                            .get(&DataKey::TotalFees(token))
                            .unwrap_or(0i128),
                    };
                    env.storage().persistent().set(&cfg_key, &info);
                    Self::extend_persistent(&env, &cfg_key);
                    migrated += 1;
                } else {
                    Self::extend_persistent(&env, &cfg_key);
                    skipped_existing += 1;
                }
            }
        }

        env.events().publish(
            (Symbol::new(&env, "fee_cfg_migrated"),),
            FeeTokenConfigsMigratedEvent {
                scanned_tokens,
                migrated_configs: migrated,
                skipped_existing,
            },
        );

        Ok(migrated)
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
        Self::bump_fee_token_accumulator(env, token, fee_amount);
    }

    fn append_fund_audit_record(
        env: &Env,
        actor: &Address,
        amount: i128,
        reason: Symbol,
        balance_impact: i128,
    ) {
        if amount <= 0 {
            return;
        }

        let count_key = DataKey::FundAuditCount(actor.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        let entry_key = DataKey::FundAuditIndexed(actor.clone(), count);
        let entry = FundMovementAuditEntry {
            actor: actor.clone(),
            amount,
            reason,
            timestamp: env.ledger().timestamp(),
            balance_impact,
        };

        env.storage().persistent().set(&entry_key, &entry);
        Self::extend_persistent(env, &entry_key);
        env.storage().persistent().set(&count_key, &(count + 1));
        Self::extend_persistent(env, &count_key);
    }

    fn transfer_tokens_and_record_audit(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
        actor: &Address,
        reason: Symbol,
        balance_impact: i128,
    ) {
        if amount <= 0 {
            return;
        }

        if !env.storage().temporary().has(&DataKey::ReentryGuard) {
            env.panic_with_error(crate::Error::ReentryDetected);
        }

        Self::append_fund_audit_record(env, actor, amount, reason, balance_impact);
        let token_client = token::Client::new(env, token);
        token_client.transfer(from, to, &amount);
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

        Self::transfer_tokens_and_record_audit(
            env,
            token,
            &env.current_contract_address(),
            platform_wallet,
            fee_amount,
            platform_wallet,
            Symbol::new(env, "platform_fee"),
            fee_amount,
        );
        Self::record_total_fees(env, token, fee_amount);
    }

    #[inline(always)]
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

    pub fn release_funds(env: Env, order_id: u32) {
        let _guard = ReentryGuardScope::new(&env);
        let escrow_for_auth = Self::get_stored_escrow(&env, order_id);

        escrow_for_auth.buyer.require_auth();

        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::ReleasePending)
                .unwrap_or_else(|e| env.panic_with_error(e));

        let config = Self::get_platform_config_internal(&env);

        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let allocation = Self::compute_fee_allocation(
            &env,
            escrow.amount,
            fee_bps,
            SettlementKind::ReleaseFunds,
        );

        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        if allocation.platform_fee > 0 {
            Self::transfer_platform_fee(
                &env,
                &escrow.token,
                &config.platform_wallet,
                allocation.platform_fee,
            );
        }

        Self::transfer_tokens_and_record_audit(
            &env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.seller,
            allocation.seller_amount,
            &escrow.seller,
            Symbol::new(&env, "escrow_released"),
            allocation.seller_amount,
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Released,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.seller.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 1,
                metrics_amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.buyer.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 0,
                metrics_amount: 0,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
    }

    pub fn auto_release(env: Env, order_id: u32) {
        let _guard = ReentryGuardScope::new(&env);
        let escrow_for_window = Self::get_stored_escrow(&env, order_id);

        if !(escrow_for_window.status == EscrowStatus::Active) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let current_time = env.ledger().timestamp();
        if time_policy::is_window_active(current_time, escrow_for_window.created_at as u64, escrow_for_window.release_window as u64) {
            env.panic_with_error(crate::Error::ReleaseWindowNotElapsed);
        }

        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::ReleasePending)
                .unwrap_or_else(|e| env.panic_with_error(e));

        let config = Self::get_platform_config_internal(&env);

        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let allocation = Self::compute_fee_allocation(
            &env,
            escrow.amount,
            fee_bps,
            SettlementKind::ReleaseFunds,
        );

        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        if allocation.platform_fee > 0 {
            Self::transfer_platform_fee(
                &env,
                &escrow.token,
                &config.platform_wallet,
                allocation.platform_fee,
            );
        }

        Self::transfer_tokens_and_record_audit(
            &env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.seller,
            allocation.seller_amount,
            &escrow.seller,
            Symbol::new(&env, "escrow_released"),
            allocation.seller_amount,
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Released,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.seller.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 1,
                metrics_amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.buyer.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 0,
                metrics_amount: 0,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
    }

    pub fn extend_release_window(env: Env, order_id: u32, additional_seconds: u32) {
        let _guard = ReentryGuardScope::new(&env);
        let escrow_key = (ESCROW, order_id);
        let escrow_opt = env.storage().persistent().get(&escrow_key);

        if escrow_opt.is_none() {
            env.panic_with_error(crate::Error::EscrowNotFound);
        }

        Self::extend_persistent(&env, &escrow_key);
        let mut escrow: Escrow = escrow_opt.unwrap();

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

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Extended,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    fn validate_upgrade_hash(env: &Env, hash: &BytesN<32>) -> Result<(), Error> {
        let zero = BytesN::<32>::from_array(env, &[0u8; 32]);
        if hash == &zero {
            return Err(Error::InvalidUpgradeHash);
        }
        Ok(())
    }

    fn emit_upgrade_event(
        env: &Env,
        action: Symbol,
        wasm_hash: BytesN<32>,
        admin: Address,
        upgrade_at: u64,
    ) {
        env.events().publish(
            (Symbol::new(env, "wasm_upgrade"), action.clone()),
            UpgradeProposalEvent {
                action,
                wasm_hash,
                admin,
                timestamp: env.ledger().timestamp(),
                upgrade_at,
            },
        );
    }

    pub fn propose_upgrade_wasm(
        env: Env,
        signer: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), Error> {
        signer.require_auth();

        Self::validate_upgrade_hash(&env, &new_wasm_hash)?;

        if let Some(cancelled_at) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::LastUpgradeCancelledAt)
        {
            let now = env.ledger().timestamp();
            if time_policy::is_window_active(now, cancelled_at, CANCEL_REPROPOSE_COOLDOWN) {
                return Err(Error::UpgradeCooldownActive);
            }
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::WasmUpgradeProposal)
        {
            return Err(Error::UpgradeProposalExists);
        }

        let state_key = DataKey::UpgradeApprovalState(0);

        let current_nonce: u32 = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&state_key)
            .map(|s| s.nonce)
            .unwrap_or(0u32);

        let fresh_state = |nonce: u32| -> UpgradeApprovalState {
            let snapshotted_signers: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::UpgradeSigners)
                .unwrap_or_else(|| {
                    let mut v = Vec::new(&env);
                    if let Ok(admin) = Self::get_admin(&env) {
                        v.push_back(admin);
                    }
                    v
                });
            let snapshotted_threshold: u32 = env
                .storage()
                .instance()
                .get(&DataKey::UpgradeThreshold)
                .unwrap_or(1u32);
            UpgradeApprovalState {
                nonce,
                signers: snapshotted_signers,
                threshold: snapshotted_threshold,
                approvals: Vec::new(&env),
            }
        };

        let mut state: UpgradeApprovalState = env
            .storage()
            .persistent()
            .get(&state_key)
            .filter(|s: &UpgradeApprovalState| s.nonce == current_nonce && !s.signers.is_empty())
            .unwrap_or_else(|| fresh_state(current_nonce));

        if !state.signers.iter().any(|s| s == signer) {
            return Err(Error::NotAnUpgradeSigner);
        }

        if state.approvals.iter().any(|a| a == signer) {
            return Err(Error::AlreadyApproved);
        }
        state.approvals.push_back(signer.clone());

        if state.approvals.len() < state.threshold {
            env.storage().persistent().set(&state_key, &state);
            Self::extend_persistent(&env, &state_key);
            return Ok(());
        }

        env.storage().persistent().remove(&state_key);

        let config = Self::get_platform_config_internal(&env);
        let proposed_at = env.ledger().timestamp();
        let upgrade_at = proposed_at + config.wasm_upgrade_cooldown as u64;
        let proposal = WasmUpgradeProposal {
            wasm_hash: new_wasm_hash.clone(),
            upgrade_at,
            proposed_by: signer.clone(),
            proposed_at,
        };

        env.storage()
            .persistent()
            .set(&DataKey::WasmUpgradeProposal, &proposal);
        Self::extend_persistent(&env, &DataKey::WasmUpgradeProposal);

        Self::emit_upgrade_event(&env, UPGRADE_PROPOSED, new_wasm_hash, signer, upgrade_at);

        Ok(())
    }

    pub fn set_upgrade_threshold(env: Env, threshold: u32) -> Result<(), Error> {
        if threshold == 0 {
            return Err(Error::InvalidFee);
        }
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::UpgradeThreshold, &threshold);
        Ok(())
    }

    pub fn set_upgrade_signers(env: Env, signers: Vec<Address>) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        if signers.is_empty() {
            env.storage().persistent().remove(&DataKey::UpgradeSigners);
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::UpgradeSigners, &signers);
            Self::extend_persistent(&env, &DataKey::UpgradeSigners);
        }
        Ok(())
    }

    pub fn get_upgrade_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeThreshold)
            .unwrap_or(1u32)
    }

    pub fn get_upgrade_proposal_nonce(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&DataKey::UpgradeApprovalState(0))
            .map(|s| s.nonce)
            .unwrap_or(0u32)
    }

    pub fn get_upgrade_approvals(env: Env, nonce: u32) -> Vec<Address> {
        env.storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&DataKey::UpgradeApprovalState(0))
            .filter(|s| s.nonce == nonce)
            .map(|s| s.approvals)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_upgrade_state_snapshot(env: Env) -> UpgradeStateSnapshot {
        let config = Self::get_platform_config_internal(&env);
        UpgradeStateSnapshot {
            contract_version: Self::get_version(env.clone()),
            escrow_count: env
                .storage()
                .persistent()
                .get(&DataKey::EscrowCount)
                .unwrap_or(0),
            recurring_escrow_next_id: env
                .storage()
                .persistent()
                .get(&DataKey::NextRecurringEscrowId)
                .unwrap_or(0),
            upgrade_threshold: Self::get_upgrade_threshold(env.clone()),
            paused: config.is_paused,
            onboarding_configured: env
                .storage()
                .persistent()
                .has(&DataKey::OnboardingContractAddress),
        }
    }

    pub fn get_upgrade_state_commitment(env: Env) -> BytesN<32> {
        let snapshot = Self::get_upgrade_state_snapshot(env.clone());
        let snapshot_bytes = snapshot.to_xdr(&env);
        env.crypto().sha256(&snapshot_bytes).into()
    }

    fn is_zero_commitment(env: &Env, commitment: &BytesN<32>) -> bool {
        commitment == &BytesN::<32>::from_array(env, &[0u8; 32])
    }

    fn validate_compatibility_manifest(
        env: &Env,
        manifest: &UpgradeCompatibilityManifest,
    ) -> Result<(), Error> {
        let current_version = Self::get_version(env.clone());
        if manifest.source_version != current_version
            || manifest.target_version != current_version.saturating_add(1)
            || Self::is_zero_commitment(env, &manifest.state_commitment)
            || Self::is_zero_commitment(env, &manifest.interface_commitment)
            || Self::is_zero_commitment(env, &manifest.authorization_commitment)
            || Self::is_zero_commitment(env, &manifest.preconditions_commitment)
            || Self::is_zero_commitment(env, &manifest.postconditions_commitment)
            || Self::is_zero_commitment(env, &manifest.rollback_commitment)
            || Self::is_zero_commitment(env, &manifest.migration_checkpoint)
        {
            return Err(Error::UpgradeCompatibilityInvalid);
        }

        if !manifest.migration_complete || manifest.manual_records != 0 {
            return Err(Error::UpgradeMigrationIncomplete);
        }

        if manifest.state_commitment != Self::get_upgrade_state_commitment(env.clone()) {
            return Err(Error::UpgradeCompatibilityInvalid);
        }
        Ok(())
    }

    pub fn submit_compat_manifest(
        env: Env,
        wasm_hash: BytesN<32>,
        manifest: UpgradeCompatibilityManifest,
    ) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        Self::validate_upgrade_hash(&env, &wasm_hash)?;
        if manifest.source_version != Self::get_version(env.clone()) {
            return Err(Error::UpgradeCompatibilityInvalid);
        }
        env.storage().persistent().set(
            &DataKey::UpgradeCompatibilityManifest(wasm_hash.clone()),
            &manifest,
        );
        Self::extend_persistent(
            &env,
            &DataKey::UpgradeCompatibilityManifest(wasm_hash),
        );
        Ok(())
    }

    pub fn get_upgrade_compat_manifest(
        env: Env,
        wasm_hash: BytesN<32>,
    ) -> Option<UpgradeCompatibilityManifest> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeCompatibilityManifest(wasm_hash))
    }

    pub fn get_storage_layout_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::StorageLayoutVersion)
            .unwrap_or(0)
    }

    pub fn migrate_storage_layout(env: Env) -> u32 {
        let admin = Self::get_admin(&env)
            .unwrap_or_else(|_| env.panic_with_error(crate::Error::PlatformNotInitialized));
        admin.require_auth();

        let current_version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::StorageLayoutVersion)
            .unwrap_or(0);
        if current_version == CURRENT_STORAGE_LAYOUT_VERSION {
            return 0;
        }

        Self::migrate_legacy_all_escrow_ids(&env);
        Self::migrate_legacy_whitelisted_tokens(&env);

        env.storage().persistent().set(
            &DataKey::StorageLayoutVersion,
            &CURRENT_STORAGE_LAYOUT_VERSION,
        );
        Self::extend_persistent(&env, &DataKey::StorageLayoutVersion);

        1
    }

    fn ensure_storage_layout_compatible(env: &Env) -> Result<(), Error> {
        let stored_version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::StorageLayoutVersion)
            .unwrap_or(0);
        if stored_version != CURRENT_STORAGE_LAYOUT_VERSION {
            return Err(Error::StorageLayoutMismatch);
        }
        Ok(())
    }

    pub fn execute_upgrade(env: Env, expected_wasm_hash: BytesN<32>) -> Result<(), Error> {
        Self::ensure_storage_layout_compatible(&env)?;

        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let proposal: WasmUpgradeProposal = env
            .storage()
            .persistent()
            .get(&DataKey::WasmUpgradeProposal)
            .ok_or(Error::NoUpgradeProposed)?;

        if proposal.wasm_hash != expected_wasm_hash {
            return Err(Error::InvalidUpgradeHash);
        }

        if env.ledger().timestamp() < proposal.upgrade_at {
            return Err(Error::UpgradeCooldownActive);
        }

        let manifest: UpgradeCompatibilityManifest = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeCompatibilityManifest(proposal.wasm_hash.clone()))
            .ok_or(Error::UpgradeCompatibilityMissing)?;
        Self::validate_compatibility_manifest(&env, &manifest)?;

        env.deployer()
            .update_current_contract_wasm(proposal.wasm_hash.clone());

        let current_version: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0);
        let new_version = current_version + 1;

        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &new_version);
        Self::extend_persistent(&env, &DataKey::ContractVersion);

        Self::append_upgrade_history(
            &env,
            UpgradeRecord {
                from_version: current_version,
                to_version: new_version,
                wasm_hash: proposal.wasm_hash.clone(),
                admin: admin.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::append_upgrade_compatibility_history(
            &env,
            UpgradeCompatibilityRecord {
                from_version: current_version,
                to_version: new_version,
                wasm_hash: proposal.wasm_hash.clone(),
                state_commitment: manifest.state_commitment,
                migration_checkpoint: manifest.migration_checkpoint,
                timestamp: env.ledger().timestamp(),
            },
        );

        env.storage()
            .persistent()
            .remove(&DataKey::WasmUpgradeProposal);
        env.storage().persistent().remove(&DataKey::UpgradeCompatibilityManifest(
            proposal.wasm_hash.clone(),
        ));

        Self::emit_upgrade_event(
            &env,
            UPGRADE_EXECUTED,
            proposal.wasm_hash,
            admin,
            proposal.upgrade_at,
        );

        Ok(())
    }

    pub fn cancel_upgrade_wasm(env: Env) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let proposal: WasmUpgradeProposal = env
            .storage()
            .persistent()
            .get(&DataKey::WasmUpgradeProposal)
            .ok_or(Error::NoUpgradeProposed)?;

        env.storage()
            .persistent()
            .remove(&DataKey::WasmUpgradeProposal);

        let state_key = DataKey::UpgradeApprovalState(0);
        let next_nonce: u32 = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&state_key)
            .map(|s| s.nonce.saturating_add(1))
            .unwrap_or(1u32);
        let reset_state = UpgradeApprovalState {
            nonce: next_nonce,
            signers: Vec::new(&env),
            threshold: 1u32,
            approvals: Vec::new(&env),
        };
        env.storage().persistent().set(&state_key, &reset_state);
        Self::extend_persistent(&env, &state_key);

        let cancelled_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&DataKey::LastUpgradeCancelledAt, &cancelled_at);
        Self::extend_persistent(&env, &DataKey::LastUpgradeCancelledAt);

        Self::emit_upgrade_event(
            &env,
            UPGRADE_CANCELLED,
            proposal.wasm_hash,
            admin,
            proposal.upgrade_at,
        );

        Ok(())
    }

    pub fn get_upgrade_proposal(env: Env) -> Option<WasmUpgradeProposal> {
        env.storage()
            .persistent()
            .get(&DataKey::WasmUpgradeProposal)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0)
    }

    fn append_upgrade_history(env: &Env, record: UpgradeRecord) {
        let mut history: Vec<UpgradeRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeHistory)
            .unwrap_or_else(|| Vec::new(env));

        history.push_back(record);
        while history.len() > MAX_UPGRADE_HISTORY {
            history.pop_front();
        }

        env.storage()
            .persistent()
            .set(&DataKey::UpgradeHistory, &history);
        Self::extend_persistent(env, &DataKey::UpgradeHistory);
    }

    fn append_upgrade_compatibility_history(env: &Env, record: UpgradeCompatibilityRecord) {
        let mut history: Vec<UpgradeCompatibilityRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::UpgradeCompatibilityHistory)
            .unwrap_or_else(|| Vec::new(env));
        history.push_back(record);
        while history.len() > MAX_UPGRADE_HISTORY {
            history.pop_front();
        }
        env.storage()
            .persistent()
            .set(&DataKey::UpgradeCompatibilityHistory, &history);
        Self::extend_persistent(env, &DataKey::UpgradeCompatibilityHistory);
    }

    pub fn get_upgrade_history(env: Env) -> Vec<UpgradeRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_upgrade_compat_history(env: Env) -> Vec<UpgradeCompatibilityRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeCompatibilityHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_version_info(env: Env) -> VersionInfo {
        let current_version = Self::get_version(env.clone());
        let history = Self::get_upgrade_history(env);
        let upgrade_count = history.len();
        VersionInfo {
            current_version,
            upgrade_count,
        }
    }

    pub fn refund(env: Env, escrow_id: u64) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let order_id = escrow_id as u32;
        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::RefundPending)?;

        let allocation =
            Self::compute_fee_allocation(&env, escrow.amount, 0, SettlementKind::FullRefundNoFee);

        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        Self::transfer_tokens_and_record_audit(
            &env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.buyer,
            allocation.buyer_amount,
            &escrow.buyer,
            Symbol::new(&env, "refund"),
            allocation.buyer_amount,
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id,
                action: EscrowAction::Refunded,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.buyer.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 0,
                metrics_amount: 0,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.seller.clone(),
                successful_delta: 0,
                disputed_delta: 1,
                metrics_sales_delta: 0,
                metrics_amount: 0,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
        Ok(())
    }

    pub fn get_escrow(env: Env, order_id: u32) -> Escrow {
        Self::get_stored_escrow(&env, order_id)
    }

    pub fn diagnose_escrow_state(env: Env, order_id: u32) -> EscrowStateDiagnostic {
        Self::inspect_escrow_state(&env, order_id)
    }

    pub fn get_fund_audit_history(env: Env, actor: Address) -> Vec<FundMovementAuditEntry> {
        let count_key = DataKey::FundAuditCount(actor.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        let mut history = Vec::new(&env);

        for index in 0..count {
            let entry_key = DataKey::FundAuditIndexed(actor.clone(), index);
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, FundMovementAuditEntry>(&entry_key)
            {
                history.push_back(entry);
            }
        }

        history
    }

    pub fn get_fund_audit_count(env: Env, actor: Address) -> u32 {
        let count_key = DataKey::FundAuditCount(actor);
        env.storage().persistent().get(&count_key).unwrap_or(0)
    }

    pub fn get_fund_audit_history_paginated(
        env: Env,
        actor: Address,
        start_index: u32,
        limit: u32,
    ) -> Result<Vec<FundMovementAuditEntry>, Error> {
        let limit = pagination_validation::validate_limit(
            limit,
            pagination_validation::MAX_ADMIN_PAGE_SIZE,
        )?;
        let count_key = DataKey::FundAuditCount(actor.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if start_index >= count {
            return Ok(Vec::new(&env));
        }

        let mut history = Vec::new(&env);
        let end_index = start_index.saturating_add(limit).min(count);
        for index in start_index..end_index {
            let entry_key = DataKey::FundAuditIndexed(actor.clone(), index);
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<DataKey, FundMovementAuditEntry>(&entry_key)
            {
                history.push_back(entry);
            }
        }

        Ok(history)
    }

    pub fn get_escrow_metadata(env: Env, order_id: u32) -> EscrowMetadata {
        let escrow = Self::get_escrow(env, order_id);
        EscrowMetadata {
            ipfs_hash: escrow.ipfs_hash,
            metadata_hash: escrow.metadata_hash,
            service_agreement_hash: escrow.service_agreement_hash,
        }
    }

    pub fn verify_metadata_reveal(
        env: Env,
        order_id: u32,
        proof: MetadataRevealProof,
        authorized_address: Address,
    ) -> bool {
        let escrow = Self::get_escrow(env.clone(), order_id);
        let config = Self::get_platform_config_internal(&env);

        let is_authorized = authorized_address == escrow.buyer
            || authorized_address == escrow.seller
            || authorized_address == config.arbitrator;
        if !is_authorized {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        if escrow.metadata_hash.is_none() {
            return false;
        }

        let stored_hash = escrow.metadata_hash.unwrap();

        let computed_hash = env.crypto().sha256(&proof.content);

        let computed_bytes: Bytes = computed_hash.into();

        computed_bytes == stored_hash
    }

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

        let is_valid =
            Self::verify_metadata_reveal(env.clone(), order_id, proof, authorized_address.clone());
        if is_valid {
            Self::emit_metadata_verified(&env, order_id, authorized_address);
        }
        is_valid
    }

    pub fn can_auto_release(env: Env, order_id: u32) -> bool {
        let escrow = Self::try_get_escrow_readonly(&env, order_id);

        if escrow.status != EscrowStatus::Active {
            return false;
        }

        let current_time = env.ledger().timestamp();
        let elapsed = current_time - (escrow.created_at as u64);

        elapsed >= escrow.release_window as u64
    }

    pub fn dispute_escrow(
        env: Env,
        order_id: u32,
        dispute_reason: Symbol, 
        authorized_address: Address,
    ) {
        authorized_address.require_auth();

        let rate_config: RateLimitConfig = env
            .storage()
            .persistent()
            .get(&DataKey::RateLimitConfig)
            .unwrap_or(RateLimitConfig {
                max_calls: DEFAULT_RATE_LIMIT_MAX_CALLS,
                window: DEFAULT_RATE_LIMIT_WINDOW,
            });

        if rate_config.max_calls > 0 && rate_config.window > 0 {
            let current_time = env.ledger().timestamp();
            let window_index = current_time / (rate_config.window as u64);
            let rate_key = DataKey::RateLimitCount(authorized_address.clone(), window_index);
            let count: u32 = env.storage().persistent().get(&rate_key).unwrap_or(0);
            if count >= rate_config.max_calls {
                env.panic_with_error(crate::Error::BatchLimitExceeded);
            }
            env.storage().persistent().set(&rate_key, &(count + 1));
        }

        let escrow_for_auth = Self::get_stored_escrow(&env, order_id);

        if !(escrow_for_auth.buyer == authorized_address
            || escrow_for_auth.seller == authorized_address)
        {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::DisputePending)
                .unwrap_or_else(|e| env.panic_with_error(e));

        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_reason = Some(dispute_reason); 
        escrow.dispute_initiated_at = Some(env.ledger().timestamp());
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::update_active_dispute_count(&env, 1);

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
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

    pub fn resolve_dispute(
        env: Env,
        order_id: u32,
        resolution: Resolution,
        authorized_address: Address,
    ) {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        authorized_address.require_auth();
        Self::assert_privileged_settlement_caller(&env, &config, &authorized_address)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let snapshot = Self::get_stored_escrow(&env, order_id);
        Self::assert_open_for_settlement(&env, &snapshot, order_id)
            .unwrap_or_else(|e| env.panic_with_error(e));
        Self::assert_arbitrator_resolution_window(&env, &snapshot, &config)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let (kind, path) = match resolution {
            Resolution::ReleaseToSeller => (
                SettlementKind::ReleaseFunds,
                SettlementPath::ArbitratedRelease,
            ),
            Resolution::RefundToBuyer => (
                SettlementKind::FullRefundNoFee,
                SettlementPath::ArbitratedRefund,
            ),
        };
        let fee_bps = match resolution {
            Resolution::ReleaseToSeller => {
                Self::get_effective_fee_bps(env.clone(), snapshot.seller.clone())
            }
            Resolution::RefundToBuyer => 0,
        };
        let allocation = Self::compute_fee_allocation(&env, snapshot.amount, fee_bps, kind);

        let escrow = Self::claim_disputed_settlement(&env, order_id)
            .unwrap_or_else(|e| env.panic_with_error(e));
        let escrow = Self::commit_resolved_escrow(&env, order_id, escrow, path, 0);

        Self::apply_fee_allocation_transfers(
            &env,
            &escrow,
            &allocation,
            &config.platform_wallet,
            "refund",
            "escrow_released",
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Resolved,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::emit_escrow_resolved_event(
            &env,
            EscrowResolvedEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                arbitrator: authorized_address.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        let ts = env.ledger().timestamp();
        match resolution {
            Resolution::ReleaseToSeller => {
                Self::emit_reputation_update(
                    &env,
                    ReputationUpdateEvent {
                        address: escrow.seller.clone(),
                        successful_delta: 1,
                        disputed_delta: 0,
                        metrics_sales_delta: 1,
                        metrics_amount: escrow.amount,
                        token: escrow.token.clone(),
                        timestamp: ts,
                    },
                );
                Self::emit_reputation_update(
                    &env,
                    ReputationUpdateEvent {
                        address: escrow.buyer.clone(),
                        successful_delta: 0,
                        disputed_delta: 1,
                        metrics_sales_delta: 0,
                        metrics_amount: 0,
                        token: escrow.token.clone(),
                        timestamp: ts,
                    },
                );
            }
            Resolution::RefundToBuyer => {
                Self::emit_reputation_update(
                    &env,
                    ReputationUpdateEvent {
                        address: escrow.buyer.clone(),
                        successful_delta: 1,
                        disputed_delta: 0,
                        metrics_sales_delta: 0,
                        metrics_amount: 0,
                        token: escrow.token.clone(),
                        timestamp: ts,
                    },
                );
                Self::emit_reputation_update(
                    &env,
                    ReputationUpdateEvent {
                        address: escrow.seller.clone(),
                        successful_delta: 0,
                        disputed_delta: 1,
                        metrics_sales_delta: 0,
                        metrics_amount: 0,
                        token: escrow.token.clone(),
                        timestamp: ts,
                    },
                );
            }
        }
    }

    pub fn submit_evidence(
        env: Env,
        order_id: u32,
        submitter: Address,
        evidence_uri: String,
    ) -> u64 {
        submitter.require_auth();

        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.status != EscrowStatus::Disputed {
            env.panic_with_error(crate::Error::NotInDispute);
        }

        if !(submitter == escrow.buyer || submitter == escrow.seller) {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let dispute_session_id = escrow
            .dispute_initiated_at
            .unwrap_or(escrow.created_at as u64);

        let len = (evidence_uri.len() as usize).min(256);
        let mut buf = [0u8; 256];
        evidence_uri.copy_into_slice(&mut buf[0..len]);
        let bytes = Bytes::from_slice(&env, &buf[0..len]);
        let hash: BytesN<32> = env.crypto().sha256(&bytes).into();
        let hash_key = DataKey::UsedEvidenceHash(hash);
        if env.storage().persistent().has(&hash_key) {
            env.panic_with_error(crate::Error::EvidenceAlreadyUsed);
        }
        env.storage().persistent().set(&hash_key, &true);

        let key = DataKey::EvidenceLog(order_id);
        let mut log: Vec<DisputeEvidence> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        let id = log.len() as u64;
        let submitted_at = env.ledger().timestamp();
        let expires_at = submitted_at + DEFAULT_EVIDENCE_EXPIRY_WINDOW;

        let evidence = DisputeEvidence {
            id,
            order_id,
            dispute_session_id,
            submitter,
            evidence_uri,
            parent_evidence_id: None,
            submitted_at,
            expires_at,
            is_invalidated: false,
        };

        log.push_back(evidence);
        env.storage().persistent().set(&key, &log);
        id
    }

    pub fn submit_counter_evidence(
        env: Env,
        order_id: u32,
        submitter: Address,
        evidence_uri: String,
        parent_evidence_id: u64,
    ) -> u64 {
        submitter.require_auth();

        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.status != EscrowStatus::Disputed {
            env.panic_with_error(crate::Error::NotInDispute);
        }

        if !(submitter == escrow.buyer || submitter == escrow.seller) {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let dispute_session_id = escrow
            .dispute_initiated_at
            .unwrap_or(escrow.created_at as u64);

        let key = DataKey::EvidenceLog(order_id);
        let mut log: Vec<DisputeEvidence> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        let mut parent_found = false;
        for item in log.iter() {
            if item.id == parent_evidence_id && item.dispute_session_id == dispute_session_id {
                parent_found = true;
                break;
            }
        }
        if !parent_found {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let len = (evidence_uri.len() as usize).min(256);
        let mut buf = [0u8; 256];
        evidence_uri.copy_into_slice(&mut buf[0..len]);
        let bytes = Bytes::from_slice(&env, &buf[0..len]);
        let hash: BytesN<32> = env.crypto().sha256(&bytes).into();
        let hash_key = DataKey::UsedEvidenceHash(hash);
        if env.storage().persistent().has(&hash_key) {
            env.panic_with_error(crate::Error::EvidenceAlreadyUsed);
        }
        env.storage().persistent().set(&hash_key, &true);

        let id = log.len() as u64;
        let submitted_at = env.ledger().timestamp();
        let expires_at = submitted_at + DEFAULT_EVIDENCE_EXPIRY_WINDOW;

        let evidence = DisputeEvidence {
            id,
            order_id,
            dispute_session_id,
            submitter,
            evidence_uri,
            parent_evidence_id: Some(parent_evidence_id),
            submitted_at,
            expires_at,
            is_invalidated: false,
        };

        log.push_back(evidence);
        env.storage().persistent().set(&key, &log);
        id
    }

    pub fn get_evidence(env: Env, order_id: u32) -> Vec<DisputeEvidence> {
        let key = DataKey::EvidenceLog(order_id);
        let log: Vec<DisputeEvidence> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        let current_time = env.ledger().timestamp();
        let mut updated_log = Vec::new(&env);
        let mut modified = false;

        for mut item in log.into_iter() {
            if !item.is_invalidated && time_policy::is_deadline_reached(current_time, item.expires_at) {
                item.is_invalidated = true;
                modified = true;
            }
            updated_log.push_back(item);
        }

        if modified {
            env.storage().persistent().set(&key, &updated_log);
        }

        updated_log
    }

    pub fn get_valid_evidence(env: Env, order_id: u32) -> Vec<DisputeEvidence> {
        let all_evidence = Self::get_evidence(env.clone(), order_id);
        let mut valid_log = Vec::new(&env);
        let current_time = env.ledger().timestamp();

        for item in all_evidence.into_iter() {
            if !item.is_invalidated && time_policy::is_deadline_pending(current_time, item.expires_at) {
                valid_log.push_back(item);
            }
        }
        valid_log
    }

    pub fn escalate_dispute(env: Env, order_id: u32, caller: Address) {
        caller.require_auth();

        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.status != EscrowStatus::Disputed {
            env.panic_with_error(crate::Error::NotInDispute);
        }

        if !(caller == escrow.buyer || caller == escrow.seller) {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let escalation_key = DataKey::DisputeEscalation(order_id);
        if env.storage().persistent().has(&escalation_key) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let config = Self::get_platform_config_internal(&env);
        let dispute_initiated_at = escrow
            .dispute_initiated_at
            .unwrap_or(escrow.created_at as u64);
        let current_time = env.ledger().timestamp();

        if time_policy::is_window_active(current_time, dispute_initiated_at, config.dispute_escalation_window as u64) {
            env.panic_with_error(crate::Error::ReleaseWindowNotElapsed);
        }

        let record = DisputeEscalationRecord {
            order_id,
            escalated_by: caller,
            escalated_at: current_time,
        };

        env.storage().persistent().set(&escalation_key, &record);

        Self::emit_dispute_escalated(&env, order_id);
    }

    pub fn get_dispute_escalation(env: Env, order_id: u32) -> Option<DisputeEscalationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeEscalation(order_id))
    }

    pub fn set_dispute_escalation_window(env: Env, window: u32) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        config.dispute_escalation_window = window;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
    }

    pub fn set_evidence_challenge_window(env: Env, window: u32) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        config.evidence_challenge_window = window;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
    }

    pub fn set_rate_limit_config(env: Env, max_calls: u32, window: u32) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        let rate_config = RateLimitConfig { max_calls, window };
        env.storage()
            .persistent()
            .set(&DataKey::RateLimitConfig, &rate_config);
    }

    fn emit_dispute_escalated(env: &Env, order_id: u32) {
        env.events()
            .publish((Symbol::new(env, "dispute_escalated"), order_id as u64), ());
    }

    pub fn resolve_dispute_partial(
        env: Env,
        order_id: u32,
        buyer_amount: i128,
        authorized_address: Address,
    ) {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        authorized_address.require_auth();
        Self::assert_privileged_settlement_caller(&env, &config, &authorized_address)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let snapshot = Self::get_stored_escrow(&env, order_id);
        Self::assert_open_for_settlement(&env, &snapshot, order_id)
            .unwrap_or_else(|e| env.panic_with_error(e));
        Self::assert_arbitrator_resolution_window(&env, &snapshot, &config)
            .unwrap_or_else(|e| env.panic_with_error(e));

        let (_seller_gross, allocation) =
            Self::validate_partial_refund_solvency(&env, &snapshot, buyer_amount)
                .unwrap_or_else(|e| env.panic_with_error(e));
        if buyer_amount >= snapshot.amount {
            env.panic_with_error(crate::Error::InvalidRefundAmount);
        }

        let escrow = Self::claim_disputed_settlement(&env, order_id)
            .unwrap_or_else(|e| env.panic_with_error(e));
        let escrow = Self::commit_resolved_escrow(
            &env,
            order_id,
            escrow,
            SettlementPath::ArbitratedPartial,
            0,
        );

        Self::apply_fee_allocation_transfers(
            &env,
            &escrow,
            &allocation,
            &config.platform_wallet,
            "partial_refund_buyer",
            "partial_refund_seller",
        );

        Self::emit_escrow_created(
            &env,
            EscrowEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                action: EscrowAction::Resolved,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Self::emit_escrow_resolved_event(
            &env,
            EscrowResolvedEvent {
                schema_version: 1,
                escrow_id: order_id as u64,
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                arbitrator: authorized_address.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.seller.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 1,
                metrics_amount: buyer_amount,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.buyer.clone(),
                successful_delta: 1,
                disputed_delta: 0,
                metrics_sales_delta: 0,
                metrics_amount: 0,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
    }

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
            dispute_escalation_window: config.dispute_escalation_window,
            evidence_challenge_window: config.evidence_challenge_window,
        };

        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &new_config);
        Self::emit_config_updated(
            &env,
            "platform_fee_bps",
            ConfigValue::U32(config.platform_fee_bps),
            ConfigValue::U32(new_fee_bps),
        );
    }

    pub fn update_platform_wallet(env: Env, new_wallet: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if let Err(e) = Self::validate_platform_wallet(&env, &new_wallet) {
            env.panic_with_error(e);
        }

        let new_config = PlatformConfig {
            platform_fee_bps: config.platform_fee_bps,
            platform_wallet: new_wallet.clone(),
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
            dispute_escalation_window: config.dispute_escalation_window,
            evidence_challenge_window: config.evidence_challenge_window,
        };

        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &new_config);
        Self::emit_config_updated(
            &env,
            "platform_wallet",
            ConfigValue::Address(config.platform_wallet),
            ConfigValue::Address(new_wallet),
        );
    }

    pub fn update_expired_dispute_policy(
        env: Env,
        policy: ExpiredDisputeFeePolicy,
    ) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let old_policy = config.expired_dispute_fee_policy;
        config.expired_dispute_fee_policy = policy;

        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        Self::emit_config_updated(
            &env,
            "expired_dispute_fee_policy",
            ConfigValue::U32(old_policy as u32),
            ConfigValue::U32(policy as u32),
        );

        Ok(())
    }

    pub fn get_expired_dispute_policy(env: Env) -> ExpiredDisputeFeePolicy {
        let config = Self::get_platform_config_internal(&env);
        config.expired_dispute_fee_policy
    }

    pub fn get_moderator(env: Env) -> Option<Address> {
        Self::get_platform_config_internal(&env).moderator
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
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        Self::emit_config_updated(&env, "moderator", previous, ConfigValue::Address(moderator));
    }

    pub fn blacklist_arbitrator(env: Env, arbitrator: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let key = DataKey::ArbitratorBlacklist(arbitrator.clone());
        env.storage().persistent().set(&key, &true);
        Self::extend_persistent(&env, &key);

        Self::emit_config_updated(
            &env,
            "arbitrator_blacklisted",
            ConfigValue::String(String::from_str(&env, "false")),
            ConfigValue::Address(arbitrator),
        );
    }

    pub fn remove_arbitrator_from_blacklist(env: Env, arbitrator: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let key = DataKey::ArbitratorBlacklist(arbitrator.clone());
        env.storage().persistent().remove(&key);

        Self::emit_config_updated(
            &env,
            "arbitrator_unblacklisted",
            ConfigValue::Address(arbitrator),
            ConfigValue::String(String::from_str(&env, "false")),
        );
    }

    pub fn is_arbitrator_blacklisted(env: Env, arbitrator: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::ArbitratorBlacklist(arbitrator))
            .unwrap_or(false)
    }

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

    pub fn get_platform_fee(env: Env) -> u32 {
        let config = Self::get_platform_config_internal(&env);
        config.platform_fee_bps
    }

    pub fn get_platform_wallet(env: Env) -> Address {
        let config = Self::get_platform_config_internal(&env);
        config.platform_wallet
    }

    pub fn get_total_fees_collected(env: Env) -> i128 {
        Self::get_all_tracked_total_fees(&env)
    }

    pub fn get_total_fees_for_token(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalFees(token))
            .unwrap_or(0)
    }

    pub fn calculate_fee_for_amount(env: Env, amount: i128) -> i128 {
        let config = Self::get_platform_config_internal(&env);
        Self::calculate_fee(&env, amount, config.platform_fee_bps)
    }

    pub fn calculate_seller_net_amount(env: Env, amount: i128) -> i128 {
        let fee = Self::calculate_fee_for_amount(env, amount);
        amount - fee
    }

    pub fn get_fee_policy_version(_env: Env) -> u32 {
        FEE_POLICY_VERSION
    }

    fn validate_escrow_params(env: &Env, params: &EscrowCreateParams) -> Result<(), Error> {
        if params.amount <= 0 {
            return Err(Error::AmountBelowMinimum);
        }

        Self::check_min_amount(env, params.token.clone(), params.amount)?;

        if params.buyer == params.seller {
            return Err(Error::SameBuyerSeller);
        }

        let whitelist: Map<Address, bool> = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or(Map::new(env));
        if !whitelist.is_empty() && !whitelist.get(params.token.clone()).unwrap_or(false) {
            return Err(Error::TokenNotWhitelisted);
        }

        let window = params.release_window.unwrap_or(604800u32);
        if window == 0 {
            return Err(Error::ReleaseWindowTooShort);
        }
        let max_window = Self::get_max_release_window(env);
        if window > max_window {
            return Err(Error::ReleaseWindowTooLong);
        }

        Self::validate_optional_ipfs_hash(env, &params.ipfs_hash);

        if let Some(hash) = &params.metadata_hash {
            if hash.len() != 32 {
                return Err(Error::InvalidMetadataHash);
            }
        }

        if let Some(hash) = &params.service_agreement_hash {
            if hash.len() != 32 {
                return Err(Error::InvalidServiceAgreementHash);
            }
        }

        if env.storage().persistent().has(&(ESCROW, params.order_id)) {
            return Err(Error::EscrowAlreadyExists);
        }

        Ok(())
    }

    fn create_single_escrow(
        env: &Env,
        params: EscrowCreateParams,
        batch_id: Option<u64>,
    ) -> Result<u64, Error> {
        Self::validate_escrow_params(env, &params)?;

        let window = params.release_window.unwrap_or(604800u32);
        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;

        Self::validate_optional_metadata_hash(env, &params.metadata_hash);
        Self::validate_optional_service_agreement_hash(env, &params.service_agreement_hash);

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
            funding_deadline: None,
            service_agreement_hash: params.service_agreement_hash.clone(),
        };

        env.storage()
            .persistent()
            .set(&(ESCROW, params.order_id), &escrow);
        Self::extend_persistent(env, &(ESCROW, params.order_id));

        Self::update_active_obligations(env, &params.buyer, 1);
        Self::update_active_obligations(env, &params.seller, 1);

        Self::update_total_locked(env, &params.token, params.amount);
        Self::transfer_tokens_and_record_audit(
            env,
            &params.token,
            &params.buyer,
            &env.current_contract_address(),
            params.amount,
            &params.buyer,
            Symbol::new(env, "escrow_funded"),
            -params.amount,
        );

        Self::emit_escrow_created(
            env,
            EscrowEvent {
                schema_version: 1,
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

    pub fn create_escrows_batch(
        env: Env,
        params: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let batch_id = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "next_batch_id"))
            .unwrap_or(1u64);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "next_batch_id"), &(batch_id + 1));
        Self::create_batch_escrow(env, batch_id, params)
    }

    pub fn create_batch_escrow(
        env: Env,
        batch_id: u64,
        escrows: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let _guard = ReentryGuardScope::new(&env);
        Self::check_not_paused(&env);

        if escrows.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchLimitExceeded);
        }

        let mut results = soroban_sdk::Vec::new(&env);

        if escrows.is_empty() {
            return Ok(results);
        }

        let mut authorized_buyers: Map<Address, u32> = Map::new(&env);
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                let buyer_key = params.buyer.clone();
                if !authorized_buyers.contains_key(buyer_key.clone()) {
                    buyer_key.require_auth();
                    authorized_buyers.set(buyer_key, 1u32);
                }
            }
        }

        let mut seen_order_ids: Map<u32, bool> = Map::new(&env);
        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                if seen_order_ids.contains_key(params.order_id) {
                    return Err(Error::EscrowAlreadyExists);
                }
                seen_order_ids.set(params.order_id, true);
                Self::validate_escrow_params(&env, &params)?;
            }
        }

        let mut buyer_count_state: Map<Address, u32> = Map::new(&env);
        let mut seller_count_state: Map<Address, u32> = Map::new(&env);

        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                let buyer_key = params.buyer.clone();
                let seller_key = params.seller.clone();

                if !buyer_count_state.contains_key(buyer_key.clone()) {
                    let count_key = DataKey::BuyerEscrowCount(buyer_key.clone());
                    let existing_count: u32 =
                        env.storage().persistent().get(&count_key).unwrap_or(0u32);
                    buyer_count_state.set(buyer_key.clone(), existing_count);
                }

                if !seller_count_state.contains_key(seller_key.clone()) {
                    let count_key = DataKey::SellerEscrowCount(seller_key.clone());
                    let existing_count: u32 =
                        env.storage().persistent().get(&count_key).unwrap_or(0u32);
                    seller_count_state.set(seller_key.clone(), existing_count);
                }
            }
        }

        let mut buyer_next_counts: Map<Address, u32> = Map::new(&env);
        let mut seller_next_counts: Map<Address, u32> = Map::new(&env);

        for i in 0..escrows.len() {
            if let Some(params) = escrows.get(i) {
                match Self::create_single_escrow(&env, params.clone(), Some(batch_id)) {
                    Ok(id) => {
                        let buyer_key = params.buyer.clone();
                        let seller_key = params.seller.clone();

                        if !buyer_next_counts.contains_key(buyer_key.clone()) {
                            let existing_count =
                                buyer_count_state.get(buyer_key.clone()).unwrap_or(0u32);
                            buyer_next_counts.set(buyer_key.clone(), existing_count);
                        }
                        let buyer_count = buyer_next_counts.get(buyer_key.clone()).unwrap();

                        let buyer_index_key =
                            DataKey::BuyerEscrowIndexed(buyer_key.clone(), buyer_count);
                        env.storage().persistent().set(&buyer_index_key, &id);
                        Self::extend_persistent(&env, &buyer_index_key);

                        buyer_next_counts.set(buyer_key, buyer_count + 1);

                        if !seller_next_counts.contains_key(seller_key.clone()) {
                            let existing_count =
                                seller_count_state.get(seller_key.clone()).unwrap_or(0u32);
                            seller_next_counts.set(seller_key.clone(), existing_count);
                        }
                        let seller_count = seller_next_counts.get(seller_key.clone()).unwrap();

                        let seller_index_key =
                            DataKey::SellerEscrowIndexed(seller_key.clone(), seller_count);
                        env.storage().persistent().set(&seller_index_key, &id);
                        Self::extend_persistent(&env, &seller_index_key);

                        seller_next_counts.set(seller_key, seller_count + 1);

                        let escrow_opt: Option<Escrow> =
                            env.storage().persistent().get(&(ESCROW, id as u32));
                        if let Some(escrow) = escrow_opt {
                            Self::emit_escrow_created(
                                &env,
                                EscrowEvent {
                                    schema_version: 1,
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
                        return Err(e);
                    }
                }
            }
        }

        let mut i = 0;
        loop {
            if i >= buyer_next_counts.len() {
                break;Sorry, something went wrong. Please try your request again.
