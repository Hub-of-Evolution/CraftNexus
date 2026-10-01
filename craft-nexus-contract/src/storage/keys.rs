use soroban_env::{Env, Address};
use soroban_contracttype::contracttype;

/// TTL constants for storage entries.
pub const DAY_IN_LEDGERS: u32 = 17280;
pub const DAY_IN_LEDGERS_EXTENDED: u32 = 34560;
pub const HOUR_IN_LEDGERS: u32 = 720;
pub const HOUR_IN_LEDGERS_EXTENDED: u32 = 1440;

/// Data keys used for persistent storage.
# [macro]
 # [derive(Clone, Debug, Eq PartialEq, PartialOrd)]
# [contracttype]
pub enum DataKey {
    // Admin / configuration
    Admin,
    PlatformConfig,
    Paused,
    FeeCollector,
    FeeBasisPoints,
    TokenAddress,
    AllowedTokens,

    // Escrow counters
    EscrowCount,
    Escrow(u64),
    EscrowItems(u64),
    EscrowStatus(u64),
    EscrowDispute(u64),
    EscrowResolution(u64),
    EscrowReport(u64),

    // Recurring escrow
    RecurringCount,
    RecurringEscrow(u64),
    RecurringStatus(u64),
    RecurringLastPayment(u64),

    // Reconciliation
    ReconReportCount,
    ReconReport(u64),
    ReconRepairPlanCount,
    ReconRepairPlan(u64),
    ReconAuditLogCount,
    ReconAuditLog(u64),

    // Staking
    ArtisanStake(Address),
    ArtisanCollateral(Address),
    ArtisanLiquidated(Address),
    StakingTotal,

    // General
    Initialized,
    Version,
}

/// Extend the TTL for a given data key.
pub fn extend_ttl(env: &Env, key: &DataKey) {
    env.storage().extend_ttl(key, DAY_IN_LEDGERS_EXTENDED);
}
