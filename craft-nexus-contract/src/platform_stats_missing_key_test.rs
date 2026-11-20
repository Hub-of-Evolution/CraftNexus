#![cfg(test)]

use crate::{CraftNexusContract, CraftNexusContractClient, PlatformStats};
use soroban_sdk::Env;

#[test]
fn get_platform_stats_before_initialize_returns_empty_stats() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    // No storage keys exist yet; the read must not trap.
    assert_eq!(
        client.get_platform_stats(),
        PlatformStats {
            total_volume: 0,
            total_escrows: 0,
            active_users: 0,
            whitelist_count: 0,
        }
    );
}

#[test]
fn get_platform_stats_is_stable_when_called_repeatedly_with_missing_keys() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    assert_eq!(client.get_platform_stats(), client.get_platform_stats());
}
