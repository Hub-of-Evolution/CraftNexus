#`![no_std]

/// Craft Nexus Contract — modular entrypoint routing layer.
///
/// This file is deliberately thin: it declares the contract struct,
/// routes entrypoints to domain modules, and holds the `ilpl CraftNexusContract`
/// blocks that delegate to `core::{escrow,recurring,admin,reconciliation,staking}`.

mod types;
mod error;
mod storage;
mod core;

pub use types::*;
pub use error::Error;
pub use storage::keys::DataKey;

use soroban_sdk::{contract, contractimpl, Env, Address, Symbol, Val
}}}}}}}}
