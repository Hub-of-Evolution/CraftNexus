//! # `cancel_recurring_escrow` auth / pause / accounting coverage (#1376)
//!
//! `cancel_recurring_escrow` refunds `total_amount - released_amount` to the buyer,
//! so a rejected call must leave both the escrow record and the token balances
//! byte-for-byte unchanged. These cases pin down the three rejection paths (missing
//! buyer authorization, paused platform, unknown id), the non-repeatable success
//! path, and the terminal second cancel.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

struct Fixture {
    env: Env,
    client: CraftNexusContractClient<'static>,
    buyer: Address,
    artisan: Address,
    token: Address,
    token_admin: token::StellarAssetClient<'static>,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.budget().reset_unlimited();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let onboarding = Address::generate(&env);
    let buyer = Address::generate(&env);
    let artisan = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000;
    });

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &Some(onboarding));
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

    Fixture {
        env,
        client,
        buyer,
        artisan,
        token: token_contract.address(),
        token_admin: token_admin_client,
    }
}

/// Fund the buyer and open a 100_000 recurring escrow, so the contract holds the
/// full deposit and nothing has been released yet.
fn open_recurring(f: &Fixture) {
    f.token_admin.mint(&f.buyer, &1_000_000);
    f.client
        .create_recurring_escrow(&f.buyer, &f.artisan, &f.token, &100_000, &86_400, &3);
}

#[test]
fn test_cancel_recurring_escrow_rejected_while_paused_leaves_value_untouched() {
    let f = setup();
    open_recurring(&f);
    let token_client = token::Client::new(&f.env, &f.token);

    let buyer_before = token_client.balance(&f.buyer);
    let contract_before = token_client.balance(&f.client.address);
    assert_eq!(buyer_before, 900_000, "the deposit is held by the contract");
    assert_eq!(contract_before, 100_000);

    f.client.set_paused(&true);
    let result = f.client.try_cancel_recurring_escrow(&1);

    assert!(matches!(result, Err(Ok(Error::ContractPaused))));
    // Rejection happens before any storage write or token transfer.
    assert_eq!(token_client.balance(&f.buyer), buyer_before);
    assert_eq!(token_client.balance(&f.client.address), contract_before);
    assert!(f.client.get_recurring_escrow(&1).is_active);
}

#[test]
fn test_cancel_recurring_escrow_without_buyer_auth_leaves_value_untouched() {
    let f = setup();
    open_recurring(&f);
    let token_client = token::Client::new(&f.env, &f.token);

    let buyer_before = token_client.balance(&f.buyer);
    let contract_before = token_client.balance(&f.client.address);

    // Drop every mocked authorization: the buyer's `require_auth` must now fail.
    f.env.set_auths(&[]);
    let result = f.client.try_cancel_recurring_escrow(&1);

    assert!(result.is_err(), "cancel must require the buyer's authorization");
    assert_eq!(token_client.balance(&f.buyer), buyer_before);
    assert_eq!(token_client.balance(&f.client.address), contract_before);
}

#[test]
fn test_cancel_recurring_escrow_unknown_id_reports_not_found() {
    let f = setup();
    open_recurring(&f);

    let result = f.client.try_cancel_recurring_escrow(&9_999);

    assert!(matches!(result, Err(Ok(Error::RecurringEscrowNotFound))));
}

#[test]
fn test_cancel_recurring_escrow_refunds_the_whole_undrawn_deposit() {
    let f = setup();
    open_recurring(&f);
    let token_client = token::Client::new(&f.env, &f.token);

    f.client.cancel_recurring_escrow(&1);

    // Nothing had been released, so `total - released` is the full deposit.
    assert_eq!(token_client.balance(&f.buyer), 1_000_000);
    assert_eq!(token_client.balance(&f.client.address), 0);
    assert!(!f.client.get_recurring_escrow(&1).is_active);

    // A second cancel is rejected: the escrow is no longer active.
    let again = f.client.try_cancel_recurring_escrow(&1);
    assert!(matches!(again, Err(Ok(Error::InvalidEscrowState))));

    // The rejected second cancel must not have paid out twice.
    assert_eq!(token_client.balance(&f.buyer), 1_000_000);
    assert_eq!(token_client.balance(&f.client.address), 0);
}
