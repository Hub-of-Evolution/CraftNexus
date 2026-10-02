use soroban_sdk::{token, Address, Env, IntoVal, Val};

/// Protocol compatibility adapter isolating protocol specifics
/// (storage TTL, token calls, ledger assumptions) from business logic.
///
/// This provides a documented adaptation point for future Stellar protocol changes.
/// Supported Protocol: Soroban Release 21.0
pub struct Protocol;

impl Protocol {
    /// Extends the TTL of a persistent storage entry.
    pub fn extend_persistent_ttl(env: &Env, key: &impl IntoVal<Env, Val>, threshold: u32, extension: u32) {
        env.storage().persistent().extend_ttl(key, threshold, extension);
    }

    /// Extends the TTL of an instance storage entry.
    pub fn extend_instance_ttl(env: &Env, threshold: u32, extension: u32) {
        env.storage().instance().extend_ttl(threshold, extension);
    }

    /// Extends the TTL of a temporary storage entry.
    pub fn extend_temporary_ttl(env: &Env, key: &impl IntoVal<Env, Val>, threshold: u32, extension: u32) {
        env.storage().temporary().extend_ttl(key, threshold, extension);
    }

    /// Safely transfers tokens from one address to another using the Soroban token client.
    pub fn transfer_tokens(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
        let client = token::Client::new(env, token);
        client.transfer(from, to, &amount);
    }

    /// Retrieves the balance of tokens for a given address.
    pub fn get_token_balance(env: &Env, token: &Address, account: &Address) -> i128 {
        let client = token::Client::new(env, token);
        client.balance(account)
    }

    /// Retrieves the decimals of a token.
    pub fn get_token_decimals(env: &Env, token: &Address) -> Result<u32, ()> {
        let client = token::Client::new(env, token);
        client.try_decimals().map_err(|_| ())?.map_err(|_| ())
    }

    /// Retrieves the current ledger timestamp.
    pub fn get_ledger_timestamp(env: &Env) -> u64 {
        env.ledger().timestamp()
    }
}
