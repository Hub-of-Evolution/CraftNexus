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
