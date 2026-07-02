//! Minimal test-only token contract with configurable decimal places.
//!
//! Used by onboarding tests to exercise volume normalization for tokens whose
//! native precision differs from the 7-decimal auto-verification baseline.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};

#[contract]
pub struct DecimalTestToken;

#[contractimpl]
impl DecimalTestToken {
    /// Store the decimal precision this token reports via [`Self::decimals`].
    pub fn initialize(env: Env, _admin: Address, decimals: u32) {
        env.storage()
            .instance()
            .set(&symbol_short!("DEC"), &decimals);
    }

    /// SEP-41 compatible decimals query used by onboarding volume normalization.
    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("DEC"))
            .unwrap_or(7)
    }
}
