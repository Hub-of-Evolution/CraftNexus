//! Storage lifecycle, compaction, and TTL-management framework (#920).
//!
//! Long-running deployments accumulate per-user history, stake records,
//! verification requests, and upgrade approvals in persistent storage. This
//! module provides an admin-gated maintenance surface that:
//!
//! 1. Separates active data from archival data by compacting old records.
//! 2. Applies explicit retention rules to audit queues so they can be pruned
//!    safely without corrupting active state.
//! 3. Ensures TTL extensions happen consistently for every active record that
//!    survives a compaction pass.
//! 4. Exposes a predictable storage-growth strategy for high-volume
//!    deployments via a configurable retention policy.
//!
//! The framework is deliberately conservative: it never removes entries that
//! are still referenced by active state (open escrows, pending withdrawals,
//! unresolved disputes). Only archival audit queues that have passed their
//! configured retention window are compacted, and every surviving entry has
//! its TTL refreshed so indexers and read paths keep working.

use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Default number of most-recent audit entries retained per actor when no
/// explicit retention policy has been configured.
pub const DEFAULT_RETAINED_AUDIT_ENTRIES: u32 = 100;
/// Default number of most-recent stake-history entries retained per artisan.
pub const DEFAULT_RETAINED_STAKE_HISTORY: u32 = 50;
/// Default number of most-recent emergency-operation history entries retained.
pub const DEFAULT_RETAINED_EMERGENCY_HISTORY: u32 = 100;
/// Default number of most-recent upgrade-history records retained.
pub const DEFAULT_RETAINED_UPGRADE_HISTORY: u32 = 32;

/// Admin-configurable retention policy governing how many archival entries are
/// kept per queue family before older entries are compacted away.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageRetentionPolicy {
    /// Most-recent fund-movement audit entries retained per actor.
    pub fund_audit_retention: u32,
    /// Most-recent stake-history entries retained per artisan.
    pub stake_history_retention: u32,
    /// Most-recent emergency-operation history entries retained.
    pub emergency_history_retention: u32,
    /// Most-recent upgrade-history records retained.
    pub upgrade_history_retention: u32,
}

impl Default for StorageRetentionPolicy {
    fn default() -> Self {
        Self {
            fund_audit_retention: DEFAULT_RETAINED_AUDIT_ENTRIES,
            stake_history_retention: DEFAULT_RETAINED_STAKE_HISTORY,
            emergency_history_retention: DEFAULT_RETAINED_EMERGENCY_HISTORY,
            upgrade_history_retention: DEFAULT_RETAINED_UPGRADE_HISTORY,
        }
    }
}

/// Result summary returned by a compaction run so operators can observe how
/// much storage was reclaimed and confirm active state was left untouched.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactionReport {
    /// Number of archival entries removed across all queue families.
    pub entries_removed: u32,
    /// Number of active entries whose TTL was refreshed.
    pub ttl_extended: u32,
    /// Monotonic run counter after this compaction.
    pub run_count: u32,
    /// Ledger timestamp when the run completed.
    pub completed_at: u64,
}

/// Emits a `storage_compaction` event describing a completed run.
pub fn emit_compaction_event(
    env: &Env,
    report: &CompactionReport,
    policy: &StorageRetentionPolicy,
) {
    env.events().publish(
        (
            Symbol::new(env, "storage_compaction"),
            Symbol::new(env, "run"),
        ),
        (
            report.run_count,
            report.entries_removed,
            report.ttl_extended,
        ),
    );
    // Keep the policy visible to off-chain dashboards on every run.
    env.events().publish(
        (
            Symbol::new(env, "storage_retention_policy"),
            Symbol::new(env, "applied"),
        ),
        (
            policy.fund_audit_retention,
            policy.stake_history_retention,
            policy.emergency_history_retention,
            policy.upgrade_history_retention,
        ),
    );
}
