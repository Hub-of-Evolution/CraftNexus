#![cfg(test)]
extern crate std;

use crate::{CraftNexusContract, CraftNexusContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::Client as TokenClient,
    Address, Env,
};

fn setup_env<'a>() -> (Env, CraftNexusContractClient<'a>, Address, Address, TokenClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();
    // Initialize ledger time to a known baseline
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let artisan = Address::generate(&env);

    // Setup native token for staking
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract);
    let stellar_asset_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_contract);
    stellar_asset_client.mint(&artisan, &10_000);

    // Setup main contract
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &admin, // platform_wallet
        &admin, // admin
        &admin, // arbitrator
        &500,   // platform_fee_bps
        &None,  // onboarding_contract
    );

    client.whitelist_token(&token_contract);

    (env, client, admin, artisan, token_client)
}

#[test]
fn test_new_deposit_does_not_bypass_cooldown() {
    let (env, client, _, artisan, token) = setup_env();

    // 1. Initial stake
    client.stake_tokens(&artisan, &token.address, &1000);
    let initial_time = env.ledger().timestamp();

    // 2. Advance time forward, but not past the 7-day cooldown (3.5 days)
    env.ledger().set_timestamp(initial_time + (86400 * 7) / 2);

    // 3. Second stake added
    client.stake_tokens(&artisan, &token.address, &500);

    // 4. Attempt withdrawal. Neither should be ready, so this should error out.
    let res = client.try_unstake_tokens(&artisan, &token.address);
    assert!(res.is_err(), "New deposit accidentally bypassed cooldown rules");
    
    assert_eq!(client.get_stake(&artisan), 1500, "Full stake should remain locked");
}

#[test]
fn test_matured_deposits_remain_withdrawable() {
    let (env, client, _, artisan, token) = setup_env();

    // 1. Initial stake
    client.stake_tokens(&artisan, &token.address, &1000);
    let initial_time = env.ledger().timestamp();

    // 2. Advance time just past the cooldown for the first stake
    env.ledger().set_timestamp(initial_time + (86400 * 7) + 1);

    // 3. Add a new stake
    client.stake_tokens(&artisan, &token.address, &500);

    // 4. Withdraw matured stakes. 
    // The first 1000 is ready, the 500 should remain locked.
    client.unstake_tokens(&artisan, &token.address);

    let remaining_stake = client.get_stake(&artisan);
    assert_eq!(remaining_stake, 500, "Matured deposit was blocked by the new deposit");
}