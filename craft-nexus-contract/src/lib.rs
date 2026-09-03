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
///
/// # Categories
///
/// | Range   | Category     | Meaning                                         | Triage                    |
/// |---------|-------------|-------------------------------------------------|---------------------------|
/// | 1â€“9     | Auth/Access | Authorization, ownership, or existence failures | Rollback immediately      |
/// | 10â€“19   | State       | Invalid state transitions or preconditions      | Retry after state change  |
/// | 20â€“29   | Config      | Operator-configurable limits or misconfig       | Operator must act         |
/// | 30â€“39   | Operational | System or cooldown gates                        | Retry after cooldown      |
/// | 40â€“42   | Validation  | Input validation failures                       | Fix caller input          |
///
/// Use [`is_retryable`] to determine whether an error may succeed on retry.
#[contracterror(export = false)]
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
#[repr(u32)]
pub enum Error {
    // â”€â”€ Auth / Access (1â€“9): rollback immediately â”€â”€
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
    // â”€â”€ State / Transition (10â€“19): retry after state change â”€â”€
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
    // â”€â”€ Config / Resource (20â€“29): operator must act â”€â”€
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
    // â”€â”€ Operational / Gates (30â€“39): retry after cooldown â”€â”€
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
    // â”€â”€ Validation (40+): fix caller input â”€â”€
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
    UnsupportedToken = 56,
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

/// The CraftNexus escrow contract.
#[contract]
pub struct CraftNexusContract;

#[contractimpl]
impl CraftNexusContract {
    /// Initialize the contract with an admin.
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().set(&Symbol::new(&env, "admin"), &admin);
    }

    /// Return the configured admin.
    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .expect("contract not initialized")
    }

    /// Return the default storage-retention policy.
    pub fn default_retention_policy() -> StorageRetentionPolicy {
        StorageRetentionPolicy::default()
    }
}
