#![cfg(test)]

use crate::{CraftNexusContract, CraftNexusContractClient, DataKey};
use soroban_sdk::Env;

#[test]
fn get_escrow_count_before_any_escrow_returns_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    assert_eq!(client.get_escrow_count(), 0);
    // Stable on repeated calls.
    assert_eq!(client.get_escrow_count(), 0);
}

#[test]
fn get_escrow_count_after_key_removed_returns_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::EscrowCount, &5u32);
    });
    assert_eq!(client.get_escrow_count(), 5);

    // Simulate archival / partial migration: key disappears.
    env.as_contract(&contract_id, || {
        env.storage().persistent().remove(&DataKey::EscrowCount);
    });
    assert_eq!(client.get_escrow_count(), 0);
}
