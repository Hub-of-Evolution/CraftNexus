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

    /// Minimal SEP-41 balance probe used by token compatibility validation.
    pub fn balance(_env: Env, _id: Address) -> i128 {
        0
    }

    /// Minimal SEP-41 transfer probe used by token compatibility validation.
    ///
    /// This is a no-op for test purposes — it never mutates balances, but its
    /// presence allows `whitelist_token` to verify that the contract exposes the
    /// required `transfer` entrypoint. The validation flow uses a zero-amount
    /// self-transfer (`contract -> contract, 0`) so no customer funds are moved.
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}
