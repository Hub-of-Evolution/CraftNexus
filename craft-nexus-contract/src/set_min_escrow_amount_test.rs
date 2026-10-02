#![cfg(test)]

use crate::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn setup() -> (Env, CraftNexusContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    
    // Initialize platform
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    
    (env, client, admin)
}

#[test]
fn test_set_min_escrow_amount_rejected_input() {
    let (env, client, _admin) = setup();
    let token = Address::generate(&env);
    
    // Try to set negative minimum amount
    let res = client.try_set_min_escrow_amount(&token, &-100);
    
    // Assert the exact Error variant
    assert_eq!(res, Err(Ok(Error::AmountBelowMinimum)));
    
    // Test paused contract
    client.set_paused(&true);
    let res_paused = client.try_set_min_escrow_amount(&token, &100);
    assert_eq!(res_paused, Err(Ok(Error::ContractPaused)));
}
