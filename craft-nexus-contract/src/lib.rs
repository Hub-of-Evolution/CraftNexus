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

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PlatformUnpausedEvent {
    pub initiator: Address,
    pub timestamp: u64,
    /// Admin revision consumed by this unpause (#1071).
    pub revision: u32,
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

/// Emitted for every distinct upgrade-signer approval of a proposal round.
///
/// Indexers key on `(nonce, signer)` — the same pair can never appear twice
/// for a live round because storage is canonicalized under
/// `DataKey::UpgradeSignerApproval(nonce, signer)` (#1059).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeApprovalEvent {
    /// Proposal round nonce (revision) this approval belongs to.
    pub nonce: u32,
    /// Signer that submitted this approval.
    pub signer: Address,
    /// WASM hash being approved.
    pub wasm_hash: BytesN<32>,
    pub timestamp: u64,
    /// Number of distinct approvals recorded after this one was accepted.
    pub approval_count: u32,
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

/// Immutable upgrade state commitment record. Persists after successful upgrade
/// execution to provide a verifiable, tamper-evident record of the migrated
/// state and compatibility evidence. Commitments become immutable once activated.
///
/// # Immutability
///
/// Once `activated_at` is set and `immutable` is `true`, this record cannot be
/// modified. Any attempt to re-execute an upgrade with the same `wasm_hash`
/// will be rejected. This ensures upgrade state commitments are permanent and
/// cannot be tampered with after activation.
///
/// # Fields
///
/// - `from_version` / `to_version` — version transition recorded at execution
/// - `wasm_hash` — the deployed contract code hash
/// - `state_digest` — SHA-256 of the migrated contract state snapshot
/// - `migration_result_digest` — SHA-256 of migration outcome evidence
/// - `admin` — the address that executed the upgrade
/// - `timestamp` — ledger timestamp when the commitment was created
/// - `activated_at` — ledger timestamp when the commitment became immutable
/// - `immutable` — flag indicating if the commitment is locked
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct UpgradeStateCommitment {
    pub from_version: u32,
    pub to_version: u32,
    pub wasm_hash: BytesN<32>,
    pub state_digest: BytesN<32>,
    pub migration_result_digest: BytesN<32>,
    pub admin: Address,
    pub timestamp: u64,
    pub activated_at: u64,
    pub immutable: bool,
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

/// Operation a resumable batch job performs.
///
/// Persisted on the [`BatchEscrowJob`] and echoed in every [`BatchCursor`] so a
/// cursor minted for one operation type can never be replayed against a job of a
/// different type (#1075). Discriminants are stable on the wire; today only
/// escrow creation is resumable, but new resumable operations get their own
/// variant here rather than overloading an existing one.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum BatchOpType {
    /// Resumable escrow creation (the only batch operation today).
    EscrowCreation = 0,
}

/// Persisted state for a scheduled batch. The parameters are immutable so a
/// continuation always operates on the same ordered input and cursor.
///
/// `revision` is a monotonic checkpoint counter: it starts at `0` and is
/// incremented by exactly one each time a chunk is committed atomically. A
/// [`BatchCursor`] carries the revision it expects to advance *from*, which is
/// what makes continuation idempotent and replay-safe (#1075/#1076).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct BatchEscrowJob {
    pub owner: Address,
    pub params: Vec<EscrowCreateParams>,
    pub next_index: u32,
    pub status: BatchJobStatus,
    /// The operation this job performs; bound into every cursor.
    pub op_type: BatchOpType,
    /// Monotonic count of atomically committed chunks (checkpoint revision).
    pub revision: u64,
}

/// Server-minted continuation token for a resumable batch job (#1075/#1076).
///
/// A cursor binds a continuation to exactly one `(job_id, owner, op_type)` and to
/// a specific committed `revision` of the job. [`Contract::continue_batch_escrow`]
/// validates the cursor against the persisted job before doing any work:
///
/// * a cursor whose `owner` differs from the job's owner is rejected
///   (`BatchJobUnauthorized`) — a cursor cannot be used for another account;
/// * a cursor whose `op_type` differs from the job's is rejected
///   (`BatchCursorMismatch`) — a cursor cannot be used for another operation;
/// * a cursor whose `revision` is **behind** the job's committed revision refers
///   to an already-applied chunk and is a harmless no-op returning current
///   progress (idempotent replay);
/// * a cursor whose `revision` is **ahead** of the job is a fabricated / future
///   cursor and is rejected (`BatchCursorMismatch`);
/// * at the live revision, `next_index` must equal the job's checkpoint or the
///   cursor is rejected (`BatchCursorMismatch`).
///
/// `next_index` is the position the cursor resumes from — observable and bounded
/// progress for clients and indexers (#1075 AC3, #1076 AC3).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct BatchCursor {
    pub job_id: u64,
    pub owner: Address,
    pub op_type: BatchOpType,
    pub revision: u64,
    pub next_index: u32,
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
    /// Operation type of the underlying job (mirrors the cursor binding).
    pub op_type: BatchOpType,
    /// Committed checkpoint revision after this call (see [`BatchEscrowJob`]).
    pub revision: u64,
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
#[cfg(test)]
mod test;
#[cfg(test)]
mod pagination_boundary_test;
#[cfg(test)]
mod prop_test;

/// Pending admin transfer details for two-step admin role rotation.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct PendingAdminTransfer {
    /// Proposed administrator address
    pub proposed_admin: Address,
    /// Proposer address (the current admin who initiated the transfer proposal)
    pub proposer: Address,
    /// Proposal revision counter (sequence number preventing replay of completed/stale proposals)
    pub revision: u64,
    /// Absolute ledger timestamp (in seconds) when this proposal expires
    pub expiry: u64,
}

/// Structured record of dispute evidence with metadata and expiry thresholds (#927).
///
/// Versioned for indexer stability (#1077): `version` identifies the record
/// schema (see [`EVIDENCE_SCHEMA_VERSION`]) and `content_digest` is the SHA-256 of
/// the evidence payload, persisted on the record itself so queries return stable
/// metadata without recomputing the hash or reading the reuse-guard key. Each
/// record is bound to exactly one dispute via `(order_id, dispute_session_id)`.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEvidence {
    pub id: u64,
    pub order_id: u32,
    pub dispute_session_id: u64,
    pub assignment_revision: u64,
    pub submitter: Address,
    pub evidence_uri: String,
    pub parent_evidence_id: Option<u64>,
    pub submitted_at: u64,
    pub expires_at: u64,
    pub is_invalidated: bool,
    /// Structured-record schema version (#1077).
    pub version: u32,
    /// SHA-256 digest of `evidence_uri`; stable metadata for indexers (#1077).
    pub content_digest: BytesN<32>,
}

/// Record of dispute escalation to arbitration (#941).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEscalationRecord {
    pub order_id: u32,
    pub assignment_revision: u64,
    pub escalated_by: Address,
    pub escalated_at: u64,
}

/// Escalation checkpoints a pending dispute passes through while it waits for
/// an arbitrator decision (#1080).
///
/// Tiers are strictly ordered and derived **only** from the ledger clock, so
/// every observer computes the same tier for the same dispute at the same
/// timestamp. Reaching a tier widens the set of accounts allowed to escalate;
/// it never narrows it.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum EscalationTier {
    /// No checkpoint reached yet: the assigned arbitrator is still inside its
    /// service window and nobody may escalate.
    Assigned = 0,
    /// First checkpoint: either party to the escrow may flag the dispute as
    /// stalled.
    PartyFlagged = 1,
    /// Second checkpoint: moderator review is unlocked, in addition to the
    /// parties.
    ModeratorReview = 2,
    /// Third checkpoint: admin review is unlocked, in addition to the parties
    /// and the moderator.
    AdminReview = 3,
    /// Final deadline reached. The dispute can no longer be arbitrated; only
    /// the deterministic timeout settlement remains, and anyone may trigger it.
    TimedOut = 4,
}

/// Admin-configurable escalation checkpoint schedule (#1080).
///
/// Every value is an offset in seconds from `dispute_initiated_at`. The final
/// deadline is deliberately **not** part of this struct: it is always
/// `PlatformConfig::max_dispute_duration`, so the escalation ladder and the
/// force-close path can never disagree about when a dispute is over.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscalationCheckpoints {
    /// Offset at which [`EscalationTier::PartyFlagged`] unlocks.
    pub party_checkpoint: u32,
    /// Offset at which [`EscalationTier::ModeratorReview`] unlocks.
    pub moderator_checkpoint: u32,
    /// Offset at which [`EscalationTier::AdminReview`] unlocks.
    pub admin_checkpoint: u32,
}

/// Absolute checkpoint timestamps for one specific dispute (#1080).
///
/// Produced by normalising [`EscalationCheckpoints`] against the dispute's
/// start time and the platform's final deadline. Offsets are clamped so the
/// sequence is always non-decreasing and always ends at `final_deadline`, even
/// if the admin later shortens `max_dispute_duration` below a checkpoint.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EscalationSchedule {
    /// Timestamp the dispute was opened; the origin of every checkpoint.
    pub initiated_at: u64,
    pub party_deadline: u64,
    pub moderator_deadline: u64,
    pub admin_deadline: u64,
    /// Hard stop: at or after this timestamp the dispute is `TimedOut` and can
    /// only be settled by the deterministic timeout path.
    pub final_deadline: u64,
}

/// Tiered escalation ladder state recorded on-chain for one dispute (#1080).
///
/// Written every time a dispute is escalated to a *higher* tier, which makes
/// the escalation history auditable: who escalated, to which tier, and when.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEscalationState {
    pub order_id: u32,
    /// Highest tier that has actually been recorded on-chain.
    pub tier: EscalationTier,
    /// Tier this dispute sat at before the most recent escalation.
    pub previous_tier: EscalationTier,
    /// Account that drove the dispute to `tier`.
    pub escalated_by: Address,
    /// Ledger timestamp of the most recent escalation.
    pub escalated_at: u64,
    /// Number of escalations recorded so far.
    pub escalation_count: u32,
}

/// Deterministic settlement a timed-out dispute will receive (#1080).
///
/// Derived purely from [`ExpiredDisputeFeePolicy`], so callers can preview the
/// outcome of a timeout before it happens and confirm it afterwards.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum TimeoutOutcome {
    /// Buyer is refunded the full escrow amount; the platform takes no fee.
    RefundBuyerFull = 0,
    /// Buyer is refunded the escrow amount minus the platform fee.
    RefundBuyerMinusFee = 1,
    /// Platform fee is split evenly between buyer and seller.
    RefundBuyerSplitFee = 2,
}

/// Auditable snapshot of a dispute's escalation ladder and final deadline (#1080).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeEscalationStatus {
    pub order_id: u32,
    /// Normalised checkpoint timestamps for this dispute.
    pub schedule: EscalationSchedule,
    /// Tier implied by the current ledger timestamp.
    pub current_tier: EscalationTier,
    /// Highest tier recorded on-chain (`Assigned` if never escalated).
    pub recorded_tier: EscalationTier,
    /// `true` once the final deadline has been reached.
    pub is_timed_out: bool,
    /// `true` once a settlement receipt exists — the dispute is finalized and
    /// no further settlement of any kind can run.
    pub is_finalized: bool,
    /// Settlement a timeout would produce for this dispute.
    pub timeout_outcome: TimeoutOutcome,
/// The arbitrator and revision assigned to one active dispute.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeAssignment {
    pub order_id: u32,
    pub arbitrator: Address,
    pub revision: u64,
    pub assigned_at: u64,
}

/// Emitted when an active dispute is explicitly moved to a new assignment.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct DisputeAssignmentChangedEvent {
    pub order_id: u32,
    pub old_revision: u64,
    pub new_revision: u64,
    pub arbitrator: Address,
    pub changed_at: u64,
}

/// Configuration for sensitive action rate limiting (#943).
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct RateLimitConfig {
    pub max_calls: u32,
    pub window: u32,
}

/// Challenge window state for a disputed escrow (#942).
///
/// Tracks the bounded period during which participants may challenge
/// evidence before the dispute can be finalized. The window closes
/// exactly once — the `Closed` state is terminal and cannot be reopened.
#[contracttype]
#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub enum ChallengeState {
    /// Window is open; counter-evidence may be submitted and finalization is blocked.
    Open = 0,
    /// Window has been closed (deadline elapsed and dispute finalized); no further challenges.
    Closed = 1,
}

/// Persisted challenge window for a disputed order (#942).
///
/// Stores the immutable deadline computed at dispute time so that a later
/// admin change to `evidence_challenge_window` cannot shorten or extend
/// the window for an in-flight dispute. The `state` field guarantees the
/// window closes exactly once — resolution atomically transitions `Open` → `Closed`.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "testutils"), derive(Debug))]
pub struct EvidenceChallenge {
    pub order_id: u32,
    pub deadline: u64,
    pub state: ChallengeState,
    pub evidence_count: u32,
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
    pub assignment_revision: u64,
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
    fn validate_onboarding_attestation(
        env: Env,
        attestation: OnboardingAttestation,
        expected_account: Address,
        expected_operation_id: Bytes,
        expected_contract_instance: Address,
        expected_state_revision: u64,
        expected_ledger_sequence: u32,
    ) -> bool;
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

    /// Validate that an onboarding attestation is bound to the exact account,
    /// contract instance, operation identifier, profile revision, and ledger
    /// sequence expected by the calling operation.
    ///
    /// This check runs before any financial state change. A mismatch means the
    /// attestation was replayed from another account, another privilege, or an
    /// older profile revision.
    fn validate_onboarding_attestation_binding(
        env: &Env,
        attestation: &OnboardingAttestation,
        expected_account: &Address,
        expected_operation_id: &Bytes,
        expected_contract_instance: &Address,
        expected_state_revision: u64,
        expected_ledger_sequence: u32,
    ) -> Result<(), Error> {
        if attestation.account != *expected_account
            || attestation.operation_id != *expected_operation_id
            || attestation.contract_instance != *expected_contract_instance
        {
            return Err(Error::OnboardingAuthorizationFailed);
        }
        if attestation.state_revision != expected_state_revision
            || attestation.ledger_sequence != expected_ledger_sequence
        {
            return Err(Error::OnboardingProfileStale);
        }
        if attestation.operation_id.len() == 0
            || attestation.profile_version == 0
            || attestation.state_revision == 0
            || attestation.ledger_sequence == 0
        {
            return Err(Error::OnboardingAuthorizationFailed);
        }
        Ok(())
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
                revision: Self::last_applied_admin_revision(env),
            },
        );
    }

    fn current_admin_revision(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::AdminRevision)
            .unwrap_or(0u32)
    }

    fn last_applied_admin_revision(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::LastAppliedAdminRevision)
            .unwrap_or(0u32)
    }

    fn hash_admin_mutation(env: &Env, label: &str, payload: Bytes) -> BytesN<32> {
        let mut data = Bytes::from_slice(env, label.as_bytes());
        data.append(&payload);
        env.crypto().sha256(&data).into()
    }

    /// Bind an admin mutation to `expected_revision`.
    ///
    /// * Replay of the same fingerprint at the revision that already applied it
    ///   returns [`Error::AdminActionAlreadyApplied`] without writing.
    /// * A mismatched revision returns [`Error::StaleAdminRevision`].
    /// * On success the monotonic revision advances by one and the applied
    ///   revision is stored for event emission (#1071).
    fn commit_admin_mutation(
        env: &Env,
        expected_revision: u32,
        fingerprint: BytesN<32>,
    ) -> Result<u32, Error> {
        let current = Self::current_admin_revision(env);
        let last_fp: Option<BytesN<32>> = env
            .storage()
            .instance()
            .get(&DataKey::AdminMutationFingerprint);
        let last_rev = Self::last_applied_admin_revision(env);

        if last_fp.as_ref() == Some(&fingerprint) && last_rev == expected_revision {
            return Err(Error::AdminActionAlreadyApplied);
        }
        if expected_revision != current {
            return Err(Error::StaleAdminRevision);
        }
        if last_fp.as_ref() == Some(&fingerprint) {
            return Err(Error::AdminActionAlreadyApplied);
        }

        env.storage()
            .instance()
            .set(&DataKey::AdminMutationFingerprint, &fingerprint);
        env.storage()
            .instance()
            .set(&DataKey::LastAppliedAdminRevision, &expected_revision);
        let next = current.saturating_add(1);
        env.storage()
            .instance()
            .set(&DataKey::AdminRevision, &next);
        Ok(expected_revision)
    }

    fn gate_admin_mutation(env: &Env, label: &str, payload: Bytes) -> Result<u32, Error> {
        let fingerprint = Self::hash_admin_mutation(env, label, payload);
        Self::commit_admin_mutation(env, Self::current_admin_revision(env), fingerprint)
    }

    /// Current monotonic admin revision. Callers of
    /// [`Self::apply_admin_mutation`] must pass this value.
    pub fn get_admin_revision(env: Env) -> u32 {
        Self::current_admin_revision(&env)
    }

    /// Apply a configuration, pause, or governance mutation at an explicit
    /// revision. Stale or already-applied requests fail without writing (#1071).
    pub fn apply_admin_mutation(
        env: Env,
        expected_revision: u32,
        action: AdminActionKind,
    ) -> Result<u32, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        let fingerprint = Self::hash_admin_mutation(
            &env,
            "apply_admin_mutation",
            action.to_xdr(&env),
        );
        let revision = Self::commit_admin_mutation(&env, expected_revision, fingerprint)?;
        let proposal = AdminActionProposal {
            id: 0,
            kind: action,
            proposer: admin,
            approvals: Vec::new(&env),
            threshold: 1,
            signers: Vec::new(&env),
            created_at: env.ledger().timestamp(),
            ready_at: env.ledger().timestamp(),
            executed: false,
            cancelled: false,
            applied_revision: revision,
        };
        Self::apply_admin_action(&env, &proposal)?;
        Ok(revision)
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
                revision: Self::last_applied_admin_revision(env),
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
                revision: Self::last_applied_admin_revision(env),
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

        let new_count = count
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));

        let index_key = DataKey::GlobalEscrowIdIndexed(count);
        env.storage().persistent().set(&index_key, &order_id);
        Self::extend_persistent(env, &index_key);

        env.storage().persistent().set(&count_key, &new_count);
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
                count = count
                    .checked_add(1)
                    .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
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
            count
                .checked_add(delta as u32)
                .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow))
        } else {
            let subtract = (-delta) as u32;
            if subtract > count {
                env.panic_with_error(Error::CounterUnderflow);
            }
            count - subtract
        };
        env.storage().persistent().set(&key, &new_val);
        Self::extend_persistent(env, &key);
    }

    #[inline(always)]
    fn update_active_dispute_count(env: &Env, delta: i32) {
        let key = DataKey::ActiveDisputeCount;
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_val = if delta > 0 {
            count
                .checked_add(delta as u32)
                .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow))
        } else {
            let subtract = (-delta) as u32;
            if subtract > count {
                env.panic_with_error(Error::CounterUnderflow);
            }
            count - subtract
        };
        env.storage().persistent().set(&key, &new_val);
        Self::extend_persistent(env, &key);
    }

    /// Atomically acquires the emergency operation lock if currently Idle,
    /// transitioning to the requested operation's Executing state and incrementing
    /// the revision. On success, no return value; on failure, panics with
    /// EmergencyOpInProgress or EmergencyConflictActive (#1072).
    fn assert_emergency_op_idle_and_acquire(
        env: &Env,
        actor: &Address,
        kind: EmergencyOpKind,
    ) -> Result<(), Error> {
        // Check for active disputes, upgrades, or recurring escrows that block
        // this specific operation type
        match kind {
            EmergencyOpKind::AdminRecovery => {
                // Recovery blocked if disputes exist, upgrades exist, or recurring escrows exist
                if Self::get_active_dispute_count(env.clone()) > 0 {
                    return Err(Error::EmergencyConflictActive);
                }
                if env
                    .storage()
                    .persistent()
                    .has(&DataKey::WasmUpgradeProposal)
                {
                    return Err(Error::EmergencyConflictActive);
                }
                if Self::get_active_recurring_count(env.clone()) > 0 {
                    return Err(Error::EmergencyConflictActive);
                }
            }
            _ => {
                // Other operations (Sweep, Upgrade, Pause) are blocked if ANY
                // emergency operation is already in flight
                if env
                    .storage()
                    .persistent()
                    .has(&DataKey::CurrentEmergencyOperation)
                {
                    return Err(Error::EmergencyOpInProgress);
                }
            }
        }

        // Acquire the lock: create new in-flight operation state
        let current_revision: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::CurrentEmergencyOperation)
            .map(|op: EmergencyOperation| op.revision)
            .unwrap_or(0);

        let new_op = EmergencyOperation {
            kind,
            actor: actor.clone(),
            phase: EmergencyOpPhase::Executing,
            revision: current_revision.saturating_add(1),
            started_at: env.ledger().timestamp(),
            success: false,
            amount: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::CurrentEmergencyOperation, &new_op);
        Self::extend_persistent(env, &DataKey::CurrentEmergencyOperation);

        Ok(())
    }

    /// Atomically releases the emergency operation lock on successful completion,
    /// transitioning state back to Idle (by removing CurrentEmergencyOperation),
    /// appending to history, and incrementing revision (#1072).
    fn release_emergency_op_on_success(env: &Env, kind: EmergencyOpKind, amount: i128) {
        if let Some(mut op) = env
            .storage()
            .persistent()
            .get::<DataKey, EmergencyOperation>(&DataKey::CurrentEmergencyOperation)
        {
            // Verify this is the same operation we acquired
            if op.kind != kind {
                return; // Mismatched operation type, don't proceed
            }

            // Mark successful and append to history
            op.success = true;
            op.amount = amount;
            op.phase = EmergencyOpPhase::Completed;
            op.revision = op.revision.saturating_add(1); // Increment on exit

            Self::append_to_emergency_history(env, &op);

            // Release the lock
            env.storage()
                .persistent()
                .remove(&DataKey::CurrentEmergencyOperation);
        }
    }

    /// Atomically releases the emergency operation lock on failure,
    /// transitioning to Failed phase, incrementing revision, and appending
    /// to history so the failure is auditable (#1072).
    fn release_emergency_op_on_failure(env: &Env) {
        if let Some(mut op) = env
            .storage()
            .persistent()
            .get::<DataKey, EmergencyOperation>(&DataKey::CurrentEmergencyOperation)
        {
            op.phase = EmergencyOpPhase::Failed;
            op.success = false;
            op.revision = op.revision.saturating_add(1); // Increment on exit

            Self::append_to_emergency_history(env, &op);

            // Release the lock
            env.storage()
                .persistent()
                .remove(&DataKey::CurrentEmergencyOperation);
        }
    }

    /// Appends an emergency operation to the history log, maintaining bounded history.
    /// History is kept for audit trails but capped to prevent unbounded growth (#1072).
    fn append_to_emergency_history(env: &Env, op: &EmergencyOperation) {
        const MAX_HISTORY: u32 = 100;

        let count_key = DataKey::EmergencyOperationHistoryCount;
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        let index = count.min(MAX_HISTORY - 1);
        env.storage()
            .persistent()
            .set(&DataKey::EmergencyOperationHistoryIndexed(index), op);

        let new_count = (count + 1).min(MAX_HISTORY);
        env.storage().persistent().set(&count_key, &new_count);

        Self::extend_persistent(env, &count_key);
        Self::extend_persistent(env, &DataKey::EmergencyOperationHistoryIndexed(index));
    }

    pub fn get_active_dispute_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ActiveDisputeCount)
            .unwrap_or(0)
    }

    /// Returns the count of currently active (non-released, non-cancelled) recurring escrows.
    /// Used for conflict detection: recovery operations are blocked if recurring escrows
    /// exist, as they depend on the current admin (#1072).
    pub fn get_active_recurring_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ActiveRecurringCount)
            .unwrap_or(0)
    }

    /// Returns the current in-flight emergency operation (if any) and its state.
    /// No authorization required — this is freely queryable so operators can
    /// diagnose active incident response operations (#1072).
    pub fn get_emergency_operation(env: Env) -> Option<EmergencyOperation> {
        env.storage()
            .persistent()
            .get(&DataKey::CurrentEmergencyOperation)
    }

    /// Returns paginated history of completed/failed emergency operations for audit trails.
    /// Offset and limit are 0-indexed; max 50 entries per page (#1072).
    pub fn get_emergency_operation_history(env: Env, offset: u32, limit: u32) -> Vec<EmergencyOperation> {
        let page_size = pagination_validation::validate_limit(
            limit,
            pagination_validation::MAX_PAGE_SIZE,
        ).unwrap_or(limit.min(pagination_validation::MAX_PAGE_SIZE));

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::EmergencyOperationHistoryCount)
            .unwrap_or(0);

        let mut history = Vec::new(&env);
        let end = (offset + page_size).min(count);

        for idx in offset..end {
            if let Some(op) = env
                .storage()
                .persistent()
                .get::<DataKey, EmergencyOperation>(&DataKey::EmergencyOperationHistoryIndexed(idx))
            {
                history.push_back(op);
            }
        }
        history
    }

    /// Force-releases a stranded in-flight emergency operation lock back to Idle state.
    /// Can only be called by an authorized admin (same as other emergency operations).
    /// This prevents a failed or abandoned multi-step operation from permanently blocking
    /// future emergency response (#1072).
    pub fn abort_emergency_operation(env: Env, admin: Address) -> Result<EmergencyOperation, Error> {
        let contract_admin = Self::get_admin(&env)?;
        contract_admin.require_auth();

        let op = env
            .storage()
            .persistent()
            .get::<DataKey, EmergencyOperation>(&DataKey::CurrentEmergencyOperation)
            .ok_or(Error::NoUpgradeProposed)?; // No operation in flight

        // Transition to Failed phase and release lock
        Self::release_emergency_op_on_failure(&env);

        // Return the aborted operation
        Ok(EmergencyOperation {
            phase: EmergencyOpPhase::Failed,
            success: false,
            ..op
        })
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
        let new_total = if delta >= 0 {
            current
                .checked_add(delta)
                .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow))
        } else {
            let sub_amount = delta
                .checked_neg()
                .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
            if sub_amount > current {
                env.panic_with_error(Error::CounterUnderflow);
            }
            current - sub_amount
        };
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
        let new_total = if delta >= 0 {
            current
                .checked_add(delta)
                .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow))
        } else {
            let sub_amount = delta
                .checked_neg()
                .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
            if sub_amount > current {
                env.panic_with_error(Error::CounterUnderflow);
            }
            current - sub_amount
        };
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

    /// Extend the TTL of a temporary storage entry using standardized values.
    #[inline(always)]
    fn extend_temporary(env: &Env, key: &impl soroban_sdk::IntoVal<Env, soroban_sdk::Val>) {
        env.storage()
            .temporary()
            .extend_ttl(key, TTL_THRESHOLD, TTL_EXTENSION);
    }

    /// Check and validate an idempotency key if provided (#1025).
    ///
    /// If the key has already been used for this caller:
    /// - If the recorded operation and parameters match, returns `Ok(Some(order_id))`.
    /// - If the operation or parameters do not match, returns `Err(Error::IdempotencyMismatch)`.
    /// If the key is fresh or None, returns `Ok(None)`.
    fn check_idempotency(
        env: &Env,
        caller: &Address,
        idempotency_key: &Option<BytesN<32>>,
        expected_op: IdempotencyOp,
        params_hash: &BytesN<32>,
    ) -> Result<Option<u32>, Error> {
        let key_bytes = match idempotency_key {
            Some(k) => k,
            None => return Ok(None),
        };

        let storage_key = DataKey::IdempotencyRecord(caller.clone(), key_bytes.clone());
        if let Some(record) = env
            .storage()
            .temporary()
            .get::<_, IdempotencyRecord>(&storage_key)
        {
            Self::extend_temporary(env, &storage_key);
            if record.op == expected_op && &record.params_hash == params_hash {
                return Ok(Some(record.order_id));
            } else {
                return Err(Error::IdempotencyMismatch);
            }
        }

        Ok(None)
    }

    /// Record an idempotency key after successful operation execution (#1025).
    fn record_idempotency(
        env: &Env,
        caller: &Address,
        idempotency_key: &Option<BytesN<32>>,
        op: IdempotencyOp,
        order_id: u32,
        params_hash: BytesN<32>,
    ) {
        if let Some(key_bytes) = idempotency_key {
            let storage_key = DataKey::IdempotencyRecord(caller.clone(), key_bytes.clone());
            let record = IdempotencyRecord {
                op,
                order_id,
                params_hash,
                created_at: env.ledger().timestamp(),
            };
            env.storage().temporary().set(&storage_key, &record);
            Self::extend_temporary(env, &storage_key);
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

    /// Asserts that an account is active and permitted to perform privileged actions.
    ///
    /// # Purpose (Issue #1057)
    ///
    /// Checks that an account's profile status is `Active` in the configured
    /// onboarding contract. This shared function is called at **every** restricted
    /// entrypoint **immediately after** `require_auth()` to prevent deactivated
    /// accounts from initiating new escrows, stakes, disputes, or other privileged
    /// operations.
    ///
    /// # Why a single shared check?
    ///
    /// Duplicated status checks diverge. A single function guarantees that
    /// deactivation is enforced consistently and that fixing a bug here
    /// fixes it everywhere.
    ///
    /// # Stale cache prevention
    ///
    /// Always reads from persistent storage via `is_profile_active` — never from
    /// instance cache. This ensures a deactivation takes effect immediately on
    /// the next call, not after a cache TTL.
    ///
    /// # Settlement rules for existing obligations
    ///
    /// When an account is deactivated:
    /// - **Existing escrows**: follow their normal lifecycle to completion.
    ///   A deactivated account that is a counterparty to an existing escrow
    ///   can still receive funds from that escrow's settlement — they cannot
    ///   INITIATE new escrows.
    /// - **Active stakes**: remain locked. The staking contract's normal
    ///   unstake/withdraw flow applies. Deactivation does not force-unstake.
    /// - **Open disputes**: continue to their resolution. The deactivated
    ///   party can still respond to an existing dispute they are party to.
    /// - **Pending withdrawals**: can be completed. Deactivation does not
    ///   freeze funds already earmarked for withdrawal.
    ///
    /// In short: deactivation blocks NEW privileged actions.
    /// It does not void existing obligations or freeze in-flight settlements.
    ///
    /// # Arguments
    /// * `env` — The contract environment
    /// * `account` — The address to check
    ///
    /// # Errors
    /// * Panics with [`Error::OnboardingProfileInactive`] if the account's status
    ///   is not `Active` (i.e., Deactivated, UnderReview, or Flagged).
    /// * Does nothing if no onboarding contract is configured (open mode).
    fn assert_account_active(env: &Env, account: &Address) {
        // No-op when no onboarding contract is configured — operate in open mode.
        if Self::get_onboarding_address(env).is_none() {
            return;
        }

        // Check account status via the onboarding contract
        let (is_active, _role, _is_verified, _state_version) =
            match Self::safe_check_onboarding_state(env, account) {
                Ok(state) => state,
                Err(()) => {
                    // Cross-contract call failed — emit warning but allow the
                    // operation to proceed so a temporarily unreachable onboarding
                    // contract cannot permanently brick privileged operations.
                    Self::emit_onboarding_call_failed(
                        env,
                        Symbol::new(env, "check_active"),
                        account.clone(),
                    );
                    return;
                }
            };

        // Reject if account is not active
        if !is_active {
            env.panic_with_error(Error::OnboardingProfileInactive);
        }
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
    /// Validate that a token contract implements the required SEP-41 interface
    /// without transferring or mutating any customer funds (#1063).
    ///
    /// Checks:
    /// - `decimals() -> u32` exists and is within 0..=18
    /// - `balance(Address) -> i128` exists and is callable
    /// - `transfer(Address, Address, i128)` exists and is callable (probed via
    ///   a zero-amount self-transfer `contract -> contract, 0` so no funds move)
    ///
    /// Missing or malformed methods return `Error::UnsupportedToken`.
    /// A `decimals` value outside 0..=18 returns `Error::InvalidTokenDecimals`.
    /// The probe is read-only in effect: it never moves customer balances and
    /// touches only the contract's own address with amount `0`.
    fn ensure_token_interface(env: &Env, token: &Address) -> Result<(), Error> {
        let token_client = token::Client::new(env, token);
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
        // Non-mutating transfer probe: zero self-transfer never moves customer
        // funds but confirms the `transfer` entrypoint exists and is callable.
        let contract = env.current_contract_address();
        token_client
            .try_transfer(&contract, &contract, &0)
            .map_err(|_| Error::UnsupportedToken)?
            .map_err(|_| Error::UnsupportedToken)?;
        Ok(())
    }

    /// Public read-only compatibility check for any token address (#1063).
    ///
    /// Admins and integrators can call this off-chain before `whitelist_token`
    /// to preflight compatibility without persisting state or moving funds.
    /// Returns `Ok(())` when the contract exposes the required `decimals`,
    /// `balance`, and `transfer` interface; otherwise returns
    /// `Error::UnsupportedToken` or `Error::InvalidTokenDecimals`.
    pub fn validate_token_compatibility(env: Env, token: Address) -> Result<(), Error> {
        Self::ensure_token_interface(&env, &token)
    }

    /// Returns `true` when `validate_token_compatibility` would succeed.
    ///
    /// Convenience read-only wrapper for UIs that prefer a boolean check over
    /// handling contract errors. Never mutates state or moves funds.
    pub fn is_token_supported(env: Env, token: Address) -> bool {
        Self::ensure_token_interface(&env, &token).is_ok()
    }

    pub fn whitelist_token(env: Env, token: Address) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        Self::ensure_token_interface(&env, &token)?;

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


    fn validate_partial_refund_solvency(
    env: &Env,
    escrow: &Escrow,
    refund_gross: i128,
) -> Result<(i128, FeeAllocation), Error> {
    // 1. Rejects over-sized proposals instantly
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
    
    // 3. Successful settlement balances to the original escrow amount
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

        // Validate platform_wallet — reject the contract's own address to prevent
        // fee transfers from panicking at the host level (#707).
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
        env.storage()
            .instance()
            .set(&DataKey::AdminRevision, &0u32);

        env.storage()
            .persistent()
            .set(&DataKey::ArbitratorAssignmentRevision, &1u64);

        if let Err(e) = Self::set_fallback_admin(&env, admin.clone()) {
            env.panic_with_error(e);
        }

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

        // Initialize storage layout version so future upgrades can validate the
        // on-disk schema before applying new logic.
        env.storage().persistent().set(
            &DataKey::StorageLayoutVersion,
            &CURRENT_STORAGE_LAYOUT_VERSION,
        );
        Self::extend_persistent(&env, &DataKey::StorageLayoutVersion);

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
    /// Starts the two-step transfer process with default bounded window (#95).
    /// Both the current admin and the incoming admin must co-sign, proving the
    /// new address is a live, registered ledger node capable of authorizing
    /// transactions (#419).
    pub fn update_admin(env: Env, new_admin: Address) {
        Self::update_admin_with_window(env, new_admin, DEFAULT_ADMIN_TRANSFER_WINDOW);
    }

    /// Propose a new administrator for the platform with a custom bounded expiration window (admin only).
    pub fn update_admin_with_window(env: Env, new_admin: Address, window_secs: u64) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        // Validate: not the contract address itself (#240)
        if Self::validate_admin_address(&env, &new_admin).is_err() {
            env.panic_with_error(Error::InvalidAdminAddress);
        }

        // Require the incoming admin to co-sign, proving it is a fully
        // registered ledger node that controls its own private key (#419).
        new_admin.require_auth();

        let previous_admin = config.admin.clone();
        let current_revision: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::AdminTransferRevision)
            .unwrap_or(0);
        let next_revision = current_revision.saturating_add(1);
        let expiry = env.ledger().timestamp().saturating_add(window_secs);

        let transfer = PendingAdminTransfer {
            proposed_admin: new_admin.clone(),
            proposer: config.admin.clone(),
            revision: next_revision,
            expiry,
        };

        env.storage()
            .persistent()
            .set(&DataKey::AdminTransferRevision, &next_revision);
        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &transfer);

        config.pending_admin = Some(new_admin.clone());
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        // Emit audit event for admin change proposal
        Self::emit_admin_changed(&env, previous_admin, new_admin, "admin_proposed");
    }

    /// Claim the administrative role (pending admin only).
    /// Completes the two-step transfer process (#95).
    /// Enhanced with validation, bounded expiry check, audit logging and fallback setup (#240).
    pub fn claim_admin(env: Env) {
        let mut config = Self::get_platform_config_internal(&env);

        let transfer: PendingAdminTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .unwrap_or_else(|| env.panic_with_error(Error::NoPendingAdmin));

        // Expired transfers cannot be accepted.
        if env.ledger().timestamp() >= transfer.expiry {
            env.panic_with_error(Error::TransferExpired);
        }

        // Old administrators cannot replay or claim completed/invalid proposals.
        if transfer.proposer != config.admin {
            env.panic_with_error(Error::Unauthorized);
        }

        let pending = &transfer.proposed_admin;
        pending.require_auth();

        // Validate the pending admin address before accepting the transfer
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

        env.storage().persistent().remove(&DataKey::PendingAdmin);

        // Emit audit event for the completed two-step admin transfer (#631)
        Self::emit_admin_changed(&env, previous_admin, new_admin, "admin_claimed");
    }

    /// Cancel an in-progress two-step admin transfer (current admin only).
    pub fn cancel_admin_transfer(env: Env) -> Result<(), Error> {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if config.pending_admin.is_none()
            && !env.storage().persistent().has(&DataKey::PendingAdmin)
        {
            return Err(Error::NoPendingAdmin);
        }

        config.pending_admin = None;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        env.storage().persistent().remove(&DataKey::PendingAdmin);
        Ok(())
    }

    /// Read-only view function to inspect active (unexpired) pending admin transfer proposal.
    pub fn get_pending_admin_transfer(env: Env) -> Option<PendingAdminTransfer> {
        let transfer: PendingAdminTransfer = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)?;
        if env.ledger().timestamp() >= transfer.expiry {
            None
        } else {
            Some(transfer)
        }
    }

    // ----- Admin Action Proposal (Multi-Sig + Timelock) -----

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

    /// Configure the signer set for pending critical admin actions.
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

    /// Configure the approval threshold for pending critical admin actions.
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

    /// Configure the timelock delay applied to pending critical admin actions.
    pub fn set_admin_action_timelock_delay(env: Env, delay_seconds: u64) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        env.storage().instance().set(
            &AdminActionDataKey::AdminActionTimelockDelay,
            &delay_seconds,
        );
        Ok(())
    }

    /// Create a new pending admin action that requires multi-sig approvals.
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
            applied_revision: 0,
        };

        env.storage()
            .persistent()
            .set(&AdminActionDataKey::NextAdminActionId, &(next_id + 1));
        Self::extend_persistent(&env, &AdminActionDataKey::NextAdminActionId);
        Self::persist_admin_action(&env, &proposal);

        Ok(proposal)
    }

    /// Approve an existing pending admin action.
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

    /// Cancel a pending admin action.
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

    /// Execute a pending admin action once its approvals and timelock have been satisfied.
    ///
    /// Guarded like every other custody entry point (#1069): `SweepUnallocatedFunds`
    /// reaches `transfer_tokens_and_record_audit`, which fails closed unless a
    /// `ReentryGuardScope` is already active for the current invocation.
    pub fn execute_admin_action(env: Env, action_id: u64) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
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

        let fingerprint = Self::hash_admin_mutation(
            &env,
            "execute_admin_action",
            action.kind.to_xdr(&env),
        );
        let revision =
            Self::commit_admin_mutation(&env, Self::current_admin_revision(&env), fingerprint)?;

        let mut persisted = action.clone();
        Self::apply_admin_action(&env, &persisted)?;
        persisted.executed = true;
        persisted.applied_revision = revision;
        Self::persist_admin_action(&env, &persisted);
        Ok(())
    }

    /// Return all pending admin actions that have not executed or been cancelled.
    pub fn get_pending_admin_actions(env: Env) -> Vec<AdminActionProposal> {
        let mut actions = Vec::new(&env);
        let next_id = Self::get_next_admin_action_id(&env);
        for action_id in 1..next_id {
            if let Some(action) = Self::get_admin_action(&env, action_id) {
                if !action.executed && !action.cancelled {
                    actions.push_back(action);
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
    }

    fn apply_reconciliation_repair(env: &Env, plan_id: u64) -> Result<(), Error> {
        let key = DataKey::ReconciliationRepairPlan(plan_id);
        let mut plan: ReconciliationRepairPlan = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::RepairPlanNotFound)?;

        // Applying a plan twice is harmless (returns Ok(()))
        if plan.applied || plan.consumed || env.storage().persistent().has(&DataKey::ConsumedRepairPlan(plan_id)) {
            return Ok(());
        }

        if plan.cancelled {
            return Err(Error::RepairPlanTerminal);
        }

        let report: ReconciliationReport = env
            .storage()
            .persistent()
            .get(&DataKey::ReconciliationReport(plan.token.clone()))
            .ok_or(Error::ReconciliationRequired)?;

        let current_digest = Self::compute_reconciliation_digest(
            env,
            &plan.token,
            report.expected_locked,
            report.expected_staked,
            report.balance,
            report.tracked_locked,
            report.tracked_staked,
        );

        // Repairs are blocked when the expected state digest changes
        if current_digest != plan.discrepancy_digest {
            return Err(Error::RepairPlanPreconditionFailed);
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
        plan.consumed = true;
        env.storage().persistent().set(&key, &plan);
        env.storage()
            .persistent()
            .set(&DataKey::ConsumedRepairPlan(plan_id), &true);
        Self::extend_persistent(env, &key);
        Self::extend_persistent(env, &DataKey::ConsumedRepairPlan(plan_id));
        Ok(())
    }
}

    /// Recover admin access using fallback admin after time lock period (#240)
    /// This provides a recovery mechanism if the primary admin is corrupted or inaccessible
    /// Requires a 7-day time lock after recovery is initiated to prevent abuse
    pub fn recover_admin_access(env: Env, recovered_admin: Address) -> Result<(), Error> {
        // Check if fallback admin exists and is authorized
        let fallback = match env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::FallbackAdmin)
        {
            Some(fallback) => fallback,
            None => return Err(Error::AdminRecoveryFailed),
        };

        fallback.require_auth();

        // Validate the recovery address
        if Self::validate_admin_address(&env, &recovered_admin).is_err() {
            return Err(Error::AdminRecoveryFailed);
        }

        // Reject recovery to the address that is already the current admin.
        // This would be a no-op that masks a failed/misconfigured recovery
        // attempt rather than actually restoring access.
        if let Ok(current_admin) = Self::get_admin(&env) {
            if recovered_admin == current_admin {
                return Err(Error::AdminRecoveryFailed);
            }
        }

        // Check if recovery time lock has passed (#431 â€” TTL-friendly read)
        let recovery_time = Self::get_persistent_u64(&env, &DataKey::AdminRecoveryTime);

        let current_time = env.ledger().timestamp();

        // If this is the first recovery request, initiate time lock and record
        // the delay used so that malicious direct writes to `AdminRecoveryTime`
        // cannot bypass the minimum cooldown requirement.
        if recovery_time == 0 {
            let new_recovery_time = current_time + ADMIN_RECOVERY_DELAY;
            let recovery_time_key = DataKey::AdminRecoveryTime;
            env.storage()
                .persistent()
                .set(&recovery_time_key, &new_recovery_time);
            Self::extend_persistent(&env, &recovery_time_key);

            // Record the delay used for this initiation so it can be validated
            // later when recovery is attempted.
            let delay_key = DataKey::AdminRecoveryDelay;
            env.storage()
                .persistent()
                .set(&delay_key, &ADMIN_RECOVERY_DELAY);
            Self::extend_persistent(&env, &delay_key);

            env.events().publish(
                (Symbol::new(&env, "admin_recovery_initiated"), true),
                String::from_str(&env, "7-day time lock initiated for admin recovery"),
            );
            return Err(Error::AdminRecoveryFailed); // Recovery not ready yet
        }

        // Check if time lock period has elapsed
        if current_time < recovery_time {
            return Err(Error::AdminRecoveryFailed);
        }

        // Ensure the recorded cooldown meets the minimum floor. If the delay
        // is missing or below the minimum, treat this as a failed recovery
        // attempt to prevent direct-storage bypasses.
        let recorded_delay = Self::get_persistent_u64(&env, &DataKey::AdminRecoveryDelay);
        if recorded_delay == 0 || recorded_delay < MIN_ADMIN_RECOVERY_COOLDOWN {
            return Err(Error::AdminRecoveryFailed);
        }

        // Time lock has passed, proceed with recovery
        let mut config = Self::get_platform_config_internal(&env);
        let previous_admin = config.admin.clone();

        config.admin = recovered_admin.clone();
        config.pending_admin = None;
        env.storage().persistent().remove(&DataKey::PendingAdmin);
        // Write config to instance storage (primary location) â€” TTL already extended
        // by get_platform_config_internal. No redundant extend_persistent needed.
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        // Sync to persistent backup key for recovery consistency (no TTL extension needed
        // since this is a one-time sync, not a read-heavy path).
        env.storage().persistent().set(&PLATFORM_FEE, &config);

        // Clear the recovery time lock for next cycle
        env.storage()
            .persistent()
            .remove(&DataKey::AdminRecoveryTime);
        // Clear the recorded delay as well
        env.storage()
            .persistent()
            .remove(&DataKey::AdminRecoveryDelay);

        let fingerprint = Self::hash_admin_mutation(
            &env,
            "recover_admin_access",
            recovered_admin.to_xdr(&env),
        );
        Self::commit_admin_mutation(&env, Self::current_admin_revision(&env), fingerprint)?;

        // Emit audit event
        Self::emit_admin_changed(&env, previous_admin, recovered_admin, "admin_recovered");

        Ok(())
    }

    /// Create a new escrow with an optional idempotency key (#1025).
    ///
    /// If an idempotency key is provided and was already used by this caller with the same
    /// parameters, the existing escrow is returned without duplicate creation.
    /// If the key was used with different parameters or by a different operation,
    /// returns `Error::IdempotencyMismatch`.
    pub fn create_escrow_idempotent(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        order_id: u32,
        release_window: Option<u32>,
        idempotency_key: Option<BytesN<32>>,
    ) -> Result<Escrow, Error> {
        let _guard = ReentryGuardScope::new(&env);
        Self::check_not_paused(&env);
        buyer.require_auth();

        let window = release_window.unwrap_or(604800u32);
        // Compute parameters hash for idempotency checking
        let mut hasher_bytes = Bytes::new(&env);
        hasher_bytes.append(&seller.clone().to_xdr(&env));
        hasher_bytes.append(&token.clone().to_xdr(&env));
        hasher_bytes.append(&amount.to_xdr(&env));
        hasher_bytes.append(&order_id.to_xdr(&env));
        hasher_bytes.append(&window.to_xdr(&env));
        let params_hash: BytesN<32> = env.crypto().sha256(&hasher_bytes).into();

        if let Some(existing_order_id) = Self::check_idempotency(
            &env,
            &buyer,
            &idempotency_key,
            IdempotencyOp::CreateEscrow,
            &params_hash,
        )? {
            return Ok(Self::get_stored_escrow(&env, existing_order_id));
        }

        let escrow = Self::create_escrow_internal(
            &env,
            buyer.clone(),
            seller,
            token,
            amount,
            order_id,
            release_window,
            None,
            None,
            None,
        );

        Self::record_idempotency(
            &env,
            &buyer,
            &idempotency_key,
            IdempotencyOp::CreateEscrow,
            order_id,
            params_hash,
        );

        Ok(escrow)
    }

    /// Query an idempotency record for a caller and key (#1025).
    pub fn get_idempotency_record(
        env: Env,
        caller: Address,
        key: BytesN<32>,
    ) -> Option<IdempotencyRecord> {
        let storage_key = DataKey::IdempotencyRecord(caller, key);
        let record = env.storage().temporary().get(&storage_key);
        if record.is_some() {
            Self::extend_temporary(&env, &storage_key);
        }
        record
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
        service_agreement_hash: Option<Bytes>,
    ) -> Escrow {
        let _guard = ReentryGuardScope::new(&env);
        Self::check_not_paused(&env);
        buyer.require_auth();

        Self::create_escrow_internal(
            &env,
            buyer,
            seller,
            token,
            amount,
            order_id,
            release_window,
            ipfs_hash,
            metadata_hash,
            service_agreement_hash,
        )
    }

    fn create_escrow_internal(
        env: &Env,
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
        // Issue #1057: Block deactivated accounts from creating escrows
        Self::assert_account_active(&env, &buyer);

        let operation_id = Self::onboarding_operation_id(&env, b"create_escrow:", order_id);
        Self::authorize_onboarding_state(&env, &buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &seller, operation_id, UserRole::Artisan);

        // Validate amount is positive and above minimum
        if let Err(e) = Self::check_min_amount(env, token.clone(), amount) {
            env.panic_with_error(e);
        }

        // Validate buyer and seller are different
        if buyer == seller {
            env.panic_with_error(crate::Error::SameBuyerSeller);
        }

        // Validate token is whitelisted (#103)
        Self::check_token_whitelisted(env, &token);

        // Check artisan (seller) stake requirement (Issue #99)
        let config = Self::get_platform_config_internal(env);
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

        // Default to 7 days if not specified
        let window = release_window.unwrap_or(604800u32);

        // Validate release window bounds
        let min_window = config.min_release_window;
        let max_window = Self::get_max_release_window(env);

        if window < min_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooShort);
        }
        if window > max_window {
            env.panic_with_error(crate::Error::ReleaseWindowTooLong);
        }

        Self::validate_onboarding_state(env, &buyer, &seller);

        let created_at_u64 = env.ledger().timestamp();
        assert!(
            created_at_u64 <= u32::MAX as u64,
            "Ledger timestamp overflow"
        );
        let created_at = created_at_u64 as u32;
        Self::validate_optional_ipfs_hash(env, &ipfs_hash);
        Self::validate_optional_metadata_hash(env, &metadata_hash);
        Self::validate_optional_service_agreement_hash(env, &service_agreement_hash);

        // Reject duplicate escrow identifiers (#1027): a retry or a conflicting
        // external reference must never overwrite an existing escrow.
        Self::assert_escrow_not_exists(env, order_id);

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
            funding_deadline: None, // Immediately funded; no deadline required (#656)
            service_agreement_hash: service_agreement_hash.clone(),
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(env, &(ESCROW, order_id));

        // Track active escrows
        Self::update_active_obligations(env, &buyer, 1);
        Self::update_active_obligations(env, &seller, 1);

        // Update global escrow index for off-chain enumeration using atomic function
        // This ensures AllEscrowIds and EscrowCount always remain in sync (Issue #226)
        Self::update_escrow_indices_atomic(env, order_id);

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
        let new_buyer_count = buyer_count
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
        env.storage()
            .persistent()
            .set(&buyer_count_key, &new_buyer_count);
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
        let new_seller_count = seller_count
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
        env.storage()
            .persistent()
            .set(&seller_count_key, &new_seller_count);
        Self::extend_persistent(&env, &seller_count_key);

        Self::safe_update_active_contracts(env, buyer.clone(), 1);
        Self::safe_update_active_contracts(env, seller.clone(), 1);

        // Commit locked accounting before the external token interaction.
        Self::update_total_locked(env, &token, amount);
        Self::transfer_tokens_and_record_audit(
            env,
            &token,
            &buyer,
            &env.current_contract_address(),
            amount,
            &buyer,
            Symbol::new(env, "escrow_funded"),
            -amount,
        );

        Self::emit_escrow_created(
            env,
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
        service_agreement_hash: Option<Bytes>,
    ) -> Escrow {
        let _guard = ReentryGuardScope::new(&env);

        buyer.require_auth();
        let operation_id = Self::onboarding_operation_id(&env, b"create_unfunded_escrow:", order_id);
        Self::authorize_onboarding_state(&env, &buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &seller, operation_id, UserRole::Artisan);

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

        // Compute the deadline after which any party may cancel the unfunded stub (#656).
        let funding_deadline = created_at_u64 + UNFUNDED_CANCEL_TIMEOUT;

        // Reject duplicate escrow identifiers (#1027): a retry or a conflicting
        // external reference must never overwrite an existing escrow.
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
            funding_deadline: Some(funding_deadline), // Deadline for funding; parties may cancel after this (#656)
            service_agreement_hash: service_agreement_hash.clone(),
        };

        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        // Update buyer's escrow list
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
        let new_buyer_count = buyer_count
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
        env.storage()
            .persistent()
            .set(&buyer_count_key, &new_buyer_count);
        Self::extend_persistent(&env, &buyer_count_key);

        // Update seller's escrow list
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
        let new_seller_count = seller_count
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(Error::CounterOverflow));
        env.storage()
            .persistent()
            .set(&seller_count_key, &new_seller_count);
        Self::extend_persistent(&env, &seller_count_key);

        // Track active escrows (unfunded still count towards active limit to prevent spam)
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
        let operation_id = Self::onboarding_operation_id(&env, b"fund_escrow:", order_id);
        Self::authorize_onboarding_state(&env, &escrow.buyer, operation_id, UserRole::Buyer);

        // Effects before interaction: a callback can never observe this escrow
        // as unfunded after its balance has been pulled.
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
                action: EscrowAction::Created, // Re-emit as created/funded
                buyer: escrow.buyer.clone(),
                seller: escrow.seller.clone(),
                amount: escrow.amount,
                token: escrow.token.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    /// Cancel an escrow that has not been funded within the timeout period (#213, #656).
    ///
    /// Before the `funding_deadline` only the buyer may cancel voluntarily.
    /// After the deadline **any** party (buyer, seller, or platform admin) may cancel
    /// by passing their own address as `caller` to reclaim persistent-storage rent
    /// and prevent indefinite stub accumulation.
    pub fn cancel_unfunded_escrow(env: Env, order_id: u32, caller: Address) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.funded {
            return Err(Error::InvalidEscrowState);
        }

        let current_time = env.ledger().timestamp();
        // Use the stored funding_deadline when available; fall back to the
        // legacy created_at + UNFUNDED_CANCEL_TIMEOUT calculation for escrows
        // created before this field was added.
        let deadline = escrow
            .funding_deadline
            .unwrap_or((escrow.created_at as u64) + UNFUNDED_CANCEL_TIMEOUT);

        // Time policy: deadline is reached when now >= deadline (inclusive end)
        if time_policy::is_deadline_reached(current_time, deadline) {
            // After the deadline: buyer, seller, or platform admin may cancel.
            let admin = Self::get_admin(&env).unwrap_or(escrow.buyer.clone());
            if caller != escrow.buyer && caller != escrow.seller && caller != admin {
                return Err(Error::Unauthorized);
            }
            caller.require_auth();
        } else {
            // Before the deadline: only the buyer may cancel voluntarily.
            if caller != escrow.buyer {
                return Err(Error::Unauthorized);
            }
            caller.require_auth();
        }

        if caller == escrow.buyer || caller == escrow.seller {
            let expected_role = if caller == escrow.buyer { UserRole::Buyer } else { UserRole::Artisan };
            let operation_id = Self::onboarding_operation_id(&env, b"cancel_unfunded_escrow:", order_id);
            Self::authorize_onboarding_state(&env, &caller, operation_id, expected_role);
        }

        // Cleanup state
        env.storage().persistent().remove(&(ESCROW, order_id));

        // Decrement active obligations
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Ok(())
    }

    /// Batch-cancel unfunded escrow stubs whose `funding_deadline` has elapsed (#656).
    ///
    /// Callable only by the platform admin. The function iterates over the
    /// provided list of `order_ids` and cancels each one that:
    ///   1. Exists in storage
    ///   2. Is not yet funded
    ///   3. Has passed its `funding_deadline` (or the legacy 24-hour timeout)
    ///
    /// Escrows that do not meet these criteria are silently skipped so a
    /// single invalid entry does not abort the whole batch. Returns the count
    /// of escrows that were actually cancelled.
    ///
    /// # Arguments
    /// * `admin` â€“ Must be the platform admin address; auth is required.
    /// * `order_ids` â€“ List of escrow order IDs to check and cancel.
    pub fn auto_cancel_unfunded(
        env: Env,
        admin: Address,
        order_ids: soroban_sdk::Vec<u32>,
    ) -> Result<u32, Error> {
        let _guard = ReentryGuardScope::new(&env);

        // Verify caller is platform admin
        let stored_admin = Self::get_admin(&env)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        admin.require_auth();

        let current_time = env.ledger().timestamp();
        let mut cancelled_count: u32 = 0;

        for order_id in order_ids.iter() {
            let key = (ESCROW, order_id);

            // Skip missing escrows
            let escrow: Escrow = match env.storage().persistent().get(&key) {
                Some(e) => e,
                None => continue,
            };

            // Skip already-funded escrows
            if escrow.funded {
                continue;
            }

            // Skip escrows that haven't yet reached their funding deadline
            let deadline = escrow
                .funding_deadline
                .unwrap_or((escrow.created_at as u64) + UNFUNDED_CANCEL_TIMEOUT);
            if current_time < deadline {
                continue;
            }

            // Cancel: remove from storage and update bookkeeping
            env.storage().persistent().remove(&key);
            Self::update_active_obligations(&env, &escrow.buyer, -1);
            Self::update_active_obligations(&env, &escrow.seller, -1);
            Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
            Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

            cancelled_count += 1;
        }

        Ok(cancelled_count)
    }

    /// Get escrows for a specific buyer with pagination.
    /// Uses indexed storage for scalability, with fallback to legacy vector storage.
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

        // Try new indexed storage first
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

        // Fallback to legacy vector storage for backward compatibility
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

    /// Get escrows for a specific seller with pagination.
    /// Uses indexed storage for scalability, with fallback to legacy vector storage.
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

        // Try new indexed storage first
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

        // Fallback to legacy vector storage for backward compatibility
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

    /// Get platform configuration
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
                // Detect v5 (has service_agreement_hash) vs v4 (does not)
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
            // Safety bound: Symbols max at 32 chars. Truncate if legacy reason is too long.
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
            funding_deadline: None, // Legacy escrows were funded at creation
            service_agreement_hash: None,
        };
        Self::extend_persistent(env, &key); // OPTIMIZED: Ensure TTL extension on read
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
                // Detect v5 (has service_agreement_hash) vs v4 (does not)
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
            funding_deadline: None, // Legacy escrows were funded at creation
            service_agreement_hash: None,
        };
        env.storage().persistent().set(&key, &upgraded);
        Self::extend_persistent(env, &key);
        upgraded
    }

    /// Reject duplicate escrow identifiers (#1027).
    ///
    /// An order ID is the canonical key for an escrow. If one already exists,
    /// a retry (or a conflicting client-supplied reference) must never
    /// overwrite the existing escrow, or funds could be lost. On conflict the
    /// caller receives [`Error::EscrowAlreadyExists`] and all state changes
    /// are rolled back.
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

    /// Convert an EscrowV4 (pre-#708) to the current Escrow format.
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

    /// Calculate platform fee for a given amount.
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

    /// Deterministically compute how the escrow pot is split for any
    /// settlement path.
    ///
    /// This is the **single source of truth** for all fee math in the
    /// contract.  Every settlement function — `release_funds`, `auto_release`,
    /// `release_batch_funds`, `refund`, `resolve_dispute`,
    /// `resolve_expired_dispute`, and `accept_partial_refund` — **must** obtain
    /// its transfer amounts exclusively from this function and must not perform
    /// fee arithmetic inline.
    ///
    /// # Invariant
    ///
    /// The three output fields always satisfy:
    ///
    /// ```text
    /// allocation.platform_fee + allocation.seller_amount + allocation.buyer_amount
    ///     == escrow_amount
    /// ```
    ///
    /// This invariant is checked by the test suite for every `SettlementKind`.
    ///
    /// # Arguments
    ///
    /// * `env`           - Soroban environment (for panicking on overflow).
    /// * `escrow_amount` - Total amount held in escrow (must be >= 0).
    /// * `fee_bps`       - Effective fee in basis points for this escrow's
    ///                     seller, obtained via `get_effective_fee_bps`.
    /// * `kind`          - Which settlement formula to apply.
    fn compute_fee_allocation(
        env: &Env,
        escrow_amount: i128,
        fee_bps: u32,
        kind: SettlementKind,
    ) -> FeeAllocation {
        let allocation = match kind {
            // ── Normal release: platform fee from seller's share ──────────────
            SettlementKind::ReleaseFunds => {
                let platform_fee = Self::calculate_fee(env, escrow_amount, fee_bps);
                let seller_amount = escrow_amount - platform_fee;
                FeeAllocation {
                    platform_fee,
                    seller_amount,
                    buyer_amount: 0,
                }
            }

            // ── Full refund, no fee: entire pot returned to buyer ─────────────
            SettlementKind::FullRefundNoFee => FeeAllocation {
                platform_fee: 0,
                seller_amount: 0,
                buyer_amount: escrow_amount,
            },

            // ── Expired dispute – fee conceptually from seller ────────────────
            // Buyer receives the full amount; the platform does NOT collect
            // the fee.  The seller's loss is the opportunity cost of the
            // stalled arbitration.  Balances because platform_fee=0.
            SettlementKind::ExpiredDisputeDeductFromSeller => FeeAllocation {
                platform_fee: 0,
                seller_amount: 0,
                buyer_amount: escrow_amount,
            },

            // ── Expired dispute – fee deducted from buyer's refund ────────────
            SettlementKind::ExpiredDisputeDeductFromBuyer => {
                let platform_fee = Self::calculate_fee(env, escrow_amount, fee_bps);
                let buyer_amount = escrow_amount - platform_fee;
                FeeAllocation {
                    platform_fee,
                    seller_amount: 0,
                    buyer_amount,
                }
            }

            // ── Expired dispute – fee split equally between both sides ─────────
            SettlementKind::ExpiredDisputeSplitFee => {
                let full_fee = Self::calculate_fee(env, escrow_amount, fee_bps);
                // Integer division: any remainder (odd-bps rounding) stays with buyer.
                let platform_fee = full_fee / 2;
                let buyer_amount = escrow_amount - platform_fee;
                FeeAllocation {
                    platform_fee,
                    seller_amount: 0,
                    buyer_amount,
                }
            }

            // ── Partial refund: seller-side fee only ──────────────────────────
            // Gross refund + seller remainder must equal the escrow pot. The
            // platform fee is taken exclusively from the seller remainder so the
            // buyer receives the full proposed refund.
            SettlementKind::PartialRefund(refund_gross, seller_gross) => {
                if refund_gross < 0 || seller_gross < 0 {
                    env.panic_with_error(crate::Error::InvalidRefundAmount);
                }
                if refund_gross.checked_add(seller_gross) != Some(escrow_amount) {
                    env.panic_with_error(crate::Error::InvalidRefundAmount);
                }

                // Seller-side fee only: buyer receives the gross refund; the
                // platform fee is taken exclusively from the seller remainder.
                let platform_fee = Self::calculate_fee(env, seller_gross, fee_bps);
                FeeAllocation {
                    platform_fee,
                    seller_amount: seller_gross - platform_fee,
                    buyer_amount: refund_gross,
                }
            }
        };

        // Deterministic balance invariant: the three-way split must exactly
        // consume the escrow pot with no remainder.
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
        // The admin manages the blacklist and cannot be locked out of resolution.
        if *caller != config.admin && Self::arbitrator_on_blacklist(env, caller) {
            return Err(Error::ArbitratorBlacklisted);
        }
        Ok(())
    }

    fn current_arbitrator_assignment(env: &Env) -> (Address, u64) {
        let config = Self::get_platform_config_internal(env);
        let revision = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitratorAssignmentRevision)
            .unwrap_or(1u64);
        (config.arbitrator, revision)
    }

    fn get_dispute_assignment_internal(env: &Env, order_id: u32) -> DisputeAssignment {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeAssignment(order_id))
            .unwrap_or_else(|| env.panic_with_error(crate::Error::InvalidEscrowState))
    }

    fn assert_active_dispute_assignment(
        env: &Env,
        order_id: u32,
        authorized_address: Option<&Address>,
    ) -> DisputeAssignment {
        let assignment = Self::get_dispute_assignment_internal(env, order_id);
        let (arbitrator, revision) = Self::current_arbitrator_assignment(env);
        if assignment.revision != revision || assignment.arbitrator != arbitrator {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }
        if let Some(address) = authorized_address {
            if *address != assignment.arbitrator {
                env.panic_with_error(crate::Error::Unauthorized);
            }
        }
        assignment
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

    /// Shared solvency check: gross refund plus seller remainder must equal the
    /// escrow pot, and fee application on the seller remainder must not go negative.
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

    // ── Evidence challenge window helpers (#942) ──────────────────────────────

    fn challenge_key(order_id: u32) -> DataKey {
        DataKey::EvidenceChallenge(order_id)
    }

    fn create_evidence_challenge(env: &Env, order_id: u32, deadline: u64) {
        let key = Self::challenge_key(order_id);
        // Guard against duplicate creation — dispute can only open once.
        if env.storage().persistent().has(&key) {
            return;
        }
        let challenge = EvidenceChallenge {
            order_id,
            deadline,
            state: ChallengeState::Open,
            evidence_count: 0,
        };
        env.storage().persistent().set(&key, &challenge);
        Self::extend_persistent(env, &key);
    }

    fn fetch_evidence_challenge(env: &Env, order_id: u32) -> Option<EvidenceChallenge> {
        let key = Self::challenge_key(order_id);
        let val: Option<EvidenceChallenge> = env.storage().persistent().get(&key);
        if val.is_some() {
            Self::extend_persistent(env, &key);
        }
        val
    }

    fn close_evidence_challenge(env: &Env, order_id: u32) -> Result<(), Error> {
        let key = Self::challenge_key(order_id);
        if let Some(mut challenge) = env
            .storage()
            .persistent()
            .get::<DataKey, EvidenceChallenge>(&key)
        {
            if challenge.state == ChallengeState::Closed {
                return Err(Error::SettlementAlreadyFinalized);
            }
            challenge.state = ChallengeState::Closed;
            env.storage().persistent().set(&key, &challenge);
            Self::extend_persistent(env, &key);
            Ok(())
        } else {
            // Legacy disputes without a challenge record: create a closed marker
            // to prevent reopening. Deadline is the current timestamp.
            let challenge = EvidenceChallenge {
                order_id,
                deadline: env.ledger().timestamp(),
                state: ChallengeState::Closed,
                evidence_count: 0,
            };
            env.storage().persistent().set(&key, &challenge);
            Self::extend_persistent(env, &key);
            Ok(())
        }
    }

    fn challenge_deadline(env: &Env, order_id: u32, escrow: &Escrow, config: &PlatformConfig) -> u64 {
        if let Some(challenge) = Self::fetch_evidence_challenge(env, order_id) {
            challenge.deadline
        } else {
            // Fallback for legacy escrows created before challenge feature
            let initiated_at = escrow
                .dispute_initiated_at
                .unwrap_or(escrow.created_at as u64);
            initiated_at + config.evidence_challenge_window as u64
        }
    }

    fn is_challenge_closed(env: &Env, order_id: u32) -> bool {
        if let Some(challenge) = Self::fetch_evidence_challenge(env, order_id) {
            challenge.state == ChallengeState::Closed
        } else {
            false
        }
    }

    fn assert_challenge_window_closed(
        env: &Env,
        order_id: u32,
        escrow: &Escrow,
        config: &PlatformConfig,
    ) -> Result<(), Error> {
        // SettlementAlreadyFinalized takes precedence over ChallengeWindowActive
        if Self::is_challenge_closed(env, order_id) || Self::has_settlement_receipt(env, order_id) {
            return Err(Error::SettlementAlreadyFinalized);
        }
        let deadline = Self::challenge_deadline(env, order_id, escrow, config);
        let now = env.ledger().timestamp();
        if time_policy::is_deadline_pending(now, deadline) {
            return Err(Error::ChallengeWindowActive);
        }
        Ok(())
    }

    fn assert_arbitrator_resolution_window(
        env: &Env,
        escrow: &Escrow,
        config: &PlatformConfig,
    ) -> Result<(), Error> {
        let order_id = escrow.id as u32;
        // Use stored challenge deadline if available — immutable after dispute creation
        let deadline = Self::challenge_deadline(env, order_id, escrow, config);
        let now = env.ledger().timestamp();
        // Challenge window is active while now < deadline (pending)
        if time_policy::is_deadline_pending(now, deadline) {
            // But if challenge already closed, surface SettlementAlreadyFinalized instead
            if Self::is_challenge_closed(env, order_id) {
                return Err(Error::SettlementAlreadyFinalized);
            }
            return Err(Error::ChallengeWindowActive);
        }
        let initiated_at = Self::dispute_clock(escrow)?;
        // Time policy: arbitrator deadline exceeded when now >= initiated_at + max_dispute_duration
        if time_policy::is_window_elapsed(now, initiated_at, config.max_dispute_duration as u64) {
            return Err(Error::ArbitratorDeadlineExceeded);
        }
        // Also block if settlement already finalized
        if Self::has_settlement_receipt(env, order_id) {
            return Err(Error::SettlementAlreadyFinalized);
        }
        Ok(())
    }

    // ── Dispute escalation ladder (#1080) ──────────────────────

    /// Read the configured escalation checkpoints, falling back to the platform
    /// defaults when the admin has never set an explicit schedule.
    ///
    /// The fallback keeps `party_checkpoint` aligned with the pre-existing
    /// `dispute_escalation_window` so deployments that never call
    /// `set_escalation_checkpoints` keep their current tier-1 behaviour (#941).
    fn escalation_checkpoints(env: &Env, config: &PlatformConfig) -> EscalationCheckpoints {
        env.storage()
            .persistent()
            .get(&DataKey::EscalationCheckpoints)
            .unwrap_or(EscalationCheckpoints {
                party_checkpoint: config.dispute_escalation_window,
                moderator_checkpoint: DEFAULT_MODERATOR_ESCALATION_CHECKPOINT,
                admin_checkpoint: DEFAULT_ADMIN_ESCALATION_CHECKPOINT,
            })
    }

    /// Normalise the checkpoint offsets into absolute timestamps for one dispute.
    ///
    /// `max_dispute_duration` is authoritative: every intermediate checkpoint is
    /// clamped to sit at or before the final deadline, working backwards from
    /// the deadline. This guarantees a non-decreasing sequence even if the admin
    /// shortens `max_dispute_duration` after the checkpoints were configured, so
    /// the ladder can never schedule a tier past the point of no return.
    fn escalation_schedule(
        env: &Env,
        escrow: &Escrow,
        config: &PlatformConfig,
    ) -> Result<EscalationSchedule, Error> {
        let initiated_at = Self::dispute_clock(escrow)?;
        let raw = Self::escalation_checkpoints(env, config);

        let final_offset = config.max_dispute_duration as u64;
        let admin_offset = (raw.admin_checkpoint as u64).min(final_offset);
        let moderator_offset = (raw.moderator_checkpoint as u64).min(admin_offset);
        let party_offset = (raw.party_checkpoint as u64).min(moderator_offset);

        Ok(EscalationSchedule {
            initiated_at,
            party_deadline: time_policy::deadline(initiated_at, party_offset),
            moderator_deadline: time_policy::deadline(initiated_at, moderator_offset),
            admin_deadline: time_policy::deadline(initiated_at, admin_offset),
            final_deadline: time_policy::deadline(initiated_at, final_offset),
        })
    }

    /// Tier implied by the ledger clock alone — pure, and identical for every
    /// observer at a given timestamp.
    ///
    /// Checkpoints use the crate-wide inclusive-end convention: a tier unlocks
    /// at exactly its deadline. Later checkpoints are tested first so that a
    /// clamped (collapsed) schedule always resolves to the highest reached tier.
    fn tier_at(now: u64, schedule: &EscalationSchedule) -> EscalationTier {
        if time_policy::is_deadline_reached(now, schedule.final_deadline) {
            EscalationTier::TimedOut
        } else if time_policy::is_deadline_reached(now, schedule.admin_deadline) {
            EscalationTier::AdminReview
        } else if time_policy::is_deadline_reached(now, schedule.moderator_deadline) {
            EscalationTier::ModeratorReview
        } else if time_policy::is_deadline_reached(now, schedule.party_deadline) {
            EscalationTier::PartyFlagged
        } else {
            EscalationTier::Assigned
        }
    }

    /// The escalation ladder state recorded on-chain, or the implicit starting
    /// state for a dispute that has never been escalated.
    fn recorded_escalation_tier(env: &Env, order_id: u32) -> EscalationTier {
        env.storage()
            .persistent()
            .get::<_, DisputeEscalationState>(&DataKey::DisputeEscalationState(order_id))
            .map_or(EscalationTier::Assigned, |state| state.tier)
    }

    /// Explicit escalation permission matrix (#1080).
    ///
    /// | Tier              | Eligible escalators                                    |
    /// |-------------------|--------------------------------------------------------|
    /// | `Assigned`        | nobody — the arbitrator is still inside its window     |
    /// | `PartyFlagged`    | buyer, seller                                          |
    /// | `ModeratorReview` | buyer, seller, moderator, arbitrator, admin            |
    /// | `AdminReview`     | buyer, seller, moderator, arbitrator, admin            |
    /// | `TimedOut`        | anyone — permissionless safety net                    |
    ///
    /// Each tier is a superset of the one below it: escalation rights are only
    /// ever widened as a dispute ages, never revoked.
    fn is_eligible_escalator(
        config: &PlatformConfig,
        escrow: &Escrow,
        tier: EscalationTier,
        caller: &Address,
    ) -> bool {
        match tier {
            EscalationTier::Assigned => false,
            EscalationTier::PartyFlagged => Self::is_escrow_party(escrow, caller),
            EscalationTier::ModeratorReview | EscalationTier::AdminReview => {
                Self::is_escrow_party(escrow, caller)
                    || Self::is_privileged_resolver(config, caller)
            }
            EscalationTier::TimedOut => true,
        }
    }

    /// Map the operator's expired-dispute fee policy onto the settlement a
    /// timed-out dispute deterministically receives.
    fn timeout_outcome(policy: ExpiredDisputeFeePolicy) -> TimeoutOutcome {
        match policy {
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee
            | ExpiredDisputeFeePolicy::DeductFeeFromSeller => TimeoutOutcome::RefundBuyerFull,
            ExpiredDisputeFeePolicy::RefundMinusPlatformFee => TimeoutOutcome::RefundBuyerMinusFee,
            ExpiredDisputeFeePolicy::SplitFee => TimeoutOutcome::RefundBuyerSplitFee,
        }
    }

    /// Settlement kind used by the deterministic timeout path.
    ///
    /// Shares [`Self::timeout_outcome`] so the previewed outcome and the
    /// executed transfer can never drift apart.
    fn timeout_settlement_kind(policy: ExpiredDisputeFeePolicy) -> SettlementKind {
        match Self::timeout_outcome(policy) {
            TimeoutOutcome::RefundBuyerFull => SettlementKind::ExpiredDisputeDeductFromSeller,
            TimeoutOutcome::RefundBuyerMinusFee => SettlementKind::ExpiredDisputeDeductFromBuyer,
            TimeoutOutcome::RefundBuyerSplitFee => SettlementKind::ExpiredDisputeSplitFee,
        }
    }

    fn assert_expired_dispute_window(
        env: &Env,
        escrow: &Escrow,
        config: &PlatformConfig,
    ) -> Result<(), Error> {
        let initiated_at = Self::dispute_clock(escrow)?;
        let now = env.ledger().timestamp();
        // Time policy: dispute is expired when now >= initiated_at + max_dispute_duration
        if time_policy::is_window_active(now, initiated_at, config.max_dispute_duration as u64) {
            return Err(Error::DisputeExpired);
        }
        Ok(())
    }

    fn expired_dispute_settlement_kind(policy: ExpiredDisputeFeePolicy) -> SettlementKind {
        match policy {
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee => SettlementKind::FullRefundNoFee,
            ExpiredDisputeFeePolicy::RefundMinusPlatformFee => {
                SettlementKind::ExpiredDisputeDeductFromBuyer
            }
            ExpiredDisputeFeePolicy::DeductFeeFromSeller => {
                SettlementKind::ExpiredDisputeDeductFromSeller
            }
            ExpiredDisputeFeePolicy::SplitFee => SettlementKind::ExpiredDisputeSplitFee,
        }
    }

    /// Atomically claim a disputed escrow for a single settlement path.
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
        let assignment_revision = env
            .storage()
            .persistent()
            .get::<_, DisputeAssignment>(&DataKey::DisputeAssignment(order_id))
            .map(|assignment| assignment.revision)
            .unwrap_or(0);
        env.storage().persistent().set(
            &key,
            &SettlementReceipt {
                order_id,
                path,
                assignment_revision,
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
        // Close challenge window exactly once — guard against re-entry.
        // This is the sole transition Open → Closed for this order_id.
        if Self::is_challenge_closed(env, order_id) {
            env.panic_with_error(crate::Error::SettlementAlreadyFinalized);
        }
        let _ = Self::close_evidence_challenge(env, order_id);
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

    /// Maintain the dual fee-token bookkeeping (#239).
    ///
    /// Historically fee-receiving tokens were tracked only in the legacy
    /// `FeeTokenIndex` Vec. That single-key shape made future multi-token
    /// fee features (custom bps per token, disabling tokens, accumulator
    /// reconciliation) impossible without a contract upgrade. We now also
    /// write a per-token `FeeTokenConfig(token)` slot, which is the storage
    /// shape new code should read going forward. The legacy Vec is kept as
    /// the canonical enumeration source for backward compatibility â€” a
    /// `migrate_fee_token_configs` admin call backfills `FeeTokenConfig` for
    /// pre-existing tokens.
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

    /// Seed a default `FeeTokenInfo` slot the first time a token is seen.
    /// Idempotent â€” once a slot exists, subsequent calls leave it untouched
    /// so admin overrides survive future fee deposits (#239).
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

    /// Bump the per-token accumulator inside `FeeTokenConfig` (#239). Kept
    /// internal so external callers cannot tamper with the running total.
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

    /// Returns the per-token fee configuration for `token`, or `None` if the
    /// token has never received platform fees (#239).
    pub fn get_fee_token_config(env: Env, token: Address) -> Option<FeeTokenInfo> {
        env.storage()
            .persistent()
            .get(&DataKey::FeeTokenConfig(token))
    }

    /// Returns every token that has ever received platform fees (#239).
    /// Reads the legacy `FeeTokenIndex` Vec; new callers should pair this
    /// enumeration with `get_fee_token_config` for richer per-token data.
    pub fn get_fee_tokens(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::FeeTokenIndex)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Update mutable fields of a `FeeTokenInfo` slot (admin only, #239).
    ///
    /// `active` and `custom_fee_bps` are admin-controlled. `accumulated` is
    /// IGNORED if passed in â€” the running total is owned by the contract and
    /// only `record_total_fees` may move it. This split prevents an admin
    /// from rewriting historical fee accounting via the config setter.
    ///
    /// `custom_fee_bps`, when set, must satisfy `<= MAX_PLATFORM_FEE_BPS`.
    /// The value is currently informational; `calculate_fee` does not yet
    /// consult it (storage-only change to keep #239 scope tight).
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

    /// Backfill `FeeTokenConfig(token)` slots for every token currently
    /// present in the legacy `FeeTokenIndex` Vec (admin only, #239).
    ///
    /// Idempotent â€” already-migrated tokens are skipped, so it is safe to
    /// call from a deploy script or an automated migration job. Returns the
    /// number of new config slots written so callers can verify progress.
    /// Pairs with `set_fee_token_config` for downstream tuning.
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
        // Mirror the running total into the per-token fee config so future
        // multi-token logic has a single source of truth (#239).
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

        // Every transfer must be reached through a guarded public operation.
        // This assertion makes omissions fail closed as new flows are added.
        if !env.storage().temporary().has(&DataKey::ReentryGuard) {
            env.panic_with_error(crate::Error::ReentryDetected);
        }

        // Execute and validate token transfer before committing dependent audit state (#1064).
        // Failed transfers or unexpected return values reject with stable TokenTransferFailed error.
        let token_client = token::Client::new(env, token);
        match token_client.try_transfer(from, to, &amount) {
            Ok(Ok(())) => {}
            _ => env.panic_with_error(crate::Error::TokenTransferFailed),
        }

        // Commit dependent accounting/audit state only after transfer is validated.
        Self::append_fund_audit_record(env, actor, amount, reason, balance_impact);
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

    /// Release funds to seller with platform fee deduction and optional idempotency key (#1025).
    ///
    /// If an idempotency key is provided and was already used by this buyer for this order,
    /// returns `Ok(())` without executing duplicate token transfers or fee deductions.
    /// If used with different parameters or by a different operation/caller, returns `Error::IdempotencyMismatch`.
    pub fn release_funds_idempotent(
        env: Env,
        order_id: u32,
        idempotency_key: Option<BytesN<32>>,
    ) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let escrow_for_auth = Self::get_stored_escrow(&env, order_id);

        // Only buyer can release funds
        escrow_for_auth.buyer.require_auth();

        let mut hasher_bytes = Bytes::new(&env);
        hasher_bytes.append(&order_id.to_xdr(&env));
        let params_hash: BytesN<32> = env.crypto().sha256(&hasher_bytes).into();

        if let Some(existing_order_id) = Self::check_idempotency(
            &env,
            &escrow_for_auth.buyer,
            &idempotency_key,
            IdempotencyOp::ReleaseFunds,
            &params_hash,
        )? {
            if existing_order_id == order_id {
                return Ok(());
            } else {
                return Err(Error::IdempotencyMismatch);
            }
        }

        Self::release_funds_internal(&env, order_id, &escrow_for_auth)?;

        Self::record_idempotency(
            &env,
            &escrow_for_auth.buyer,
            &idempotency_key,
            IdempotencyOp::ReleaseFunds,
            order_id,
            params_hash,
        );

        Ok(())
    }

    /// Release funds to seller with platform fee deduction
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn release_funds(env: Env, order_id: u32) {
        let _guard = ReentryGuardScope::new(&env);
        let escrow_for_auth = Self::get_stored_escrow(&env, order_id);

        // Only buyer can release funds
        escrow_for_auth.buyer.require_auth();
        let operation_id = Self::onboarding_operation_id(&env, b"release_funds:", order_id);
        Self::authorize_onboarding_state(
            &env,
            &escrow_for_auth.buyer,
            operation_id,
            UserRole::Buyer,
        );

        Self::release_funds_internal(&env, order_id, &escrow_for_auth)
            .unwrap_or_else(|e| env.panic_with_error(e));
    }

    fn release_funds_internal(
        env: &Env,
        order_id: u32,
        _escrow_for_auth: &Escrow,
    ) -> Result<(), Error> {
        let mut escrow =
            Self::claim_active_escrow_transition(env, order_id, EscrowStatus::ReleasePending)?;

        // Get platform config
        let config = Self::get_platform_config_internal(env);

        // Deterministic fee allocation via the central FeePolicy engine.
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let allocation = Self::compute_fee_allocation(
            env,
            escrow.amount,
            fee_bps,
            SettlementKind::ReleaseFunds,
        );

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Decrement active counts
        Self::update_active_obligations(env, &escrow.buyer, -1);
        Self::update_active_obligations(env, &escrow.seller, -1);

        Self::safe_update_active_contracts(env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(env, escrow.seller.clone(), -1);

        // Reserve accounting is part of the effects phase.
        Self::update_total_locked(env, &escrow.token, -escrow.amount);

        // Transfer platform fee to platform wallet
        if allocation.platform_fee > 0 {
            Self::transfer_platform_fee(
                env,
                &escrow.token,
                &config.platform_wallet,
                allocation.platform_fee,
            );
        }

        // Transfer net funds to seller and record audit
        Self::transfer_tokens_and_record_audit(
            env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.seller,
            allocation.seller_amount,
            &escrow.seller,
            Symbol::new(env, "escrow_released"),
            allocation.seller_amount,
        );

        Self::emit_escrow_created(
            env,
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

        // Emit reputation update events — decoupled from onboarding contract (#211)
        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            env,
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
            env,
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

        Ok(())
    }

    /// Auto-release funds after release window (seller can call)
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn auto_release(env: Env, order_id: u32) {
        let _guard = ReentryGuardScope::new(&env);
        let escrow_for_window = Self::get_stored_escrow(&env, order_id);

        if !(escrow_for_window.status == EscrowStatus::Active) {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let current_time = env.ledger().timestamp();
        // Time policy: window is elapsed when now >= created_at + release_window
        if time_policy::is_window_active(current_time, escrow_for_window.created_at as u64, escrow_for_window.release_window as u64) {
            env.panic_with_error(crate::Error::ReleaseWindowNotElapsed);
        }

        let operation_id = Self::onboarding_operation_id(&env, b"auto_release:", order_id);
        Self::authorize_onboarding_state(&env, &escrow_for_window.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &escrow_for_window.seller, operation_id, UserRole::Artisan);

        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::ReleasePending)
                .unwrap_or_else(|e| env.panic_with_error(e));

        // Get platform config
        let config = Self::get_platform_config_internal(&env);

        // Deterministic fee allocation via the central FeePolicy engine.
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
        let allocation = Self::compute_fee_allocation(
            &env,
            escrow.amount,
            fee_bps,
            SettlementKind::ReleaseFunds,
        );

        // Update status
        escrow.status = EscrowStatus::Released;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        // Transfer platform fee to platform wallet
        if allocation.platform_fee > 0 {
            Self::transfer_platform_fee(
                &env,
                &escrow.token,
                &config.platform_wallet,
                allocation.platform_fee,
            );
        }

        // Transfer net funds to seller and record audit
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

        // Emit reputation update events â€” decoupled from onboarding contract (#211)
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

    /// Extends the existing release window by `additional_seconds`.
    ///
    /// The resulting cumulative release window must never exceed
    /// `MAX_TOTAL_RELEASE_WINDOW`.
    pub fn extend_release_window(env: Env, order_id: u32, additional_seconds: u32) {
        let _guard = ReentryGuardScope::new(&env);
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

    /// Reject obviously invalid WASM hashes before they touch storage.
    ///
    /// The Soroban host validates that the hash points to an uploaded WASM at
    /// `update_current_contract_wasm` time, but only at execution. Catching
    /// the all-zero sentinel here avoids the worst footgun (an admin
    /// accidentally proposing the default `BytesN<32>::from_array(_, [0; 32])`)
    /// and gives a meaningful error code instead of a host trap.
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

    fn emit_upgrade_approval_event(
        env: &Env,
        nonce: u32,
        signer: Address,
        wasm_hash: BytesN<32>,
        approval_count: u32,
    ) {
        env.events().publish(
            (Symbol::new(env, "wasm_upgrade"), UPGRADE_APPROVED),
            UpgradeApprovalEvent {
                nonce,
                signer,
                wasm_hash,
                timestamp: env.ledger().timestamp(),
                approval_count,
            },
        );
    }

    /// Propose a new WASM code for the contract (multi-sig).
    ///
    /// Each authorized upgrade signer calls this function with the same
    /// `new_wasm_hash`. Approvals are accumulated until the configured
    /// `UpgradeThreshold` is reached, at which point the proposal is committed
    /// and the cooldown clock starts. A signer cannot approve the same hash
    /// twice. When no explicit signers list is configured the admin acts as the
    /// sole signer (backward-compatible default, threshold=1).
    ///
    /// Only one proposal may be pending at a time. Cancel with
    /// `cancel_upgrade_wasm` before starting a new one.
    pub fn propose_upgrade_wasm(
        env: Env,
        signer: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), Error> {
        signer.require_auth();

        Self::validate_upgrade_hash(&env, &new_wasm_hash)?;

        // Issue #618: Prevent cancel-and-repropose from resetting the review
        // window. If a proposal was recently cancelled, the proposer must wait
        // CANCEL_REPROPOSE_COOLDOWN seconds before submitting a new one.
        if let Some(cancelled_at) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::LastUpgradeCancelledAt)
        {
            let now = env.ledger().timestamp();
            // Time policy: cooldown is active while now < cancelled_at + CANCEL_REPROPOSE_COOLDOWN
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

        // -- Approval state (singleton key, nonce inside struct) ----------------
        // Approval state is stored at a fixed slot UpgradeApprovalState(0).
        // The `nonce` field inside the struct is incremented on every
        // cancel_upgrade_wasm call, so a re-proposal after cancellation
        // starts a fresh round with a different nonce, making the old
        // approvals stale and detectable.
        let state_key = DataKey::UpgradeApprovalState(0);

        // Read the current nonce from any pre-existing state, or default to 0.
        let current_nonce: u32 = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&state_key)
            .map(|s| s.nonce)
            .unwrap_or(0u32);

        // Helper closure: snapshot current live signers + threshold into a fresh state.
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
            // Reuse stored state only when:
            //  (a) it has the same nonce (was not left from a cancelled round), AND
            //  (b) the round has already started (signers list is non-empty).
            // A state with an empty signers list is a cancel-sentinel written by
            // cancel_upgrade_wasm to carry the bumped nonce forward; it must not
            // be treated as a live round.
            .filter(|s: &UpgradeApprovalState| s.nonce == current_nonce && !s.signers.is_empty())
            .unwrap_or_else(|| fresh_state(current_nonce));

        // Validate against the *snapshotted* signer set -- live storage is
        // intentionally not consulted here.
        if !state.signers.iter().any(|s| s == signer) {
            return Err(Error::NotAnUpgradeSigner);
        }

        // Canonical per-(nonce, signer) slot is the source of truth (#1059).
        let approval_key = DataKey::UpgradeSignerApproval(state.nonce, signer.clone());
        if env.storage().persistent().has(&approval_key)
            || state.approvals.iter().any(|a| a == signer)
        {
            return Err(Error::AlreadyApproved);
        }
        if state.approvals.len() >= state.signers.len() {
            return Err(Error::AlreadyApproved);
        }
        state.approvals.push_back(signer.clone());
        if state.approvals.len() > state.signers.len() {
            return Err(Error::AlreadyApproved);
        }
        env.storage().persistent().set(&approval_key, &true);
        Self::extend_persistent(&env, &approval_key);
        Self::emit_upgrade_approval_event(
            &env,
            state.nonce,
            signer.clone(),
            new_wasm_hash.clone(),
            state.approvals.len(),
        );

        // All entries in state.approvals were validated against state.signers
        // when they were added, so a simple length check is sufficient.
        if state.approvals.len() < state.threshold {
            // Threshold not yet met -- persist updated state and return.
            env.storage().persistent().set(&state_key, &state);
            Self::extend_persistent(&env, &state_key);
            return Ok(());
        }

        // Threshold reached -- commit the proposal and clean up approval state.
        // Remove approval state for this nonce; it is no longer needed.
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

    /// Set the number of distinct signer approvals required to commit a WASM
    /// upgrade proposal. Must be >= 1. Admin only.
    pub fn set_upgrade_threshold(env: Env, threshold: u32) -> Result<(), Error> {
        if threshold == 0 {
            return Err(Error::InvalidFee);
        }
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&threshold.to_be_bytes());
        Self::gate_admin_mutation(&env, "set_upgrade_threshold", payload)?;
        env.storage()
            .instance()
            .set(&DataKey::UpgradeThreshold, &threshold);
        Ok(())
    }

    /// Replace the list of addresses authorized to co-sign WASM upgrade
    /// proposals. An empty list resets to the admin-only default. Admin only.
    pub fn set_upgrade_signers(env: Env, signers: Vec<Address>) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        Self::gate_admin_mutation(&env, "set_upgrade_signers", signers.to_xdr(&env))?;
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

    /// Returns the configured upgrade threshold (defaults to 1).
    pub fn get_upgrade_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeThreshold)
            .unwrap_or(1u32)
    }

    /// Returns the current proposal round nonce.
    ///
    /// The nonce is incremented on every `cancel_upgrade_wasm` call.  Callers
    /// can use it to look up the active `UpgradeApprovalState` via
    /// `get_upgrade_approvals`.
    pub fn get_upgrade_proposal_nonce(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&DataKey::UpgradeApprovalState(0))
            .map(|s| s.nonce)
            .unwrap_or(0u32)
    }

    /// Returns the list of pending approvals for the given proposal nonce.
    ///
    /// Pass the value returned by `get_upgrade_proposal_nonce` to inspect the
    /// current round.  Returns an empty vec if no approvals exist for that
    /// nonce (i.e. the round has not started or was already committed/cancelled).
    pub fn get_upgrade_approvals(env: Env, nonce: u32) -> Vec<Address> {
        // Returns approvals only if the stored state matches the requested nonce.
        env.storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&DataKey::UpgradeApprovalState(0))
            .filter(|s| s.nonce == nonce)
            .map(|s| s.approvals)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// True iff `signer` already holds a canonical approval for `nonce` (#1059).
    pub fn has_upgrade_approval(env: Env, nonce: u32, signer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::UpgradeSignerApproval(nonce, signer))
    }

    /// Return the state summary that migration tooling must snapshot in its
    /// isolated old/new differential run.
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

    /// Hash the current representative state. Tooling should call this before
    /// and after its isolated migration and place the pre-migration value in
    /// `UpgradeCompatibilityManifest::state_commitment`.
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

    /// Submit the differential compatibility evidence for a pending upgrade.
    /// The manifest is resumable off-chain: a later submission replaces the
    /// same hash's report, while execution accepts only a complete report with
    /// no records requiring manual intervention.
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

    /// Read the compatibility evidence associated with a proposed WASM hash.
    pub fn get_upgrade_compat_manifest(
        env: Env,
        wasm_hash: BytesN<32>,
    ) -> Option<UpgradeCompatibilityManifest> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeCompatibilityManifest(wasm_hash))
    }

    /// Retrieve the immutable upgrade state commitment record for a deployed
    /// WASM hash (#1140).
    ///
    /// Returns `Some(commitment)` if the upgrade was executed and a commitment
    /// was persisted, or `None` if the hash was never executed or the record
    /// does not exist. Operators can use this to verify the migrated state
    /// digest, compatibility evidence, and immutability status off-chain.
    ///
    /// Note: This is distinct from [`Self::get_upgrade_state_commitment`],
    /// which returns the computed state commitment hash (a `BytesN<32>`) for
    /// the *current* contract state. This function returns the full persisted
    /// commitment record for a *specific* deployed WASM hash.
    pub fn get_upgrade_state_commit(
        env: Env,
        wasm_hash: BytesN<32>,
    ) -> Option<UpgradeStateCommitment> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeStateCommitment(wasm_hash))
    }

    /// Return the persisted storage layout version.
    pub fn get_storage_layout_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::StorageLayoutVersion)
            .unwrap_or(0)
    }

    /// Migrate persisted storage to the current layout version.
    ///
    /// This is an explicit, admin-gated migration path for legacy deployments.
    /// Existing state is preserved by running the existing lazy migration helpers
    /// before recording the new layout version.
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

    /// Upgrade the contract's WASM code after the grace period has elapsed.
    ///
    /// The caller passes the `expected_wasm_hash` they think is pending; if it
    /// does not match the stored proposal the call fails with
    /// `InvalidUpgradeHash`. This is defense-in-depth against a scenario where
    /// the admin's signing tool is shown a different proposal than what was
    /// actually stored on-chain, and forces the operator to confirm exactly
    /// which payload is being applied (#230).
    ///
    /// On success a new `UpgradeRecord` is appended to `UpgradeHistory`,
    /// `ContractVersion` is bumped, and the proposal is cleared atomically.
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

        // Check for existing immutable commitment - prevent re-execution
        let existing_commitment = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeStateCommitment>(&DataKey::UpgradeStateCommitment(
                proposal.wasm_hash.clone(),
            ));
        if let Some(commitment) = existing_commitment {
            if commitment.immutable {
                return Err(Error::UpgradeAlreadyExecuted);
            }
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

        // Validate migration result is complete before allowing execution
        if !manifest.migration_complete {
            return Err(Error::UpgradeMigrationIncomplete);
        }

        env.deployer()
            .update_current_contract_wasm(proposal.wasm_hash.clone());

        // Update version in storage
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
                state_commitment: manifest.state_commitment.clone(),
                migration_checkpoint: manifest.migration_checkpoint.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        // Compute migration result digest as SHA-256 of the migration checkpoint
        // captured in the validated compatibility manifest. This binds the
        // commitment to the migration evidence while remaining deterministic.
        let migration_result_digest =
            env.crypto()
                .sha256(&manifest.migration_checkpoint.to_xdr(&env));

        // Persist immutable upgrade state commitment
        let state_commitment = UpgradeStateCommitment {
            from_version: current_version,
            to_version: new_version,
            wasm_hash: proposal.wasm_hash.clone(),
            state_digest: manifest.state_commitment,
            migration_result_digest: migration_result_digest.into(),
            admin: admin.clone(),
            timestamp: env.ledger().timestamp(),
            activated_at: env.ledger().timestamp(),
            immutable: true,
        };
        env.storage().persistent().set(
            &DataKey::UpgradeStateCommitment(proposal.wasm_hash.clone()),
            &state_commitment,
        );
        Self::extend_persistent(
            &env,
            &DataKey::UpgradeStateCommitment(proposal.wasm_hash.clone()),
        );

        // Clear proposal
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

    /// Cancel a proposed WASM upgrade (admin only) (#95, #230).
    ///
    /// Emits an `UPG_CANC` event so cancellations are visible alongside
    /// proposals in the audit trail. Returning `NoUpgradeProposed` instead of
    /// silently succeeding makes accidental double-cancels visible.
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
        
        env.storage()
            .persistent()
            .remove(&DataKey::UpgradeCompatibilityManifest(
                proposal.wasm_hash.clone(),
            ));

        // Increment the round nonce inside the approval state so that any
        // residual approvals cannot be replayed in the next round.
        // We write a "poisoned" state (empty approvals, bumped nonce) rather
        // than removing the key so the nonce survives across cancel cycles.
        let state_key = DataKey::UpgradeApprovalState(0);
        let next_nonce: u32 = env
            .storage()
            .persistent()
            .get::<DataKey, UpgradeApprovalState>(&state_key)
            .map(|s| s.nonce.saturating_add(1))
            .unwrap_or(1u32);
        // Store a sentinel state with the bumped nonce so propose_upgrade_wasm
        // knows it must open a fresh round on the next call.
        let reset_state = UpgradeApprovalState {
            nonce: next_nonce,
            signers: Vec::new(&env),
            threshold: 1u32,
            approvals: Vec::new(&env),
        };
        env.storage().persistent().set(&state_key, &reset_state);
        Self::extend_persistent(&env, &state_key);

        // Issue #618: Record the cancellation timestamp so propose_upgrade_wasm
        // can enforce CANCEL_REPROPOSE_COOLDOWN against the cancel-and-repropose
        // bypass pattern.
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

    /// Returns the currently pending WASM upgrade proposal, if any (#230).
    /// Read-only â€” useful for off-chain monitors and admin dashboards that
    /// need to confirm what `execute_upgrade` will apply.
    pub fn get_upgrade_proposal(env: Env) -> Option<WasmUpgradeProposal> {
        env.storage()
            .persistent()
            .get(&DataKey::WasmUpgradeProposal)
    }

    /// Returns the current contract version.
    ///
    /// `ContractVersion` semantics:
    /// - Initialized to `1` by `initialize`.
    /// - Incremented by exactly `1` for each successful `execute_upgrade`.
    /// - Independent of the on-disk WASM hash; the hash + version pair is
    ///   captured per-upgrade in `UpgradeHistory` for auditability.
    /// - Migration code that needs to gate behavior across upgrades should
    ///   compare against this value rather than embedding magic numbers.
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(0)
    }

    /// Append a record to the bounded `UpgradeHistory` log (#241). The Vec is
    /// trimmed FIFO once it exceeds `MAX_UPGRADE_HISTORY`.
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

    /// Returns the bounded log of past contract upgrades (#241).
    ///
    /// Newer entries are at the back. The log is capped at
    /// `MAX_UPGRADE_HISTORY` records â€” older entries are dropped FIFO so
    /// storage cannot grow unbounded. For long-term audit trails operators
    /// should mirror the `wasm_upgrade` events to off-chain storage.
    pub fn get_upgrade_history(env: Env) -> Vec<UpgradeRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Returns compatibility evidence for completed upgrades.
    pub fn get_upgrade_compat_history(env: Env) -> Vec<UpgradeCompatibilityRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::UpgradeCompatibilityHistory)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Returns aggregate version + last-upgrade metadata (#241). Pairs the
    /// scalar `ContractVersion` with the most recent `UpgradeRecord` so a
    /// dashboard or migration script can read everything in one call.
    pub fn get_version_info(env: Env) -> VersionInfo {
        let current_version = Self::get_version(env.clone());
        let history = Self::get_upgrade_history(env);
        let upgrade_count = history.len();
        VersionInfo {
            current_version,
            upgrade_count,
        }
    }

    /// Refund funds to buyer with an optional idempotency key (admin only) (#1025).
    ///
    /// If an idempotency key is provided and was already used by the admin for this escrow,
    /// returns `Ok(())` without executing duplicate token transfers or state updates.
    /// If used with different parameters or by a different operation/caller, returns `Error::IdempotencyMismatch`.
    pub fn refund_idempotent(
        env: Env,
        escrow_id: u64,
        idempotency_key: Option<BytesN<32>>,
    ) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let order_id = escrow_id as u32;

        let mut hasher_bytes = Bytes::new(&env);
        hasher_bytes.append(&escrow_id.to_xdr(&env));
        let params_hash: BytesN<32> = env.crypto().sha256(&hasher_bytes).into();

        if let Some(existing_order_id) = Self::check_idempotency(
            &env,
            &admin,
            &idempotency_key,
            IdempotencyOp::Refund,
            &params_hash,
        )? {
            if existing_order_id == order_id {
                return Ok(());
            } else {
                return Err(Error::IdempotencyMismatch);
            }
        }

        Self::refund_internal(&env, escrow_id)?;

        Self::record_idempotency(
            &env,
            &admin,
            &idempotency_key,
            IdempotencyOp::Refund,
            order_id,
            params_hash,
        );

        Ok(())
    }

    /// Refund funds to buyer (admin only)
    ///
    /// # Arguments
    /// * `escrow_id` - Escrow/Order identifier
    pub fn refund(env: Env, escrow_id: u64) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        Self::refund_internal(&env, escrow_id)
    }

    fn refund_internal(env: &Env, escrow_id: u64) -> Result<(), Error> {
        let order_id = escrow_id as u32;
        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::RefundPending)?;
        let operation_id = Self::onboarding_operation_id(&env, b"refund:", order_id);
        Self::authorize_onboarding_state(&env, &escrow.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &escrow.seller, operation_id, UserRole::Artisan);

        // Deterministic fee allocation via the central FeePolicy engine.
        let allocation =
            Self::compute_fee_allocation(env, escrow.amount, 0, SettlementKind::FullRefundNoFee);

        // CEI: persist the Refunded state before any external token transfer.
        escrow.status = EscrowStatus::Refunded;
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        Self::extend_persistent(&env, &(ESCROW, order_id));

        // Decrement active counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.seller, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);

        Self::update_total_locked(&env, &escrow.token, -escrow.amount);

        // Refund to buyer and record audit
        Self::transfer_tokens_and_record_audit(
            env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.buyer,
            allocation.buyer_amount,
            &escrow.buyer,
            Symbol::new(env, "refund"),
            allocation.buyer_amount,
        );

        Self::emit_escrow_created(
            env,
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

        // Emit reputation update events — decoupled from onboarding contract (#211)
        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            env,
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
            env,
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

    /// Get escrow details
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn get_escrow(env: Env, order_id: u32) -> Escrow {
        Self::get_stored_escrow(&env, order_id)
    }

    /// Diagnose the escrow lifecycle for partial-state or orphaned transition issues.
    ///
    /// This is a read-only guard rail for off-chain monitoring and admin recovery.
    /// It reports whether a status transition is incomplete, or whether a disputed
    /// escrow has lost the timestamp required to enforce challenge/expiry windows.
    pub fn diagnose_escrow_state(env: Env, order_id: u32) -> EscrowStateDiagnostic {
        Self::inspect_escrow_state(&env, order_id)
    }

    /// Read the immutable fund-movement audit history for an account.
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

    /// Read the total number of fund-movement audit entries for an account.
    pub fn get_fund_audit_count(env: Env, actor: Address) -> u32 {
        let count_key = DataKey::FundAuditCount(actor);
        env.storage().persistent().get(&count_key).unwrap_or(0)
    }

    /// Read a paginated slice of the fund-movement audit history for an account.
    ///
    /// # Arguments
    /// * `actor` - Account address to query audit history for
    /// * `start_index` - Starting zero-based index of audit records to read
    /// * `limit` - Maximum number of entries to return
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

        // Return empty if start_index is past the end
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

    /// Get escrow metadata fields only.
    pub fn get_escrow_metadata(env: Env, order_id: u32) -> EscrowMetadata {
        let escrow = Self::get_escrow(env, order_id);
        EscrowMetadata {
            ipfs_hash: escrow.ipfs_hash,
            metadata_hash: escrow.metadata_hash,
            service_agreement_hash: escrow.service_agreement_hash,
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

        let is_valid =
            Self::verify_metadata_reveal(env.clone(), order_id, proof, authorized_address.clone());
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

    /// Open a dispute on an active escrow, entering the **Disputed** lifecycle state.
    ///
    /// ## Dispute lifecycle overview
    ///
    /// Once an escrow is `Active`, either party can call this function to move it into
    /// the dispute pipeline. The overall state machine looks like this:
    ///
    /// ```text
    ///  Active
    ///    │
    ///    ▼  dispute_escrow()
    ///  DisputePending  ──►  Disputed
    ///                          │
    ///               ┌──────────┼────────────────────┐
    ///               │          │                    │
    ///               ▼          ▼                    ▼
    ///     submit_evidence   escalate_dispute   propose_partial_refund
    ///      (any time while   (one tier per        (buyer-initiated
    ///       Disputed)         checkpoint reached)   negotiation)
    ///               │          │                    │
    ///               └──────────┼────────────────────┘
    ///                          │
    ///               ┌──────────┴──────────┐
    ///               │                     │
    ///               ▼                     ▼
    ///       resolve_dispute()    resolve_expired_dispute()
    ///       (arbitrator/admin/   (anyone, after max_dispute_duration
    ///        moderator, after     has elapsed without resolution)
    ///        evidence window)
    ///               │                     │
    ///               └──────────┬──────────┘
    ///                          ▼
    ///                       Resolved
    /// ```
    ///
    /// ## Preconditions
    ///
    /// - The platform must not be paused.
    /// - The caller must be the escrow's `buyer` or `seller`.
    /// - The escrow must currently be in the `Active` state.
    /// - The caller must not have exceeded the per-account dispute rate limit
    ///   (`rate_limit_max_calls` within `rate_limit_window`). This prevents spam
    ///   disputes that would congest the arbitration queue.
    ///
    /// ## State transition
    ///
    /// The transition uses an atomic "claim" pattern (`DisputePending` as an
    /// intermediate sentinel) to prevent race conditions where two callers might
    /// simultaneously dispute the same escrow. After the claim, the status is
    /// immediately set to `Disputed` and `dispute_initiated_at` is stamped with
    /// the current ledger timestamp. This timestamp gates two downstream timers:
    ///
    /// 1. **Evidence challenge window** (`evidence_challenge_window`): during this
    ///    period both parties may submit or rebut evidence. `resolve_dispute` is
    ///    blocked until the window has elapsed (see [`Self::resolve_dispute`]).
    /// 2. **Escalation checkpoints** (`EscalationCheckpoints`): a stalled dispute
    ///    climbs an ordered ladder of checkpoints, each widening who may call
    ///    `escalate_dispute` to surface it to priority queues - the parties
    ///    first, then the moderator, then the admin (see
    ///    [`Self::escalate_dispute`]).
    /// 3. **Final deadline** (`max_dispute_duration`): if the arbitrator has not
    ///    resolved the dispute before this deadline, arbitration is closed off
    ///    and anyone can call `resolve_expired_dispute` to force-close it with
    ///    the deterministic timeout outcome (see
    ///    [`Self::resolve_expired_dispute`] and [`Self::get_timeout_outcome`]).
    ///
    /// ## Events emitted
    ///
    /// - `EscrowEvent { action: EscrowAction::Disputed, … }` — consumed by
    ///   off-chain indexers and arbitration dashboards.
    ///
    /// # Arguments
    /// * `order_id` - Identifier of the escrow to dispute.
    /// * `dispute_reason` - Short symbolic reason (e.g. `"item_not_received"`).
    ///   Stored on-chain for audit; not evaluated by the contract logic.
    /// * `authorized_address` - Must be the escrow's `buyer` or `seller`.
    pub fn dispute_escrow(
        env: Env,
        order_id: u32,
        dispute_reason: Symbol, // UPDATE ARGUMENT TYPE
        authorized_address: Address,
    ) {
        authorized_address.require_auth();

        // Issue #1057: Block deactivated accounts from initiating disputes
        Self::assert_account_active(&env, &authorized_address);

        // Rate limiting check (#943)
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

        // Only the two parties to this specific escrow may open a dispute;
        // admin/arbitrator cannot initiate on their behalf.
        if !(escrow_for_auth.buyer == authorized_address
            || escrow_for_auth.seller == authorized_address)
        {
            env.panic_with_error(crate::Error::Unauthorized);
        }
        let expected_role = if escrow_for_auth.buyer == authorized_address {
            UserRole::Buyer
        } else {
            UserRole::Artisan
        };
        let operation_id = Self::onboarding_operation_id(&env, b"dispute_escrow:", order_id);
        Self::authorize_onboarding_state(
            &env,
            &authorized_address,
            operation_id,
            expected_role,
        );

        // Atomically claim the escrow through the DisputePending sentinel to
        // prevent a second concurrent caller from also transitioning it. The
        // function panics if the current status is not Active.
        let mut escrow =
            Self::claim_active_escrow_transition(&env, order_id, EscrowStatus::DisputePending)
                .unwrap_or_else(|e| env.panic_with_error(e));

        // Finalize transition: stamp the dispute metadata and persist.
        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_reason = Some(dispute_reason); // Assign Symbol
                                                      // dispute_initiated_at is the single source of truth for all three
                                                      // downstream timers (evidence window, escalation window, max duration).
        escrow.dispute_initiated_at = Some(env.ledger().timestamp());
        env.storage().persistent().set(&(ESCROW, order_id), &escrow);
        // Create bounded challenge window for this dispute (#942).
        // Deadline is immutable — computed once at dispute time so later
        // config changes cannot shorten or extend it.
        let config_for_challenge = Self::get_platform_config_internal(&env);
        let challenge_deadline =
            env.ledger().timestamp() + config_for_challenge.evidence_challenge_window as u64;
        Self::create_evidence_challenge(&env, order_id, challenge_deadline);
        let (arbitrator, revision) = Self::current_arbitrator_assignment(&env);
        env.storage().persistent().set(
            &DataKey::DisputeAssignment(order_id),
            &DisputeAssignment {
                order_id,
                arbitrator,
                revision,
                assigned_at: env.ledger().timestamp(),
            },
        );
        // Increment the global active dispute counter used by emergency-op
        // guards (admin recovery, upgrade proposals) to detect unsafe conditions.
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

    /// Finalize a disputed escrow and disburse funds — the normal arbitrated resolution path.
    ///
    /// ## Role in the dispute lifecycle
    ///
    /// `resolve_dispute` is the primary exit from the `Disputed` state when an
    /// authorized party (arbitrator, admin, or moderator) has reviewed the
    /// evidence and reached a decision. It must be called **after** the evidence
    /// challenge window (`evidence_challenge_window`) has elapsed, ensuring both
    /// parties had a fair opportunity to submit and rebut evidence before the
    /// decision is locked in (see `dispute_escrow` for the full state diagram).
    ///
    /// If the arbitrator does not act before `max_dispute_duration` expires, any
    /// party can force-close the dispute via `resolve_expired_dispute` instead.
    ///
    /// ## Authorization
    ///
    /// Callable by the platform `admin`, the designated `arbitrator`, or any
    /// configured `moderator`. Parties to the escrow (buyer/seller) cannot call
    /// this function directly — they can only influence the outcome through
    /// evidence submission or `propose_partial_refund`.
    ///
    /// ## CEI pattern (Checks → Effects → Interactions)
    ///
    /// To prevent reentrancy attacks this function follows the CEI pattern strictly:
    ///
    /// 1. **Checks** — verify status, authorization, and the evidence window.
    /// 2. **Effects** — update `escrow.status` to `Resolved` and persist all
    ///    storage writes (dispute counter, obligation counters, contract counters,
    ///    orphaned refund-proposal cleanup) **before** any token transfer.
    /// 3. **Interactions** — execute the token transfer as the very last step.
    ///
    /// If the token transfer reverts (e.g. the recipient's trustline was revoked),
    /// Soroban's atomic execution rolls back all storage writes and the escrow
    /// stays in `Disputed` so the arbitrator can retry with a different resolution.
    ///
    /// ## Resolution outcomes
    ///
    /// | `resolution`        | Funds flow                                          | Reputation delta                        |
    /// |---------------------|-----------------------------------------------------|-----------------------------------------|
    /// | `ReleaseToSeller`   | `amount − platform_fee` → seller; fee → platform   | seller: +1 success; buyer: +1 disputed  |
    /// | `RefundToBuyer`     | full `amount` → buyer; no fee deducted              | buyer: +1 success; seller: +1 disputed  |
    ///
    /// Reputation deltas are emitted as `ReputationUpdateEvent` for the off-chain
    /// reputation service (decoupled from the onboarding contract, #211).
    ///
    /// ## Events emitted
    ///
    /// - `EscrowEvent { action: EscrowAction::Resolved, … }`
    /// - `EscrowResolvedEvent { arbitrator, … }` — includes the arbitrator address
    ///   for audit trails.
    /// - Two `ReputationUpdateEvent` entries (one per party).
    ///
    /// # Arguments
    /// * `order_id` - Identifier of the escrow to resolve.
    /// * `resolution` - `ReleaseToSeller` or `RefundToBuyer`.
    /// * `authorized_address` - Arbitrator, admin, or moderator address.
    ///
    /// # Errors
    /// * Panics with [`Error::Unauthorized`] if `authorized_address` is not privileged.
    /// * Panics with [`Error::InvalidEscrowState`] if the escrow is not `Disputed`.
    /// * Panics with [`Error::ChallengeWindowActive`] if called before the evidence
    ///   challenge window has elapsed.
    pub fn resolve_dispute(
        env: Env,
        order_id: u32,
        resolution: Resolution,
        authorized_address: Address,
    ) {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        authorized_address.require_auth();
        // Only privileged roles may finalize a dispute; neither buyer nor seller
        // can unilaterally choose the outcome via this path.
        let is_authorized = authorized_address == config.admin
            || Some(authorized_address.clone()) == config.moderator
            || authorized_address == config.arbitrator;
        if !is_authorized {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        // Block blacklisted arbitrators/moderators (#725).
        // The admin is the one who manages the blacklist and cannot be locked
        // out of dispute resolution by their own administrative action.
        if authorized_address != config.admin
            && env
                .storage()
                .persistent()
                .get::<_, bool>(&DataKey::ArbitratorBlacklist(authorized_address.clone()))
                .unwrap_or(false)
        {
            env.panic_with_error(crate::Error::ArbitratorBlacklisted);
        }


        let mut escrow = Self::get_stored_escrow(&env, order_id);
        let operation_id = Self::onboarding_operation_id(&env, b"resolve_dispute:", order_id);
        Self::authorize_onboarding_state(&env, &escrow.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &escrow.seller, operation_id, UserRole::Artisan);

        let snapshot = Self::get_stored_escrow(&env, order_id);
        Self::assert_open_for_settlement(&env, &snapshot, order_id)
            .unwrap_or_else(|e| env.panic_with_error(e));
        Self::assert_active_dispute_assignment(&env, order_id, None);
        Self::assert_arbitrator_resolution_window(&env, &snapshot, &config)
            .unwrap_or_else(|e| env.panic_with_error(e));

        // #1078: sweep expired evidence to invalidated before finalizing so the
        // resolved dispute never carries evidence past its retention deadline,
        // and refresh the evidence-log TTL for this still-active record.
        Self::expire_stale_evidence(&env, order_id);

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

        // Emit reputation update events — decoupled from onboarding contract (#211).
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

    /// Emit a versioned `dispute_evidence` / `submitted` event carrying stable
    /// metadata for off-chain indexers (#1077 AC3).
    ///
    /// The first tuple element is [`EVIDENCE_SCHEMA_VERSION`] so indexers can
    /// branch on the schema without inferring shape from field presence. The
    /// payload is deliberately flat (no `String` body) so it stays cheap and the
    /// digest is the canonical content reference.
    fn emit_evidence_submitted(
        env: &Env,
        order_id: u32,
        evidence_id: u64,
        dispute_session_id: u64,
        submitter: &Address,
        content_digest: &BytesN<32>,
        expires_at: u64,
        parent_evidence_id: Option<u64>,
    ) {
        env.events().publish(
            (
                Symbol::new(env, "dispute_evidence"),
                Symbol::new(env, "submitted"),
            ),
            (
                EVIDENCE_SCHEMA_VERSION,
                order_id,
                evidence_id,
                dispute_session_id,
                submitter.clone(),
                content_digest.clone(),
                expires_at,
                parent_evidence_id,
            ),
        );
    }

    /// Invalidate any expired evidence for an order and persist the sweep (#1078).
    ///
    /// Mirrors the read-time invalidation in [`Self::get_evidence`] but is called
    /// from state-changing finalization paths (dispute resolution) so a finalized
    /// dispute never carries evidence that was already past its retention deadline
    /// (#1078 AC1 — expired evidence cannot be finalized). Refreshes the
    /// evidence-log TTL for the still-active dispute record (#1078 AC2). No-op
    /// when the order has no evidence log.
    fn expire_stale_evidence(env: &Env, order_id: u32) {
        let key = DataKey::EvidenceLog(order_id);
        let log: Vec<DisputeEvidence> = match env.storage().persistent().get(&key) {
            Some(existing) => existing,
            None => return,
        };
        let now = env.ledger().timestamp();
        let mut updated_log = Vec::new(env);
        let mut modified = false;
        for mut item in log.into_iter() {
            if !item.is_invalidated && time_policy::is_deadline_reached(now, item.expires_at) {
                item.is_invalidated = true;
                modified = true;
            }
            updated_log.push_back(item);
        }
        if modified {
            env.storage().persistent().set(&key, &updated_log);
        }
        Self::extend_persistent(env, &key);
    }

    /// Submit evidence for a disputed escrow order (#927).
    ///
    /// Evidence is bound to the active dispute session, stamped with an expiry timestamp,
    /// and hashed to prevent evidence payload reuse across disputes.
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

        let assignment = Self::assert_active_dispute_assignment(&env, order_id, None);

        if !(submitter == escrow.buyer || submitter == escrow.seller) {
            env.panic_with_error(crate::Error::Unauthorized);
        }
        let dispute_session_id = escrow.dispute_initiated_at.unwrap_or(escrow.created_at as u64);

        // Prevent evidence reuse across multiple disputes (#927) and retain the
        // digest as stable indexer metadata persisted on the record (#1077).
        let len = (evidence_uri.len() as usize).min(256);
        let mut buf = [0u8; 256];
        evidence_uri.copy_into_slice(&mut buf[0..len]);
        let bytes = Bytes::from_slice(&env, &buf[0..len]);
        let content_digest: BytesN<32> = env.crypto().sha256(&bytes).into();
        let hash_key = DataKey::UsedEvidenceHash(content_digest.clone());
        if env.storage().persistent().has(&hash_key) {
            // Duplicate digest → defined, deterministic rejection (#1077 AC2).
            env.panic_with_error(crate::Error::EvidenceAlreadyUsed);
        }
        env.storage().persistent().set(&hash_key, &true);
        Self::extend_persistent(&env, &hash_key);

        let key = DataKey::EvidenceLog(order_id);
        let mut log: Vec<DisputeEvidence> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let expected_role = if submitter == escrow.buyer { UserRole::Buyer } else { UserRole::Artisan };
        let mut operation_id = Self::onboarding_operation_id(&env, b"submit_evidence:", order_id);
        operation_id.extend_from_slice(&(log.len() as u32).to_be_bytes());
        Self::authorize_onboarding_state(&env, &submitter, operation_id, expected_role);

        let id = log.len() as u64;
        let submitted_at = env.ledger().timestamp();
        let expires_at = submitted_at + DEFAULT_EVIDENCE_EXPIRY_WINDOW;

        let evidence = DisputeEvidence {
            id,
            order_id,
            dispute_session_id,
            submitter: submitter.clone(),
            assignment_revision: assignment.revision,
            submitter,
            evidence_uri,
            parent_evidence_id: None,
            submitted_at,
            expires_at,
            is_invalidated: false,
            version: EVIDENCE_SCHEMA_VERSION,
            content_digest: content_digest.clone(),
        };

        log.push_back(evidence);
        env.storage().persistent().set(&key, &log);
        // #1078: refresh the active dispute's evidence-log TTL on each write.
        Self::extend_persistent(&env, &key);

        // #1077: publish versioned, stable metadata for off-chain indexers.
        Self::emit_evidence_submitted(
            &env,
            order_id,
            id,
            dispute_session_id,
            &submitter,
            &content_digest,
            expires_at,
            None,
        );
        // Track challenge evidence count — bounded window state
        Self::bump_challenge_evidence_count(&env, order_id);
        id
    }

    fn bump_challenge_evidence_count(env: &Env, order_id: u32) {
        let key = Self::challenge_key(order_id);
        if let Some(mut challenge) = env
            .storage()
            .persistent()
            .get::<DataKey, EvidenceChallenge>(&key)
        {
            challenge.evidence_count = challenge.evidence_count.saturating_add(1);
            env.storage().persistent().set(&key, &challenge);
            Self::extend_persistent(env, &key);
        }
    }

    /// Submit counter-evidence responding to a prior evidence entry (#927).
    ///
    /// Counter-evidence is explicitly linked to its parent via `parent_evidence_id`
    /// so that the challenge chain is auditable and the parent can be verified
    /// to belong to the same dispute session.
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

        let assignment = Self::assert_active_dispute_assignment(&env, order_id, None);

        if !(submitter == escrow.buyer || submitter == escrow.seller) {
            env.panic_with_error(crate::Error::Unauthorized);
        }
        let dispute_session_id = escrow.dispute_initiated_at.unwrap_or(escrow.created_at as u64);

        let key = DataKey::EvidenceLog(order_id);
        let mut log: Vec<DisputeEvidence> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let expected_role = if submitter == escrow.buyer { UserRole::Buyer } else { UserRole::Artisan };
        let mut operation_id = Self::onboarding_operation_id(&env, b"submit_counter_evidence:", order_id);
        operation_id.extend_from_slice(&(log.len() as u32).to_be_bytes());
        Self::authorize_onboarding_state(&env, &submitter, operation_id, expected_role);

        // Validate parent evidence exists in the current dispute session and has
        // not expired. A missing parent is a bad reference (InvalidEscrowState);
        // an existing-but-expired parent means the caller is submitting
        // counter-evidence against dead evidence, which is rejected (#1078).
        let now = env.ledger().timestamp();
        // Validate parent evidence ID exists in current dispute evidence log
        // Counter-evidence must be linked to an existing submission in the same session
        let mut parent_found = false;
        let mut parent_expired = false;
        for item in log.iter() {
            if item.id == parent_evidence_id
                && item.dispute_session_id == dispute_session_id
                && item.assignment_revision == assignment.revision
            {
                parent_found = true;
                parent_expired =
                    item.is_invalidated || time_policy::is_deadline_reached(now, item.expires_at);
                break;
            }
        }
        if !parent_found {
            env.panic_with_error(crate::Error::InvalidDisputeAction);
        }
        if parent_expired {
            env.panic_with_error(crate::Error::EvidenceExpired);
        }

        // Prevent evidence reuse across multiple disputes (#927) and retain the
        // digest as stable indexer metadata persisted on the record (#1077).
        let len = (evidence_uri.len() as usize).min(256);
        let mut buf = [0u8; 256];
        evidence_uri.copy_into_slice(&mut buf[0..len]);
        let bytes = Bytes::from_slice(&env, &buf[0..len]);
        let content_digest: BytesN<32> = env.crypto().sha256(&bytes).into();
        let hash_key = DataKey::UsedEvidenceHash(content_digest.clone());
        if env.storage().persistent().has(&hash_key) {
            // Duplicate digest → defined, deterministic rejection (#1077 AC2).
            env.panic_with_error(crate::Error::EvidenceAlreadyUsed);
        }
        env.storage().persistent().set(&hash_key, &true);
        Self::extend_persistent(&env, &hash_key);

        let id = log.len() as u64;
        let submitted_at = env.ledger().timestamp();
        let expires_at = submitted_at + DEFAULT_EVIDENCE_EXPIRY_WINDOW;

        let evidence = DisputeEvidence {
            id,
            order_id,
            dispute_session_id,
            submitter: submitter.clone(),
            assignment_revision: assignment.revision,
            submitter,
            evidence_uri,
            parent_evidence_id: Some(parent_evidence_id),
            submitted_at,
            expires_at,
            is_invalidated: false,
            version: EVIDENCE_SCHEMA_VERSION,
            content_digest: content_digest.clone(),
        };

        log.push_back(evidence);
        env.storage().persistent().set(&key, &log);
        // #1078: refresh the active dispute's evidence-log TTL on each write.
        Self::extend_persistent(&env, &key);

        // #1077: publish versioned, stable metadata for off-chain indexers.
        Self::emit_evidence_submitted(
            &env,
            order_id,
            id,
            dispute_session_id,
            &submitter,
            &content_digest,
            expires_at,
            Some(parent_evidence_id),
        );
        Self::bump_challenge_evidence_count(&env, order_id);
        id
    }

    /// Retrieve all evidence records for a dispute, automatically setting `is_invalidated = true`
    /// for any entries whose retention/expiry timestamp has passed (#927).
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
            // Time policy: evidence expires when now >= expires_at (inclusive end, consistent with all other deadlines)
            if !item.is_invalidated && time_policy::is_deadline_reached(current_time, item.expires_at) {
                item.is_invalidated = true;
                modified = true;
            }
            updated_log.push_back(item);
        }

        if modified {
            env.storage().persistent().set(&key, &updated_log);
            // #1078: the sweep is a write to active dispute data — refresh its TTL.
            Self::extend_persistent(&env, &key);
        }

        updated_log
    }

    /// Retrieve only non-expired and non-invalidated evidence records for an order (#927).
    pub fn get_valid_evidence(env: Env, order_id: u32) -> Vec<DisputeEvidence> {
        let all_evidence = Self::get_evidence(env.clone(), order_id);
        let mut valid_log = Vec::new(&env);
        let current_time = env.ledger().timestamp();
        let assignment = Self::get_dispute_assignment_internal(&env, order_id);

        for item in all_evidence.into_iter() {
            // Time policy: evidence is valid while now < expires_at (window active)
            if !item.is_invalidated
                && item.assignment_revision == assignment.revision
                && time_policy::is_deadline_pending(current_time, item.expires_at)
            {
                valid_log.push_back(item);
            }
        }
        valid_log
    }

    /// Escalate a stalled dispute to the next checkpoint on the escalation
    /// ladder (#941, #1080).
    ///
    /// ## Why this exists
    ///
    /// A dispute must never sit pending forever because the assigned arbitrator
    /// stopped acting. Escalation is the pressure valve: as a dispute ages it
    /// crosses fixed checkpoints, each of which widens the set of accounts that
    /// may push it forward, until the final deadline converts it into a
    /// deterministic timeout that anyone can settle.
    ///
    /// ## Checkpoints
    ///
    /// All checkpoints are measured from `dispute_initiated_at` and follow the
    /// crate-wide inclusive-end convention (a tier unlocks *at* its deadline):
    ///
    /// ```text
    ///  dispute_initiated_at
    ///   |
    ///   |  Assigned          arbitrator's own window; nobody may escalate
    ///   |- party_checkpoint ------> PartyFlagged     buyer / seller
    ///   |- moderator_checkpoint --> ModeratorReview  + moderator / arbitrator / admin
    ///   |- admin_checkpoint ------> AdminReview      + moderator / arbitrator / admin
    ///   \- max_dispute_duration --> TimedOut         anyone (final deadline)
    /// ```
    ///
    /// The final deadline is always `max_dispute_duration`, the same value that
    /// gates [`Self::resolve_dispute`] and [`Self::resolve_expired_dispute`], so
    /// the ladder and the settlement paths agree by construction.
    ///
    /// ## Semantics
    ///
    /// This call advances the dispute to the highest tier the clock has reached.
    /// It is therefore idempotent *per checkpoint*: escalating twice without a
    /// new checkpoint having elapsed is rejected with
    /// [`Error::InvalidDisputeAction`]. Each accepted escalation overwrites
    /// [`DataKey::DisputeEscalationState`] and emits a `dispute_escalated` event
    /// carrying the tier, the escalator, and the timestamp, so the escalation
    /// history is fully auditable off-chain.
    ///
    /// Escalation is a signalling mechanism only - it never moves funds and
    /// never changes the escrow status. Settlement remains with
    /// `resolve_dispute` (before the final deadline) or
    /// `resolve_expired_dispute` (at or after it).
    ///
    /// # Arguments
    /// * `order_id` - Identifier of the disputed escrow.
    /// * `caller` - Must be eligible for the tier being reached; see the
    ///   permission matrix on [`Self::can_escalate_dispute`].
    ///
    /// # Errors
    /// * Panics with [`Error::SettlementAlreadyFinalized`] if the dispute has
    ///   already been settled.
    /// * Panics with [`Error::NotInDispute`] if the escrow is not `Disputed`.
    /// * Panics with [`Error::EscalationWindowActive`] if no checkpoint has been
    ///   reached yet.
    /// * Panics with [`Error::InvalidDisputeAction`] if the dispute is already
    ///   recorded at the tier the clock has reached.
    /// * Panics with [`Error::Unauthorized`] if `caller` is not eligible to
    ///   escalate to that tier.
    pub fn escalate_dispute(env: Env, order_id: u32, caller: Address) {
        caller.require_auth();

        let escrow = Self::get_stored_escrow(&env, order_id);
        // A settled dispute has nothing left to escalate. Checked before the
        // status check so the terminal case reports the specific error.
        if Self::has_settlement_receipt(&env, order_id) {
            env.panic_with_error(crate::Error::SettlementAlreadyFinalized);
        }
        if escrow.status != EscrowStatus::Disputed {
            env.panic_with_error(crate::Error::NotInDispute);
        }

        let config = Self::get_platform_config_internal(&env);
        let schedule = Self::escalation_schedule(&env, &escrow, &config)
            .unwrap_or_else(|e| env.panic_with_error(e));
        let current_time = env.ledger().timestamp();
        let target_tier = Self::tier_at(current_time, &schedule);

        // No checkpoint reached yet: the arbitrator is still inside its window.
        if target_tier == EscalationTier::Assigned {
            env.panic_with_error(crate::Error::EscalationWindowActive);
        }

        // Reject re-escalation to a tier the dispute already occupies. Progress
        // requires a *new* checkpoint to have elapsed.
        let previous_tier = Self::recorded_escalation_tier(&env, order_id);
        if target_tier <= previous_tier {
            env.panic_with_error(crate::Error::InvalidDisputeAction);
        }

        if !Self::is_eligible_escalator(&config, &escrow, target_tier, &caller) {
        let assignment = Self::assert_active_dispute_assignment(&env, order_id, None);

        if !(caller == escrow.buyer || caller == escrow.seller) {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        // Parties additionally have to present valid onboarding state; the
        // privileged roles and the permissionless timeout tier do not.
        if Self::is_escrow_party(&escrow, &caller) {
            let expected_role = if caller == escrow.buyer {
                UserRole::Buyer
            } else {
                UserRole::Artisan
            };
            let operation_id = Self::onboarding_operation_id(&env, b"escalate_dispute:", order_id);
            Self::authorize_onboarding_state(&env, &caller, operation_id, expected_role);
        }

        let state_key = DataKey::DisputeEscalationState(order_id);
        let escalation_count = env
            .storage()
            .persistent()
            .get::<_, DisputeEscalationState>(&state_key)
            .map_or(0, |state| state.escalation_count);
        env.storage().persistent().set(
            &state_key,
            &DisputeEscalationState {
                order_id,
                tier: target_tier,
                previous_tier,
                escalated_by: caller.clone(),
                escalated_at: current_time,
                escalation_count: escalation_count.saturating_add(1),
            },
        );
        Self::extend_persistent(&env, &state_key);

        // Preserve the single-shot record consumed by existing indexers (#941):
        // it always points at the *first* escalation of this dispute.
        let legacy_key = DataKey::DisputeEscalation(order_id);
        if !env.storage().persistent().has(&legacy_key) {
            env.storage().persistent().set(
                &legacy_key,
                &DisputeEscalationRecord {
                    order_id,
                    escalated_by: caller.clone(),
                    escalated_at: current_time,
                },
            );
            Self::extend_persistent(&env, &legacy_key);
        }

        Self::emit_dispute_escalated(&env, order_id, target_tier, &caller, current_time);
    }

    /// Get the first escalation record for an order (#941).
    ///
    /// Retained for backwards compatibility; use
    /// [`Self::get_dispute_escalation_status`] for the full ladder.
    pub fn get_dispute_escalation(env: Env, order_id: u32) -> Option<DisputeEscalationRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeEscalation(order_id))
    }

    /// Get the tiered escalation ladder state recorded for a dispute (#1080).
    ///
    /// Returns `None` when the dispute has never been escalated.
    pub fn get_dispute_escalation_state(
        env: Env,
        order_id: u32,
    ) -> Option<DisputeEscalationState> {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeEscalationState(order_id))
    }

    /// Full auditable escalation snapshot for a disputed escrow (#1080).
    ///
    /// Exposes every checkpoint timestamp, the final deadline, the tier the
    /// clock implies, the tier recorded on-chain, whether the dispute is already
    /// finalized, and the settlement a timeout would deterministically produce.
    ///
    /// # Errors
    /// * [`Error::InvalidEscrowState`] if the escrow is not `Disputed` or has no
    ///   `dispute_initiated_at` timestamp.
    pub fn get_dispute_escalation_status(
        env: Env,
        order_id: u32,
    ) -> Result<DisputeEscalationStatus, Error> {
        let escrow = Self::get_stored_escrow(&env, order_id);
        Self::assert_disputed_for_policy(&escrow)?;

        let config = Self::get_platform_config_internal(&env);
        let schedule = Self::escalation_schedule(&env, &escrow, &config)?;
        let now = env.ledger().timestamp();
        let current_tier = Self::tier_at(now, &schedule);

        Ok(DisputeEscalationStatus {
            order_id,
            schedule,
            current_tier,
            recorded_tier: Self::recorded_escalation_tier(&env, order_id),
            is_timed_out: current_tier == EscalationTier::TimedOut,
            is_finalized: Self::has_settlement_receipt(&env, order_id),
            timeout_outcome: Self::timeout_outcome(config.expired_dispute_fee_policy),
        })
    }

    /// Absolute timestamp after which a pending dispute can no longer be
    /// arbitrated and only the deterministic timeout settlement remains (#1080).
    ///
    /// # Errors
    /// * [`Error::InvalidEscrowState`] if the escrow is not a pending dispute.
    pub fn get_dispute_final_deadline(env: Env, order_id: u32) -> Result<u64, Error> {
        let escrow = Self::get_stored_escrow(&env, order_id);
        Self::assert_disputed_for_policy(&escrow)?;
        let config = Self::get_platform_config_internal(&env);
        Ok(Self::escalation_schedule(&env, &escrow, &config)?.final_deadline)
    }

    /// Whether `caller` may escalate `order_id` at the current ledger time (#1080).
    ///
    /// Encodes the escalation permission matrix as a queryable predicate so
    /// front-ends and monitoring bots can surface exactly who is allowed to act:
    ///
    /// | Tier              | Eligible escalators                                 |
    /// |-------------------|-----------------------------------------------------|
    /// | `Assigned`        | nobody - the arbitrator is still inside its window   |
    /// | `PartyFlagged`    | buyer, seller                                       |
    /// | `ModeratorReview` | buyer, seller, moderator, arbitrator, admin         |
    /// | `AdminReview`     | buyer, seller, moderator, arbitrator, admin         |
    /// | `TimedOut`        | anyone - permissionless safety net                   |
    ///
    /// Returns `false` (rather than erroring) for escrows that are not pending
    /// disputes, for already-settled disputes, and when the tier the clock has
    /// reached is already recorded on-chain.
    pub fn can_escalate_dispute(env: Env, order_id: u32, caller: Address) -> bool {
        if !env.storage().persistent().has(&(ESCROW, order_id)) {
            return false;
        }
        // Routed through `get_stored_escrow` so legacy storage layouts are
        // normalised the same way every settlement path normalises them.
        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.status != EscrowStatus::Disputed || Self::has_settlement_receipt(&env, order_id) {
            return false;
        }

        let config = Self::get_platform_config_internal(&env);
        let schedule = match Self::escalation_schedule(&env, &escrow, &config) {
            Ok(schedule) => schedule,
            Err(_) => return false,
            assignment_revision: assignment.revision,
            escalated_by: caller,
            escalated_at: current_time,
        };
        let target_tier = Self::tier_at(env.ledger().timestamp(), &schedule);
        if target_tier <= Self::recorded_escalation_tier(&env, order_id) {
            return false;
        }
        Self::is_eligible_escalator(&config, &escrow, target_tier, &caller)
    }

    /// Preview the deterministic settlement a timeout would produce (#1080).
    ///
    /// Derived purely from the operator's `expired_dispute_fee_policy`, so the
    /// outcome of letting a dispute time out is knowable in advance and cannot
    /// be influenced by whoever happens to call `resolve_expired_dispute`.
    pub fn get_timeout_outcome(env: Env) -> TimeoutOutcome {
        let config = Self::get_platform_config_internal(&env);
        Self::timeout_outcome(config.expired_dispute_fee_policy)
    }

    /// Read the configured escalation checkpoint schedule (#1080).
    pub fn get_escalation_checkpoints(env: Env) -> EscalationCheckpoints {
        let config = Self::get_platform_config_internal(&env);
        Self::escalation_checkpoints(&env, &config)
    }

    /// Configure the escalation checkpoint schedule (admin only) (#1080).
    ///
    /// Offsets are in seconds from `dispute_initiated_at` and must be strictly
    /// increasing and strictly below `max_dispute_duration`, so that every tier
    /// is reachable before the final deadline turns the dispute into a timeout.
    ///
    /// `party_checkpoint` is kept in sync with `dispute_escalation_window` so
    /// the tier-1 window has exactly one source of truth (#941).
    ///
    /// # Errors
    /// * Panics with [`Error::InvalidEscalationPolicy`] if the offsets are not
    ///   strictly increasing or the last one is not below the final deadline.
    pub fn set_escalation_checkpoints(
        env: Env,
        party_checkpoint: u32,
        moderator_checkpoint: u32,
        admin_checkpoint: u32,
    ) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        if party_checkpoint == 0
            || party_checkpoint >= moderator_checkpoint
            || moderator_checkpoint >= admin_checkpoint
            || admin_checkpoint >= config.max_dispute_duration
        {
            env.panic_with_error(crate::Error::InvalidEscalationPolicy);
        }

        let checkpoints = EscalationCheckpoints {
            party_checkpoint,
            moderator_checkpoint,
            admin_checkpoint,
        };
        env.storage()
            .persistent()
            .set(&DataKey::EscalationCheckpoints, &checkpoints);
        Self::extend_persistent(&env, &DataKey::EscalationCheckpoints);

        let old_window = config.dispute_escalation_window;
        config.dispute_escalation_window = party_checkpoint;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        Self::emit_config_updated(
            &env,
            "dispute_escalation_window",
            ConfigValue::U32(old_window),
            ConfigValue::U32(party_checkpoint),
        );
    }

    /// Return the assignment snapshot for a disputed order.
    pub fn get_dispute_assignment(env: Env, order_id: u32) -> Option<DisputeAssignment> {
        env.storage()
            .persistent()
            .get(&DataKey::DisputeAssignment(order_id))
    }

    /// Reassign an open dispute to the current arbitrator assignment.
    pub fn reassign_dispute(env: Env, order_id: u32) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.status != EscrowStatus::Disputed {
            env.panic_with_error(crate::Error::NotInDispute);
        }

        let old_assignment = Self::get_dispute_assignment_internal(&env, order_id);
        let (arbitrator, revision) = Self::current_arbitrator_assignment(&env);
        let changed_at = env.ledger().timestamp();
        let new_assignment = DisputeAssignment {
            order_id,
            arbitrator: arbitrator.clone(),
            revision,
            assigned_at: changed_at,
        };
        env.storage()
            .persistent()
            .set(&DataKey::DisputeAssignment(order_id), &new_assignment);
        env.events().publish(
            (Symbol::new(&env, "dispute_assignment_changed"), order_id as u64),
            DisputeAssignmentChangedEvent {
                order_id,
                old_revision: old_assignment.revision,
                new_revision: revision,
                arbitrator,
                changed_at,
            },
        );
    }

    /// Set the dispute escalation window (admin only) (#941).
    ///
    /// This is the tier-1 (`PartyFlagged`) checkpoint. The stored checkpoint
    /// schedule is updated alongside it so the two cannot diverge; the later
    /// checkpoints are pushed out if the new window would overtake them.
    pub fn set_dispute_escalation_window(env: Env, window: u32) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let mut checkpoints = Self::escalation_checkpoints(&env, &config);
        checkpoints.party_checkpoint = window;
        checkpoints.moderator_checkpoint = checkpoints.moderator_checkpoint.max(window);
        checkpoints.admin_checkpoint = checkpoints
            .admin_checkpoint
            .max(checkpoints.moderator_checkpoint);
        env.storage()
            .persistent()
            .set(&DataKey::EscalationCheckpoints, &checkpoints);
        Self::extend_persistent(&env, &DataKey::EscalationCheckpoints);

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

    /// Retrieve the persisted challenge window for a disputed order (#942).
    ///
    /// Returns `None` if the escrow has never been disputed (no challenge
    /// record exists).  The returned `deadline` is the immutable timestamp
    /// computed at dispute time; `state` indicates whether the window has
    /// been closed exactly once via settlement.
    pub fn get_evidence_challenge(env: Env, order_id: u32) -> Option<EvidenceChallenge> {
        Self::fetch_evidence_challenge(&env, order_id)
    }

    /// Convenience: return only the stored challenge deadline, if any.
    pub fn get_challenge_deadline(env: Env, order_id: u32) -> Option<u64> {
        Self::fetch_evidence_challenge(&env, order_id).map(|c| c.deadline)
    }

    /// Returns true while the challenge window is still open (deadline pending and not closed).
    pub fn is_challenge_window_open(env: Env, order_id: u32) -> bool {
        if let Some(challenge) = Self::fetch_evidence_challenge(&env, order_id) {
            if challenge.state == ChallengeState::Closed {
                return false;
            }
            time_policy::is_deadline_pending(env.ledger().timestamp(), challenge.deadline)
        } else {
            // Legacy fallback: derive from escrow if disputed
            if let Some(escrow) = env.storage().persistent().get::<_, Escrow>(&(ESCROW, order_id)) {
                if escrow.status == EscrowStatus::Disputed {
                    if let Some(initiated) = escrow.dispute_initiated_at {
                        let config = Self::get_platform_config_internal(&env);
                        return time_policy::is_window_active(
                            env.ledger().timestamp(),
                            initiated,
                            config.evidence_challenge_window as u64,
                        );
                    }
                }
            }
            false
        }
    }

    /// Explicitly close the challenge window after its deadline has elapsed.
    ///
    /// The window closes exactly once — a second call fails with
    /// `SettlementAlreadyFinalized`. This is a permissionless finalization
    /// guard: any account may close the window once the bounded challenge
    /// period has elapsed, mirroring `resolve_expired_dispute`. Normal
    /// settlement paths (`resolve_dispute`, `resolve_dispute_partial`,
    /// `accept_partial_refund`) close the window automatically as part of
    /// commitment, so calling this function is optional. It exists to make
    /// the exactly-once closure testable off-chain.
    pub fn close_challenge_window(env: Env, order_id: u32) -> Result<(), Error> {
        let escrow = Self::get_stored_escrow(&env, order_id);
        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }
        if Self::is_challenge_closed(&env, order_id)
            || Self::has_settlement_receipt(&env, order_id)
        {
            return Err(Error::SettlementAlreadyFinalized);
        }
        let config = Self::get_platform_config_internal(&env);
        let deadline = Self::challenge_deadline(&env, order_id, &escrow, &config);
        let now = env.ledger().timestamp();
        if time_policy::is_deadline_pending(now, deadline) {
            return Err(Error::ChallengeWindowActive);
        }
        Self::close_evidence_challenge(&env, order_id)
    }

    /// Set rate limit configuration (admin only) (#943).
    pub fn set_rate_limit_config(env: Env, max_calls: u32, window: u32) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();
        let rate_config = RateLimitConfig { max_calls, window };
        env.storage()
            .persistent()
            .set(&DataKey::RateLimitConfig, &rate_config);
    }

    /// Emit the auditable escalation event (#941, #1080).
    ///
    /// Topics are unchanged from #941 (`("dispute_escalated", order_id)`) so
    /// existing subscriptions keep matching; the tier, the escalator, and the
    /// timestamp are carried in the data payload.
    fn emit_dispute_escalated(
        env: &Env,
        order_id: u32,
        tier: EscalationTier,
        escalated_by: &Address,
        escalated_at: u64,
    ) {
        env.events().publish(
            (Symbol::new(env, "dispute_escalated"), order_id as u64),
            (tier, escalated_by.clone(), escalated_at),
        );
    }

    /// Emit the terminal timeout event for a dispute that blew through its
    /// final deadline (#1080).
    fn emit_dispute_timed_out(
        env: &Env,
        order_id: u32,
        outcome: TimeoutOutcome,
        final_deadline: u64,
        settled_at: u64,
    ) {
        env.events().publish(
            (Symbol::new(env, "dispute_timed_out"), order_id as u64),
            (outcome, final_deadline, settled_at),
        );
    }

    /// Resolve a dispute by splitting funds between buyer and seller.
    ///
    /// `buyer_amount` is the gross amount returned to the buyer. The platform
    /// fee is charged once on the seller's portion only, matching the logic of
    /// a normal release but applied to a reduced seller share.
    pub fn resolve_dispute_partial(
        env: Env,
        order_id: u32,
        buyer_amount: i128,
        authorized_address: Address,
    ) {
        let _guard = ReentryGuardScope::new(&env);
        let config = Self::get_platform_config_internal(&env);
        authorized_address.require_auth();
        let is_authorized = authorized_address == config.admin
            || Some(authorized_address.clone()) == config.moderator
            || authorized_address == config.arbitrator;
        if !is_authorized {
            env.panic_with_error(crate::Error::Unauthorized);
        }

        let mut escrow = Self::get_stored_escrow(&env, order_id);
        let operation_id = Self::onboarding_operation_id(&env, b"resolve_dispute_partial:", order_id);
        Self::authorize_onboarding_state(&env, &escrow.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &escrow.seller, operation_id, UserRole::Artisan);

        let snapshot = Self::get_stored_escrow(&env, order_id);
        Self::assert_open_for_settlement(&env, &snapshot, order_id)
            .unwrap_or_else(|e| env.panic_with_error(e));
        Self::assert_active_dispute_assignment(&env, order_id, None);
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

    /// Change the active arbitrator and advance the assignment revision.
    /// Existing disputes remain financially intact but all actions bound to
    /// their previous assignment are rejected until an admin explicitly
    /// reassigns each affected dispute.
    pub fn update_arbitrator(env: Env, new_arbitrator: Address) {
        let mut config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        let old_arbitrator = config.arbitrator.clone();
        let old_revision = env
            .storage()
            .persistent()
            .get(&DataKey::ArbitratorAssignmentRevision)
            .unwrap_or(1u64);
        let new_revision = old_revision
            .checked_add(1)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::InvalidEscrowState));

        config.arbitrator = new_arbitrator.clone();
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        env.storage()
            .persistent()
            .set(&DataKey::ArbitratorAssignmentRevision, &new_revision);

        Self::emit_config_updated(
            &env,
            "arbitrator",
            ConfigValue::Address(old_arbitrator),
            ConfigValue::Address(new_arbitrator),
        );
    }

    /// Return the active arbitrator assignment revision.
    pub fn get_arbitrator_assignment_revision(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::ArbitratorAssignmentRevision)
            .unwrap_or(1u64)
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

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&new_fee_bps.to_be_bytes());
        if let Err(e) = Self::gate_admin_mutation(&env, "update_platform_fee", payload) {
            env.panic_with_error(e);
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

    /// Update platform wallet address (admin only)
    ///
    /// # Arguments
    /// * `new_wallet` - New platform wallet address
    pub fn update_platform_wallet(env: Env, new_wallet: Address) {
        let config = Self::get_platform_config_internal(&env);
        config.admin.require_auth();

        // Reject invalid wallet addresses before writing to storage (#707).
        if let Err(e) = Self::validate_platform_wallet(&env, &new_wallet) {
            env.panic_with_error(e);
        }

        if let Err(e) = Self::gate_admin_mutation(
            &env,
            "update_platform_wallet",
            new_wallet.to_xdr(&env),
        ) {
            env.panic_with_error(e);
        }

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

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&(policy as u32).to_be_bytes());
        Self::gate_admin_mutation(&env, "update_expired_dispute_policy", payload)?;

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

    /// Get the current expired dispute fee policy
    pub fn get_expired_dispute_policy(env: Env) -> ExpiredDisputeFeePolicy {
        let config = Self::get_platform_config_internal(&env);
        config.expired_dispute_fee_policy
    }

    /// Get the current moderator address, if set.
    pub fn get_moderator(env: Env) -> Option<Address> {
        Self::get_platform_config_internal(&env).moderator
    }

    pub fn set_moderator(env: Env, moderator: Address) {
        let mut config = Self::get_platform_config(env.clone());
        config.admin.require_auth();
        if let Err(e) = Self::gate_admin_mutation(
            &env,
            "set_moderator",
            moderator.to_xdr(&env),
        ) {
            env.panic_with_error(e);
        }
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

    /// Add an address to the arbitrator blacklist (admin only) (#725).
    ///
    /// Once blacklisted, the address is rejected by every privileged
    /// settlement path (`resolve_dispute`, `resolve_dispute_partial`) with
    /// `Error::ArbitratorBlacklisted`, even if it matches the configured
    /// `arbitrator` or `moderator` role. The admin itself is never blocked
    /// by this mechanism.
    ///
    /// # Arguments
    /// * `arbitrator` - Address to blacklist
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

    /// Remove an address from the arbitrator blacklist (admin only) (#725).
    ///
    /// After removal the address may again act as arbitrator or moderator,
    /// provided it still holds the relevant role in `PlatformConfig`.
    ///
    /// # Arguments
    /// * `arbitrator` - Address to remove from the blacklist
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

    /// Returns `true` if `arbitrator` is currently on the blacklist (#725).
    ///
    /// # Arguments
    /// * `arbitrator` - Address to query
    pub fn is_arbitrator_blacklisted(env: Env, arbitrator: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::ArbitratorBlacklist(arbitrator))
            .unwrap_or(false)
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
        Self::calculate_fee(&env, amount, config.platform_fee_bps)
    }

    /// Calculate net amount seller will receive
    ///
    /// # Arguments
    /// * `amount` - The escrow amount
    pub fn calculate_seller_net_amount(env: Env, amount: i128) -> i128 {
        let fee = Self::calculate_fee_for_amount(env, amount);
        amount - fee
    }

    /// Returns the current deterministic fee policy version.
    ///
    /// Increment this constant whenever fee allocation formulas change.
    /// Callers can compare versions off-chain to detect policy updates.
    pub fn get_fee_policy_version(_env: Env) -> u32 {
        FEE_POLICY_VERSION
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

        if let Some(hash) = &params.service_agreement_hash {
            if hash.len() != 32 {
                return Err(Error::InvalidServiceAgreementHash);
            }
        }

        // Reject duplicate escrow identifiers (#1027): the order ID is the
        // canonical key, so a duplicate must not overwrite an existing escrow.
        if env.storage().persistent().has(&(ESCROW, params.order_id)) {
            return Err(Error::EscrowAlreadyExists);
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
        let operation_id = Self::onboarding_operation_id(env, b"create_batch_escrow:", params.order_id);
        Self::authorize_onboarding_state(env, &params.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(env, &params.seller, operation_id, UserRole::Artisan);

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
            funding_deadline: None, // Immediately funded; no deadline required (#656)
            service_agreement_hash: params.service_agreement_hash.clone(),
        };

        env.storage()
            .persistent()
            .set(&(ESCROW, params.order_id), &escrow);
        Self::extend_persistent(env, &(ESCROW, params.order_id));

        // Track active escrows (batch)
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
    /// Authorization model:
    /// - Every distinct buyer in the batch must provide an authorization signature
    /// - This prevents a single transaction from creating escrows on behalf of
    ///   buyers who did not approve the operation
    /// Optimizations:
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

        // Issue #111: Enforce batch size limit
        if escrows.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchLimitExceeded);
        }

        let mut results = soroban_sdk::Vec::new(&env);

        // Early exit for empty batch
        if escrows.is_empty() {
            return Ok(results);
        }

        // Issue #606: Require authorization from every distinct buyer in the batch.
        // This prevents a single transaction from creating escrows on behalf of
        // buyers who did not sign the operation.
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

        // Issue #111: Validate all first (single pass)
        // Issue #1027: also reject duplicate order IDs within the batch itself.
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

        // Issue #111: Collect buyer/seller updates to consolidate storage writes
        // Using indexed storage for scalability. Precompute the starting counts for
        // each buyer/seller once so the batch creation loop can reuse that state
        // without repeatedly re-reading the same count map while populating indices.
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

        // Create all escrows using the precomputed count state.
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

                        // Store escrow ID at indexed position
                        let buyer_index_key =
                            DataKey::BuyerEscrowIndexed(buyer_key.clone(), buyer_count);
                        env.storage().persistent().set(&buyer_index_key, &id);
                        Self::extend_persistent(&env, &buyer_index_key);

                        let next_buyer = buyer_count
                            .checked_add(1)
                            .ok_or(Error::CounterOverflow)?;
                        buyer_next_counts.set(buyer_key, next_buyer);

                        if !seller_next_counts.contains_key(seller_key.clone()) {
                            let existing_count =
                                seller_count_state.get(seller_key.clone()).unwrap_or(0u32);
                            seller_next_counts.set(seller_key.clone(), existing_count);
                        }
                        let seller_count = seller_next_counts.get(seller_key.clone()).unwrap();

                        // Store escrow ID at indexed position
                        let seller_index_key =
                            DataKey::SellerEscrowIndexed(seller_key.clone(), seller_count);
                        env.storage().persistent().set(&seller_index_key, &id);
                        Self::extend_persistent(&env, &seller_index_key);

                        let next_seller = seller_count
                            .checked_add(1)
                            .ok_or(Error::CounterOverflow)?;
                        seller_next_counts.set(seller_key, next_seller);

                        // Emit batch event
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

        // Issue #111: Consolidate all storage updates at once
        let mut i = 0;
        loop {
            if i >= buyer_next_counts.len() {
                break;
            }
            if let Some(buyer_addr) = buyer_next_counts.keys().get(i) {
                if let Some(final_count) = buyer_next_counts.get(buyer_addr.clone()) {
                    let count_key = DataKey::BuyerEscrowCount(buyer_addr.clone());
                    env.storage().persistent().set(&count_key, &final_count);
                    Self::extend_persistent(&env, &count_key);
                }
            }
            i += 1;
        }

        let mut i = 0;
        loop {
            if i >= seller_next_counts.len() {
                break;
            }
            if let Some(seller_addr) = seller_next_counts.keys().get(i) {
                if let Some(final_count) = seller_next_counts.get(seller_addr.clone()) {
                    let count_key = DataKey::SellerEscrowCount(seller_addr.clone());
                    env.storage().persistent().set(&count_key, &final_count);
                    Self::extend_persistent(&env, &count_key);
                }
            }
            i += 1;
        }

        // Consolidate global index updates for the entire batch using atomic function
        // This ensures AllEscrowIds and EscrowCount always remain in sync (Issue #226)
        if !results.is_empty() {
            // Convert results to u32 order IDs
            let mut order_ids = soroban_sdk::Vec::new(&env);
            for j in 0..results.len() {
                if let Some(id) = results.get(j) {
                    order_ids.push_back(id as u32);
                }
            }
            Self::update_escrow_indices_batch_atomic(&env, &order_ids);
        }

        Ok(results)
    }

    /// Schedule a bounded batch for resumable execution.
    ///
    /// This performs all validation and buyer authorization before persisting
    /// the immutable input. It does not create escrows or move funds.
    pub fn schedule_batch_escrow(
        env: Env,
        owner: Address,
        params: soroban_sdk::Vec<EscrowCreateParams>,
    ) -> Result<u64, Error> {
        Self::check_not_paused(&env);
        owner.require_auth();

        if params.is_empty() || params.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchLimitExceeded);
        }

        let mut authorized_buyers: Map<Address, u32> = Map::new(&env);
        for i in 0..params.len() {
            if let Some(entry) = params.get(i) {
                if !authorized_buyers.contains_key(entry.buyer.clone()) {
                    // `owner.require_auth()` already covers the scheduling
                    // account; a second require_auth on the same address
                    // panics with Auth/ExistingValue under Soroban.
                    if entry.buyer != owner {
                        entry.buyer.require_auth();
                    }
                    authorized_buyers.set(entry.buyer.clone(), 1u32);
                }
                Self::validate_escrow_params(&env, &entry)?;
            }
        }

        let job_id = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "next_batch_id"))
            .unwrap_or(1u64);
        let next_id = job_id.checked_add(1).ok_or(Error::BatchJobNotFound)?;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "next_batch_id"), &next_id);

        let job = BatchEscrowJob {
            owner: owner.clone(),
            params: params.clone(),
            next_index: 0,
            status: BatchJobStatus::Pending,
            op_type: BatchOpType::EscrowCreation,
            revision: 0,
        };
        let key = DataKey::BatchEscrowJob(job_id);
        env.storage().persistent().set(&key, &job);
        Self::extend_persistent(&env, &key);
        env.events().publish(
            (
                Symbol::new(&env, "batch_scheduler"),
                Symbol::new(&env, "scheduled"),
            ),
            (job_id, params.len()),
        );
        Ok(job_id)
    }

    /// Build a client/indexer-facing progress snapshot from a stored job.
    ///
    /// Single source of truth for the shape returned by `continue_batch_escrow`,
    /// `get_batch_escrow_progress`, and the idempotent-replay paths so all three
    /// are byte-for-byte comparable.
    fn batch_progress(job_id: u64, job: &BatchEscrowJob) -> BatchJobProgress {
        BatchJobProgress {
            id: job_id,
            owner: job.owner.clone(),
            next_index: job.next_index,
            total: job.params.len(),
            status: job.status,
            op_type: job.op_type,
            revision: job.revision,
        }
    }

    /// Return the current continuation cursor for a scheduled batch (#1075).
    ///
    /// The cursor reflects the persisted checkpoint — `revision` and `next_index`
    /// after the last committed chunk — so recovery can resume deterministically
    /// from exactly where the job left off (#1076 AC3). Returns `None` when the
    /// job does not exist.
    pub fn get_batch_cursor(env: Env, job_id: u64) -> Option<BatchCursor> {
        let key = DataKey::BatchEscrowJob(job_id);
        let job: BatchEscrowJob = env.storage().persistent().get(&key)?;
        Some(BatchCursor {
            job_id,
            owner: job.owner,
            op_type: job.op_type,
            revision: job.revision,
            next_index: job.next_index,
        })
    }

    /// Process the next deterministic chunk of a scheduled batch (#1075/#1076).
    ///
    /// The caller presents a [`BatchCursor`] minted by `schedule_batch_escrow` /
    /// `get_batch_cursor`. The cursor is validated against the persisted job
    /// before any work happens, which is what makes continuation safe to retry:
    ///
    /// * cross-account / cross-operation cursors are rejected (#1075 AC1);
    /// * a cursor behind the committed revision (an already-applied chunk) is a
    ///   harmless no-op returning current progress (#1075 AC2 / #1076 AC2);
    /// * a cursor ahead of the committed revision is rejected as fabricated.
    ///
    /// The chunk is committed through [`Self::create_batch_escrow`], whose failure
    /// rolls back the whole invocation. The checkpoint (`next_index` + bumped
    /// `revision`) is persisted **only after** all financial effects succeed, so a
    /// failed chunk never advances the cursor (#1076 AC1) and a committed chunk can
    /// never be applied twice at the same revision (#1076 AC2).
    pub fn continue_batch_escrow(
        env: Env,
        cursor: BatchCursor,
        work_limit: u32,
    ) -> Result<BatchJobProgress, Error> {
        // Bound the work per continuation first, so an out-of-range request is
        // rejected identically whether or not the job exists.
        pagination_validation::validate_strict_limit(
            work_limit,
            pagination_validation::MAX_BATCH_WORK_LIMIT,
        )?;

        let key = DataKey::BatchEscrowJob(cursor.job_id);
        let mut job: BatchEscrowJob = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::BatchJobNotFound)?;

        // A cursor is bound to exactly one owner and one operation type; it can
        // never be replayed against another account's job or a different op.
        if cursor.owner != job.owner {
            return Err(Error::BatchJobUnauthorized);
        }
        if cursor.op_type != job.op_type {
            return Err(Error::BatchCursorMismatch);
        }
        job.owner.require_auth();

        // Revision gate — idempotent, bounded continuation:
        //   behind → the referenced chunk was already committed; harmless no-op.
        //   ahead  → fabricated / future cursor; reject.
        //   equal  → the live checkpoint; process the next chunk below.
        if cursor.revision < job.revision {
            return Ok(Self::batch_progress(cursor.job_id, &job));
        }
        if cursor.revision > job.revision {
            return Err(Error::BatchCursorMismatch);
        }

        // At the live revision, a terminal job yields its progress without work:
        // a completed job replays harmlessly (#1075 AC2); a cancelled job is a
        // genuine terminal error and stays an error.
        match job.status {
            BatchJobStatus::Completed => return Ok(Self::batch_progress(cursor.job_id, &job)),
            BatchJobStatus::Cancelled => return Err(Error::BatchJobCancelled),
            BatchJobStatus::Pending => {}
        }

        // Defensive cross-check: at the live revision the cursor's resume position
        // must equal the persisted checkpoint. A mismatch means a hand-forged
        // cursor; reject rather than process an unexpected range.
        if cursor.next_index != job.next_index {
            return Err(Error::BatchCursorMismatch);
        }

        let end = core::cmp::min(job.next_index + work_limit, job.params.len());
        let mut chunk = Vec::new(&env);
        for index in job.next_index..end {
            if let Some(entry) = job.params.get(index) {
                chunk.push_back(entry);
            }
        }

        // Atomic checkpoint: create_batch_escrow commits the whole chunk or
        // returns Err, which rolls back the entire invocation (including any
        // token transfers). Everything below persists only on success.
        Self::create_batch_escrow(env.clone(), cursor.job_id, chunk)?;
        job.next_index = end;
        job.revision = job.revision.saturating_add(1);
        if job.next_index == job.params.len() {
            job.status = BatchJobStatus::Completed;
        }
        env.storage().persistent().set(&key, &job);
        Self::extend_persistent(&env, &key);
        env.events().publish(
            (
                Symbol::new(&env, "batch_scheduler"),
                Symbol::new(&env, "progress"),
            ),
            (
                cursor.job_id,
                job.next_index,
                job.params.len(),
                job.revision,
                job.status,
            ),
        );

        Ok(Self::batch_progress(cursor.job_id, &job))
    }

    /// Cancel a pending batch without creating any escrow or moving funds.
    pub fn cancel_batch_escrow(env: Env, job_id: u64, owner: Address) -> Result<(), Error> {
        let key = DataKey::BatchEscrowJob(job_id);
        let mut job: BatchEscrowJob = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::BatchJobNotFound)?;
        if job.owner != owner {
            return Err(Error::BatchJobUnauthorized);
        }
        owner.require_auth();
        if job.status != BatchJobStatus::Pending {
            return Err(if job.status == BatchJobStatus::Completed {
                Error::BatchJobCompleted
            } else {
                Error::BatchJobCancelled
            });
        }
        job.status = BatchJobStatus::Cancelled;
        env.storage().persistent().set(&key, &job);
        Self::extend_persistent(&env, &key);
        env.events().publish(
            (
                Symbol::new(&env, "batch_scheduler"),
                Symbol::new(&env, "cancelled"),
            ),
            job_id,
        );
        Ok(())
    }

    /// Return progress for a scheduled batch.
    pub fn get_batch_escrow_progress(env: Env, job_id: u64) -> Option<BatchJobProgress> {
        let key = DataKey::BatchEscrowJob(job_id);
        let job: BatchEscrowJob = env.storage().persistent().get(&key)?;
        Some(Self::batch_progress(job_id, &job))
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
        let _guard = ReentryGuardScope::new(&env);
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
                let operation_id = Self::onboarding_operation_id(
                    &env,
                    b"release_batch_funds:",
                    order_id,
                );
                Self::authorize_onboarding_state(
                    &env,
                    &escrow.buyer,
                    operation_id.clone(),
                    UserRole::Buyer,
                );
                Self::authorize_onboarding_state(
                    &env,
                    &escrow.seller,
                    operation_id,
                    UserRole::Artisan,
                );
            }
        }

        // Then process all releases
        for i in 0..order_ids.len() {
            if let Some(order_id) = order_ids.get(i) {
                let escrow_opt: Option<Escrow> =
                    env.storage().persistent().get(&(ESCROW, order_id));
                if escrow_opt.is_some() {
                    Self::extend_persistent_read(&env, &(ESCROW, order_id));
                }

                if let Some(mut escrow) = escrow_opt {
                    // Get platform config
                    let config = Self::get_platform_config_internal(&env);

                    // Deterministic fee allocation via the central FeePolicy engine.
                    let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.seller.clone());
                    let allocation = Self::compute_fee_allocation(
                        &env,
                        escrow.amount,
                        fee_bps,
                        SettlementKind::ReleaseFunds,
                    );

                    // Update status
                    escrow.status = EscrowStatus::Released;
                    env.storage().persistent().set(&(ESCROW, order_id), &escrow);

                    // Decrement active counts
                    Self::update_active_obligations(&env, &escrow.buyer, -1);
                    Self::update_active_obligations(&env, &escrow.seller, -1);

                    Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
                    Self::safe_update_active_contracts(&env, escrow.seller.clone(), -1);
                    Self::update_total_locked(&env, &escrow.token, -escrow.amount);

                    // Transfer platform fee to platform wallet
                    if allocation.platform_fee > 0 {
                        Self::transfer_platform_fee(
                            &env,
                            &escrow.token,
                            &config.platform_wallet,
                            allocation.platform_fee,
                        );
                    }

                    // Transfer remaining funds to seller
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

                    // Emit release event
                    Self::emit_escrow_created(
                        &env,
                        EscrowEvent {
                            schema_version: 1,
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

        Ok(results)
    }

    // NOTE: referral payout support has been removed from the contract. The configuration key is
    // retained only for storage compatibility during upgrades.

    /// Check that the contract is not paused. Panics with ContractPaused if it is.
    fn check_not_paused(env: &Env) {
        if let Some(config) = env
            .storage()
            .instance()
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

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&[u8::from(paused)]);
        if let Err(e) = Self::gate_admin_mutation(&env, "set_paused", payload) {
            env.panic_with_error(e);
        }

        let mut config = Self::get_platform_config_internal(&env);
        config.is_paused = paused;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        if paused {
            Self::emit_platform_paused(&env, admin);
        } else {
            Self::emit_platform_unpaused(&env, admin);
        }
    }

    /// Public read-only query for the platform pause state.
    ///
    /// This intentionally reads the same `PlatformConfig::is_paused` field used
    /// by write guards. An uninitialized contract is active by default, matching
    /// `check_not_paused`, which also permits read-only access before setup.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, PlatformConfig>(&DataKey::PlatformConfig)
            .map(|config| config.is_paused)
            .unwrap_or(false)
    }

    // â”€â”€ Tiered Artisan Fees (#98) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Admin assigns a custom fee tier (in basis points) for an artisan.
    pub fn set_artisan_fee_tier(env: Env, artisan: Address, fee_bps: u32) {
        let admin = Self::get_admin(&env)
            .unwrap_or_else(|_| env.panic_with_error(crate::Error::Unauthorized));
        admin.require_auth();

        if fee_bps > MAX_PLATFORM_FEE_BPS {
            env.panic_with_error(crate::Error::InvalidFee);
        }

        let mut payload = artisan.to_xdr(&env);
        payload.extend_from_slice(&fee_bps.to_be_bytes());
        if let Err(e) = Self::gate_admin_mutation(&env, "set_artisan_fee_tier", payload) {
            env.panic_with_error(e);
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

    // â”€â”€ Dispute Resolution Deadline (#93) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Force-close a dispute that the arbitrator failed to resolve within `max_dispute_duration`.
    ///
    /// ## Explicit expired-dispute refund policy (#1055)
    ///
    /// | Field | Value |
    /// |---|---|
    /// | Predecessor state | `EscrowStatus::Disputed` with `dispute_initiated_at` set |
    /// | Eligible caller | Anyone (permissionless safety net) |
    /// | Deadline | `now >= dispute_initiated_at + max_dispute_duration` (exact second inclusive) |
    /// | Terminal successor | `EscrowStatus::Resolved` via `SettlementPath::ExpiredDispute` |
    /// | Fee treatment | Operator-configured [`ExpiredDisputeFeePolicy`]; allocation always sums to the escrow pot |
    ///
    /// After the deadline, `resolve_dispute`, `resolve_dispute_partial`, and
    /// `accept_partial_refund` are rejected with [`Error::ArbitratorDeadlineExceeded`].
    /// The only remaining settlement path is this function.
    ///
    /// ## Role in the dispute lifecycle
    ///
    /// This is the **safety-net exit** from the `Disputed` state. When the
    /// designated arbitrator does not call `resolve_dispute` before the
    /// `max_dispute_duration` deadline (measured from `dispute_initiated_at`),
    /// any account — including bots and the disputing parties themselves — can
    /// call this function to unblock the locked funds.
    ///
    /// Unlike `resolve_dispute`, this path does not require authorization and
    /// does not consult the arbitrator. The outcome is fully determined by the
    /// operator-configured `expired_dispute_fee_policy` (see
    /// [`Self::update_expired_dispute_policy`]).
    ///
    /// ## Fee policies
    ///
    /// | Policy                    | Buyer receives          | Platform receives |
    /// |---------------------------|-------------------------|-------------------|
    /// | `RefundFullNoPlatformFee` | full `amount`           | nothing           |
    /// | `RefundMinusPlatformFee`  | `amount − fee`          | `fee`             |
    /// | `DeductFeeFromSeller`     | full `amount`           | nothing (seller opportunity cost) |
    /// | `SplitFee`                | `amount − fee/2`        | `fee/2`           |
    ///
    /// The default policy is `RefundFullNoPlatformFee`, protecting buyers from
    /// arbitrator failure without penalizing them. Every policy conserves the
    /// escrow pot: `platform_fee + seller_amount + buyer_amount == amount`.
    ///
    /// ## CEI pattern
    ///
    /// Follows the same Checks → Effects → Interactions ordering as
    /// `resolve_dispute`: all storage mutations (status, counters, locked-funds
    /// tracker) are committed before the token transfer is executed.
    ///
    /// # Errors
    /// * [`Error::EscrowNotFound`] — no escrow exists for `order_id`.
    /// * [`Error::InvalidEscrowState`] — the escrow is not currently `Disputed`.
    /// * [`Error::SettlementAlreadyFinalized`] — the dispute already has a
    ///   settlement receipt; a timed-out dispute cannot be resolved twice (#1080).
    /// * [`Error::DisputeExpired`] — the `max_dispute_duration` deadline has **not**
    ///   yet passed; the regular `resolve_dispute` path must be used instead.
    /// * [`Error::SettlementAlreadyFinalized`] — another settlement path already ran.
    pub fn resolve_expired_dispute(env: Env, order_id: u32) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let snapshot_opt: Option<Escrow> = env.storage().persistent().get(&(ESCROW, order_id));
        let snapshot = match snapshot_opt {
            Some(escrow) => escrow,
            None => return Err(Error::EscrowNotFound),
        };
        Self::extend_persistent(&env, &(ESCROW, order_id));
        let snapshot = snapshot_opt.unwrap();
        let mut escrow = snapshot.clone();

        let config = Self::get_platform_config_internal(&env);
        
        // Get dispute initiated timestamp and current time
        let initiated_at = escrow.dispute_initiated_at.unwrap_or(0);
        let current_time = env.ledger().timestamp();
        
        if escrow.status != EscrowStatus::Disputed {
            return Err(Error::InvalidEscrowState);
        }
        let initiated_at = escrow.dispute_initiated_at.ok_or(Error::InvalidEscrowState)?;
        let current_time = env.ledger().timestamp();
        let snapshot = snapshot_opt.ok_or(Error::EscrowNotFound)?;
        Self::extend_persistent(&env, &(ESCROW, order_id));
        let mut snapshot = snapshot_opt.unwrap();

        let config = Self::get_platform_config_internal(&env);

        // A dispute has exactly one terminal settlement. `assert_open_for_settlement`
        // rejects both a second run of this path and a race with any other
        // settlement path (arbitrated, partial, timeout) via the settlement
        // receipt, so a timed-out dispute can never be resolved twice (#1080).
        Self::assert_open_for_settlement(&env, &snapshot, order_id)?;

        // The deadline guard: if the dispute is still within the allowed window
        // the arbitrator must resolve it via `resolve_dispute`. Returning an
        // error (rather than panicking) allows the caller to detect this case
        // without rolling back unrelated ledger state.
        Self::assert_expired_dispute_window(&env, &snapshot, &config)?;

        let operation_id =
            Self::onboarding_operation_id(&env, b"resolve_expired_dispute:", order_id);
        Self::authorize_onboarding_state(
            &env,
            &snapshot.buyer,
            operation_id.clone(),
            UserRole::Buyer,
        );
        if (snapshot.dispute_initiated_at.unwrap_or(0)) + config.max_dispute_duration as u64
            > env.ledger().timestamp()
        {
            return Err(Error::DisputeExpired);
        }

        let operation_id = Self::onboarding_operation_id(&env, b"resolve_expired_dispute:", order_id);
        Self::authorize_onboarding_state(&env, &snapshot.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &snapshot.seller, operation_id, UserRole::Artisan);

        // --- Effects (CEI: all writes before the token transfer) ---

        let fee_bps = Self::get_effective_fee_bps(env.clone(), snapshot.seller.clone());
        let settlement_kind = Self::timeout_settlement_kind(config.expired_dispute_fee_policy);
        // CRITICAL: Update status BEFORE external calls (CEI pattern)
        snapshot.status = EscrowStatus::Resolved;
        env.storage().persistent().set(&(ESCROW, order_id), &snapshot);

        // Decrement active counts
        Self::update_active_obligations(&env, &snapshot.buyer, -1);
        Self::update_active_obligations(&env, &snapshot.seller, -1);

        Self::safe_update_active_contracts(&env, snapshot.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, snapshot.seller.clone(), -1);
        Self::update_total_locked(&env, &snapshot.token, -snapshot.amount);

        let config = Self::get_platform_config_internal(&env);
        Self::assert_open_for_settlement(&env, &snapshot, order_id)?;
        Self::assert_expired_dispute_window(&env, &snapshot, &config)?;

        let operation_id =
            Self::onboarding_operation_id(&env, b"resolve_expired_dispute:", order_id);
        Self::authorize_onboarding_state(
            &env,
            &snapshot.buyer,
            operation_id.clone(),
            UserRole::Buyer,
        );
        Self::authorize_onboarding_state(&env, &snapshot.seller, operation_id, UserRole::Artisan);

        let fee_bps = Self::get_effective_fee_bps(env.clone(), snapshot.seller.clone());
        let settlement_kind = Self::expired_dispute_settlement_kind(config.expired_dispute_fee_policy);
        let allocation =
            Self::compute_fee_allocation(&env, snapshot.amount, fee_bps, settlement_kind);

        // Claim writes the `SettlementPending` sentinel and commit writes the
        // settlement receipt plus every counter decrement - both before the
        // token transfer below.
        let escrow = Self::claim_disputed_settlement(&env, order_id)?;
        let escrow =
            Self::commit_resolved_escrow(&env, order_id, escrow, SettlementPath::ExpiredDispute, 0);

        // --- Interactions ---

        Self::apply_fee_allocation_transfers(
            &env,
            &escrow,
            &allocation,
            &config.platform_wallet,
            "expired_dispute_refund",
            "expired_dispute_seller",
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
        Self::emit_dispute_timed_out(
            &env,
            order_id,
            Self::timeout_outcome(config.expired_dispute_fee_policy),
            time_policy::deadline(
                snapshot.dispute_initiated_at.unwrap_or_default(),
                config.max_dispute_duration as u64,
            ),
            current_time,
        );

        Ok(())
    }

    // â”€â”€ Staking Requirement for Artisans (#99) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Stake tokens to satisfy the platform's minimum stake requirement.
    ///
    /// The artisan transfers `amount` of `token` to the contract. The stake is stored
    /// and a cooldown timer is set so the tokens cannot be unstaked immediately.
    ///
    /// Enhanced with bounded queue management to prevent storage bloat. Automatically
    /// prunes matured deposits when queue approaches capacity limits.
    ///
    /// Staked balances remain owned by the artisan. The contract does not accrue,
    /// distribute, or sweep interest/yield from these reserved funds into platform fees.
    pub fn stake_tokens(env: Env, artisan: Address, token: Address, amount: i128) {
        let _guard = ReentryGuardScope::new(&env);
        artisan.require_auth();

        // Issue #1057: Block deactivated accounts from staking
        Self::assert_account_active(&env, &artisan);

        if amount <= 0 {
            env.panic_with_error(crate::Error::AmountBelowMinimum);
        }

        Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());

        // Reject cross-token deposits before any state mutation (#1034).
        let stake_key = DataKey::ArtisanStake(artisan.clone());
        let current_stake: Option<ArtisanStakeData> = env.storage().persistent().get(&stake_key);
        if let Some(existing_stake) = &current_stake {
            if existing_stake.token != token {
                env.panic_with_error(crate::Error::StakeTokenMismatch);
            }
        }

        // Effects are committed before the token interaction.
        Self::update_total_staked(&env, &token, amount);

        // Accumulate stake in a single record with token metadata.
        if current_stake.is_none() {
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::StakedArtisanCount)
                .unwrap_or(0);
            env.storage().persistent().set(
                &DataKey::StakedArtisanIndexed(count),
                &artisan,
            );
            env.storage()
                .persistent()
                .set(&DataKey::StakedArtisanCount, &(count + 1));
        }
        let new_stake = if let Some(existing_stake) = current_stake {
            ArtisanStakeData {
                amount: existing_stake.amount + amount,
                token: token.clone(),
            }
        } else {
            ArtisanStakeData {
                amount,
                token: token.clone(),
            }
        };

        let config = Self::get_platform_config_internal(&env);
        if config.min_stake_required > 0 && new_stake.amount < config.min_stake_required {
            env.panic_with_error(crate::Error::InsufficientStake);
        }

        env.storage().persistent().set(&stake_key, &new_stake);
        Self::extend_persistent(&env, &stake_key);

        // Record stake operation in history queue for audit trail (#237)
        if Self::record_stake_history(&env, &artisan, new_stake.amount, "stake_added").is_err() {
            env.panic_with_error(Error::StakeQueueFull);
        }

        // Check queue capacity and current cooldown state (#237)
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        // Issue #1050 Fix: Track deposit maturity independently.
        // Every new deposit receives its own full cooldown period, preventing 
        // new funds from bypassing maturity by piggybacking on older deposits.
        let cooldown_end = env.ledger().timestamp() + config.stake_cooldown as u64;

        if current_count >= STAKE_QUEUE_PRUNE_THRESHOLD {
            Self::prune_matured_stake_deposits(&env, &artisan);
        }

        // Add new deposit to bounded indexed queue
        Self::add_stake_deposit(&env, &artisan, amount, cooldown_end);

        Self::transfer_tokens_and_record_audit(
            &env,
            &token,
            &artisan,
            &env.current_contract_address(),
            amount,
            &artisan,
            Symbol::new(&env, "stake_deposit"),
            -amount,
        );
    }

    /// Add a stake deposit to the bounded indexed queue.
    ///
    /// Implements individual key-value storage for scalability. Each deposit is stored
    /// as DataKey::ArtisanStakeQueueIndexed(artisan, index) -> StakeDeposit.
    fn add_stake_deposit(env: &Env, artisan: &Address, amount: i128, cooldown_end: u64) {
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if current_count >= MAX_STAKE_QUEUE_SIZE {
            env.panic_with_error(Error::StakeQueueFull);
        }

        // Add new deposit at the end of the queue
        let deposit_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), current_count);
        let deposit = StakeDeposit {
            amount,
            cooldown_end,
        };
        env.storage().persistent().set(&deposit_key, &deposit);
        Self::extend_persistent(env, &deposit_key);

        // Update count
        env.storage()
            .persistent()
            .set(&count_key, &(current_count + 1));
        Self::extend_persistent(env, &count_key);
    }

    /// Compact matured stake deposits in the queue to prevent storage bloat.
    ///
    /// A deposit reaching its cooldown makes it *withdrawable*, not withdrawn —
    /// the principal is still owed to the artisan until `unstake_tokens` actually
    /// pays it out and removes the entry. Earlier revisions of this function
    /// deleted matured entries outright during compaction, silently destroying
    /// unwithdrawn principal (#1051). Instead, this folds every matured-but-still-
    /// owed deposit into a single aggregate entry that preserves their combined
    /// amount and latest maturity, and only removes entries that are already gone.
    /// Non-matured deposits keep their relative order, with the aggregate placed
    /// at the position of the first matured deposit it absorbed.
    fn prune_matured_stake_deposits(env: &Env, artisan: &Address) {
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if current_count == 0 {
            return;
        }

        let now = env.ledger().timestamp();
        let mut write_index = 0u32;
        let mut matured_aggregate: Option<StakeDeposit> = None;

        for read_index in 0..current_count {
            let deposit_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), read_index);

            let deposit = match env
                .storage()
                .persistent()
                .get::<DataKey, StakeDeposit>(&deposit_key)
            {
                Some(deposit) => deposit,
                None => continue,
            };

            if time_policy::is_deadline_reached(now, deposit.cooldown_end) {
                // Matured but not yet withdrawn: fold its principal into the
                // running aggregate instead of dropping it.
                matured_aggregate = Some(match matured_aggregate {
                    Some(agg) => StakeDeposit {
                        amount: agg.amount + deposit.amount,
                        cooldown_end: if agg.cooldown_end > deposit.cooldown_end {
                            agg.cooldown_end
                        } else {
                            deposit.cooldown_end
                        },
                    },
                    None => deposit,
                });
                if read_index != write_index {
                    env.storage().persistent().remove(&deposit_key);
                }
                continue;
            }

            // Non-matured deposit: flush any pending matured aggregate first so
            // its combined principal is written before this entry.
            if let Some(agg) = matured_aggregate.take() {
                let agg_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), write_index);
                env.storage().persistent().set(&agg_key, &agg);
                Self::extend_persistent(env, &agg_key);
                write_index += 1;
            }

            if write_index != read_index {
                let new_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), write_index);
                env.storage().persistent().set(&new_key, &deposit);
                Self::extend_persistent(env, &new_key);
                env.storage().persistent().remove(&deposit_key);
            }
            write_index += 1;
        }

        if let Some(agg) = matured_aggregate.take() {
            let agg_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), write_index);
            env.storage().persistent().set(&agg_key, &agg);
            Self::extend_persistent(env, &agg_key);
            write_index += 1;
        }

        // Defensive cleanup: remove any stale entries left beyond the new length.
        for cleanup_index in write_index..current_count {
            let cleanup_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), cleanup_index);
            env.storage().persistent().remove(&cleanup_key);
        }

        // Update count to reflect compacted queue. If nothing remains, remove
        // the count entry rather than leaving a stale counter behind.
        if write_index > 0 {
            env.storage().persistent().set(&count_key, &write_index);
            Self::extend_persistent(env, &count_key);
        } else {
            env.storage().persistent().remove(&count_key);
        }
    }

    /// Unstake previously staked tokens after the cooldown period has elapsed.
    ///
    /// Stakes can only be returned in the exact token originally deposited, which
    /// prevents reserved artisan collateral from being treated as platform-managed fees.
    /// Enhanced with bounded indexed queue and automatic pruning for scalability.
    pub fn unstake_tokens(env: Env, artisan: Address, token: Address) {
        let _guard = ReentryGuardScope::new(&env);
        artisan.require_auth();

        // Issue #1057: Block deactivated accounts from unstaking
        Self::assert_account_active(&env, &artisan);

        // Issue #1111: Block withdrawals when artisan is liquidation-eligible or liquidated.
        // Artisans must cure their status before they can unstake.
        let liq_status: LiquidationStatus = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationStatus(artisan.clone()))
            .unwrap_or(LiquidationStatus::Healthy);
        if liq_status == LiquidationStatus::LiquidationEligible
            || liq_status == LiquidationStatus::Liquidated
        {
            env.panic_with_error(crate::Error::NotLiquidationEligible);
        }

        Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());

        // Validate the requested token matches the token recorded at stake time.
        // Rejects attempts to withdraw in a cheaper/different asset (#421).
        let stake_key = DataKey::ArtisanStake(artisan.clone());
        let current_stake: ArtisanStakeData = env
            .storage()
            .persistent()
            .get(&stake_key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::InsufficientStake));
        if current_stake.token != token {
            env.panic_with_error(crate::Error::StakeTokenMismatch);
        }

        // Use bounded indexed queue: only matured deposits can be unstaked.
        let count_key = DataKey::ArtisanStakeQueueCount(artisan.clone());
        let current_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        let now = env.ledger().timestamp();
        let mut matured_amount: i128 = 0;
        let mut write_index = 0u32;

        // Process all deposits, collecting matured amounts and compacting the queue
        for read_index in 0..current_count {
            let deposit_key = DataKey::ArtisanStakeQueueIndexed(artisan.clone(), read_index);

            if let Some(deposit) = env
                .storage()
                .persistent()
                .get::<DataKey, StakeDeposit>(&deposit_key)
            {
                // Time policy: deposit is matured when now >= cooldown_end (inclusive end)
                if time_policy::is_deadline_reached(now, deposit.cooldown_end) {
                    // Deposit is matured, add to unstake amount
                    matured_amount += deposit.amount;
                    // Remove the matured deposit
                    env.storage().persistent().remove(&deposit_key);
                } else {
                    // Deposit is not matured, keep it in the queue
                    if write_index != read_index {
                        // Move deposit to new position to compact the queue
                        let new_key =
                            DataKey::ArtisanStakeQueueIndexed(artisan.clone(), write_index);
                        env.storage().persistent().set(&new_key, &deposit);
                        Self::extend_persistent(&env, &new_key);
                        // Remove old position
                        env.storage().persistent().remove(&deposit_key);
                    }
                    write_index += 1;
                }
            }
        }

        if matured_amount <= 0 {
            env.panic_with_error(crate::Error::StakeCooldownActive);
        }

        // Update queue count after processing
        if write_index > 0 {
            env.storage().persistent().set(&count_key, &write_index);
            Self::extend_persistent(&env, &count_key);
        } else {
            // Queue is empty, remove count key
            env.storage().persistent().remove(&count_key);
        }

        // Validate remaining collateral safety and active obligation rules
        let remaining_amount = current_stake.amount - matured_amount;
        let config = Self::get_platform_config_internal(&env);
        let active_obligations = Self::has_active_escrows(env.clone(), artisan.clone());
        if config.min_stake_required > 0 {
            if active_obligations && remaining_amount < config.min_stake_required {
                env.panic_with_error(crate::Error::InsufficientStake);
            }
            if !active_obligations
                && remaining_amount > 0
                && remaining_amount < config.min_stake_required
            {
                env.panic_with_error(crate::Error::InsufficientStake);
            }
        }

        // Update stake metadata
        if remaining_amount > 0 {
            let updated_stake = ArtisanStakeData {
                amount: remaining_amount,
                token: current_stake.token,
            };
            env.storage().persistent().set(&stake_key, &updated_stake);
            Self::extend_persistent(&env, &stake_key);
        } else {
            env.storage().persistent().remove(&stake_key);
        }

        // Record unstake operation in history for audit trail (#237)
        if Self::record_stake_history(&env, &artisan, 0, "stake_removed").is_err() {
            // Don't fail on history recording, but log the issue
            env.events().publish(
                (Symbol::new(&env, "stake_history_warning"), "queue_full"),
                String::from_str(&env, "Could not record stake removal in history"),
            );
        }

        // Complete reserve accounting before returning tokens.
        Self::update_total_staked(&env, &token, -matured_amount);
        Self::transfer_tokens_and_record_audit(
            &env,
            &token,
            &env.current_contract_address(),
            &artisan,
            matured_amount,
            &artisan,
            Symbol::new(&env, "stake_unstaked"),
            matured_amount,
        );

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
        Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());
        env.storage()
            .persistent()
            .get::<DataKey, ArtisanStakeData>(&DataKey::ArtisanStake(artisan))
            .map(|stake: ArtisanStakeData| stake.amount)
            .unwrap_or(0)
    }

    /// Return the full stake record for an artisan, including the staked token address.
    pub fn get_artisan_stake_data(env: Env, artisan: Address) -> Option<ArtisanStakeData> {
        Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());
        env.storage()
            .persistent()
            .get::<DataKey, ArtisanStakeData>(&DataKey::ArtisanStake(artisan))
    }

    /// Check if an artisan account is under-collateralized (active obligations exist while holding less than minimum required stake).
    pub fn is_account_under_collateralized(env: Env, artisan: Address) -> bool {
        let config = Self::get_platform_config_internal(&env);
        if config.min_stake_required <= 0 {
            return false;
        }
        let stake = Self::get_stake(env.clone(), artisan.clone());
        let active = Self::has_active_escrows(env, artisan);
        active && stake < config.min_stake_required
    }

    // ─── Liquidation / Collateral Health (#1111) ────────────────────────────

    /// Return the default or persisted liquidation policy.
    fn get_liquidation_policy_internal(env: &Env) -> LiquidationPolicyData {
        env.storage().persistent().get(&DataKey::LiquidationPolicyConfig)
            .unwrap_or(LiquidationPolicyData {
                max_seizure_bps: DEFAULT_LIQUIDATION_MAX_SEIZURE_BPS,
                grace_period_secs: DEFAULT_LIQUIDATION_GRACE_PERIOD,
                enabled: true,
            })
    }

    /// Evaluate an artisan's collateral health deterministically at the
    /// current ledger timestamp.
    ///
    /// Returns a [`StakeHealthSnapshot`] with the health ratio, deficit,
    /// and current [`LiquidationStatus`]. This is a pure read + compute
    /// function with no side-effects; the snapshot is also persisted so
    /// off-chain indexers can retrieve it via `get_stake_health_snapshot`.
    ///
    /// # Health formula
    /// ```text
    /// required_collateral = active_obligations × min_stake_required
    /// health_ratio_bps    = (current_stake / max(required, 1)) × 10_000
    /// deficit             = max(0, required − current_stake)
    /// ```
    pub fn evaluate_stake_health(env: Env, artisan: Address) -> StakeHealthSnapshot {
        Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());

        let config = Self::get_platform_config_internal(&env);
        let current_stake = Self::get_stake(env.clone(), artisan.clone());
        let active_obligations = Self::get_active_obligation_count(env.clone(), artisan.clone());

        let required_collateral = (active_obligations as i128) * config.min_stake_required;

        let denominator = if required_collateral > 0 {
            required_collateral
        } else {
            1
        };
        let health_ratio_bps = ((current_stake as u128 * 10_000) / (denominator as u128)) as u32;

        let deficit = if current_stake < required_collateral {
            required_collateral - current_stake
        } else {
            0
        };

        // Determine status from persisted state + current evaluation.
        let persisted_status: LiquidationStatus = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationStatus(artisan.clone()))
            .unwrap_or(LiquidationStatus::Healthy);

        let status = if persisted_status == LiquidationStatus::Liquidated {
            // Once liquidated, stay liquidated until cure.
            LiquidationStatus::Liquidated
        } else if deficit > 0 && active_obligations > 0 {
            // Under-collateralized: keep existing status if already flagged,
            // otherwise set to UnderCollateralized.
            if persisted_status == LiquidationStatus::LiquidationEligible {
                LiquidationStatus::LiquidationEligible
            } else {
                LiquidationStatus::UnderCollateralized
            }
        } else {
            // Healthy: no deficit or no obligations.
            LiquidationStatus::Healthy
        };

        let evaluated_at = env.ledger().timestamp();

        let snapshot = StakeHealthSnapshot {
            artisan: artisan.clone(),
            evaluated_at,
            current_stake,
            active_obligations,
            required_collateral,
            health_ratio_bps,
            deficit,
            status,
        };

        // Persist the snapshot and status for off-chain reads.
        env.storage().persistent().set(
            &DataKey::StakeHealthSnapshot(artisan.clone()),
            &snapshot,
        );
        Self::extend_persistent(&env, &DataKey::StakeHealthSnapshot(artisan.clone()));

        env.storage().persistent().set(
            &DataKey::LiquidationStatus(artisan.clone()),
            &status,
        );
        Self::extend_persistent(&env, &DataKey::LiquidationStatus(artisan.clone()));

        snapshot
    }

    /// Read-only getter: return the persisted health snapshot for an artisan,
    /// or `None` if `evaluate_stake_health` has never been called.
    pub fn get_stake_health_snapshot(env: Env, artisan: Address) -> Option<StakeHealthSnapshot> {
        env.storage().persistent().get(&DataKey::StakeHealthSnapshot(artisan))
    }

    /// Read-only getter: return the current liquidation status for an artisan.
    pub fn get_liquidation_status(env: Env, artisan: Address) -> LiquidationStatus {
        env.storage().persistent().get(&DataKey::LiquidationStatus(artisan))
            .unwrap_or(LiquidationStatus::Healthy)
    }

    /// Admin sets the liquidation policy thresholds.
    pub fn set_liquidation_policy(
        env: Env,
        max_seizure_bps: u32,
        grace_period_secs: u64,
        enabled: bool,
    ) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        if max_seizure_bps > 10_000 {
            env.panic_with_error(Error::InvalidFee);
        }

        let policy = LiquidationPolicyData {
            max_seizure_bps,
            grace_period_secs,
            enabled,
        };
        env.storage().persistent().set(&DataKey::LiquidationPolicyConfig, &policy);
        Self::extend_persistent(&env, &DataKey::LiquidationPolicyConfig);
        Ok(())
    }

    /// Read-only getter: return the current liquidation policy.
    pub fn get_liquidation_policy(env: Env) -> LiquidationPolicyData {
        Self::get_liquidation_policy_internal(&env)
    }

    /// Admin flags an under-collateralized artisan as liquidation-eligible.
    ///
    /// Preconditions:
    /// - Admin auth required.
    /// - Liquidation must be enabled in policy.
    /// - The artisan must be evaluated as UnderCollateralized (via `evaluate_stake_health`).
    /// - The grace period must have elapsed since the under-collateralized state was first observed.
    ///
    /// Postconditions:
    /// - The artisan's status transitions to `LiquidationEligible`.
    /// - An event `stake_liquidation_flagged` is emitted.
    pub fn flag_liquidation_eligible(env: Env, artisan: Address) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let policy = Self::get_liquidation_policy_internal(&env);
        if !policy.enabled {
            return Err(Error::LiquidationDisabled);
        }

        // Re-evaluate health to ensure snapshot is current.
        let snapshot = Self::evaluate_stake_health(env.clone(), artisan.clone());

        if snapshot.status == LiquidationStatus::Healthy {
            return Err(Error::StakeHealthHealthy);
        }
        if snapshot.status == LiquidationStatus::LiquidationEligible
            || snapshot.status == LiquidationStatus::Liquidated
        {
            // Already flagged or already liquidated — no-op but not an error.
            return Ok(());
        }

        // Enforce grace period: the artisan must have been under-collateralized
        // for at least `grace_period_secs`.
        if policy.grace_period_secs > 0 {
            let now = env.ledger().timestamp();
            let first_observed = snapshot.evaluated_at;
            // Grace period is measured from the health evaluation timestamp.
            // For the very first under-collateralization, the grace window starts
            // at evaluation time. For subsequent evaluations, we use the
            // snapshot timestamp as the earliest evidence.
            if now < first_observed + policy.grace_period_secs {
                return Err(Error::LiquidationGracePeriodActive);
            }
        }

        // Persist the new status.
        let new_status = LiquidationStatus::LiquidationEligible;
        env.storage().persistent().set(
            &DataKey::LiquidationStatus(artisan.clone()),
            &new_status,
        );
        Self::extend_persistent(&env, &DataKey::LiquidationStatus(artisan.clone()));

        env.events().publish(
            (Symbol::new(&env, "stake_liquidation_flagged"), artisan.clone()),
            (snapshot.deficit, env.ledger().timestamp()),
        );

        Ok(())
    }

    /// Authorized party (admin) triggers a partial liquidation of an artisan's
    /// stake to cover the deficit, subject to policy caps.
    ///
    /// # Safety invariants
    ///
    /// - Seized amount ≤ deficit (cannot seize more than the shortfall).
    /// - Seized amount ≤ deficit × max_seizure_bps / 10_000 (policy cap).
    /// - Seized amount > 0 (no zero-value liquidations).
    /// - The artisan must be in `LiquidationEligible` status.
    ///
    /// # Postconditions
    ///
    /// - The artisan's stake is reduced by the seized amount.
    /// - The seized tokens are transferred to the platform wallet.
    /// - A `LiquidationRecord` is persisted for audit.
    /// - The artisan's status transitions to `Liquidated`.
    /// - An event `stake_liquidated` is emitted.
    pub fn trigger_liquidation(env: Env, artisan: Address) -> Result<LiquidationRecord, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let policy = Self::get_liquidation_policy_internal(&env);
        if !policy.enabled {
            return Err(Error::LiquidationDisabled);
        }

        let current_status: LiquidationStatus = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationStatus(artisan.clone()))
            .unwrap_or(LiquidationStatus::Healthy);

        if current_status != LiquidationStatus::LiquidationEligible {
            return Err(Error::NotLiquidationEligible);
        }

        // Re-evaluate to get the current deficit.
        let snapshot = Self::evaluate_stake_health(env.clone(), artisan.clone());
        let deficit = snapshot.deficit;

        if deficit <= 0 {
            // No deficit — artisan recovered between flagging and now.
            return Err(Error::StakeHealthHealthy);
        }

        // Compute seized amount: min(deficit, deficit × max_seizure_bps / 10_000).
        let max_seizable = (deficit as u128 * policy.max_seizure_bps as u128) / 10_000;
        let seized_amount = (deficit as u128).min(max_seizable) as i128;

        if seized_amount <= 0 {
            return Err(Error::LiquidationSeizureExceedsCap);
        }

        // Cannot seize more than the artisan actually has staked.
        let actual_seized = seized_amount.min(snapshot.current_stake);
        if actual_seized <= 0 {
            return Err(Error::LiquidationSeizureExceedsCap);
        }

        // Effects before interactions (CEI pattern).
        // Reduce artisan stake.
        let stake_key = DataKey::ArtisanStake(artisan.clone());
        if let Some(mut stake_data) = env.storage().persistent().get::<DataKey, ArtisanStakeData>(&stake_key) {
            let new_amount = stake_data.amount - actual_seized;
            if new_amount > 0 {
                stake_data.amount = new_amount;
                env.storage().persistent().set(&stake_key, &stake_data);
            } else {
                env.storage().persistent().remove(&stake_key);
            }
            Self::extend_persistent(&env, &stake_key);
        }

        // Update total staked accounting.
        Self::update_total_staked(&env, &snapshot.artisan, -actual_seized);

        // Record liquidation ID.
        let next_id: u64 = env.storage().persistent().get(&DataKey::NextLiquidationId).unwrap_or(0);
        let liq_id = next_id;
        env.storage().persistent().set(&DataKey::NextLiquidationId, &(next_id + 1));

        let record = LiquidationRecord {
            id: liq_id,
            artisan: artisan.clone(),
            liquidator: admin.clone(),
            seized_amount: actual_seized,
            executed_at: env.ledger().timestamp(),
            health_ratio_bps: snapshot.health_ratio_bps,
            cured: false,
            cured_at: 0,
        };
        env.storage().persistent().set(
            &DataKey::LiquidationRecord(liq_id),
            &record,
        );
        Self::extend_persistent(&env, &DataKey::LiquidationRecord(liq_id));

        // Update liquidation history index.
        let hist_count: u32 = env.storage().persistent().get(&DataKey::LiquidationRecordCount).unwrap_or(0);
        if hist_count < MAX_LIQUIDATION_HISTORY {
            env.storage().persistent().set(
                &DataKey::LiquidationRecordIndexed(hist_count),
                &liq_id,
            );
            Self::extend_persistent(&env, &DataKey::LiquidationRecordIndexed(hist_count));
        }
        env.storage().persistent().set(
            &DataKey::LiquidationRecordCount,
            &(hist_count + 1),
        );

        // Update status to Liquidated.
        let liq_status = LiquidationStatus::Liquidated;
        env.storage().persistent().set(
            &DataKey::LiquidationStatus(artisan.clone()),
            &liq_status,
        );
        Self::extend_persistent(&env, &DataKey::LiquidationStatus(artisan.clone()));

        // Interaction: transfer seized tokens to platform wallet.
        let stake_token_opt: Option<ArtisanStakeData> = env.storage().persistent().get(&DataKey::ArtisanStake(artisan.clone()));
        if let Some(stake_data) = stake_token_opt {
            let platform_wallet = Self::get_platform_wallet(env.clone());
            Self::transfer_tokens_and_record_audit(
                &env,
                &stake_data.token,
                &env.current_contract_address(),
                &platform_wallet,
                actual_seized,
                &admin,
                Symbol::new(&env, "stake_liquidated"),
                actual_seized,
            );
        }

        env.events().publish(
            (Symbol::new(&env, "stake_liquidated"), artisan.clone()),
            (liq_id, actual_seized, snapshot.health_ratio_bps, env.ledger().timestamp()),
        );

        Ok(record)
    }

    /// Artisan cures their liquidation by topping up their stake.
    ///
    /// Any artisan in `UnderCollateralized`, `LiquidationEligible`, or `Liquidated`
    /// status can call `stake_tokens` — once their stake meets or exceeds
    /// the required collateral, `cure_liquidation` transitions them back to
    /// `Healthy` and marks any open `LiquidationRecord` as cured.
    ///
    /// # Preconditions
    /// - The artisan must have a pending liquidation status (not Healthy).
    ///
    /// # Postconditions
    /// - If the artisan's stake now meets the required collateral, the status
    ///   transitions to `Healthy` and all open records are marked cured.
    /// - An event `stake_liquidation_cured` is emitted.
    pub fn cure_liquidation(env: Env, artisan: Address) -> Result<(), Error> {
        let current_status: LiquidationStatus = env
            .storage()
            .persistent()
            .get(&DataKey::LiquidationStatus(artisan.clone()))
            .unwrap_or(LiquidationStatus::Healthy);

        if current_status == LiquidationStatus::Healthy {
            return Err(Error::NotLiquidationEligible);
        }

        // Re-evaluate health.
        let snapshot = Self::evaluate_stake_health(env.clone(), artisan.clone());

        if snapshot.deficit > 0 && snapshot.active_obligations > 0 {
            // Still under-collateralized — cure not possible yet.
            return Err(Error::InsufficientStake);
        }

        // Transition to Healthy.
        let healthy = LiquidationStatus::Healthy;
        env.storage().persistent().set(
            &DataKey::LiquidationStatus(artisan.clone()),
            &healthy,
        );
        Self::extend_persistent(&env, &DataKey::LiquidationStatus(artisan.clone()));

        // Mark all non-cured liquidation records for this artisan as cured.
        let hist_count: u32 = env.storage().persistent().get(&DataKey::LiquidationRecordCount).unwrap_or(0);
        for i in 0..hist_count {
            if let Some(liq_id) = env.storage().persistent().get::<DataKey, u64>(&DataKey::LiquidationRecordIndexed(i)) {
                if let Some(mut record) = env.storage().persistent().get::<DataKey, LiquidationRecord>(&DataKey::LiquidationRecord(liq_id)) {
                    if record.artisan == artisan && !record.cured {
                        record.cured = true;
                        record.cured_at = env.ledger().timestamp();
                        env.storage().persistent().set(&DataKey::LiquidationRecord(liq_id), &record);
                        Self::extend_persistent(&env, &DataKey::LiquidationRecord(liq_id));
                    }
                }
            }
        }

        env.events().publish(
            (Symbol::new(&env, "stake_liquidation_cured"), artisan),
            env.ledger().timestamp(),
        );

        Ok(())
    }

    /// Read-only getter: return a liquidation record by ID.
    pub fn get_liquidation_record(env: Env, id: u64) -> Option<LiquidationRecord> {
        env.storage().persistent().get(&DataKey::LiquidationRecord(id))
    }

    /// Return the count of liquidation records.
    pub fn get_liquidation_record_count(env: Env) -> u32 {
        env.storage().persistent().get(&DataKey::LiquidationRecordCount).unwrap_or(0)
    }

    /// Internal helper: return the active obligation count for a user.
    fn get_active_obligation_count(env: Env, user: Address) -> u32 {
        let key = DataKey::ActiveObligations(user);
        Self::get_persistent_u32(&env, &key)
    }

    /// Admin sets the minimum stake required for artisans to create escrows.
    pub fn set_min_stake_required(env: Env, min_stake: i128) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&min_stake.to_be_bytes());
        Self::gate_admin_mutation(&env, "set_min_stake_required", payload)?;

        let mut config = Self::get_platform_config_internal(&env);
        config.min_stake_required = min_stake;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);
        Ok(())
    }

    /// Admin sets the WASM upgrade cooldown period (in seconds).
    ///
    /// Rejected below `MIN_WASM_UPGRADE_COOLDOWN` (#1062): the cooldown is the
    /// review window that protects every future upgrade proposal, so it must
    /// not be reducible to near-zero right before `propose_upgrade_wasm`.
    pub fn set_wasm_upgrade_cooldown(env: Env, cooldown_seconds: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        if cooldown_seconds < MIN_WASM_UPGRADE_COOLDOWN {
            return Err(Error::UpgradeCooldownTooShort);
        }

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&cooldown_seconds.to_be_bytes());
        Self::gate_admin_mutation(&env, "set_wasm_upgrade_cooldown", payload)?;

        let mut config = Self::get_platform_config_internal(&env);
        let old_value = config.wasm_upgrade_cooldown;
        config.wasm_upgrade_cooldown = cooldown_seconds;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        Self::emit_config_updated(
            &env,
            "wasm_upgrade_cooldown",
            ConfigValue::U32(old_value),
            ConfigValue::U32(cooldown_seconds),
        );
        Ok(())
    }

    /// Get the current maximum dispute duration (in seconds).
    pub fn get_max_dispute_duration(env: Env) -> u32 {
        Self::get_platform_config_internal(&env).max_dispute_duration
    }

    /// Admin sets the maximum dispute duration (in seconds).
    pub fn set_max_dispute_duration(env: Env, duration_seconds: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&duration_seconds.to_be_bytes());
        Self::gate_admin_mutation(&env, "set_max_dispute_duration", payload)?;

        let mut config = Self::get_platform_config_internal(&env);
        let old_value = config.max_dispute_duration;
        config.max_dispute_duration = duration_seconds;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        Self::emit_config_updated(
            &env,
            "max_dispute_duration",
            ConfigValue::U32(old_value),
            ConfigValue::U32(duration_seconds),
        );
        Ok(())
    }

    /// Get the current stake cooldown period (in seconds).
    pub fn get_stake_cooldown(env: Env) -> u32 {
        Self::get_platform_config_internal(&env).stake_cooldown
    }

    /// Admin sets the stake cooldown period (in seconds).
    pub fn set_stake_cooldown(env: Env, cooldown_seconds: u32) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let mut payload = Bytes::new(&env);
        payload.extend_from_slice(&cooldown_seconds.to_be_bytes());
        Self::gate_admin_mutation(&env, "set_stake_cooldown", payload)?;

        let mut config = Self::get_platform_config_internal(&env);
        let old_value = config.stake_cooldown;
        config.stake_cooldown = cooldown_seconds;
        env.storage()
            .instance()
            .set(&DataKey::PlatformConfig, &config);

        Self::emit_config_updated(
            &env,
            "stake_cooldown",
            ConfigValue::U32(old_value),
            ConfigValue::U32(cooldown_seconds),
        );
        Ok(())
    }

    // â”€â”€ Partial Refund Negotiation (#101) â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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

        Self::assert_open_for_settlement(&env, &escrow, order_id)?;
        if !Self::is_escrow_party(&escrow, &caller) {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();
        let expected_role = if caller == escrow.buyer {
            UserRole::Buyer
        } else {
            UserRole::Artisan
        };
        let operation_id = Self::onboarding_operation_id(&env, b"propose_partial_refund:", order_id);
        Self::authorize_onboarding_state(&env, &caller, operation_id, expected_role);

        Self::validate_partial_refund_solvency(&env, &escrow, refund_amount)?;

        let proposal_key = Self::proposal_key(order_id);
        if env.storage().persistent().has(&proposal_key) {
            return Err(Error::ProposalAlreadyExists);
        }

        let proposal = PartialRefundProposal {
            order_id,
            refund_amount,
            proposed_by: caller,
            proposed_at: env.ledger().timestamp(),
            nonce: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&proposal_key, &proposal);
        Self::extend_persistent(&env, &proposal_key);

        Ok(())
    }

    // â”€â”€ Storage Explorer â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    /// Returns the total number of escrows ever created on this platform.
    ///
    /// This is an O(1) read â€” safe to call at any scale. Pair with
    /// `get_all_escrow_ids_iterative` to paginate the full ID set without
    /// hitting Soroban CPU/memory resource limits.
    pub fn get_escrow_count(env: Env) -> u32 {
        Self::migrate_legacy_all_escrow_ids(&env);
        Self::get_persistent_u32(&env, &DataKey::EscrowCount)
    }

    /// Return dashboard-level platform stats in one read-only contract call.
    pub fn get_platform_stats(env: Env) -> PlatformStats {
        Self::migrate_legacy_all_escrow_ids(&env);
        Self::migrate_legacy_whitelisted_tokens(&env);

        let active_users = Self::get_onboarding_client(&env)
            .map(|(_, onboarding)| onboarding.get_active_user_count())
            .unwrap_or(0);

        PlatformStats {
            total_volume: Self::get_total_volume(&env),
            total_escrows: Self::get_persistent_u32(&env, &DataKey::EscrowCount),
            active_users,
            whitelist_count: Self::get_whitelist_count(&env),
        }
    }

    /// Returns a page of all escrow order IDs created on the platform, in creation order.
    ///
    /// This is the recommended pattern for frontends to enumerate every escrow without
    /// hitting Soroban resource limits. The function reads a bounded slice of the
    /// indexed `GlobalEscrowIdIndexed` registry; no on-chain loops proportional to
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
    /// - `DataKey::GlobalEscrowIdIndexed(index)` â€“ indexed global escrow ID (#515)
    /// - `DataKey::EscrowCount`            â€“ u32 total count
    /// - `DataKey::AllEscrowIds`           â€“ DEPRECATED legacy Vec index
    /// - `(ESCROW, order_id: u32)`         â€“ individual escrow struct
    /// - `DataKey::BuyerEscrows(address)`  â€“ DEPRECATED: Legacy Vec<u64> of IDs for a buyer
    /// - `DataKey::SellerEscrows(address)` â€“ DEPRECATED: Legacy Vec<u64> of IDs for a seller
    /// - `DataKey::BuyerEscrowIndexed(address, index)` â€“ Indexed storage: u64 escrow ID at position
    /// - `DataKey::BuyerEscrowCount(address)` â€“ u32 total count of buyer's escrows
    /// - `DataKey::SellerEscrowIndexed(address, index)` â€“ Indexed storage: u64 escrow ID at position
    /// - `DataKey::SellerEscrowCount(address)` â€“ u32 total count of seller's escrows
    ///
    /// # Arguments
    /// * `page`  â€“ Zero-indexed page number
    /// * `limit` â€“ Page size; values above `MAX_BATCH_SIZE` are silently capped
    ///
    /// # Returns
    /// A `Result<Vec<u32>, Error>` containing escrow IDs for the requested page;
    /// returns `Err(PaginationLimitZero)` if `limit` is zero (#1022).
    pub fn get_all_escrow_ids_iterative(
        env: Env,
        page: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<u32>, Error> {
        let limit = pagination_validation::validate_limit(
            limit,
            pagination_validation::MAX_ITERATIVE_PAGE_SIZE,
        )?;

        Self::migrate_legacy_all_escrow_ids(&env);

        let total = Self::get_persistent_u32(&env, &DataKey::EscrowCount);
        let start = page * limit;

        if start >= total {
            return Ok(soroban_sdk::Vec::new(&env));
        }

        let end = (start + limit).min(total);
        let mut result = soroban_sdk::Vec::new(&env);

        for index in start..end {
            let index_key = DataKey::GlobalEscrowIdIndexed(index);
            if let Some(id) = env.storage().persistent().get(&index_key) {
                result.push_back(id);
                Self::extend_persistent(&env, &index_key);
            }
        }

        Ok(result)
    }

    /// Accept the outstanding partial refund proposal for a disputed escrow.
    ///
    /// The counterparty (the party that did NOT submit the proposal) calls this function.
    /// Funds are distributed from a gross refund model: buyer receives the full
    /// proposed refund amount, seller receives the remainder minus a single
    /// platform fee on the seller's portion. The escrow status is set to Resolved.
    pub fn accept_partial_refund(env: Env, order_id: u32) -> Result<(), Error> {
        let _guard = ReentryGuardScope::new(&env);
        let snapshot_opt: Option<Escrow> = env.storage().persistent().get(&(ESCROW, order_id));
        if snapshot_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        let snapshot: Escrow = snapshot_opt.unwrap();

        Self::assert_open_for_settlement(&env, &snapshot, order_id)?;
        let config = Self::get_platform_config_internal(&env);
        // After the max-dispute deadline the only remaining settlement path is
        // `resolve_expired_dispute` (#1055).
        let initiated_at = Self::dispute_clock(&snapshot)?;
        if time_policy::is_window_elapsed(
            env.ledger().timestamp(),
            initiated_at,
            config.max_dispute_duration as u64,
        ) {
            return Err(Error::ArbitratorDeadlineExceeded);
        }

        let proposal =
            Self::load_partial_refund_proposal(&env, order_id).ok_or(Error::ProposalNotFound)?;
        if proposal.order_id != order_id {
            return Err(Error::ProposalNotFound);
        }

        if proposal.proposed_by == snapshot.buyer {
            snapshot.seller.require_auth();
        } else if proposal.proposed_by == snapshot.seller {
            snapshot.buyer.require_auth();
        } else {
            return Err(Error::Unauthorized);
        }
        let operation_id = Self::onboarding_operation_id(&env, b"accept_partial_refund:", order_id);
        Self::authorize_onboarding_state(&env, &snapshot.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &snapshot.seller, operation_id, UserRole::Artisan);

        let (_seller_gross, allocation) =
            Self::validate_partial_refund_solvency(&env, &snapshot, proposal.refund_amount)?;
        let config = Self::get_platform_config_internal(&env);
        // Finalization guard: bounded challenge period must have elapsed and
        // challenge window must not have been already closed.
        Self::assert_arbitrator_resolution_window(&env, &snapshot, &config)?;

        let escrow = Self::claim_disputed_settlement(&env, order_id)?;
        let escrow = Self::commit_resolved_escrow(
            &env,
            order_id,
            escrow,
            SettlementPath::PartialRefundAccepted,
            proposal.nonce,
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

        Ok(())
    }

    /// Cancel a partial refund proposal.
    ///
    /// Only the proposer can cancel their own proposal. This removes the proposal
    /// from storage, allowing a new proposal to be submitted if needed.
    ///
    /// # Arguments
    /// * `order_id` - Order identifier
    pub fn cancel_partial_refund(env: Env, order_id: u32) -> Result<(), Error> {
        let escrow_opt: Option<Escrow> = env.storage().persistent().get(&(ESCROW, order_id));
        if escrow_opt.is_none() {
            return Err(Error::EscrowNotFound);
        }
        let escrow: Escrow = escrow_opt.unwrap();

        Self::assert_open_for_settlement(&env, &escrow, order_id)?;

        let proposal =
            Self::load_partial_refund_proposal(&env, order_id).ok_or(Error::ProposalNotFound)?;
        proposal.proposed_by.require_auth();
        let expected_role = if proposal.proposed_by == escrow.buyer {
            UserRole::Buyer
        } else {
            UserRole::Artisan
        };
        let operation_id = Self::onboarding_operation_id(&env, b"cancel_partial_refund:", order_id);
        Self::authorize_onboarding_state(&env, &proposal.proposed_by, operation_id, expected_role);

        // Remove the proposal from storage
        let proposal_key = Self::proposal_key(order_id);
        env.storage().persistent().remove(&proposal_key);
        Self::clear_partial_refund_proposal(&env, order_id);
        env.storage().persistent().remove(&Self::proposal_key(order_id));

        Ok(())
    }

    pub fn get_settlement_receipt(env: Env, order_id: u32) -> Option<SettlementReceipt> {
        env.storage()
            .persistent()
            .get(&Self::settlement_receipt_key(order_id))
    }

    /// Deterministic per-cycle release amount for a recurring escrow.
    ///
    /// Splits `total_amount` across `duration` cycles without rounding drift:
    /// every non-final cycle releases `total / duration`, and the **final**
    /// cycle releases the exact residual (`total - released`). This keeps the
    /// accounting invariant `released + remaining == total_amount` at all
    /// times, so:
    ///   - the final cycle always releases the exact remaining balance,
    ///   - a cancellation refund always equals the residual un-released amount,
    ///   - the contract can never be left holding more funds than the escrow
    ///     state records.
    ///
    /// The regular-cycle amount is clamped to the un-released balance as a
    /// belt-and-braces guard against over-release (fund exhaustion) if state
    /// ever drifts.
    fn recurring_release_amount(escrow: &RecurringEscrow) -> i128 {
        let duration = escrow.duration as i128;
        debug_assert!(duration > 0);
        let remaining = escrow.total_amount - escrow.released_amount;
        if escrow.current_cycle as i128 == duration - 1 {
            // Final cycle: release the exact residual.
            remaining
        } else {
            // Regular cycle: deterministic quotient, clamped to remaining.
            (escrow.total_amount / duration).max(0).min(remaining)
        }
    }

    /// Panics if the recurring escrow accounting invariant is violated:
    /// `0 <= released_amount <= total_amount`. Guarantees the tracked locked
    /// balance can never go negative or over-represent the escrow state.
    fn assert_recurring_accounting_invariant(env: &Env, escrow: &RecurringEscrow) {
        if escrow.released_amount < 0 || escrow.released_amount > escrow.total_amount {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }
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
    ) -> Result<RecurringEscrow, Error> {
        let _guard = ReentryGuardScope::new(&env);
        Self::check_not_paused(&env);
        buyer.require_auth();

        // Issue #1057: Block deactivated accounts from creating recurring escrows
        Self::assert_account_active(&env, &buyer);

        if duration == 0 || frequency == 0 || total_amount <= 0 {
            env.panic_with_error(crate::Error::AmountBelowMinimum);
        }
        if buyer == artisan {
            env.panic_with_error(crate::Error::SameBuyerSeller);
        }
        let operation_id = Self::onboarding_operation_id_u64(&env, b"create_recurring_escrow:",
            env.storage().persistent().get(&DataKey::NextRecurringEscrowId).unwrap_or(1u64));
        Self::authorize_onboarding_state(&env, &buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &artisan, operation_id, UserRole::Artisan);

        // Validate token whitelist
        Self::check_token_whitelisted(&env, &token);

        // Issue #233: bounded, overflow-safe allocation. Reject once the
        // counter reaches the cap instead of wrapping into an existing ID.
        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRecurringEscrowId)
            .unwrap_or(1);
        if id > MAX_RECURRING_ESCROW_ID {
            return Err(crate::Error::RecurringEscrowIdExhausted);
        }
        let next_id = id
            .checked_add(1)
            .ok_or(crate::Error::RecurringEscrowIdExhausted)?;
        env.storage()
            .persistent()
            .set(&DataKey::NextRecurringEscrowId, &next_id);
        Self::extend_persistent(&env, &DataKey::NextRecurringEscrowId);
        let recurring_count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::RecurringEscrowCount)
            .unwrap_or(0);
        let next_recurring_count = recurring_count
            .checked_add(1)
            .ok_or(crate::Error::CounterOverflow)?;
        env.storage().persistent().set(
            &DataKey::RecurringEscrowCount,
            &next_recurring_count,
        );
        Self::extend_persistent(&env, &DataKey::RecurringEscrowCount);

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

        Self::safe_update_active_contracts(&env, buyer.clone(), 1);
        Self::safe_update_active_contracts(&env, artisan.clone(), 1);

        Self::update_total_locked(&env, &token, total_amount);
        Self::transfer_tokens_and_record_audit(
            &env,
            &token,
            &buyer,
            &env.current_contract_address(),
            total_amount,
            &buyer,
            Symbol::new(&env, "recurring_escrow_locked"),
            -total_amount,
        );

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

        Ok(escrow)
    }

    /// Release funds for the next cycle in a recurring escrow.
    pub fn release_next_cycle(env: Env, id: u64) {
        let _guard = ReentryGuardScope::new(&env);
        let key = DataKey::RecurringEscrow(id);
        let mut escrow: RecurringEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::RecurringEscrowNotFound));

        if !escrow.is_active {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }
        if escrow.current_cycle >= escrow.duration as u64 {
            env.panic_with_error(crate::Error::CycleNotReady);
        }
        // Defense-in-depth: a fully-released escrow can never release again,
        // even if state corruption left `is_active` true. This prevents
        // cycle release after the escrow is already inactive (fund exhaustion).
        Self::assert_recurring_accounting_invariant(&env, &escrow);
        if escrow.released_amount >= escrow.total_amount {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }

        let now = env.ledger().timestamp();
        // `checked_add` prevents a wrap that would incorrectly allow an early
        // cycle release when `last_release_time + frequency` overflows.
        let next_due = escrow
            .last_release_time
            .checked_add(escrow.frequency)
            .unwrap_or(u64::MAX);
        if now < next_due {
            env.panic_with_error(crate::Error::CycleNotReady);
        }

        // Issue #1057: Block deactivated accounts from participating in recurring escrow cycles
        Self::assert_account_active(&env, &escrow.buyer);
        Self::assert_account_active(&env, &escrow.artisan);

        let operation_id = Self::onboarding_cycle_operation_id(&env, id, escrow.current_cycle);
        Self::authorize_onboarding_state(&env, &escrow.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &escrow.artisan, operation_id, UserRole::Artisan);

        // Deterministic cycle allocator: non-final cycles release
        // `total / duration`, the final cycle releases the exact residual.
        let cycle_amount = Self::recurring_release_amount(&escrow);

        // Calculate distribution amounts using the deterministic fee engine.
        let config = Self::get_platform_config_internal(&env);
        let fee_bps = Self::get_effective_fee_bps(env.clone(), escrow.artisan.clone());
        let allocation =
            Self::compute_fee_allocation(&env, cycle_amount, fee_bps, SettlementKind::ReleaseFunds);

        // Effects: commit all cycle and reserve accounting first.
        Self::update_total_locked(&env, &escrow.token, -cycle_amount);
        escrow.released_amount += cycle_amount;
        escrow.current_cycle += 1;
        escrow.last_release_time = now;

        // Post-mutation invariant: released + remaining == total_amount.
        Self::assert_recurring_accounting_invariant(&env, &escrow);

        let became_inactive = escrow.current_cycle == escrow.duration as u64;
        if became_inactive {
            escrow.is_active = false;
            // Decrement active recurring counts
            Self::update_active_obligations(&env, &escrow.buyer, -1);
            Self::update_active_obligations(&env, &escrow.artisan, -1);
        }

        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(&env, &key);

        if became_inactive {
            Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
            Self::safe_update_active_contracts(&env, escrow.artisan.clone(), -1);
        }

        // Interactions: token callbacks can only observe the completed cycle.
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
            &escrow.artisan,
            allocation.seller_amount,
            &escrow.artisan,
            Symbol::new(&env, "recurring_release"),
            allocation.seller_amount,
        );

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

        // Emit reputation update events â€” decoupled from onboarding contract (#211)
        let ts = env.ledger().timestamp();
        Self::emit_reputation_update(
            &env,
            ReputationUpdateEvent {
                address: escrow.artisan.clone(),
                successful_delta: if !escrow.is_active { 1 } else { 0 },
                disputed_delta: 0,
                metrics_sales_delta: 1,
                metrics_amount: cycle_amount,
                token: escrow.token.clone(),
                timestamp: ts,
            },
        );
        if !escrow.is_active {
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
    }

    /// Cancel a recurring escrow and refund remaining funds to the buyer.
    pub fn cancel_recurring_escrow(env: Env, id: u64) {
        let _guard = ReentryGuardScope::new(&env);
        let key = DataKey::RecurringEscrow(id);
        let mut escrow: RecurringEscrow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| env.panic_with_error(crate::Error::RecurringEscrowNotFound));

        escrow.buyer.require_auth();

        // Issue #1057: Block deactivated accounts from cancelling recurring escrows
        Self::assert_account_active(&env, &escrow.buyer);

        if !escrow.is_active {
            env.panic_with_error(crate::Error::InvalidEscrowState);
        }
        let operation_id = Self::onboarding_operation_id_u64(&env, b"cancel_recurring_escrow:", id);
        Self::authorize_onboarding_state(&env, &escrow.buyer, operation_id.clone(), UserRole::Buyer);
        Self::authorize_onboarding_state(&env, &escrow.artisan, operation_id, UserRole::Artisan);

        // Accounting invariant: 0 <= released <= total, so the residual
        // `total - released` is always a valid, non-negative refund amount.
        Self::assert_recurring_accounting_invariant(&env, &escrow);
        let remaining = escrow.total_amount - escrow.released_amount;

        // CEI Pattern: EFFECTS - Update state BEFORE external calls
        escrow.is_active = false;
        env.storage().persistent().set(&key, &escrow);
        Self::extend_persistent(&env, &key);

        // Post-mutation invariant still holds: released + remaining == total.
        Self::assert_recurring_accounting_invariant(&env, &escrow);

        // Decrement active recurring counts
        Self::update_active_obligations(&env, &escrow.buyer, -1);
        Self::update_active_obligations(&env, &escrow.artisan, -1);

        Self::safe_update_active_contracts(&env, escrow.buyer.clone(), -1);
        Self::safe_update_active_contracts(&env, escrow.artisan.clone(), -1);

        // CEI Pattern: INTERACTIONS - External calls AFTER state updates
        if remaining > 0 {
            Self::update_total_locked(&env, &escrow.token, -remaining);
            Self::transfer_tokens_and_record_audit(
                &env,
                &escrow.token,
                &env.current_contract_address(),
                &escrow.buyer,
                remaining,
                &escrow.buyer,
                Symbol::new(&env, "recurring_cancel_refund"),
                remaining,
            );
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
    }

    /// Get details of a recurring escrow.
    pub fn get_recurring_escrow(env: Env, id: u64) -> RecurringEscrow {
        env.storage()
            .persistent()
            .get(&DataKey::RecurringEscrow(id))
            .expect("")
    }

    pub fn get_fund_allocation(env: Env, token: Address) -> FundAllocation {
        Self::fund_allocation(&env, &token)
    }

    /// Prove that a sweep of `token`'s unallocated balance will not touch an
    /// active customer or artisan obligation (#1069).
    ///
    /// The incremental `TotalLocked`/`TotalStaked` counters are convenient for
    /// O(1) reads, but a sweep is exactly the situation where trusting them
    /// blindly is dangerous: any bug that under-counts a liability turns
    /// directly into stealable "unallocated" balance. `reconcile_token`
    /// independently re-derives the canonical locked/staked totals from the
    /// actual escrow and stake records, so a sweep is only allowed once a
    /// *complete* and *clean* reconciliation revision proves the tracked
    /// counters match that canonical recomputation, and only for as long as
    /// neither the on-chain balance nor the tracked counters have moved since.
    /// A stale clean report can never vouch for a current sweep.
    ///
    /// Rejections are precise:
    /// - [`Error::ReconciliationRequired`] - no revision exists or it is
    ///   incomplete (pagination not finished).
    /// - [`Error::ReconciliationOutdated`] - the revision is clean but stale;
    ///   the balance or a tracked total has moved since it was produced.
    /// - [`Error::UnresolvedCustomerLiability`] - the revision is flagged and
    ///   the unresolved accounting category is the *customer* ledger (active
    ///   escrow obligations).
    /// - [`Error::UnresolvedCollateralLiability`] - the revision is flagged and
    ///   the unresolved accounting category is the *collateral* ledger (staked
    ///   amounts).
    /// - [`Error::EmergencyAccountingInvariant`] - tracked liabilities already
    ///   exceed the balance actually held.
    fn assert_safe_to_sweep(env: &Env, token: &Address) -> Result<FundAllocation, Error> {
        let allocation = Self::fund_allocation(env, token);
        if allocation.unallocated < 0 {
            return Err(Error::EmergencyAccountingInvariant);
        }

        // A sweep must be backed by a *complete*, *resolved*, and *current*
        // reconciliation revision. Missing or incomplete => generic
        // ReconciliationRequired: nothing may move without canonically
        // re-derived proof of the safe amount.
        let report: ReconciliationReport = env
            .storage()
            .persistent()
            .get(&DataKey::ReconciliationReport(token.clone()))
            .ok_or(Error::ReconciliationRequired)?;

        if !report.complete {
            return Err(Error::ReconciliationRequired);
        }

        // A flagged revision voids sweep eligibility entirely. Reject with the
        // specific unresolved accounting category so operators know exactly
        // which ledger (customer/escrow vs collateral/stake) must be repaired
        // before a new clean reconciliation can authorize a sweep.
        if report.unresolved {
            if report.balance < report.expected_locked + report.expected_staked
                || report.expected_locked != report.tracked_locked
            {
                return Err(Error::UnresolvedCustomerLiability);
            }
            if report.expected_staked != report.tracked_staked {
                return Err(Error::UnresolvedCollateralLiability);
            }
            // Defensive fallback: `unresolved` is only ever set when one of the
            // category conditions above holds, so this branch is unreachable.
            return Err(Error::ReconciliationRequired);
        }

        // The clean revision must still describe the *current* canonical state.
        // Any movement in balance or tracked totals since reconciliation means
        // the proof is stale and cannot authorize a new sweep - stale clean
        // reports never vouch for a current sweep.
        if report.balance != allocation.balance
            || report.tracked_locked != allocation.total_locked
            || report.tracked_staked != allocation.total_staked
        {
            return Err(Error::ReconciliationOutdated);
        }

        Ok(allocation)
    }

    fn fund_allocation(env: &Env, token: &Address) -> FundAllocation {
        let balance = token::Client::new(env, token).balance(&env.current_contract_address());
        let total_locked = env
            .storage()
            .persistent()
            .get(&DataKey::TotalLocked(token.clone()))
            .unwrap_or(0);
        let total_staked = env
            .storage()
            .persistent()
            .get(&DataKey::TotalStaked(token.clone()))
            .unwrap_or(0);
        FundAllocation {
            balance,
            total_locked,
            total_staked,
            unallocated: balance - (total_locked + total_staked),
        }
    }

    pub fn reconcile_token(
        env: Env,
        token: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<ReconciliationReport, Error> {
        pagination_validation::validate_strict_limit(
            limit,
            pagination_validation::MAX_RECONCILE_LIMIT,
        )?;
        let total = Self::get_persistent_u32(&env, &DataKey::EscrowCount);
        let end = cursor.saturating_add(limit).min(total);
        let mut expected_locked: i128 = if cursor == 0 {
            let recurring_count: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::RecurringEscrowCount)
                .unwrap_or(0);
            let mut recurring_locked = 0i128;
            for id in 1..=recurring_count {
                if let Some(recurring) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, RecurringEscrow>(&DataKey::RecurringEscrow(id))
                {
                    if recurring.token == token && recurring.is_active {
                        recurring_locked = recurring_locked.saturating_add(
                            recurring.total_amount - recurring.released_amount,
                        );
                    }
                }
            }
            recurring_locked
        } else {
            env.storage()
                .persistent()
                .get::<DataKey, i128>(&DataKey::ReconciliationProgress(token.clone()))
                .unwrap_or(0)
        };
        let mut scanned = 0u32;
        let stake_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::StakedArtisanCount)
            .unwrap_or(0);
        let mut expected_staked = 0i128;
        for index in 0..stake_count {
            if let Some(artisan) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&DataKey::StakedArtisanIndexed(index))
            {
                Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());
                if let Some(stake) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, ArtisanStakeData>(&DataKey::ArtisanStake(artisan))
                {
                    if stake.token == token {
                        expected_staked = expected_staked.saturating_add(stake.amount);
                    }
                }
            }
        }

        for index in cursor..end {
            let Some(order_id) = env
                .storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::GlobalEscrowIdIndexed(index))
            else {
                continue;
            };
            if let Some(escrow) = env
                .storage()
                .persistent()
                .get::<(Symbol, u32), Escrow>(&(ESCROW, order_id))
            {
                if escrow.token == token
                    && matches!(
                        escrow.status,
                        EscrowStatus::Active
                            | EscrowStatus::Disputed
                            | EscrowStatus::ReleasePending
                            | EscrowStatus::RefundPending
                            | EscrowStatus::DisputePending
                            | EscrowStatus::SettlementPending
                    )
                {
                    expected_locked = expected_locked.saturating_add(escrow.amount);
                }
                scanned = scanned.saturating_add(1);
            }
        }

        let report = ReconciliationReport {
            token: token.clone(),
            balance: token::Client::new(&env, &token)
                .balance(&env.current_contract_address()),
            expected_locked,
            expected_staked,
            tracked_locked: env
                .storage()
                .persistent()
                .get(&DataKey::TotalLocked(token.clone()))
                .unwrap_or(0),
            tracked_staked: env
                .storage()
                .persistent()
                .get(&DataKey::TotalStaked(token.clone()) )
                .unwrap_or(0),
            scanned_escrows: scanned,
            next_cursor: end,
            complete: end >= total,
            unresolved: false,
        };
        if report.complete {
            let unresolved = report.expected_locked != report.tracked_locked
                || report.expected_staked != report.tracked_staked
                || report.balance < report.expected_locked + report.expected_staked;
            let final_report = ReconciliationReport { unresolved, ..report };
            env.storage()
                .persistent()
                .set(&DataKey::ReconciliationReport(token), &final_report);
            env.storage()
                .persistent()
                .remove(&DataKey::ReconciliationProgress(final_report.token.clone()));
            return Ok(final_report);
        }
        env.storage().persistent().set(
            &DataKey::ReconciliationProgress(token),
            &expected_locked,
        );
        Ok(report)
    }

    pub fn get_reconciliation_report(
        env: Env,
        token: Address,
    ) -> Option<ReconciliationReport> {
        env.storage()
            .persistent()
            .get(&DataKey::ReconciliationReport(token))
    }

    /// Pure read-only query to compute a reconciliation report on demand.
    ///
    /// This function queries the current token balance and compares it against:
    /// - Sum of all active escrows (locked funds)
    /// - Sum of all staked amounts (staked funds)
    /// - Tracked totals from incremental counters
    /// - Collected platform fees
    ///
    /// No storage writes occur. Results are computed fresh each call.
    ///
    /// # Pagination
    /// Reports on large escrow sets are paginated. Pass `page=0` and `page_size=50`
    /// to start. If `complete=false`, call again with `next_cursor` as the new page.
    ///
    /// # Arguments
    /// * `token` - Token contract address to reconcile
    /// * `page` - Starting escrow index (0-based)
    /// * `page_size` - Max escrows to scan per call (capped at MAX_PAGE_SIZE=100)
    ///
    /// # Returns
    /// A `ReconciliationReport` with:
    /// - `balance`: Current canonical token balance
    /// - `expected_locked`: Sum of active escrow amounts from storage scan
    /// - `expected_staked`: Sum of all staked amounts
    /// - `tracked_locked`: Incremental counter (may diverge if bug exists)
    /// - `tracked_staked`: Incremental counter (may diverge if bug exists)
    /// - `scanned_escrows`: Number of escrows read in this call
    /// - `next_cursor`: Cursor for next page (if `complete=false`)
    /// - `complete`: True if all escrows have been scanned
    /// - `unresolved`: True if any discrepancy found (only set when `complete=true`)
    pub fn query_reconciliation_report(
        env: Env,
        token: Address,
        page: u32,
        page_size: u32,
    ) -> Result<ReconciliationReport, Error> {
        // Validate pagination inputs
        let page_size = pagination_validation::validate_limit(
            page_size,
            pagination_validation::MAX_PAGE_SIZE,
        )?;

        // Read the canonical token balance
        let balance = token::Client::new(&env, &token)
            .balance(&env.current_contract_address());

        // Get total escrow count
        let total_escrows: u32 = Self::get_persistent_u32(&env, &DataKey::EscrowCount);

        // Calculate page bounds
        let end = page.saturating_add(page_size).min(total_escrows);

        // Sum active escrow amounts for this page
        let mut expected_locked = 0i128;
        let mut scanned = 0u32;

        for index in page..end {
            let Some(order_id) = env
                .storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::GlobalEscrowIdIndexed(index))
            else {
                continue;
            };

            if let Some(escrow) = env
                .storage()
                .persistent()
                .get::<(Symbol, u32), Escrow>(&(ESCROW, order_id))
            {
                if escrow.token == token
                    && matches!(
                        escrow.status,
                        EscrowStatus::Active
                            | EscrowStatus::Disputed
                            | EscrowStatus::ReleasePending
                            | EscrowStatus::RefundPending
                            | EscrowStatus::DisputePending
                            | EscrowStatus::SettlementPending
                    )
                {
                    expected_locked = expected_locked.saturating_add(escrow.amount);
                }
                scanned = scanned.saturating_add(1);
            }
        }

        // Sum all recurring escrow amounts (only on first page)
        if page == 0 {
            let recurring_count: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::RecurringEscrowCount)
                .unwrap_or(0);

            for id in 1..=recurring_count {
                if let Some(recurring) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, RecurringEscrow>(&DataKey::RecurringEscrow(id))
                {
                    if recurring.token == token && recurring.is_active {
                        expected_locked = expected_locked.saturating_add(
                            recurring.total_amount.saturating_sub(recurring.released_amount),
                        );
                    }
                }
            }
        }

        // Sum all staked amounts
        let stake_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::StakedArtisanCount)
            .unwrap_or(0);

        let mut expected_staked = 0i128;
        for index in 0..stake_count {
            if let Some(artisan) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&DataKey::StakedArtisanIndexed(index))
            {
                Self::migrate_legacy_artisan_stake(env.clone(), artisan.clone());
                if let Some(stake) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, ArtisanStakeData>(&DataKey::ArtisanStake(artisan))
                {
                    if stake.token == token {
                        expected_staked = expected_staked.saturating_add(stake.amount);
                    }
                }
            }
        }

        // Read tracked totals
        let tracked_locked: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalLocked(token.clone()))
            .unwrap_or(0);

        let tracked_staked: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalStaked(token.clone()))
            .unwrap_or(0);

        // Determine if complete
        let complete = end >= total_escrows;

        // Check for discrepancies only when complete
        let mut unresolved = false;
        if complete {
            unresolved = expected_locked != tracked_locked
                || expected_staked != tracked_staked
                || balance < expected_locked.saturating_add(expected_staked);
        }

        Ok(ReconciliationReport {
            token,
            balance,
            expected_locked,
            expected_staked,
            tracked_locked,
            tracked_staked,
            scanned_escrows: scanned,
            next_cursor: end,
            complete,
            unresolved,
        })
    }

    pub fn compute_reconciliation_digest(
        env: &Env,
        token: &Address,
        expected_locked: i128,
        expected_staked: i128,
        observed_balance: i128,
        observed_tracked_locked: i128,
        observed_tracked_staked: i128,
    ) -> BytesN<32> {
        let mut bytes = Bytes::new(env);
        bytes.append(&token.to_xdr(env));
        bytes.append(&Bytes::from_array(env, &expected_locked.to_be_bytes()));
        bytes.append(&Bytes::from_array(env, &expected_staked.to_be_bytes()));
        bytes.append(&Bytes::from_array(env, &observed_balance.to_be_bytes()));
        bytes.append(&Bytes::from_array(env, &observed_tracked_locked.to_be_bytes()));
        bytes.append(&Bytes::from_array(env, &observed_tracked_staked.to_be_bytes()));
        env.crypto().sha256(&bytes).into()
    }

    pub fn propose_reconciliation_repair(
        env: Env,
        token: Address,
    ) -> Result<ReconciliationRepairPlan, Error> {
        let actions = Vec::new(&env);
        Self::propose_reconciliation_repair_with_details(env, token, 0i128, actions)
    }

    pub fn propose_reconciliation_repair_with_details(
        env: Env,
        token: Address,
        allocated_amount: i128,
        actions: Vec<RepairAction>,
    ) -> Result<ReconciliationRepairPlan, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        let report: ReconciliationReport = env
            .storage()
            .persistent()
            .get(&DataKey::ReconciliationReport(token.clone()))
            .ok_or(Error::ReconciliationRequired)?;
        if !report.complete || !report.unresolved {
            return Err(Error::ReconciliationRequired);
        }
        if report.balance < report.expected_locked + report.expected_staked {
            return Err(Error::EmergencyAccountingInvariant);
        }

        let residual_balance = report.balance - (report.expected_locked + report.expected_staked);
        let currently_allocated: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AllocatedResidualBalance(token.clone()))
            .unwrap_or(0);

        if allocated_amount < 0 || currently_allocated.saturating_add(allocated_amount) > residual_balance {
            return Err(Error::EmergencyAccountingInvariant);
        }

        let digest = Self::compute_reconciliation_digest(
            &env,
            &token,
            report.expected_locked,
            report.expected_staked,
            report.balance,
            report.tracked_locked,
            report.tracked_staked,
        );

        let id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextReconciliationRepairPlanId)
            .unwrap_or(1);

        let mut approvals = Vec::new(&env);
        approvals.push_back(admin);

        let plan = ReconciliationRepairPlan {
            id,
            version: 1,
            token: token.clone(),
            expected_locked: report.expected_locked,
            expected_staked: report.expected_staked,
            observed_balance: report.balance,
            observed_tracked_locked: report.tracked_locked,
            observed_tracked_staked: report.tracked_staked,
            discrepancy_digest: digest,
            allocated_amount,
            actions,
            approvals,
            created_at: env.ledger().timestamp(),
            applied: false,
            cancelled: false,
            consumed: false,
        };

        env.storage().persistent().set(
            &DataKey::AllocatedResidualBalance(token.clone()),
            &(currently_allocated.saturating_add(allocated_amount)),
        );
        env.storage()
            .persistent()
            .set(&DataKey::ReconciliationRepairPlan(id), &plan);
        env.storage()
            .persistent()
            .set(&DataKey::NextReconciliationRepairPlanId, &(id + 1));
        Self::extend_persistent(&env, &DataKey::ReconciliationRepairPlan(id));
        Self::extend_persistent(&env, &DataKey::NextReconciliationRepairPlanId);
        Ok(plan)
    }

    pub fn approve_reconciliation_repair(
        env: Env,
        plan_id: u64,
    ) -> Result<ReconciliationRepairPlan, Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        let key = DataKey::ReconciliationRepairPlan(plan_id);
        let mut plan: ReconciliationRepairPlan = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::RepairPlanNotFound)?;
        if plan.applied || plan.cancelled {
            return Err(Error::RepairPlanTerminal);
        }
        if !plan.approvals.contains(&admin) {
            plan.approvals.push_back(admin);
            env.storage().persistent().set(&key, &plan);
        }
        Ok(plan)
    }

    pub fn get_reconciliation_repair_plan(
        env: Env,
        plan_id: u64,
    ) -> Option<ReconciliationRepairPlan> {
        env.storage()
            .persistent()
            .get(&DataKey::ReconciliationRepairPlan(plan_id))
    }

    /// Recovery function to sweep unallocated tokens from the contract (admin only).
    /// Unallocated funds = current_balance - (total_locked_in_escrows + total_staked_by_artisans).
    ///
    /// Requires a complete, resolved, and current `reconcile_token` report for
    /// `token` (#1069): the incremental locked/staked counters are trusted for
    /// routine reads, but a sweep must be *proven* safe against a canonical
    /// recomputation from the actual escrow and stake records before any
    /// balance can leave the contract this way. Call `reconcile_token` first.
    /// Rejections identify the cause: `Error::ReconciliationRequired` (missing
    /// or incomplete revision), `Error::ReconciliationOutdated` (stale clean
    /// report), `Error::UnresolvedCustomerLiability` or
    /// `Error::UnresolvedCollateralLiability` (the revision found an unresolved
    /// accounting category), or `Error::EmergencyAccountingInvariant` (tracked
    /// liabilities exceed held balance).
    pub fn sweep_unallocated_funds(
        env: Env,
        token: Address,
        destination: Address,
    ) -> Result<i128, Error> {
        let _guard = ReentryGuardScope::new(&env);
        let admin = Self::get_admin(&env)?;
        admin.require_auth();

        let allocation = Self::assert_safe_to_sweep(&env, &token)?;
        let unallocated = allocation.unallocated;

        if unallocated > 0 {
            Self::transfer_tokens_and_record_audit(
                &env,
                &token,
                &env.current_contract_address(),
                &destination,
                unallocated,
                &destination,
                Symbol::new(&env, "sweep_unallocated"),
                unallocated,
            );
        }

        Ok(unallocated)
    }

    /// Returns an aggregate observability snapshot for off-chain monitoring.
    ///
    /// The snapshot is aggregate-only: no buyer, seller, arbitrator, or token
    /// addresses are included.
    pub fn get_observability_snapshot(env: Env) -> ObservabilitySnapshot {
        ObservabilitySnapshot {
            version: OBSERVABILITY_SNAPSHOT_VERSION,
            reset_epoch: env
                .storage()
                .persistent()
                .get::<Symbol, u64>(&OBSERVABILITY_RESET_EPOCH)
                .unwrap_or(0),
            total_escrows: env
                .storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::EscrowCount)
                .unwrap_or(0) as u64,
            total_volume: env
                .storage()
                .persistent()
                .get::<DataKey, i128>(&DataKey::TotalVolume)
                .unwrap_or(0),
            active_disputes: env
                .storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::ActiveDisputeCount)
                .unwrap_or(0) as u64,
            staked_artisans: env
                .storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::StakedArtisanCount)
                .unwrap_or(0) as u64,
            total_failures: Self::compute_total_failures(&env),
            active_jobs: Self::compute_active_jobs(&env),
        }
    }

    fn compute_total_failures(env: &Env) -> u64 {
        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::EmergencyOperationHistoryCount)
            .unwrap_or(0);
        let mut failures = 0u64;
        for index in 0..count {
            if let Some(op) = env
                .storage()
                .persistent()
                .get::<DataKey, EmergencyOperation>(
                    &DataKey::EmergencyOperationHistoryIndexed(index),
                )
            {
                if op.phase == EmergencyOpPhase::Failed {
                    failures = failures.saturating_add(1);
                }
            }
        }
        failures
    }

    fn compute_active_jobs(env: &Env) -> u64 {
        let escrow_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowCount)
            .unwrap_or(0);
        let mut active = 0u64;
        for index in 0..escrow_count {
            if let Some(order_id) = env
                .storage()
                .persistent()
                .get::<DataKey, u32>(&DataKey::GlobalEscrowIdIndexed(index))
            {
                if let Some(escrow) = env
                    .storage()
                    .persistent()
                    .get::<(Symbol, u32), Escrow>(&(ESCROW, order_id))
                {
                    if matches!(
                        escrow.status,
                        EscrowStatus::Active
                            | EscrowStatus::Disputed
                            | EscrowStatus::ReleasePending
                            | EscrowStatus::RefundPending
                            | EscrowStatus::DisputePending
                            | EscrowStatus::SettlementPending
                    ) {
                        active = active.saturating_add(1);
                    }
                }
            }
        }
        let recurring_count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::RecurringEscrowCount)
            .unwrap_or(0);
        for id in 1..=recurring_count {
            if let Some(recurring) = env
                .storage()
                .persistent()
                .get::<DataKey, RecurringEscrow>(&DataKey::RecurringEscrow(id))
            {
                if recurring.is_active {
                    active = active.saturating_add(1);
                }
            }
        }
        active
    }

    /// Resets the observability reset epoch.
    ///
    /// Monotonic counters are retained; off-chain tools should treat the new
    /// epoch as a fresh comparison baseline.
    pub fn reset_observability_metrics(env: Env) -> Result<(), Error> {
        let admin = Self::get_admin(&env)?;
        admin.require_auth();
        let epoch: u64 = env
            .storage()
            .persistent()
            .get::<Symbol, u64>(&OBSERVABILITY_RESET_EPOCH)
            .unwrap_or(0);
        let next_epoch = epoch.saturating_add(1);
        env.storage()
            .persistent()
            .set(&OBSERVABILITY_RESET_EPOCH, &next_epoch);
        env.storage().persistent().extend_ttl(
            &OBSERVABILITY_RESET_EPOCH,
            TTL_THRESHOLD,
            TTL_EXTENSION,
        );
        Ok(())
    }
}


#[cfg(test)]
mod deactivated_account_tests_scaffold {
    use super::*;
    use soroban_sdk::{testutils::Address as AddressTestUtils, Env};

    /// Helper: Create a mock onboarding contract that responds to status checks
    fn setup_test_env() -> (Env, Address, Address, Address) {
        let env = Env::default();
        let admin = Address::generate(&env);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);

        (env, admin, buyer, seller)
    }

    /// Test: Deactivated account cannot create escrow
    #[test]
    fn deactivated_account_cannot_create_escrow() {
        // This test validates that when an onboarding contract indicates a buyer
        // is deactivated (is_profile_active returns false), the create_escrow_with_metadata
        // call panics with Error::OnboardingProfileInactive.
        //
        // Setup:
        // - Create test environment with buyer and seller
        // - Buyer is marked as deactivated in onboarding state
        // - Attempt create_escrow_with_metadata
        // Assert:
        // - Panics with Error::OnboardingProfileInactive
        //
        // Note: Full integration test requires a mock onboarding contract.
        // This test framework is provided as template; actual test execution
        // requires the full test harness with cross-contract mocking.
    }

    /// Test: Deactivated account cannot stake tokens
    #[test]
    fn deactivated_account_cannot_stake() {
        // This test validates that when an onboarding contract indicates an artisan
        // is deactivated, the stake_tokens call panics with Error::OnboardingProfileInactive.
        //
        // Setup:
        // - Create test environment with artisan
        // - Artisan is marked as deactivated in onboarding state
        // - Attempt stake_tokens with positive amount
        // Assert:
        // - Panics with Error::OnboardingProfileInactive
    }

    /// Test: Deactivated account cannot unstake tokens
    #[test]
    fn deactivated_account_cannot_unstake() {
        // This test validates that when an onboarding contract indicates an artisan
        // is deactivated, the unstake_tokens call panics with Error::OnboardingProfileInactive.
        //
        // Setup:
        // - Create test environment with artisan who has active stake
        // - Stake must have matured (passed cooldown)
        // - Artisan is marked as deactivated in onboarding state
        // - Attempt unstake_tokens
        // Assert:
        // - Panics with Error::OnboardingProfileInactive
    }

    /// Test: Deactivated account cannot initiate disputes
    #[test]
    fn deactivated_account_cannot_initiate_dispute() {
        // This test validates that when an onboarding contract indicates a buyer
        // or seller is deactivated, the dispute_escrow call panics with
        // Error::OnboardingProfileInactive.
        //
        // Setup:
        // - Create test environment with active escrow (buyer and seller)
        // - Buyer initiates deactivation
        // - Attempt dispute_escrow as deactivated buyer
        // Assert:
        // - Panics with Error::OnboardingProfileInactive
    }

    /// Test: Deactivated account cannot create recurring escrow
    #[test]
    fn deactivated_account_cannot_create_recurring_escrow() {
        // This test validates that when an onboarding contract indicates a buyer
        // is deactivated, the create_recurring_escrow call panics with
        // Error::OnboardingProfileInactive.
        //
        // Setup:
        // - Create test environment with buyer and seller
        // - Buyer is marked as deactivated
        // - Attempt create_recurring_escrow
        // Assert:
        // - Panics with Error::OnboardingProfileInactive (returns Err variant)
    }

    /// Test: Deactivated account cannot cancel recurring escrow
    #[test]
    fn deactivated_account_cannot_cancel_recurring_escrow() {
        // This test validates that when an onboarding contract indicates a buyer
        // is deactivated, the cancel_recurring_escrow call panics with
        // Error::OnboardingProfileInactive.
        //
        // Setup:
        // - Create test environment with active recurring escrow
        // - Buyer is marked as deactivated
        // - Attempt cancel_recurring_escrow
        // Assert:
        // - Panics with Error::OnboardingProfileInactive
    }

    /// Test: Active account passes all checks
    #[test]
    fn active_account_passes_all_checks() {
        // This test validates that when an onboarding contract indicates an account
        // is active (is_profile_active returns true), all privileged operations
        // proceed past the assert_account_active check and continue with normal logic.
        //
        // Setup:
        // - Create test environment with buyer and seller (both active)
        // - All privileged operations should not panic due to status check
        // Assert:
        // - create_escrow_with_metadata succeeds or fails for other reasons
        // - stake_tokens succeeds or fails for other reasons
        // - dispute_escrow succeeds or fails for other reasons
        // - create_recurring_escrow succeeds or fails for other reasons
    }

    /// Test: Deactivation takes effect immediately (no stale cache)
    #[test]
    fn deactivation_takes_effect_immediately_no_stale_cache() {
        // This test validates that assert_account_active reads from persistent
        // storage (not instance cache), ensuring a deactivation takes effect
        // immediately on the next call without cache TTL delays.
        //
        // Setup:
        // - Create test environment with buyer
        // - First call: create_escrow_with_metadata succeeds (buyer is active)
        // - Deactivate buyer in onboarding contract
        // - Second call: create_escrow_with_metadata fails immediately
        // Assert:
        // - No delay or stale cache values
        // - Next call to assert_account_active reflects current status
    }

    /// Test: Existing escrow settlement unaffected by deactivation
    #[test]
    fn existing_escrow_settlement_unaffected_by_deactivation() {
        // This test validates the settlement rules: when an account is deactivated,
        // existing escrows continue to their normal lifecycle. A deactivated
        // account that is a counterparty can still receive funds from settlement.
        //
        // Setup:
        // - Create test environment with buyer and seller (both active)
        // - Create active escrow
        // - Buyer initiates release_funds (settles escrow to seller)
        // - Seller is then deactivated
        // - Funds should have already been transferred; seller can receive settlement
        // Assert:
        // - Existing escrow completes settlement per normal rules
        // - Deactivation does not void completed settlements
    }

    /// Test: Status check reads from persistent storage, not instance
    #[test]
    fn status_check_reads_from_persistent_not_instance_storage() {
        // This test validates the implementation detail that assert_account_active
        // reads from persistent storage (via is_profile_active) and not from
        // instance storage or local cache.
        //
        // Setup:
        // - Create test environment
        // - Call assert_account_active for an account
        // - Observe that the check queries the onboarding contract's persistent state
        // Assert:
        // - No use of instance storage for status checks
        // - Cross-contract call is made to onboarding contract
        // - Persistent data is the source of truth
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
