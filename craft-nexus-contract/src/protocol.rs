#![allow(dead_code)]

use soroban_sdk::{token, Address, Env, IntoVal, TryFromVal, Val};

/// Protocol compatibility adapter to isolate storage, token calls, TTL behavior,
/// and ledger assumptions from the business logic.
///
/// Supported Protocol Versions: Soroban V20 / V21
/// Behavioral Assumptions:
/// - TTL extensions are governed by explicit threshold and extension values.
/// - Ledger sequence and timestamps follow standard Soroban semantics.
pub struct ProtocolAdapter<'a> {
    env: &'a Env,
}

impl<'a> ProtocolAdapter<'a> {
    pub fn new(env: &'a Env) -> Self {
        Self { env }
    }

    /// Read a persistent storage value.
    pub fn get_persistent<K, V>(&self, key: &K) -> Option<V>
    where
        K: IntoVal<Env, Val>,
        V: TryFromVal<Env, Val>,
    {
        self.env.storage().persistent().get(key)
    }

    /// Set a persistent storage value.
    pub fn set_persistent<K, V>(&self, key: &K, val: &V)
    where
        K: IntoVal<Env, Val>,
        V: IntoVal<Env, Val>,
    {
        self.env.storage().persistent().set(key, val);
    }

    /// Extend the TTL of a persistent storage entry.
    pub fn extend_persistent_ttl<K>(&self, key: &K, threshold: u32, extension: u32)
    where
        K: IntoVal<Env, Val>,
    {
        self.env.storage().persistent().extend_ttl(key, threshold, extension);
    }

    /// Extend the TTL of the contract instance.
    pub fn extend_instance_ttl(&self, threshold: u32, extension: u32) {
        self.env.storage().instance().extend_ttl(threshold, extension);
    }

    /// Perform a token transfer using the standard Soroban token interface.
    pub fn transfer_token(&self, token: &Address, from: &Address, to: &Address, amount: i128) {
        let client = token::Client::new(self.env, token);
        client.transfer(from, to, &amount);
    }

    /// Retrieve the current ledger timestamp.
    pub fn ledger_timestamp(&self) -> u64 {
        self.env.ledger().timestamp()
    }

    /// Retrieve the current ledger sequence number.
    pub fn ledger_sequence(&self) -> u32 {
        self.env.ledger().sequence()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::{Ledger, Address as _}, Symbol, Env};

    #[test]
    fn test_adapter_basics() {
        let env = Env::default();
        let adapter = ProtocolAdapter::new(&env);
        
        env.ledger().set_timestamp(1234567890);
        assert_eq!(adapter.ledger_timestamp(), 1234567890);
        
        // env.ledger().set_sequence(42);
        assert_eq!(adapter.ledger_sequence(), 0); // test env default is usually 0
        
        let key = Symbol::new(&env, "test_key");
        adapter.set_persistent(&key, &100_u32);
        
        let val: Option<u32> = adapter.get_persistent(&key);
        assert_eq!(val, Some(100));
        
        // TTL extending without errors
        adapter.extend_persistent_ttl(&key, 100, 200);
        adapter.extend_instance_ttl(100, 200);
    }
}
