#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup() -> (
    Env,
    CraftNexusContractClient<'static>,
    Address,
    token::StellarAssetClient<'static>,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());
    (
        env,
        client,
        platform_wallet,
        token_admin_client,
        token_contract.address(),
    )
}

#[test]
fn invalid_archival_policy_returns_error_without_changing_policy() {
    let (_env, client, wallet, token_admin, token) = setup();
    let original = client.get_archival_policy();
    token_admin.mint(&wallet, &100);
    let token_client = token::Client::new(&_env, &token);
    let balance_before = token_client.balance(&wallet);

    let result = client.try_set_archival_policy(&0, &10);

    assert!(matches!(result, Err(Ok(Error::InvalidArchivalPolicy))));
    assert_eq!(client.get_archival_policy(), original);
    assert_eq!(token_client.balance(&wallet), balance_before);
}

#[test]
fn archival_policy_cannot_change_while_paused() {
    let (_env, client, _wallet, _token_admin, _token) = setup();
    let original = client.get_archival_policy();
    client.set_paused(&true);

    let result = client.try_set_archival_policy(&120, &10);

    assert!(matches!(result, Err(Ok(Error::ContractPaused))));
    assert_eq!(client.get_archival_policy(), original);
}
