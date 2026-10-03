#![cfg(test)]
//! Regression tests for terminal escrow archival failure paths.

use super::*;
use soroban_sdk::testutils::Address as _;

fn setup_archive_test(
    env: &Env,
) -> (
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let onboarding = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding),
    );
    client.set_min_escrow_amount(&token_contract.address(), &0);

    (
        client,
        buyer,
        seller,
        token_contract.address(),
        token_admin_client,
    )
}

fn archive_count_and_summary(env: &Env, client: &CraftNexusContractClient) -> (u32, bool) {
    env.as_contract(&client.address, || {
        (
            env.storage()
                .persistent()
                .get(&DataKey::ArchivalSummaryCount)
                .unwrap_or(0),
            env.storage()
                .persistent()
                .has(&DataKey::ArchivalSummary(1)),
        )
    })
}

#[test]
fn archive_rejections_preserve_storage_and_balances() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_archive_test(&env);
    token_admin.mint(&buyer, &1_000);
    client.create_escrow(&buyer, &seller, &token_id, &1_000, &1, &None);

    let token_client = token::Client::new(&env, &token_id);
    let buyer_balance = token_client.balance(&buyer);
    let seller_balance = token_client.balance(&seller);
    let contract_balance = token_client.balance(&client.address);
    let escrow_before = client.get_escrow(&1);

    client.set_paused(&true);
    let paused = client.try_archive_terminal_escrow(&1);
    assert_eq!(paused.unwrap_err(), Ok(Error::ContractPaused));
    assert_eq!(client.get_escrow(&1), escrow_before);
    assert_eq!(archive_count_and_summary(&env, &client), (0, false));
    assert_eq!(token_client.balance(&buyer), buyer_balance);
    assert_eq!(token_client.balance(&seller), seller_balance);
    assert_eq!(token_client.balance(&client.address), contract_balance);

    client.set_paused(&false);
    let invalid_state = client.try_archive_terminal_escrow(&1);
    assert_eq!(invalid_state.unwrap_err(), Ok(Error::InvalidEscrowState));
    assert_eq!(client.get_escrow(&1), escrow_before);
    assert_eq!(archive_count_and_summary(&env, &client), (0, false));
    assert_eq!(token_client.balance(&buyer), buyer_balance);
    assert_eq!(token_client.balance(&seller), seller_balance);
    assert_eq!(token_client.balance(&client.address), contract_balance);
}

#[test]
fn archive_without_admin_auth_preserves_storage_and_balances() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_archive_test(&env);
    token_admin.mint(&buyer, &1_000);
    client.create_escrow(&buyer, &seller, &token_id, &1_000, &1, &None);

    let token_client = token::Client::new(&env, &token_id);
    let buyer_balance = token_client.balance(&buyer);
    let seller_balance = token_client.balance(&seller);
    let contract_balance = token_client.balance(&client.address);
    let escrow_before = client.get_escrow(&1);
    env.set_auths(&[]);

    assert!(client.try_archive_terminal_escrow(&1).is_err());
    assert_eq!(client.get_escrow(&1), escrow_before);
    assert_eq!(archive_count_and_summary(&env, &client), (0, false));
    assert_eq!(token_client.balance(&buyer), buyer_balance);
    assert_eq!(token_client.balance(&seller), seller_balance);
    assert_eq!(token_client.balance(&client.address), contract_balance);
}
