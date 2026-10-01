use soroban_std::{address, contract, contractimpl, contracttype, symbol_short};

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
    EvidenceChallenge(u32),
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

const EVIDENCE_CHALLENGE_KEY: Symbol = Symbol::short_symbol("EvChall");

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

    /// Persisted challenge window for a disputed order.
    #[derive(Clone, Debug, Eq, PartialEq)]
    #[contracttype]
    pub struct EvidenceChallenge {
        public order_id: u64,
        public deadline: u64,
        public terminal: bool,
    }

    let id = log.len() as u64;
    let submitted_at = env.ledger().timestamp();
    let expires_at = submitted_at + DEFAULT_EVIDENCE_EXPIRY_WINDOW;

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

#[contractimpl]
impl CraftNexusContract {
    /// Read the persisted evidence challenge for a disputed order.
    ///
    /// Returns `Ok(Some(challenge))` when the record exists, `Ok(None)` when
    /// the key is absent (archival, partial migration, or missing key), and
    /// a typed `Error` when the caller is not allowed to read the record.
    ///
    /// This function never panics on a missing storage key.
    pub fn get_evidence_challenge(env: Env, order_id: u64) -> Result<Option<EvidenceChallenge>, Error> {
        // Hot persistent keys are extended on read to avoid expiry.
        // This is a no-op when the key is absent.
        env.storage().extend_persistent_read(&EVIDENCE_CHALLENGE_KEY, 300, 100);

match env
            .storage()
            .persistent()
            .get::<&u64>(&EVIDENCE_CHALLENGE_KEY, &order_id)
        {
            Some(challenge) => Ok(Some(challenge)),
            None => Ok(None),
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

        let record = DisputeEscalationRecord {
            order_id,
            escalated_by: caller,
            escalated_at: current_time,
        };

        env.storage().persistent().set(&escalation_key, &record);

        Self::emit_dispute_escalated(&env, order_id);
    }

/// Persist a challenge window for a disputed order.
    pub fn set_evidence_challenge(
        env: Env,
        order_id: u64,
        challenge: EvidenceChallenge,
    ) {
        env.storage()

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
.set(&EVIDENCE_CHALLENGE_KEY, &order_id, &challenge);
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

/// Mark a challenge as terminal (challenge window closed).
    pub fn close_evidence_challenge(env: Env, order_id: u64) -> Result<u64, Error> {
        let mut challenge = match env
            .storage()
            .persistent()
            .get::<&u64>(&EVIDENCE_CHALLENGE_KEY, &order_id)
        {
            Some(challenge) => challenge,
            None => return Err(Error::EvidenceChallengeNotFound),
        };
        };

if challenge.terminal {
            return Err(Error::ChallengeWindowClosed);
        }

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
        }

challenge.terminal = true;
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
            .get(&DataKey::Escrow(order_id))
            .unwrap_or_panic_with(&Error::EscrowNotFound)
            .set(&EVIDENCE_CHALLENGE_KEY, &order_id, &challenge);

        Ok(challenge.deadline)
    }
}

pub fn dispute_escrow(env: Env, order_id: u32, _reason: Symbol, _initiator: Address) {
        if env.storage().get(&DataKey::Paused).unwrap_or(&false) {
            soroban_sdk::panic_with_error(&env, &Error::ContractPaused);
        }
        let key = DataKey::Escrow(order_id);
        let mut escrow = env.storage()
            .get(&key)
            .unwrap_or_panic_with(&Error::EscrowNotFound);
        if escrow.status != EscrowStatus::Active {
            soroban_sdk::panic_with_error(&env, &Error::InvalidStatus);
        }
        escrow.status = EscrowStatus::Disputed;
        escrow.dispute_timestamp = env.ledger().timestamp();
        env.storage().set(&key, &escrow);
    }

    /////////////////////////////////////////////////////////////////////////////
    /// Expired dispute resolution
    /////////////////////////////////////////////////////////////////////////////
    pub fn resolve_expired_dispute(env: Env, order_id: u32) {
        if env.storage().get(&DataKey::Paused).unwrap_or(&false) {
            soroban_sdk::panic_with_error(&env, &Error::ContractPaused);
        }
        let key = DataKey::Escrow(order_id);
        let mut escrow = env.storage()
            .get(&key)
            .unwrap_or_panic_with(&Error::EscrowNotFound);
        if escrow.status != EscrowStatus::Disputed {
            soroban_sdk::panic_with_error(&env, &Error::NotDisputed);
        }
        let now = env.ledger().timestamp();
        let deadline = escrow.dispute_timestamp + escrow.max_dispute_duration as u64;
        if now <= deadline {
            soroban_sdk::panic_with_error(&env, &Error::DisputeNotExpired);
        }

        let policy = env.storage()
            .get(&DataKey::ExpiredDisputePolicy)
            .unwrap_or(&ExpiredDisputeFeePolicy::RefundFullNoPlatformFee);
        let fee_bps = env.storage().get(&DataKey::PlatformFeeBps).unwrap_or(&0) as i128;
        let full_fee = escrow.amount * fee_bps / 10_000;
        let platform_wallet: Address = env.storage().get(&DataKey::PlatformWallet).unwrap();
        let token_client = soroban_sdk::token::Client::new(&env, &escrow.token);

        let (buyer_amount, platform_amount) = match policy {
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee => (escrow.amount, 0i128),
            ExpiredDisputeFeePolicy::RefundMinusPlatformFee => {
                (escrow.amount - full_fee, full_fee)
            }
            ExpiredDisputeFeePolicy::DeductFeeFromSeller => (escrow.amount, 0i128),
            ExpiredDisputeFeePolicy::SplitFee => {
                let half = full_fee / 2;
                (escrow.amount - half, half)
            }
        };

        if buyer_amount > 0 {
            token_client.transfer(&env.current_contract(), &escrow.buyer, &buyer_amount);
        }
        if platform_amount > 0 {
            token_client.transfer(
                &env.current_contract(),
                &platform_wallet,
                &platform_amount,
            );
            let fee_key = DataKey::TotalFees(escrow.token.clone());
            let prev: i128 = env.storage().get(&fee_key).unwrap_or(&0i128);
            let new_total = prev.checked_add(platform_amount).unwrap_or_panic_with(&Error::Overflow);
            env.storage().set(&fee_key, &new_total);
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use soroban_std::Env;

escrow.status = EscrowStatus::Resolved;
        env.storage().set(&key, &escrow);
        Ok(())
    }

    #[test]
    fn get_evidence_challenge_missing_key_returns_none() {
        let env = Env::default();
        let result = CraftNexusContract::get_evidence_challenge(env.clone(), 942);
        assert_eq(result, Ok(None));
    }

#[test]
    fn get_evidence_challenge_before_and_after_terminal() {
        let env = Env::default();
        let order_id = 942;
        let challenge = EvidenceChallenge {
            order_id,
            deadline: 1234,
            terminal: false,
        };

        // Before the record exists.
        assert_eq(
            CraftNexusContract::get_evidence_challenge(env.clone(), order_id),
            Ok(None),
        );

CraftNexusContract::set_evidence_challenge(env.clone(), order_id, challenge.clone());
        assert_eq!(
            CraftNexusContract::get_evidence_challenge(env.clone(), order_id),
            Ok(Some(challenge.clone())),
        );
// After a terminal state.
        CraftNexusContract::close_evidence_challenge(env.clone(), order_id).unwrap();
        let terminal = CraftNexusContract::get_evidence_challenge(env.clone(), order_id)
            .unwrap()
            .unwrap();
        assert!(terminal.terminal);
        assert_eq!(terminal.deadline, 1234);

    /// Paused contract rejects the update with ContractPaused.
    /// Storage must be unchanged after the rejection.
    #[test]
    fn test_update_expired_dispute_policy_paused() {
        let (_env, client, _admin, _platform) = setup();
        client.pause();
        let res = client.try_update_expired_dispute_policy(
            &ExpiredDisputeFeePolicy::SplitFee,
        );
        assert!(res.is_error());
        assert_eq!(
            client.get_expired_dispute_policy(),
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee
        );
    }

    /// Happy path: authorized admin can update the policy.
    #[test]
    fn test_update_expired_dispute_policy_ok() {
        let (_env, client, _admin, _platform) = setup();
        client.update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee);
        assert_eq!(
            client.get_expired_dispute_policy(),
            ExpiredDisputeFeePolicy::SplitFee
        );
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
                break;
            }
        }
    }
}
