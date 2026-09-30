#![cfg(test)]
extern crate std;

use crate::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::Client as TokenClient,
    Address, Env,
};

fn setup_env<'a>() -> (
    Env,
    CraftNexusContractClient<'a>,
    Address,
    Address,
    TokenClient<'a>,
) {
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
    assert!(
        res.is_err(),
        "New deposit accidentally bypassed cooldown rules"
    );

    assert_eq!(
        client.get_stake(&artisan),
        1500,
        "Full stake should remain locked"
    );
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
    assert_eq!(
        remaining_stake, 500,
        "Matured deposit was blocked by the new deposit"
    );
}

// ── Issue #1366: get_stake_cooldown must tolerate missing storage ────────────

/// The getter must return the typed `PlatformNotInitialized` error — instead of
/// trapping the host — when the platform config key has never been written.
/// This is the state a fresh deployment, an archived key, or a partial
/// migration presents to callers.
#[test]
fn test_get_stake_cooldown_returns_error_when_config_missing() {
    let env = Env::default();
    env.mock_all_auths();

    // Deliberately skip `initialize`: nothing has been written to
    // `DataKey::PlatformConfig` yet.
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let result = client.try_get_stake_cooldown();
    assert_eq!(result, Err(Ok(Error::PlatformNotInitialized)));
}

/// Once initialized, the getter keeps returning the configured cooldown even
/// after the stake lifecycle reaches a terminal (fully withdrawn) state.
#[test]
fn test_get_stake_cooldown_readable_after_terminal_stake_state() {
    let (env, client, _, artisan, token) = setup_env();

    const COOLDOWN: u32 = 86_400; // 1 day
    client.set_stake_cooldown(&COOLDOWN);
    assert_eq!(client.get_stake_cooldown(), COOLDOWN);

    // Drive the artisan's stake to a terminal state: stake, wait out the
    // cooldown, then withdraw everything.
    client.stake_tokens(&artisan, &token.address, &1000);
    let stake_time = env.ledger().timestamp();
    env.ledger().set_timestamp(stake_time + COOLDOWN as u64 + 1);
    client.unstake_tokens(&artisan, &token.address);
    assert_eq!(
        client.get_stake(&artisan),
        0,
        "stake should be fully withdrawn"
    );

    // The config entry is untouched by the staking lifecycle, so the getter
    // must still return the configured value rather than an error.
    assert_eq!(client.get_stake_cooldown(), COOLDOWN);
}

#[test]
fn test_get_artisan_stake_data_missing_key() {
    let (_env, client, _, artisan, _token) = setup_env();

    // Before any stake record exists, the call must not trap.
    let result = client.try_get_artisan_stake_data(&artisan);
    assert!(
        result.is_err(),
        "get_artisan_stake_data should return an error when the key is absent"
    );
}

#[test]
fn test_get_artisan_stake_data_after_terminal_state() {
    let (env, client, _, artisan, token) = setup_env();

    client.stake_tokens(&artisan, &token.address, &1000);

    // Advance past the cooldown and fully unstake to reach a terminal state.
    env.ledger().set_timestamp(env.ledger().timestamp() + (86400 * 7) + 1);
    client.unstake_tokens(&artisan, &token.address);

    // After the terminal state, the call must not trap and should return an error.
    let result = client.try_get_artisan_stake_data(&artisan);
    assert!(
        result.is_err(),
        "get_artisan_stake_data should return an error after the terminal state"
    );
}
