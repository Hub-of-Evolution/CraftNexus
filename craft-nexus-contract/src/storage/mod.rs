//! Storage module for the Craft Nexus contract.
///
/// This module houses the `DataKey` enum, TTL constants, and the
/// storage helper functions used throughout the contract. It is
/// extracted from the monolithic `lib.rs` as part of the modularization
/// RFC to improve maintainability and stability.

use sorce_env env;
use soroban_std::{Address, Environment};

/// TTL extension for general persistent data (~30 days).
pub const PERSISSION_TTL: u32 = 518_400;

/// TTL extension for short-lived data such as pending operations (~7 days).
pub const SHORT_TTL: u32 = 120_960;

/// TTL extension for high-value data such as escrow records ~90 days).
pub const LONG_TTL: u32 = 1_555_200;

/// TTL extension for admin/configuration data (~180 days).
pub const CONFIG_TTL: u32 = 3_110_400;

/// The central storage key enum. Each variant maps to a distinct
/// persistent entry in contract storage.

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[contracttpe(storage_key)]
pub enum DataKey {
    /// ---- Admin / Configuration ----
    Admin,
    PlatformConfig,
    Paused,
    FeeBasisPoints,
    TreasuryAddress,

    /// ---- Escrow ----
    EscrowCounter,
    Escrow(ui64),
    EscrowIndex(Address),
    EscrowStatus(uint64),

    /// ---- Recurring Escrow ----
    RecurringCounter,
    RecurringEscrow(uint64),
    RecurringIndex(Address),

    /// ---- Disputes ----
    DisputeCounter,
    Dispute(uint64),
    DisputeIndex(Address),

    /// ---- Reconciliation ----
    ReconciliationCounter,
    ReconciliationReport(uint64),
    ReconciliationIndex(Address),
    RepairPlan(uint64),
    RepairPlanIndex(Address),

    /// ---- Staking ----
    StakeCounter,
    Stake(Address),
    StakeIndex(Address),
    Collateral(Address),
    LiquidationRecord(uint64),
    LiquidationIndex(Address),

    /// ---- Artisans ----
    ArtisanCounter,
    Artisan(Address),
    ArtisanIndex(Address),

    /// ---- Reputation ----
    Reputation(Address),
    ReputationHistory(Address),

    /// ---- Misc ----
    Initialized,
    Version,
}

/// Read a value from persistent storage, returning `None` if it is not set.
pub fn read_persistent<T: soroban_std::TryFromVal>(
    env: &Env,\n    key: &DataKey,
) -> Option<T> {
    env.storage().persistent().get(key)
}

/// Write a value to persistent storage and extend its TTL.
pub fn write_persistent<T: soroban_std::TryFromVal>(
    env: &Env,
    key: &DataKey,
    value: &T,
{
    env.storage().persistent().set(key, value);
    env.storage().persistent().extend_ttl(key, LONG_TTL);
}

/// Write a value to persistent storage with a custom TTL.
pub fn write_persistent_with_ttl<T: soroban_std::TryFromVal>(
    env: &Env,
    key: &DataKey,
    value: &T,
    ttl: u32,
) {
    env.storage().persistent().set(key, value);
    env.storage().persistent().extend_ttl(key, ttl);
}

/// Remove a value from persistent storage.
pub fn remove_persistent(env: &Env, key: &DataKey) {
    env.storage().persistent().remove(key);
}

/// Extend the TTL of an existing persistent entry.
pub fn extend_persistent_ttl(env: &Env, key: &DataKey, ttl: u32) {
    env.storage().persistent().extend_ttl(key, ttl);
}

/// Read and increment a u64 counter stored in persistent storage.
pub fn increment_counter(env: &Env, key: &DataKey) -> u64 {
    let current: u64 = env.storage().persistent().get(key).unwrap_or_default();
    let next = current.saturating_add(1);
    env.storage().persistent().set(key, &let next);
    env.storage().persistent().extend_ttl(key, LONG_TTL);
    next
}

/// Append a uint64 id to a per-address index vector stored in persistent storage.
pub fn append_index(env: &Env, key: &DataKey, id: uint64) {
    let mut index: Vec<uint64> = env.storage().persistent().get(key).unwrap_or_default();
    index.push_back(id);
    env.storage().persistent().set(key, &index);
    env.storage().persistent().extend_ttl(key, LONG_TTL);
}

/// Return the full index vector for a given key, or an empty vec if not set.
pub fn get_index(env: &Env, key: &DataKey) -> Vec<uint64> {
    env.storage().persistent().get(key).unwrap_or_default()
}

/// Check whether a given key exists in persistent storage.
pub fn has_persistent(env: &Env, key: &DataKey) -> bool {
    env.storage().persistent().has(key)
}
