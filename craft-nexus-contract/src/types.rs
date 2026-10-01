use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

/// Represents the lifecycle status of an escrow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    /// Escrow has been created but not yet funded.
    Pending,
    /// Escrow is funded and awaiting fulfillment.
    Funded,
    /// Work has been delivered and is awaiting approval.
    Delivered,
    /// Escrow has been released to the artisan.
    Released,
    /// Escrow has been disputed by one of the parties.
    Disputed,
    /// Escrow has been refunded to the client.
    Refunded,
    /// Escrow has been cancelled.
    Cancelled,
}

/// Represents the type of escrow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowType {
    /// Standard one-time escrow.
    Standard,
    /// Recurring escrow with scheduled payments.
    Recurring,
}

/// Core escrow data structure.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    /// Unique identifier for the escrow.
    pub id: u64,
    /// Address of the client funding the escrow.
    pub client: Address,
    /// Address of the artisan performing the work.
    pub artisan: Address,
    /// Amount held in escrow (in stroops).
    pub amount: i128,
    /// Current status of the escrow.
    pub status: EscrowStatus,
    /// Type of escrow.
    pub escrow_type: EscrowType,
    /// Ledger timestamp when the escrow was created.
    pub created_at: u64,
    /// Ledger timestamp when the escrow was last updated.
    pub updated_at: u64,
    /// Optional deadline for delivery.
    pub deadline: Option<u64>,
    /// Optional description or metadata hash.
    pub metadata_hash: Option<BytesN<32>>,
}

/// Configuration for recurring escrow payments.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringConfig {
    /// Interval between payments in seconds.
    pub interval: u64,
    /// Number of payments remaining.
    pub remaining_payments: u32,
    /// Total number of payments in the schedule.
    pub total_payments: u32,
    /// Timestamp of the next scheduled payment.
    pub next_payment_at: u64,
}

/// Platform-wide configuration managed by the admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformConfig {
    /// Address of the platform admin.
    pub admin: Address,
    /// Platform fee in basis points (1/100th of a percent).
    pub fee_bps: u32,
    /// Address where platform fees are collected.
    pub fee_recipient: Address,
    /// Whether the platform is currently paused.
    pub paused: bool,
    /// Minimum escrow amount allowed.
    pub min_escrow_amount: i128,
    /// Maximum escrow amount allowed.
    pub max_escrow_amount: i128,
}

/// Represents an action that can be taken to repair a reconciliation discrepancy.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairAction {
    /// Credit funds to an account.
    Credit(Address, i128),
    /// Debit funds from an account.
    Debit(Address, i128),
    /// Mark an escrow as resolved.
    ResolveEscrow(u64),
    /// Mark an escrow as cancelled.
    CancelEscrow(u64),
    /// No action required.
    Noop,
}

/// A single discrepancy found during reconciliation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discrepancy {
    /// Identifier of the affected escrow, if applicable.
    pub escrow_id: Option<u64>,
    /// Address of the affected account, if applicable.
    pub account: Option<Address>,
    /// Expected balance.
    pub expected: i128,
    /// Actual balance.
    pub actual: i128,
    /// Human-readable description of the discrepancy.
    pub description: String,
}

/// A reconciliation report summarizing the state of the contract's accounting.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationReport {
    /// Unique identifier for the report.
    pub id: u64,
    /// Ledger timestamp when the report was generated.
    pub generated_at: u64,
    /// Address that generated the report.
    pub reporter: Address,
    /// Total expected balance across all accounts.
    pub total_expected: i128,
    /// Total actual balance across all accounts.
    pub total_actual: i128,
    /// List of discrepancies found.
    pub discrepancies: Vec<Discrepancy>,
    /// Whether the report has been resolved.
    pub resolved: bool,
}

/// A proposed repair plan to address reconciliation discrepancies.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairPlan {
    /// Unique identifier for the repair plan.
    pub id: u64,
    /// Identifier of the reconciliation report this plan addresses.
    pub report_id: u64,
    /// Address that proposed the plan.
    pub proposer: Address,
    /// List of actions to be taken.
    pub actions: Vec<RepairAction>,
    /// Ledger timestamp when the plan was proposed.
    pub proposed_at: u64,
    /// Whether the plan has been executed.
    pub executed: bool,
}

/// Staking position for an artisan.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakePosition {
    /// Address of the staker (artisan).
    pub staker: Address,
    /// Amount currently staked.
    pub amount: i128,
    /// Ledger timestamp when the stake was created.
    pub staked_at: u64,
    /// Ledger timestamp when the stake was last updated.
    pub updated_at: u64,
    /// Whether the stake is currently locked as collateral.
    pub locked: bool,
}

/// Collateral posted against an escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Collateral {
    /// Identifier of the escrow this collateral secures.
    pub escrow_id: u64,
    /// Address of the staker providing collateral.
    pub staker: Address,
    /// Amount of collateral posted.
    pub amount: i128,
    /// Ledger timestamp when the collateral was posted.
    pub posted_at: u64,
    /// Whether the collateral has been liquidated.
    pub liquidated: bool,
}

/// Dispute record for an escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    /// Identifier of the disputed escrow.
    pub escrow_id: u64,
    /// Address that raised the dispute.
    pub raised_by: Address,
    /// Reason for the dispute.
    pub reason: String,
    /// Ledger timestamp when the dispute was raised.
    pub raised_at: u64,
    /// Whether the dispute has been resolved.
    pub resolved: bool,
    /// Optional resolution notes.
    pub resolution: Option<String>,
}

/// Role assigned to an address for access control.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Platform administrator with full privileges.
    Admin,
    /// Address authorized to arbitrate disputes.
    Arbitrator,
    /// Address authorized to propose reconciliation repairs.
    Reconciler,
    /// Standard user (client or artisan).
    User,
}

/// A paginated result set.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page<T> {
    /// Items in the current page.
    pub items: Vec<T>,
    /// Cursor for the next page, if any.
    pub next_cursor: Option<u64>,
}
