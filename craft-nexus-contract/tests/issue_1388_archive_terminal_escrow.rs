//! Regression coverage for issue #1388 — harden terminal escrow archival.

use craft_nexus_contract::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = token_id.address();

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &100u32,
        &None,
    );
    token::StellarAssetClient::new(&env, &token_address).mint(&buyer, &10_000);
    client.create_escrow(
        &buyer,
        &seller,
        &token_address,
        &1_000,
        &1,
        &Some(86_400),
    );

    (env, contract_id, buyer, token_address, admin)
}

#[test]
fn non_terminal_escrow_rejection_preserves_balances_and_archival_state() {
    let (env, contract_id, buyer, token_address, _) = setup();
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_address);
    let contract_balance = token_client.balance(&contract_id);
    let buyer_balance = token_client.balance(&buyer);

    let result = client.try_archive_terminal_escrow(&1);

    assert!(matches!(result, Err(Ok(Error::InvalidEscrowState))));
    assert_eq!(token_client.balance(&contract_id), contract_balance);
    assert_eq!(token_client.balance(&buyer), buyer_balance);
    assert_eq!(client.get_archival_summary(&1), None);
}

#[test]
fn paused_archival_rejection_preserves_balances_and_archival_state() {
    let (env, contract_id, buyer, token_address, _) = setup();
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_address);
    let contract_balance = token_client.balance(&contract_id);
    let buyer_balance = token_client.balance(&buyer);

    client.set_paused(&true);
    let result = client.try_archive_terminal_escrow(&1);

    assert!(matches!(result, Err(Ok(Error::ContractPaused))));
    assert_eq!(token_client.balance(&contract_id), contract_balance);
    assert_eq!(token_client.balance(&buyer), buyer_balance);
    assert_eq!(client.get_archival_summary(&1), None);
}
