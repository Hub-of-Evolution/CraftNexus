#![cfg(test)]
//! Regression tests for Issue #1334 — harden `calculate_seller_net_amount`
//! failure and authorization paths.
//!
//! `calculate_seller_net_amount` is a pure fee-quoting view helper: it never
//! writes storage and never moves tokens. The hardening contract it must
//! uphold is therefore:
//!
//! * an unauthorized caller (no auths supplied) may invoke it, but can never
//!   change storage or balances through it;
//! * it is rejected with `Error::ContractPaused` while the platform is paused,
//!   and the pause/unpause path itself is unaffected;
//! * its arithmetic is fully checked, so an input it cannot quote (a negative
//!   amount, or fee math that overflows) fails with `Error::InvalidFee` instead
//!   of wrapping around;
//! * every rejection is raised with `panic_with_error` before any storage write
//!   or token transfer, so counters, escrows and balances are unchanged.

use crate::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{testutils::Address as _, token, Address, Env};

/// Deploys the contract, funds the buyer with a real token and returns
/// everything the tests need to observe balances and fee state.
fn setup() -> (
    Env,
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    // Deploy a real SAC token so the "balances unchanged" assertions are not
    // vacuous, and fund the buyer.
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_id.address();
    let token_asset = token::StellarAssetClient::new(&env, &token_addr);
    token_asset.mint(&buyer, &10_000_000);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500, // 5% platform fee
        &Some(onboarding_contract),
    );

    (env, client, buyer, seller, token_addr, platform_wallet)
}

/// `calculate_seller_net_amount` returns a plain `i128`, so the generated
/// `try_*` client reports a rejected call as `Err(Ok(<soroban Error>))` holding
/// the wrapped contract error code. Assert the exact variant.
fn assert_contract_error(
    result: Result<
        Result<i128, soroban_sdk::Error>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
    expected: Error,
) {
    let expected = soroban_sdk::Error::from_contract_error(expected as u32);
    assert!(matches!(result, Err(Ok(err)) if err == expected));
}

/// An unauthorized caller (no auth provided) must be able to read the quote,
/// but must not be able to mutate any contract or token state through it.
#[test]
fn unauthorized_caller_cannot_change_storage_through_calculate_seller_net_amount() {
    let (env, client, buyer, seller, token_addr, platform_wallet) = setup();

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &order_id,
        &Some(604800),
    );

    let token = token::Client::new(&env, &token_addr);
    let buyer_before = token.balance(&buyer);
    let seller_before = token.balance(&seller);
    let platform_before = token.balance(&platform_wallet);
    let fees_before = client.get_total_fees_collected();
    let escrow_before = client.get_escrow(&order_id);

    // Drop every mocked authorization: nothing the caller does is authorized.
    env.set_auths(&[]);

    // The quote is still served — the function holds no privileged state and
    // performs no authenticated action — but it must be side-effect free.
    assert_eq!(client.calculate_seller_net_amount(&1_000_000), 950_000);

    assert_eq!(token.balance(&buyer), buyer_before);
    assert_eq!(token.balance(&seller), seller_before);
    assert_eq!(token.balance(&platform_wallet), platform_before);
    assert_eq!(client.get_total_fees_collected(), fees_before);
    assert_eq!(client.get_escrow(&order_id), escrow_before);
}

/// While the platform is paused the quote must be rejected with the exact
/// `ContractPaused` variant, and the rejection must leave all state untouched.
/// The pause/unpause path itself must keep working (otherwise the contract
/// could never be resumed).
#[test]
fn calculate_seller_net_amount_is_rejected_while_paused() {
    let (env, client, buyer, seller, token_addr, platform_wallet) = setup();

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &order_id,
        &Some(604800),
    );

    // Control value captured while the platform is live.
    assert_eq!(client.calculate_seller_net_amount(&1_000_000), 950_000);

    let token = token::Client::new(&env, &token_addr);
    let buyer_before = token.balance(&buyer);
    let seller_before = token.balance(&seller);
    let platform_before = token.balance(&platform_wallet);
    let fees_before = client.get_total_fees_collected();
    let fee_bps_before = client.get_platform_config().platform_fee_bps;
    let escrow_before = client.get_escrow(&order_id);

    client.set_paused(&true);
    assert!(client.is_paused());

    assert_contract_error(
        client.try_calculate_seller_net_amount(&1_000_000),
        Error::ContractPaused,
    );

    // Nothing moved and nothing was recorded by the rejected call.
    assert_eq!(token.balance(&buyer), buyer_before);
    assert_eq!(token.balance(&seller), seller_before);
    assert_eq!(token.balance(&platform_wallet), platform_before);
    assert_eq!(client.get_total_fees_collected(), fees_before);
    assert_eq!(
        client.get_platform_config().platform_fee_bps,
        fee_bps_before
    );
    assert_eq!(client.get_escrow(&order_id), escrow_before);

    // The pause/unpause path is never gated on itself, and once the platform is
    // live again the quote returns exactly the pre-rejection value.
    client.set_paused(&false);
    assert!(!client.is_paused());
    assert_eq!(client.calculate_seller_net_amount(&1_000_000), 950_000);
}

/// The main rejected input is an amount that cannot be quoted: a negative
/// amount, or one whose fee math overflows. Both must fail with the exact
/// `InvalidFee` variant rather than wrapping or underflowing, and both
/// rejections must be side-effect free.
#[test]
fn calculate_seller_net_amount_rejects_unquotable_amount_with_invalid_fee() {
    let (env, client, buyer, seller, token_addr, platform_wallet) = setup();

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &order_id,
        &Some(604800),
    );

    let token = token::Client::new(&env, &token_addr);
    let buyer_before = token.balance(&buyer);
    let platform_before = token.balance(&platform_wallet);
    let fees_before = client.get_total_fees_collected();
    let escrow_before = client.get_escrow(&order_id);

    // Negative amount: rejected by the fee math, never by `amount - fee`
    // underflowing.
    assert_contract_error(
        client.try_calculate_seller_net_amount(&-1),
        Error::InvalidFee,
    );

    // Overflowing fee math (`i128::MAX * fee_bps`) must also surface as
    // `InvalidFee`, not as a host-level arithmetic panic or a wrapped value.
    assert_contract_error(
        client.try_calculate_seller_net_amount(&i128::MAX),
        Error::InvalidFee,
    );

    // Both rejections are side-effect free.
    assert_eq!(token.balance(&buyer), buyer_before);
    assert_eq!(token.balance(&platform_wallet), platform_before);
    assert_eq!(client.get_total_fees_collected(), fees_before);
    assert_eq!(client.get_escrow(&order_id), escrow_before);

    // Valid quotes are unaffected and still exact.
    assert_eq!(client.calculate_seller_net_amount(&1_000_000), 950_000);
    assert_eq!(client.calculate_seller_net_amount(&500), 475);
    assert_eq!(client.calculate_seller_net_amount(&0), 0);
}
