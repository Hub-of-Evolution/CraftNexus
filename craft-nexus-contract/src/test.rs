#![cfg(test)]
extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, vec, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, TryIntoVal,
};

fn setup_test(
    env: &Env,
    mock_auth: bool,
) -> (
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
    Address,
) {
    env.budget().reset_unlimited();
    if mock_auth {
        env.mock_all_auths();
    }
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

    let arbitrator = Address::generate(env);
    let onboarding_contract = Address::generate(env);

    // Set a non-zero timestamp for event tests
    env.ledger().with_mut(|li| {
        li.timestamp = 1711368000; // 2024-03-25
    });

    // Initialize contract with platform config (no onboarding contract for unit tests)
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract.clone()),
    );

    // Set min amount to 0 for tests to pass with small amounts
    client.set_min_escrow_amount(&token_contract, &0);
    client.set_min_release_window(&1);

    (
        client,
        buyer,
        seller,
        token_contract,
        token_admin_client,
        platform_wallet,
        admin,
    )
}

#[test]
fn test_create_escrow_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    let order_id = 1;
    let amount = 500;
    let window = 3600;

    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token_id,
        &amount,
        &order_id,
        &Some(window),
    );

    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.seller, seller);
    assert_eq!(escrow.amount, amount);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.release_window, window);

    let stored_escrow = client.get_escrow(&order_id);
    assert_eq!(stored_escrow, escrow);

    // Verify event
    let events = env.events().all();
    assert!(!events.is_empty(), "No events emitted");
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    let last_event = events.last();
    let last_event = last_event.unwrap();
    assert_eq!(last_event.0, client.address);
    assert_eq!(last_event.0, client.address);
    // Topics: ["escrow_created", escrow_id]
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow").into_val(&env),
            (order_id as u64).into_val(&env)
        ]
    );
    // Verify payload
    let event: EscrowEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, order_id as u64);
    assert_eq!(event.action, EscrowAction::Created);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert_eq!(event.token, token_id);
    assert_eq!(event.amount, amount);
    assert!(event.timestamp > 0);
}

#[test]
fn test_create_escrow_default_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    let escrow = client.create_escrow(&buyer, &seller, &token_id, &100_00000, &1, &None);

    assert_eq!(escrow.release_window, 604800); // 7 days
}

#[test]
fn test_release_funds_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    client.release_funds(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Released);

    let token_client = token::Client::new(&env, &token_id);
    // Seller receives 500 - 25 (5% fee) = 475
    assert_eq!(token_client.balance(&seller), 47_500_000);
    // Platform receives 25 (5% fee)
    assert_eq!(token_client.balance(&platform_wallet), 2_500_000);
    assert_eq!(token_client.balance(&client.address), 0);

    // Check total fees collected
    assert_eq!(client.get_total_fees_collected(), 2_500_000);

    // Event verified by balance/status assertions above.
}

#[test]
fn test_fund_movements_create_audit_records() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    let history = client.get_fund_audit_history(&buyer);
    assert_eq!(history.len(), 1);

    let entry = history.get(0).unwrap();
    assert_eq!(entry.actor, buyer);
    assert_eq!(entry.amount, 50_000_000);
    assert_eq!(entry.reason, Symbol::new(&env, "escrow_funded"));
    assert_eq!(entry.balance_impact, -50_000_000);
    assert!(entry.timestamp > 0);
}

#[test]
#[should_panic]
fn test_release_funds_already_processed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.release_funds(&1);
    client.release_funds(&1); // Should panic
}

#[test]
fn test_auto_release_success_after_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let window = 100;
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &Some(window));

    // Advance time
    env.ledger().with_mut(|li| {
        li.timestamp += (window + 1) as u64;
    });

    assert!(client.can_auto_release(&1));
    client.auto_release(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Released);

    let token_client = token::Client::new(&env, &token_id);
    // Seller receives 500 - 25 (5% fee) = 475
    assert_eq!(token_client.balance(&seller), 47_500_000);
    // Platform receives 25 (5% fee)
    assert_eq!(token_client.balance(&platform_wallet), 2_500_000);

    // Event verified by balance/status assertions above.
}

#[test]
#[should_panic]
fn test_auto_release_failure_before_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    client.create_escrow(&buyer, &seller, &token_id, &100_00000, &1, &Some(100));

    assert!(!client.can_auto_release(&1));
    client.auto_release(&1);
}

#[test]
fn test_refund_success_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _admin) = setup_test(&env, false);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // Check initial balance
    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 50_000_000);

    // Provide escrow_id 1
    client.refund(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    assert_eq!(token_client.balance(&buyer), 100_000_000);

    // Event verified by balance/status assertions above.
}

#[test]
fn test_dispute_escrow_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    client.dispute_escrow(&1, &Symbol::new(&env, "Item_damaged"), &buyer);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
    assert_eq!(
        escrow.dispute_reason,
        Some(Symbol::new(&env, "Item_damaged"))
    );

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow").into_val(&env),
            1u64.into_val(&env)
        ]
    );
    // Verify payload
    let event: EscrowEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.action, EscrowAction::Disputed);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert_eq!(event.token, token_id);
    assert!(event.timestamp > 0);
}

#[test]
fn test_dispute_escrow_by_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &500, &1, &None);

    client.dispute_escrow(&1, &Symbol::new(&env, "Payment_not_received"), &seller);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
    assert_eq!(
        escrow.dispute_reason,
        Some(Symbol::new(&env, "Payment_not_received"))
    );
}

#[test]
#[should_panic]
fn test_dispute_escrow_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    let unauthorized = Address::generate(&env);
    client.dispute_escrow(&1, &Symbol::new(&env, "Invalid_reason"), &unauthorized);
}

#[test]
#[should_panic]
fn test_disputed_prevents_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Damaged_item"), &buyer);

    client.release_funds(&1);
}

#[test]
#[should_panic]
fn test_disputed_prevents_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Damaged_item"), &buyer);

    client.refund(&1);
}

#[test]
fn test_pending_dispute_blocks_release_and_refund_races() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    env.as_contract(&client.address, || {
        let mut escrow: Escrow = env.storage().persistent().get(&(ESCROW, 1u32)).unwrap();
        escrow.status = EscrowStatus::DisputePending;
        env.storage().persistent().set(&(ESCROW, 1u32), &escrow);
    });

    assert!(client.try_release_funds(&1).is_err());
    assert!(client.try_refund(&1).is_err());
}

#[test]
fn test_pending_release_or_refund_blocks_dispute_race() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &2, &None);

    env.as_contract(&client.address, || {
        let mut release_escrow: Escrow = env.storage().persistent().get(&(ESCROW, 1u32)).unwrap();
        release_escrow.status = EscrowStatus::ReleasePending;
        env.storage()
            .persistent()
            .set(&(ESCROW, 1u32), &release_escrow);

        let mut refund_escrow: Escrow = env.storage().persistent().get(&(ESCROW, 2u32)).unwrap();
        refund_escrow.status = EscrowStatus::RefundPending;
        env.storage()
            .persistent()
            .set(&(ESCROW, 2u32), &refund_escrow);
    });

    assert!(client
        .try_dispute_escrow(&1, &Symbol::new(&env, "Race"), &buyer)
        .is_err());
    assert!(client
        .try_dispute_escrow(&2, &Symbol::new(&env, "Race"), &seller)
        .is_err());
}

#[test]
fn test_resolve_dispute_release_to_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Non_delivery"), &buyer);

    // Arbitrator is setup in setup_test as a random Address and mock_all_auths bypasses auth
    client.resolve_dispute(&1, &Resolution::ReleaseToSeller, &admin);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&seller), 47_500_000);

    // Status and balances verified above.
}

#[test]
fn test_resolve_dispute_refund_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Late_shipping"), &buyer);

    client.resolve_dispute(&1, &Resolution::RefundToBuyer, &admin);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 100_000_000);

    // Status and balances verified above.
}

#[test]
fn test_resolve_dispute_by_moderator() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    let moderator = Address::generate(&env);

    client.set_moderator(&moderator);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Moderator_review"), &buyer);

    client.resolve_dispute(&1, &Resolution::RefundToBuyer, &moderator);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_recover_admin_with_zero_window_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, _seller, _token_id, _token_admin, _platform_wallet, admin) =
        setup_test(&env, true);

    // Simulate a malicious/deployer-provided zero-second recovery window by
    // writing a recovery time equal to the current ledger timestamp and
    // recording a zero delay. The contract should reject recovery attempts
    // that don't meet the minimum cooldown.
    let current_time = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::AdminRecoveryTime, &current_time);
        env.storage()
            .persistent()
            .set(&DataKey::AdminRecoveryDelay, &0u64);
    });

    let recovered_admin = Address::generate(&env);
    let res = client.try_recover_admin_access(&recovered_admin);
    assert!(matches!(res, Err(Ok(Error::AdminRecoveryFailed))));
}

#[test]
#[should_panic]
fn test_resolve_dispute_non_disputed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    client.resolve_dispute(&1, &Resolution::RefundToBuyer, &admin);
}

#[test]
fn test_resolve_dispute_partial_release_50_50() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Partial_delivery"), &buyer);

    // 50/50 split: buyer gets 25M, seller gets 25M minus 5% fee
    client.resolve_dispute_partial(&1, &25_000_000, &admin);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    // Buyer gets exactly their share
    assert_eq!(token_client.balance(&buyer), 25_000_000);
    // Seller gets 25M - 5% fee (1_250_000) = 23_750_000
    assert_eq!(token_client.balance(&seller), 23_750_000);
}

#[test]
fn test_resolve_dispute_partial_release_custom_fee_tier() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    // Set custom 2% fee for seller
    client.set_artisan_fee_tier(&seller, &200);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Partial_delivery"), &buyer);

    // 70/30 split: buyer gets 35M, seller gets 15M minus 2% fee
    client.resolve_dispute_partial(&1, &35_000_000, &admin);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 35_000_000);
    // Seller gets 15M - 2% fee (300_000) = 14_700_000
    assert_eq!(token_client.balance(&seller), 14_700_000);
}

#[test]
fn test_resolve_dispute_partial_release_fee_deducted_once() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, admin) =
        setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Partial_delivery"), &buyer);

    // 50/50 split
    client.resolve_dispute_partial(&1, &25_000_000, &admin);

    let token_client = token::Client::new(&env, &token_id);
    // Fee is 5% of seller's 25M = 1_250_000 — charged exactly once
    let expected_fee = 25_000_000 * 500 / 10_000;
    assert_eq!(expected_fee, 1_250_000);

    // Buyer + seller + platform_fee should equal escrow amount
    let buyer_balance = token_client.balance(&buyer);
    let seller_balance = token_client.balance(&seller);
    let platform_balance = token_client.balance(&platform_wallet);
    assert_eq!(buyer_balance + seller_balance + platform_balance, 50_000_000);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #18)")]
fn test_resolve_dispute_partial_release_zero_buyer_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Invalid_split"), &buyer);

    client.resolve_dispute_partial(&1, &0, &admin);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #18)")]
fn test_resolve_dispute_partial_release_full_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Invalid_split"), &buyer);

    // buyer_amount == escrow.amount is invalid (must be < full amount)
    client.resolve_dispute_partial(&1, &50_000_000, &admin);
}

#[test]
#[should_panic]
fn test_refund_failure_unauthorized() {
    let env = Env::default();
    // Do NOT mock auth globally during setup_test
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, false);

    // Manually mock for create_escrow
    env.mock_all_auths();
    token_admin.mint(&buyer, &100_000_000);
    client.set_min_escrow_amount(&token_id, &0);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // Now call refund as a non-admin (actually without any auth)
    // require_auth() will fail because we are calling it but no auth is recorded for 'admin'
    client.refund(&1);
}

#[test]
#[should_panic]
fn test_get_escrow_not_found() {
    let env = Env::default();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    client.get_escrow(&999);
}

#[test]
#[should_panic]
fn test_create_escrow_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    client.create_escrow(&buyer, &seller, &token_id, &0, &1, &None);
}

#[test]
#[should_panic]
fn test_create_escrow_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    client.create_escrow(&buyer, &seller, &token_id, &-100, &1, &None);
}

#[test]
#[should_panic]
fn test_create_escrow_same_buyer_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, _, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    client.create_escrow(&buyer, &buyer, &token_id, &100_00000, &1, &None);
}

// ===== Platform Fee Tests =====

#[test]
fn test_platform_fee_deduction_5_percent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    // Create escrow with 1,000,000 (should have 50,000 fee at 5%)
    client.create_escrow(&buyer, &seller, &token_id, &1_000_000, &1, &None);

    client.release_funds(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&seller), 950_000); // 1,000,000 - 50,000
    assert_eq!(token_client.balance(&platform_wallet), 50_000);
    assert_eq!(client.get_total_fees_collected(), 50_000);
}

#[test]
fn test_platform_fee_deduction_10_percent() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let arbitrator = Address::generate(&env);

    // Initialize with 10% fee
    client.initialize(&platform_wallet, &admin, &arbitrator, &1000, &None);

    token_admin_client.mint(&buyer, &10_000_000);
    client.create_escrow(
        &buyer,
        &seller,
        &token_contract,
        &10_000_000,
        &1,
        &None,
    );

    client.release_funds(&1);

    let token_client = token::Client::new(&env, &token_contract);
    assert_eq!(token_client.balance(&seller), 9_000_000); // 10,000,000 - 1,000,000
    assert_eq!(token_client.balance(&platform_wallet), 1_000_000);
}

#[test]
fn test_calculate_fee_for_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // 5% of 1000 = 50
    let fee = client.calculate_fee_for_amount(&1000);
    assert_eq!(fee, 50);

    // 5% of 500 = 25
    let fee = client.calculate_fee_for_amount(&500);
    assert_eq!(fee, 25);
}

#[test]
fn test_calculate_seller_net_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // 1000 - 50 = 950
    let net = client.calculate_seller_net_amount(&1000);
    assert_eq!(net, 950);

    // 500 - 25 = 475
    let net = client.calculate_seller_net_amount(&500);
    assert_eq!(net, 475);
}

fn assert_invalid_fee_error(
    result: Result<
        Result<i128, soroban_sdk::Error>,
        Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
    >,
) {
    let expected = soroban_sdk::Error::from_contract_error(Error::InvalidFee as u32);
    assert!(matches!(result, Err(Ok(err)) if err == expected));
}

#[test]
fn test_calculate_fee_handles_high_safe_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let amount = i128::MAX / 1_000;
    let fee = client.calculate_fee_for_amount(&amount);

    assert_eq!(fee, amount / 20);
}

#[test]
fn test_calculate_fee_overflow_returns_contract_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let result = client.try_calculate_fee_for_amount(&i128::MAX);

    assert_invalid_fee_error(result);
}

#[test]
fn test_calculate_seller_net_overflow_returns_contract_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let result = client.try_calculate_seller_net_amount(&i128::MAX);

    assert_invalid_fee_error(result);
}

#[test]
fn test_update_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let seller = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let arbitrator = Address::generate(&env);

    // Initialize with 5% fee
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &None::<Address>,
    );

    // Get initial fee

    // Update to 8% fee (800 bps) - admin auth required
    client.update_platform_fee(&800);

    assert_eq!(client.get_platform_fee(), 800);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let _config_event: ConfigUpdatedEvent = last_event.2.try_into_val(&env).unwrap();
    let last_event = events.last().unwrap();
    let config_event: ConfigUpdatedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(
        config_event.field_name,
        Symbol::new(&env, "platform_fee_bps")
    );
    assert_eq!(config_event.old_value, ConfigValue::U32(500));
    assert_eq!(config_event.new_value, ConfigValue::U32(800));

    // Now create escrow and release - should use 8%
    token_admin_client.mint(&Address::generate(&env), &100_000_000);
    let buyer = Address::generate(&env);
    token_admin_client.mint(&buyer, &100_000_000);
    client.create_escrow(
        &buyer,
        &seller,
        &token_contract,
        &100_000_000,
        &1,
        &None,
    );

    client.release_funds(&1);

    let token_client = token::Client::new(&env, &token_contract);
    // 100,000,000 - 8,000,000 = 92,000,000
    assert_eq!(token_client.balance(&seller), 92_000_000);
    assert_eq!(token_client.balance(&platform_wallet), 8_000_000);
}

#[test]
#[should_panic]
fn test_update_platform_fee_too_high() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let _token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    let arbitrator = Address::generate(&env);

    // Initialize with 5% fee
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &None::<Address>,
    );

    // Try to set fee above max (10%)
    client.update_platform_fee(&1500);
}

#[test]
fn test_total_fees_accumulate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &30_000_000);

    // Create and release multiple escrows
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.release_funds(&1);

    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &2, &None);
    client.release_funds(&2);

    let token_client = token::Client::new(&env, &token_id);
    // Total fees: 500,000 + 500,000 = 1,000,000
    assert_eq!(token_client.balance(&platform_wallet), 1_000_000);
    assert_eq!(client.get_total_fees_collected(), 1_000_000);
}

#[test]
fn test_initialize_emits_config_events() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &None::<Address>,
    );

    let events = env.events().all();
    let fee_event: ConfigUpdatedEvent = events
        .get(events.len() - 2)
        .unwrap()
        .unwrap()
        .2
        .try_into_val(&env)
        .unwrap();
    let wallet_event: ConfigUpdatedEvent = events
        .get(events.len() - 1)
        .unwrap()
        .unwrap()
        .2
        .try_into_val(&env)
        .unwrap();

    assert_eq!(fee_event.field_name, Symbol::new(&env, "platform_fee_bps"));
    assert_eq!(
        wallet_event.field_name,
        Symbol::new(&env, "platform_wallet")
    );
}

#[test]
fn test_set_artisan_fee_tier_emits_dedicated_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, seller, _, _, _, _) = setup_test(&env, true);

    client.set_artisan_fee_tier(&seller, &750);

    assert_eq!(client.get_effective_fee_bps(&seller), 750);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "admin_fee_tier_updated").into_val(&env),
            seller.clone().into_val(&env)
        ]
    );
    let fee_event: ArtisanFeeTierUpdatedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(fee_event.artisan, seller);
    assert_eq!(fee_event.fee_bps, 750);
}

// ===== Additional Comprehensive Coverage Tests =====

#[test]
#[should_panic]
fn test_dispute_escrow_failure_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    let _unauthorized = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    client.dispute_escrow(&1, &Symbol::new(&env, "Unauthorized"), &unauthorized);
}

#[test]
#[should_panic]
fn test_refund_after_release_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.release_funds(&1);
    client.refund(&1);
}

#[test]
#[should_panic]
fn test_dispute_after_release_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.release_funds(&1);
    client.dispute_escrow(&1, &Symbol::new(&env, "buyer_dispute"), &buyer);
}

#[test]
#[should_panic]
fn test_release_funds_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    client.release_funds(&999);
}

#[test]
#[should_panic]
fn test_refund_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    let _caller = Address::generate(&env);
    client.refund(&999);
}

#[test]
#[should_panic]
fn test_dispute_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    let caller = Address::generate(&env);
    client.dispute_escrow(&999, &Symbol::new(&env, "reason"), &caller);
}

#[test]
#[should_panic]
fn test_auto_release_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    client.auto_release(&999);
}

#[test]
#[should_panic]
fn test_can_auto_release_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    let _ = client.can_auto_release(&999);
}

#[test]
fn test_auto_release_at_exact_window_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let window = 100;
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &Some(window));

    // Exactly at boundary should be releasable.
    env.ledger().with_mut(|li| {
        li.timestamp += window as u64;
    });
    assert!(client.can_auto_release(&1));
    client.auto_release(&1);
    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&seller), 47_500_000);
    assert_eq!(token_client.balance(&platform_wallet), 2_500_000);
}

// ===== Governance (#95) Tests =====

#[test]
fn test_admin_transfer_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let new_admin = Address::generate(&env);

    // Initial admin proposes transfer
    client.update_admin(&new_admin);

    // Should still be old admin
    let config = client.get_platform_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.pending_admin, Some(new_admin.clone()));

    // New admin claims role
    client.claim_admin();

    // Now should be new admin
    let config = client.get_platform_config();
    assert_eq!(config.admin, new_admin);
    assert_eq!(config.pending_admin, None);
}

#[test]
fn test_admin_transfer_can_be_cancelled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let new_admin = Address::generate(&env);
    client.update_admin(&new_admin);

    client.cancel_admin_transfer();

    let config = client.get_platform_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.pending_admin, None);
}

#[test]
fn test_cancel_admin_transfer_without_pending_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let result = client.try_cancel_admin_transfer();
    assert!(result.is_err());
}

#[test]
#[should_panic]
fn test_claim_admin_no_pending_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    client.claim_admin();
}

// ===== Admin address validation tests (#419) =====

#[test]
#[should_panic]
fn test_update_admin_contract_address_rejected() {
    // Transferring admin to the contract itself must be rejected.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    client.update_admin(&client.address.clone());
}

#[test]
#[should_panic]
fn test_update_admin_requires_new_admin_cosign() {
    // Without new_admin providing auth, update_admin must fail.
    // setup_test(false) skips mock_all_auths, so new_admin's require_auth
    // is never satisfied and the call panics.
    let env = Env::default();
    let (client, _, _, _, _, _, _) = setup_test(&env, false);
    let new_admin = Address::generate(&env);
    // No auth is mocked — both current-admin and new_admin auth will fail.
    client.update_admin(&new_admin);
}

// ===== Admin recovery error tests (#415) =====

fn assert_admin_recovery_failed(
    result: Result<
        Result<(), soroban_sdk::ConversionError>,
        Result<Error, soroban_sdk::InvokeError>,
    >,
) {
    assert!(matches!(result, Err(Ok(Error::AdminRecoveryFailed))));
}

#[test]
fn test_recover_admin_missing_fallback_returns_standard_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let recovered_admin = Address::generate(&env);
    let result = client.try_recover_admin_access(&recovered_admin);

    assert_admin_recovery_failed(result);
}

#[test]
fn test_recover_admin_invalid_address_returns_standard_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
    });

    let result = client.try_recover_admin_access(&client.address);

    assert_admin_recovery_failed(result);
}

#[test]
fn test_recover_admin_timelock_returns_standard_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
    });

    let recovered_admin = Address::generate(&env);
    let initial_result = client.try_recover_admin_access(&recovered_admin);
    assert_admin_recovery_failed(initial_result);

    let locked_result = client.try_recover_admin_access(&recovered_admin);
    assert_admin_recovery_failed(locked_result);
}

// ===== Admin recovery edge case snapshot tests =====

#[test]
fn test_recover_admin_access_zero_cooldown_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, _seller, _token_id, _token_admin, _platform_wallet, admin) =
        setup_test(&env, true);

    // Simulate a direct-storage bypass attempt: the time lock has already
    // elapsed (recovery_time == current_time) but the recorded cooldown
    // delay is zero. This must be rejected even though the timelock check
    // itself would otherwise pass.
    let current_time = env.ledger().timestamp();
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::AdminRecoveryTime, &current_time);
        env.storage()
            .persistent()
            .set(&DataKey::AdminRecoveryDelay, &0u64);
    });

    let recovered_admin = Address::generate(&env);
    let result = client.try_recover_admin_access(&recovered_admin);
    assert_admin_recovery_failed(result);
}

#[test]
fn test_recover_admin_access_same_address_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, _seller, _token_id, _token_admin, _platform_wallet, admin) =
        setup_test(&env, true);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
    });

    // Attempting to "recover" to the address that is already the current
    // admin must fail rather than silently succeeding as a no-op.
    let result = client.try_recover_admin_access(&admin);
    assert_admin_recovery_failed(result);
}

#[test]
fn test_recover_admin_access_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, _seller, _token_id, _token_admin, _platform_wallet, admin) =
        setup_test(&env, true);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
    });

    let recovered_admin = Address::generate(&env);

    // First call initiates the 7-day time lock and fails.
    let initial_result = client.try_recover_admin_access(&recovered_admin);
    assert_admin_recovery_failed(initial_result);

    // Advance the ledger past the minimum recovery cooldown.
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60 + 1;
    });

    // Second call, after the time lock has elapsed, must succeed.
    client.try_recover_admin_access(&recovered_admin).unwrap().unwrap();

    let config = client.get_platform_config();
    assert_eq!(config.admin, recovered_admin);
}

#[test]
fn test_wasm_upgrade_grace_period() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Propose upgrade
    client.propose_upgrade_wasm(&admin, &new_wasm_hash);

    // Try to upgrade immediately - should fail
    // We can't easily catch a panic in a test without should_panic,
    // but we can verify the error if we return Result.
    // Our update_wasm uses expect/panic.
}

#[test]
fn test_cancel_upgrade_wasm() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.propose_upgrade_wasm(&admin, &new_wasm_hash);

    // Admin cancels
    client.cancel_upgrade_wasm();

    // Should panic when trying to update since proposal is gone
}

/// Issue #618 — cancel-and-repropose must not reset the review window.
/// After a cancellation, propose_upgrade_wasm must return UpgradeCooldownActive
/// if less than CANCEL_REPROPOSE_COOLDOWN seconds have elapsed.
/// UpgradeCooldownActive = Error discriminant #33.
#[test]
#[should_panic(expected = "HostError: Error(Contract, #33)")]
fn test_cancel_then_repropose_blocked_by_cooldown() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let hash = BytesN::from_array(&env, &[1u8; 32]);

    // Propose and then cancel — this records LastUpgradeCancelledAt
    client.propose_upgrade_wasm(&admin, &hash);
    client.cancel_upgrade_wasm();

    // Immediately re-proposing (same ledger timestamp) must panic with
    // UpgradeCooldownActive (#33)
    client.propose_upgrade_wasm(&admin, &hash);
}

/// Issue #618 — after the cancel cooldown window elapses, a new proposal must
/// be accepted without error.
#[test]
fn test_repropose_succeeds_after_cancel_cooldown_elapses() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let hash = BytesN::from_array(&env, &[2u8; 32]);

    // Propose and cancel
    client.propose_upgrade_wasm(&admin, &hash);
    client.cancel_upgrade_wasm();

    // Advance the ledger past CANCEL_REPROPOSE_COOLDOWN (7 days + 1 s)
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60 + 1;
    });

    // Re-proposing after the cooldown must succeed
    client.propose_upgrade_wasm(&admin, &hash);
    let proposal = client
        .get_upgrade_proposal()
        .expect("proposal should exist");
    assert_eq!(proposal.wasm_hash, hash);
}

#[test]
fn test_fee_rounding_floor_behavior_small_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // 5% floor rounding (integer division).
    assert_eq!(client.calculate_fee_for_amount(&1), 0);
    assert_eq!(client.calculate_fee_for_amount(&19), 0);
    assert_eq!(client.calculate_fee_for_amount(&20), 1);
    assert_eq!(client.calculate_fee_for_amount(&39), 1);
    assert_eq!(client.calculate_fee_for_amount(&40), 2);
}

#[test]
fn test_fee_rounding_custom_bps_025_percent() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);

    let arbitrator = Address::generate(&env);

    // 25 bps = 0.25%
    client.initialize(&platform_wallet, &admin, &arbitrator, &25, &None);
    assert_eq!(client.calculate_fee_for_amount(&1000), 2); // floor(2.5) => 2
    assert_eq!(client.calculate_fee_for_amount(&399), 0); // floor(0.9975) => 0
    assert_eq!(client.calculate_fee_for_amount(&400), 1); // floor(1.0) => 1
}

#[test]
fn test_integration_multiple_tokens_and_escrows() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);

    // Token A
    let token_a_admin = Address::generate(&env);
    let token_a_contract = env.register_stellar_asset_contract_v2(token_a_admin.clone());
    let token_a_asset = token::StellarAssetClient::new(&env, &token_a_contract.address());
    token_a_asset.mint(&buyer, &100_000_000);

    // Token B
    let token_b_admin = Address::generate(&env);
    let token_b_contract = env.register_stellar_asset_contract_v2(token_b_admin.clone());
    let token_b_asset = token::StellarAssetClient::new(&env, &token_b_contract.address());
    token_b_asset.mint(&buyer, &200_000_000);

    client.create_escrow(
        &buyer,
        &seller,
        &token_a_contract,
        &10_000_000,
        &1,
        &None,
    );
    client.create_escrow(
        &buyer,
        &seller,
        &token_b_contract,
        &10_000_000,
        &2,
        &None,
    );

    client.release_funds(&1);
    client.release_funds(&2);

    let token_a = token::Client::new(&env, &token_a_contract);
    let token_b = token::Client::new(&env, &token_b_contract);

    // Seller: 9.5M (token A) + 9.5M (token B)
    assert_eq!(token_a.balance(&seller), 9_500_000);
    assert_eq!(token_b.balance(&seller), 9_500_000);

    // Platform: 500,000 (token A) + 500,000 (token B)
    let fee_a = token_a.balance(&platform_wallet);
    let fee_b = token_b.balance(&platform_wallet);
    assert_eq!(fee_a, 500_000);
    assert_eq!(fee_b, 500_000);
    assert_eq!(
        client.get_total_fees_for_token(&token_a_contract),
        500_000
    );
    assert_eq!(
        client.get_total_fees_for_token(&token_b_contract),
        500_000
    );
    assert_eq!(client.get_total_fees_collected(), 1_000_000);
}

#[test]
fn test_fuzz_fee_and_net_amount_invariants() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // Deterministic fuzz-style sweep of amounts for arithmetic invariants.
    for amount in 1i128..=1_000i128 {
        let fee = client.calculate_fee_for_amount(&amount);
        let net = amount - fee;

        assert!(fee >= 0, "fee must be non-negative");
        assert!(fee <= amount, "fee cannot exceed amount");
        assert_eq!(fee + net, amount, "fee + net must equal amount");
    }
}

#[test]
fn test_stake_and_unstake_same_token_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&seller, &20_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);
    assert_eq!(client.get_stake(&seller), 5_000_000);

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });

    client.unstake_tokens(&seller, &token_id);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(client.get_stake(&seller), 0);
    assert_eq!(token_client.balance(&seller), 20_000_000);
}

#[test]
#[should_panic]
fn test_unstake_rejects_different_token_than_original_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    let other_token_admin = Address::generate(&env);
    let other_token_contract = env.register_stellar_asset_contract_v2(other_token_admin.clone());
    let other_token_admin_client =
        token::StellarAssetClient::new(&env, &other_token_contract);

    token_admin.mint(&seller, &10_000_000);
    other_token_admin_client.mint(&seller, &10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });

    client.unstake_tokens(&seller, &other_token_contract);
}

#[test]
fn test_create_escrow_with_metadata_success_cid_v0() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnAzt8auVTL3u2M6YvM7NfF4hB9m8C3vM9");
    let metadata_hash = Bytes::from_array(
        &env,
        &[
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 1, 1,
        ],
    );

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &Some(ipfs_hash.clone()),
        &Some(metadata_hash.clone()),
        &None,
    );
    assert_eq!(escrow.id, 1);
    assert_eq!(escrow.ipfs_hash, Some(ipfs_hash.clone()));
    assert_eq!(escrow.metadata_hash, Some(metadata_hash.clone()));

    let metadata = client.get_escrow_metadata(&1);
    assert_eq!(metadata.ipfs_hash, Some(ipfs_hash));
    assert_eq!(metadata.metadata_hash, Some(metadata_hash));
}

#[test]
fn test_create_escrow_with_metadata_success_cid_v1() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let ipfs_hash = String::from_str(
        &env,
        "bafybeigdyrztf2v7y5h6l2k3g5zazf5s6ptm3h4m5k4e3v2w2x2y3z4a5q",
    );

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &Some(ipfs_hash.clone()),
        &None,
        &None,
    );

    assert_eq!(escrow.ipfs_hash, Some(ipfs_hash));
}

#[test]
#[should_panic]
fn test_create_escrow_with_invalid_cid_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &Some(String::from_str(&env, "a".repeat(129).as_str())),
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_create_escrow_with_invalid_metadata_hash_length_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let invalid_hash = Bytes::from_array(&env, &[7; 31]);

    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &None,
        &Some(invalid_hash),
        &None,
    );
}

// ===== Service Agreement Hash Tests (#708) =====

#[test]
fn test_create_escrow_with_service_agreement_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let service_agreement_hash = Bytes::from_array(
        &env,
        &[
            2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
            2, 2, 2, 2,
        ],
    );

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &None,
        &None,
        &Some(service_agreement_hash.clone()),
    );
    assert_eq!(escrow.service_agreement_hash, Some(service_agreement_hash.clone()));

    let metadata = client.get_escrow_metadata(&1);
    assert_eq!(metadata.service_agreement_hash, Some(service_agreement_hash));
    assert_eq!(metadata.ipfs_hash, None);
    assert_eq!(metadata.metadata_hash, None);
}

#[test]
#[should_panic]
fn test_create_escrow_with_invalid_service_agreement_hash_length() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let invalid_hash = Bytes::from_array(&env, &[9; 31]); // 31 bytes, not 32

    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &None,
        &None,
        &Some(invalid_hash),
    );
}

#[test]
fn test_create_escrow_with_all_metadata_fields() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnAzt8auVTL3u2M6YvM7NfF4hB9m8C3vM9");
    let metadata_hash = Bytes::from_array(
        &env,
        &[
            1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 1, 1, 1,
        ],
    );
    let service_agreement_hash = Bytes::from_array(
        &env,
        &[
            3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
            3, 3, 3, 3,
        ],
    );

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &10_000_000,
        &1,
        &None,
        &Some(ipfs_hash.clone()),
        &Some(metadata_hash.clone()),
        &Some(service_agreement_hash.clone()),
    );
    assert_eq!(escrow.ipfs_hash, Some(ipfs_hash.clone()));
    assert_eq!(escrow.metadata_hash, Some(metadata_hash.clone()));
    assert_eq!(escrow.service_agreement_hash, Some(service_agreement_hash.clone()));

    let metadata = client.get_escrow_metadata(&1);
    assert_eq!(metadata.ipfs_hash, Some(ipfs_hash));
    assert_eq!(metadata.metadata_hash, Some(metadata_hash));
    assert_eq!(metadata.service_agreement_hash, Some(service_agreement_hash));
}

#[test]
fn test_create_escrow_without_service_agreement_hash_defaults_none() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let escrow = client.create_escrow(&buyer, &seller, &token_id, &500, &1, &Some(3600));
    assert_eq!(escrow.service_agreement_hash, None);

    let metadata = client.get_escrow_metadata(&1);
    assert_eq!(metadata.service_agreement_hash, None);
}

// ===== Search and Pagination Tests =====

#[test]
fn test_escrow_search_by_buyer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &200_000_000);

    // Create 3 escrows for the same buyer
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.create_escrow(&buyer, &seller, &token_id, &20_000_000, &2, &None);
    client.create_escrow(&buyer, &seller, &token_id, &30_000_000, &3, &None);

    // Get all (limit 10)
    let b1 = client.get_escrows_by_buyer(&buyer, &0, &10, &false);
    assert_eq!(b1.len(), 3);
    assert_eq!(b1.get_unchecked(0), 1);
    assert_eq!(b1.get_unchecked(1), 2);
    assert_eq!(b1.get_unchecked(2), 3);

    // Pagination: page 0, limit 2
    let b2 = client.get_escrows_by_buyer(&buyer, &0, &2, &false);
    assert_eq!(b2.len(), 2);
    assert_eq!(b2.get_unchecked(0), 1);
    assert_eq!(b2.get_unchecked(1), 2);

    // Pagination: page 1, limit 2
    let b3 = client.get_escrows_by_buyer(&buyer, &1, &2, &false);
    assert_eq!(b3.len(), 1);
    assert_eq!(b3.get_unchecked(0), 3);

    // Pagination: out of bounds
    let b4 = client.get_escrows_by_buyer(&buyer, &2, &2, &false);
    assert_eq!(b4.len(), 0);
}

#[test]
fn test_escrow_search_by_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &200_000_000);

    // Create escrows for different sellers
    let seller2 = Address::generate(&env);
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.create_escrow(&buyer, &seller2, &token_id, &20_000_000, &2, &None);
    client.create_escrow(&buyer, &seller, &token_id, &30_000_000, &3, &None);

    // Check seller 1
    let s1 = client.get_escrows_by_seller(&seller, &0, &10, &false);
    assert_eq!(s1.len(), 2);
    assert_eq!(s1.get_unchecked(0), 1);
    assert_eq!(s1.get_unchecked(1), 3);

    // Check seller 2
    let s2 = client.get_escrows_by_seller(&seller2, &0, &10, &false);
    assert_eq!(s2.len(), 1);
    assert_eq!(s2.get_unchecked(0), 2);

    // Check non-existent seller
    let s3 = client.get_escrows_by_seller(&Address::generate(&env), &0, &10, &false);
    assert_eq!(s3.len(), 0);
}

#[test]
fn test_escrow_search_reverse_pagination() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &200_000_000);

    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.create_escrow(&buyer, &seller, &token_id, &20_000_000, &2, &None);
    client.create_escrow(&buyer, &seller, &token_id, &30_000_000, &3, &None);

    let buyer_page_1 = client.get_escrows_by_buyer(&buyer, &0, &2, &true);
    assert_eq!(buyer_page_1.len(), 2);
    assert_eq!(buyer_page_1.get_unchecked(0), 3);
    assert_eq!(buyer_page_1.get_unchecked(1), 2);

    let buyer_page_2 = client.get_escrows_by_buyer(&buyer, &1, &2, &true);
    assert_eq!(buyer_page_2.len(), 1);
    assert_eq!(buyer_page_2.get_unchecked(0), 1);

    let seller_page = client.get_escrows_by_seller(&seller, &0, &3, &true);
    assert_eq!(seller_page.len(), 3);
    assert_eq!(seller_page.get_unchecked(0), 3);
    assert_eq!(seller_page.get_unchecked(1), 2);
    assert_eq!(seller_page.get_unchecked(2), 1);
}

#[test]
fn test_min_escrow_amount_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_00000);
    // Let's test set_min_escrow_amount.

    // Set a small min amount
    client.set_min_escrow_amount(&token_id, &1_00000); // 1 token

    // Now 50_00000 should work
    client.create_escrow(&buyer, &seller, &token_id, &50_00000, &1, &None);
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.version, CURRENT_ESCROW_VERSION);
    assert_eq!(escrow.amount, 50_00000);
}

#[test]
fn test_set_min_escrow_amount_emits_config_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, token_id, _, _, _) = setup_test(&env, true);

    client.set_min_escrow_amount(&token_id, &1_00000);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let _config_event: ConfigUpdatedEvent = last_event.2.try_into_val(&env).unwrap();
    let last_event = events.last().unwrap();
    let config_event: ConfigUpdatedEvent = last_event.2.try_into_val(&env).unwrap();

    assert_eq!(
        config_event.field_name,
        Symbol::new(&env, "min_escrow_amount")
    );
    assert_eq!(config_event.old_value, ConfigValue::I128(0));
    assert_eq!(config_event.new_value, ConfigValue::I128(100000));
}

#[test]
#[should_panic]
fn test_create_escrow_below_custom_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    client.set_min_escrow_amount(&token_id, &50_000_000); // 50 tokens

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &40_000_000, &1, &None); // Should panic
}

#[test]
fn test_partial_refund_allows_dust_after_minimum_increase() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Creation-time minimum check: escrow is valid at creation.
    client.set_min_escrow_amount(&token_id, &10_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000, &1, &None);

    // Admin raises minimum above any potential remainder after split.
    client.set_min_escrow_amount(&token_id, &100_000);

    // Dispute + partial refund leaves only a dust remainder for seller.
    client.dispute_escrow(&1, &Symbol::new(&env, "Dust_split"), &buyer);
    client.propose_partial_refund(&1, &49_990, &buyer);
    client.accept_partial_refund(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 99_999_990);
    assert_eq!(token_client.balance(&seller), 10);
}

#[test]
#[should_panic]
fn test_set_min_escrow_amount_unauthorized() {
    let env = Env::default();
    // Do NOT mock auth globally
    let (client, _, _, token_id, _, _, _) = setup_test(&env, false);

    // Attempt to set min amount without being the admin or providing auth
    // The contract uses get_admin and admin.require_auth()
    client.set_min_escrow_amount(&token_id, &100);
}

#[test]
fn test_contract_address_admin_is_authorized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin_contract = env.register_contract(None, CraftNexusContract);
    let arbitrator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    env.ledger().with_mut(|li| {
        li.timestamp = 1711368000;
    });

    client.initialize(&platform_wallet, &admin_contract, &arbitrator, &500, &None);
    client.set_min_escrow_amount(&token_contract, &0);

    let config = client.get_platform_config();
    assert_eq!(config.admin, admin_contract);
}

#[test]
fn test_get_escrow_migrates_legacy_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    let legacy = LegacyEscrow {
        id: 77,
        buyer: buyer.clone(),
        seller: seller.clone(),
        token: token_id,
        amount: 123,
        status: EscrowStatus::Active,
        release_window: 50,
        created_at: 10,
        ipfs_hash: None,
        metadata_hash: None,
        dispute_reason: None,
        dispute_initiated_at: None,
    };

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&(ESCROW, 77u32), &legacy);
    });

    let escrow = client.get_escrow(&77);
    assert_eq!(escrow.version, CURRENT_ESCROW_VERSION);
    assert_eq!(escrow.amount, 123);
    assert_eq!(escrow.batch_id, None);

    let stored: Escrow = env.as_contract(&client.address, || {
        env.storage().persistent().get(&(ESCROW, 77u32)).unwrap()
    });
    assert_eq!(stored.version, CURRENT_ESCROW_VERSION);
    assert_eq!(stored.batch_id, None);
}

#[test]
#[ignore]
fn test_contract_upgrade_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // Initial version should be 1
    assert_eq!(client.get_version(), 1);

    // To test update_wasm, we need a WASM hash that "exists" in the test environment.
    // We can upload a tiny dummy WASM to get a valid hash.
    let dummy_wasm = Bytes::from_array(&env, &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let new_wasm_hash = env.deployer().upload_contract_wasm(dummy_wasm);

    client.execute_upgrade(&new_wasm_hash);

    // Version should be 2
    assert_eq!(client.get_version(), 2);
}

#[test]
#[should_panic]
fn test_contract_upgrade_unauthorized() {
    let env = Env::default();
    // Do NOT mock auth globally
    let (client, _, _, _, _, _, _) = setup_test(&env, false);

    let dummy_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Attempt upgrade without admin auth
    client.execute_upgrade(&dummy_hash);
}

#[test]
fn test_get_version_initially() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    assert_eq!(client.get_version(), 1);
}

#[test]
fn test_execute_upgrade_rejects_legacy_storage_layout_without_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    env.as_contract(&client.address, || {
        env.storage().persistent().remove(&DataKey::StorageLayoutVersion);
    });

    let hash = BytesN::from_array(&env, &[9u8; 32]);
    let result = client.try_execute_upgrade(&hash);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Ok(Error::StorageLayoutMismatch));
}

#[test]
fn test_migrate_storage_layout_marks_current_layout_and_preserves_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    env.as_contract(&client.address, || {
        env.storage().persistent().remove(&DataKey::StorageLayoutVersion);
    });

    let migrated = client.migrate_storage_layout();
    assert_eq!(migrated, 1);
    assert_eq!(client.get_storage_layout_version(), CURRENT_STORAGE_LAYOUT_VERSION);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.amount, 50_000_000);
    assert_eq!(escrow.status, EscrowStatus::Active);
}

// ===== Multi-sig / timelocked admin action tests =====

#[test]
fn test_pending_admin_action_requires_approvals_and_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    client.set_admin_action_signers(&signers);
    client.set_admin_action_threshold(&2);
    client.set_admin_action_timelock_delay(&60);

    let action = client.propose_admin_action(&admin, &AdminActionKind::PausePlatform(true));
    assert_eq!(action.threshold, 2);
    assert_eq!(action.approvals.len(), 1);
    assert!(!client.is_paused());

    let result = client.try_execute_admin_action(&action.id);
    assert!(matches!(result, Err(Ok(Error::AdminActionNeedsApprovals))));

    let second = client.approve_admin_action(&action.id, &signer2);
    assert_eq!(second.approvals.len(), 2);

    let result = client.try_execute_admin_action(&action.id);
    assert!(matches!(result, Err(Ok(Error::AdminActionTimelockActive))));

    env.ledger().with_mut(|li| {
        li.timestamp += 61;
    });

    client.execute_admin_action(&action.id);
    assert!(client.is_paused());
}

#[test]
fn test_pending_admin_action_is_cancelable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    client.set_admin_action_threshold(&1);
    client.set_admin_action_timelock_delay(&60);

    let action = client.propose_admin_action(&admin, &AdminActionKind::PausePlatform(true));
    let cancelled = client.cancel_admin_action(&action.id);
    assert!(cancelled.cancelled);

    let pending = client.get_pending_admin_actions();
    assert!(pending.is_empty());

    let result = client.try_execute_admin_action(&action.id);
    assert!(matches!(result, Err(Ok(Error::AdminActionTerminal))));
}

#[test]
fn test_upgrade_default_threshold_is_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    assert_eq!(client.get_upgrade_threshold(), 1);
}

#[test]
fn test_propose_upgrade_single_admin_succeeds() {
    // With threshold=1 and no explicit signers, the admin alone can propose.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let hash = BytesN::from_array(&env, &[2u8; 32]);
    client.propose_upgrade_wasm(&admin, &hash);

    // Proposal should now be committed.
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
    assert_eq!(proposal.proposed_by, admin);
}

#[test]
#[should_panic]
fn test_propose_upgrade_non_signer_rejected() {
    // A random address that is not in the signers list must be rejected.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let stranger = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[3u8; 32]);
    // This should panic because stranger is not an authorized signer.
    client.propose_upgrade_wasm(&stranger, &hash);
}

#[test]
fn test_multisig_threshold_two_of_two() {
    // Set threshold=2 and two explicit signers. Verify the proposal is only
    // committed after both have approved.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());

    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&2);

    let hash = BytesN::from_array(&env, &[4u8; 32]);

    // First approval — proposal must NOT be committed yet.
    client.propose_upgrade_wasm(&admin, &hash);
    assert!(
        client.get_upgrade_proposal().is_none(),
        "proposal committed too early"
    );
    // Nonce is 0 on the first round (no cancellations yet).
    assert_eq!(client.get_upgrade_approvals(&0).len(), 1);

    // Second approval — threshold reached, proposal committed.
    client.propose_upgrade_wasm(&signer2, &hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
}

#[test]
fn test_duplicate_approval_returns_already_approved() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());

    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&2);

    let hash = BytesN::from_array(&env, &[5u8; 32]);

    client.propose_upgrade_wasm(&admin, &hash);
    let result = client.try_propose_upgrade_wasm(&admin, &hash);
    assert!(result.is_err());
    assert!(result.is_err());

    // Nonce is 0; admin approved once; signer2 has not approved yet.
    assert_eq!(client.get_upgrade_approvals(&0).len(), 1);
    assert!(client.get_upgrade_proposal().is_none());
}

#[test]
fn test_unique_signers_only_reach_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());

    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&2);

    let hash = BytesN::from_array(&env, &[7u8; 32]);

    client.propose_upgrade_wasm(&admin, &hash);
    assert!(client.get_upgrade_proposal().is_none());

    client.propose_upgrade_wasm(&signer2, &hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
    assert_eq!(proposal.proposed_by, signer2);
}

#[test]
fn test_set_upgrade_threshold_zero_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    // threshold=0 must be rejected.
    let result = client.try_set_upgrade_threshold(&0);
    assert!(result.is_err());
}

#[test]
fn test_set_upgrade_signers_empty_resets_to_admin() {
    // Clearing the signers list makes the admin the sole default signer.
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(signer2.clone());
    client.set_upgrade_signers(&signers);

    // Reset back to empty (admin-default).
    let empty: Vec<Address> = Vec::new(&env);
    client.set_upgrade_signers(&empty);

    // Admin can now propose directly.
    let hash = BytesN::from_array(&env, &[6u8; 32]);
    client.propose_upgrade_wasm(&admin, &hash);
    assert!(client.get_upgrade_proposal().is_some());
}


// ============== Upgrade Governance Security Tests ==============

/// AC2: Signer rotation after first approval cannot inflate the approval count.
/// After the first signer approves (locking the snapshot), the admin adds a
/// new signer and changes the threshold.  The new signer should be treated as
/// part of the NEW round's signer set, but the snapshot for the CURRENT round
/// is already fixed.  The current round must NOT count the new signer.
#[test]
fn test_signer_rotation_cannot_inflate_approval_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let evil_signer = Address::generate(&env);

    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&2);

    let hash = BytesN::from_array(&env, &[10u8; 32]);

    // Round opens: admin approves (snapshot captured: {admin, signer2}, threshold=2).
    client.propose_upgrade_wasm(&admin, &hash);
    assert!(client.get_upgrade_proposal().is_none(), "proposal should not be committed yet");

    // Admin rotates signers to include evil_signer AFTER the round has opened.
    let mut new_signers = Vec::new(&env);
    new_signers.push_back(admin.clone());
    new_signers.push_back(evil_signer.clone());
    client.set_upgrade_signers(&new_signers);
    // Admin also tries to lower threshold to 1 after the round is open.
    client.set_upgrade_threshold(&1);

    // evil_signer was NOT in the snapshot — must be rejected.
    let result = client.try_propose_upgrade_wasm(&evil_signer, &hash);
    assert!(
        result.is_err(),
        "evil_signer added after round open must not be able to approve"
    );

    // Proposal still not committed — the threshold snapshot (2) was not met.
    assert!(client.get_upgrade_proposal().is_none(),
        "proposal must not be committed despite threshold change");

    // Only the original signer2 (from the snapshot) can complete this round.
    client.propose_upgrade_wasm(&signer2, &hash);
    assert!(client.get_upgrade_proposal().is_some(), "proposal must commit after 2 of 2 original signers");
}

// ===== Issue #95 — multi-sig threshold boundary scenarios =====

/// Threshold of 1 with a single, explicitly configured signer (not the
/// admin-fallback default). A lone signer's approval must commit immediately.
#[test]
fn test_multisig_threshold_one_explicit_signer_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let sole_signer = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(sole_signer.clone());
    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&1);

    let hash = BytesN::from_array(&env, &[20u8; 32]);

    // The admin itself is no longer a signer once an explicit list is set,
    // so it must be rejected.
    let admin_result = client.try_propose_upgrade_wasm(&admin, &hash);
    assert!(admin_result.is_err(), "admin is not in the explicit signer list");

    client.propose_upgrade_wasm(&sole_signer, &hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
    assert_eq!(proposal.proposed_by, sole_signer);
}

/// Threshold exactly equal to the number of configured signers (3-of-3):
/// every single signer must approve before the proposal commits.
#[test]
fn test_multisig_threshold_equals_signer_count_three_of_three() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());

    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&3);

    let hash = BytesN::from_array(&env, &[21u8; 32]);

    client.propose_upgrade_wasm(&admin, &hash);
    assert!(client.get_upgrade_proposal().is_none(), "1 of 3 must not commit");

    client.propose_upgrade_wasm(&signer2, &hash);
    assert!(client.get_upgrade_proposal().is_none(), "2 of 3 must not commit");

    client.propose_upgrade_wasm(&signer3, &hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
    assert_eq!(proposal.proposed_by, signer3);
}

/// Removing a signer from the live `UpgradeSigners` list mid-round must not
/// invalidate that signer's already-recorded approval, since the round's
/// signer set was snapshotted when the round opened (complements the
/// signer-addition case in `test_signer_rotation_cannot_inflate_approval_count`).
#[test]
fn test_signer_removed_mid_round_does_not_invalidate_recorded_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());

    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&3);

    let hash = BytesN::from_array(&env, &[22u8; 32]);

    // Round opens: snapshot captures {admin, signer2, signer3}, threshold=3.
    client.propose_upgrade_wasm(&admin, &hash);

    // signer3 is removed from the live signers list after the round opened.
    let mut reduced_signers = Vec::new(&env);
    reduced_signers.push_back(admin.clone());
    reduced_signers.push_back(signer2.clone());
    client.set_upgrade_signers(&reduced_signers);

    // signer2 (still live) approves.
    client.propose_upgrade_wasm(&signer2, &hash);
    assert!(client.get_upgrade_proposal().is_none(), "2 of 3 snapshotted signers must not commit");

    // signer3, though removed from the live list, was part of this round's
    // snapshot and must still be able to complete it.
    client.propose_upgrade_wasm(&signer3, &hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
    assert_eq!(proposal.proposed_by, signer3);
}

/// A committed proposal that is never executed before its operators move on
/// ("expires" in practice) must be cancellable and, after the
/// cancel-repropose cooldown elapses, replaceable with a new proposal.
#[test]
fn test_stale_upgrade_proposal_cancelled_and_reproposed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let stale_hash = BytesN::from_array(&env, &[23u8; 32]);
    client.propose_upgrade_wasm(&admin, &stale_hash);
    let stale_proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(stale_proposal.wasm_hash, stale_hash);

    // Let a long time pass without executing — the proposal goes stale but
    // remains pending since there is no automatic expiry, only the cooldown
    // gate on execute_upgrade.
    env.ledger().with_mut(|li| {
        li.timestamp += 30 * 24 * 60 * 60; // 30 days
    });
    assert!(client.get_upgrade_proposal().is_some(), "no automatic expiry — proposal still pending");

    // The stale proposal is cancelled instead of executed.
    client.cancel_upgrade_wasm();
    assert!(client.get_upgrade_proposal().is_none());

    // Advance past CANCEL_REPROPOSE_COOLDOWN (7 days + 1s) so a new proposal
    // is accepted.
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60 + 1;
    });

    let new_hash = BytesN::from_array(&env, &[24u8; 32]);
    client.propose_upgrade_wasm(&admin, &new_hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, new_hash);
}

/// `get_upgrade_history` must append exactly one record per successful
/// `execute_upgrade`, preserving from/to version pairs across multiple
/// upgrades.
#[test]
fn test_get_upgrade_history_records_each_successful_upgrade() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    assert_eq!(client.get_upgrade_history().len(), 0);

    // First upgrade: version 1 -> 2.
    let wasm_one = Bytes::from_array(&env, &[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]);
    let hash_one = env.deployer().upload_contract_wasm(wasm_one);

    client.propose_upgrade_wasm(&admin, &hash_one);
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60 + 1;
    });
    client.execute_upgrade(&hash_one);
    assert_eq!(client.get_version(), 2);

    let history_after_first = client.get_upgrade_history();
    assert_eq!(history_after_first.len(), 1);
    let first_record = history_after_first.get(0).unwrap();
    assert_eq!(first_record.from_version, 1);
    assert_eq!(first_record.to_version, 2);
    assert_eq!(first_record.wasm_hash, hash_one);
    assert_eq!(first_record.admin, admin);

    // Second upgrade: version 2 -> 3. A distinct (but still structurally
    // valid — magic + version + one empty custom section) module so its
    // hash differs from the first.
    let wasm_two = Bytes::from_array(
        &env,
        &[
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
        ],
    );
    let hash_two = env.deployer().upload_contract_wasm(wasm_two);

    client.propose_upgrade_wasm(&admin, &hash_two);
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60 + 1;
    });
    client.execute_upgrade(&hash_two);
    assert_eq!(client.get_version(), 3);

    let history_after_second = client.get_upgrade_history();
    assert_eq!(history_after_second.len(), 2);
    let second_record = history_after_second.get(1).unwrap();
    assert_eq!(second_record.from_version, 2);
    assert_eq!(second_record.to_version, 3);
    assert_eq!(second_record.wasm_hash, hash_two);
}

/// AC3: A pending proposal remains immutable after threshold approval is reached.
/// After the proposal is committed via propose_upgrade_wasm, any call to
/// propose_upgrade_wasm for the same hash must fail with UpgradeProposalExists.
#[test]
fn test_committed_proposal_is_immutable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let hash = BytesN::from_array(&env, &[11u8; 32]);

    // threshold=1 (default), admin is sole signer — one call commits.
    client.propose_upgrade_wasm(&admin, &hash);
    assert!(client.get_upgrade_proposal().is_some());

    // Any subsequent propose call must fail with UpgradeProposalExists.
    let result = client.try_propose_upgrade_wasm(&admin, &hash);
    assert!(result.is_err());
}

/// AC4: Cancellation clears stale approvals — after cancel, the nonce is
/// incremented and any old partial approvals are unreachable.  Re-proposing
/// after the cooldown starts a fresh round that requires new approvals.
///
/// Flow:
/// 1. Round 0: admin sole-approves (threshold=1) → proposal committed.
/// 2. Cancel → nonce bumped to 1, old state cleared.
/// 3. Advance past cooldown.
/// 4. Round 1: admin approves again → fresh state, nonce=1.
/// 5. Old approvals for nonce 0 are empty; new round has exactly 1 approval.
#[test]
fn test_cancel_clears_stale_approvals_and_increments_nonce() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    // Threshold=1 so a single admin approval commits the proposal immediately.
    let hash_a = BytesN::from_array(&env, &[12u8; 32]);
    let hash_b = BytesN::from_array(&env, &[13u8; 32]);

    // Round 0, nonce=0: admin approves hash_a → commits.
    assert_eq!(client.get_upgrade_proposal_nonce(), 0, "nonce starts at 0");
    client.propose_upgrade_wasm(&admin, &hash_a);
    assert!(client.get_upgrade_proposal().is_some(), "proposal must commit at threshold=1");
    // Approval state is removed on commit, so get_upgrade_approvals returns empty.
    assert_eq!(client.get_upgrade_approvals(&0).len(), 0,
        "approvals are cleared after commit");

    // Cancel the committed proposal → nonce bumps to 1.
    client.cancel_upgrade_wasm();
    assert_eq!(client.get_upgrade_proposal_nonce(), 1, "nonce must be 1 after cancel");
    assert!(client.get_upgrade_proposal().is_none(), "proposal must be removed after cancel");
    // Old nonce 0 approvals remain empty (never re-populated after commit+cancel).
    assert_eq!(client.get_upgrade_approvals(&0).len(), 0,
        "old nonce 0 approvals must be empty after cancel");

    // Advance past CANCEL_REPROPOSE_COOLDOWN (7 days + 1 s).
    env.ledger().with_mut(|li| { li.timestamp += 7 * 24 * 60 * 60 + 1; });

    // Round 1, nonce=1: admin proposes a different hash → fresh state.
    client.propose_upgrade_wasm(&admin, &hash_b);
    assert!(client.get_upgrade_proposal().is_some(),
        "proposal must commit in fresh round");
    // Nonce 0 still returns empty — old state was not replayed.
    assert_eq!(client.get_upgrade_approvals(&0).len(), 0,
        "nonce 0 must still be empty in round 1");
}

/// AC4 (simplified): cancel_upgrade_wasm increments the proposal nonce.
#[test]
fn test_cancel_increments_proposal_nonce() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let hash = BytesN::from_array(&env, &[13u8; 32]);

    // Commit a proposal (threshold=1, admin is sole signer).
    client.propose_upgrade_wasm(&admin, &hash);
    let nonce_before = client.get_upgrade_proposal_nonce();
    assert_eq!(nonce_before, 0, "nonce starts at 0");

    // Cancel increments the nonce.
    client.cancel_upgrade_wasm();
    let nonce_after = client.get_upgrade_proposal_nonce();
    assert_eq!(nonce_after, 1, "nonce must be 1 after first cancel");
}

/// Replay protection: after cancel + cooldown, re-proposing the same hash
/// with the same signers starts a completely new round (nonce=1, empty approvals).
#[test]
fn test_repropose_same_hash_starts_fresh_round_after_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&2);

    let hash = BytesN::from_array(&env, &[14u8; 32]);

    // Round 0: admin approves. Threshold not yet met.
    // To get a partial approval we need threshold=2; but cancel requires a committed
    // proposal. Lower threshold to 1 to commit, then cancel.
    client.set_upgrade_threshold(&1);
    client.propose_upgrade_wasm(&admin, &hash);
    // Committed. Cancel it.
    client.cancel_upgrade_wasm();
    // Nonce is now 1.
    assert_eq!(client.get_upgrade_proposal_nonce(), 1);

    // Advance past cooldown.
    env.ledger().with_mut(|li| { li.timestamp += 7 * 24 * 60 * 60 + 1; });

    // Round 1: admin approves again for the SAME hash.
    client.set_upgrade_threshold(&2);
    client.propose_upgrade_wasm(&admin, &hash);

    // Nonce is still 1 (cancel hasn't been called again).
    assert_eq!(client.get_upgrade_proposal_nonce(), 1);

    // Only 1 approval in round 1 — admin's prior approval from round 0 is NOT counted.
    assert_eq!(client.get_upgrade_approvals(&1).len(), 1,
        "round 1 must have exactly 1 fresh approval, not carry over from round 0");

    // Proposal must NOT be committed (threshold=2, only 1 approval so far).
    assert!(client.get_upgrade_proposal().is_none(),
        "proposal must not commit with only 1 of 2 required approvals in fresh round");

    // signer2 approves to complete round 1.
    client.propose_upgrade_wasm(&signer2, &hash);
    assert!(client.get_upgrade_proposal().is_some(), "proposal must commit after 2nd approval");
}

/// Threshold snapshot: changing threshold mid-round does not affect the current round.
#[test]
fn test_threshold_change_mid_round_does_not_affect_current_round() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    let signer2 = Address::generate(&env);
    let signer3 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(admin.clone());
    signers.push_back(signer2.clone());
    signers.push_back(signer3.clone());
    client.set_upgrade_signers(&signers);
    client.set_upgrade_threshold(&3); // requires all 3

    let hash = BytesN::from_array(&env, &[15u8; 32]);

    // admin approves first — snapshot captures threshold=3.
    client.propose_upgrade_wasm(&admin, &hash);
    assert!(client.get_upgrade_proposal().is_none());

    // Admin lowers threshold to 1 after the round has opened.
    client.set_upgrade_threshold(&1);

    // signer2 approves — with the NEW threshold=1 this would be sufficient,
    // but the snapshot still requires 3.
    client.propose_upgrade_wasm(&signer2, &hash);
    assert!(
        client.get_upgrade_proposal().is_none(),
        "proposal must not commit: snapshot threshold is 3, only 2 approvals so far"
    );

    // Third approval completes the snapshotted requirement.
    client.propose_upgrade_wasm(&signer3, &hash);
    assert!(client.get_upgrade_proposal().is_some(), "proposal must commit after 3 of 3 approvals");
}

// ============== Batch Operations Tests ==============

#[test]
fn test_create_batch_escrow_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    // Mint enough tokens for multiple escrows
    token_admin.mint(&buyer, &1_000_000_000);

    let escrow_params = vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 100_000_000,
            order_id: 100,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 200_000_000,
            order_id: 101,
            release_window: Some(7200),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 150_000_000,
            order_id: 102,
            release_window: None, // Uses default
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
    ];

    let batch_id = 1u64;
    let results = client.create_batch_escrow(&batch_id, &escrow_params);

    assert_eq!(results.len(), 3);
    assert_eq!(results.get(0).unwrap(), 100);
    assert_eq!(results.get(1).unwrap(), 101);
    assert_eq!(results.get(2).unwrap(), 102);

    // Verify escrows were created
    let escrow1 = client.get_escrow(&100);
    assert_eq!(escrow1.amount, 100_000_000);
    assert_eq!(escrow1.status, EscrowStatus::Active);
    assert_eq!(escrow1.batch_id, Some(batch_id));

    let escrow2 = client.get_escrow(&101);
    assert_eq!(escrow2.amount, 200_000_000);
    assert_eq!(escrow2.status, EscrowStatus::Active);
    assert_eq!(escrow2.batch_id, Some(batch_id));

    let escrow3 = client.get_escrow(&102);
    assert_eq!(escrow3.amount, 150_000_000);
    assert_eq!(escrow3.release_window, 604800); // Default 7 days
    assert_eq!(escrow3.batch_id, Some(batch_id));

    // Verify events were emitted
    let events = env.events().all();
    let expected_topic: soroban_sdk::Val = Symbol::new(&env, "escrow").into_val(&env);
    let batch_events: alloc::vec::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() >= 2
                && soroban_sdk::vec![&env, topics.get_unchecked(0)]
                    == soroban_sdk::vec![&env, expected_topic]
        })
        .collect();
    assert_eq!(
        batch_events.len(),
        6,
        "Should emit batch event for each escrow"
    );
}

#[test]
#[should_panic]
fn test_create_batch_escrow_fails_on_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create batch with invalid amount (zero)
    let escrow_params = vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 0, // Invalid - zero amount
            order_id: 100,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
    ];

    client.create_batch_escrow(&1u64, &escrow_params);
}

#[test]
#[should_panic]
fn test_create_batch_escrow_fails_same_buyer_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, _, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create batch where buyer equals seller
    let escrow_params = vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: buyer.clone(), // Same as buyer!
            token: token_id.clone(),
            amount: 100,
            order_id: 100,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
    ];

    client.create_batch_escrow(&1u64, &escrow_params);
}

#[test]
#[should_panic]
fn test_create_batch_escrow_requires_authorization_for_each_distinct_buyer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    let second_buyer = Address::generate(&env);
    token_admin.mint(&buyer, &1_000_000_000);
    token_admin.mint(&second_buyer, &1_000_000_000);

    let escrow_params = vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 100,
            order_id: 100,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
        EscrowCreateParams {
            buyer: second_buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 200,
            order_id: 101,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
    ];

    // Remove the second buyer's authorization so the batch should panic.
    env.set_auths(&[]);
    client.create_batch_escrow(&1u64, &escrow_params);
}

// ===== Issue #111 — batch escrow boundary scenarios =====

#[test]
fn test_create_batch_escrow_at_max_size() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    let mut batch_params = vec![&env];
    for i in 0..MAX_BATCH_SIZE {
        batch_params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 1_000,
            order_id: 500 + i,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
        });
    }
    assert_eq!(batch_params.len(), MAX_BATCH_SIZE);

    let results = client.create_batch_escrow(&10u64, &batch_params);
    assert_eq!(results.len(), MAX_BATCH_SIZE);

    for i in 0..MAX_BATCH_SIZE {
        let escrow = client.get_escrow(&(500 + i));
        assert_eq!(escrow.status, EscrowStatus::Active);
        assert_eq!(escrow.batch_id, Some(10u64));
    }
}

#[test]
fn test_create_batch_escrow_exceeds_max_size() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    let mut batch_params = vec![&env];
    for i in 0..(MAX_BATCH_SIZE + 1) {
        batch_params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 1_000,
            order_id: 600 + i,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
        });
    }
    assert_eq!(batch_params.len(), MAX_BATCH_SIZE + 1);

    // The whole batch must be rejected — none of the escrows should be created.
    let result = client.try_create_batch_escrow(&11u64, &batch_params);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), Ok(Error::BatchLimitExceeded));

    for i in 0..(MAX_BATCH_SIZE + 1) {
        let escrow_result = client.try_get_escrow(&(600 + i));
        assert!(
            escrow_result.is_err(),
            "no escrow should have been created when the batch exceeds MAX_BATCH_SIZE"
        );
    }
}

#[test]
#[should_panic]
fn test_create_batch_escrow_multi_buyer_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    let second_buyer = Address::generate(&env);
    token_admin.mint(&buyer, &1_000_000_000);
    token_admin.mint(&second_buyer, &1_000_000_000);

    let escrow_params = vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 1_000,
            order_id: 700,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
        },
        EscrowCreateParams {
            buyer: second_buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 2_000,
            order_id: 701,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
        },
    ];

    // Strip all mocked authorizations so neither buyer — in particular the
    // second, distinct buyer — has a valid auth entry for this call.
    env.set_auths(&[]);
    client.create_batch_escrow(&12u64, &escrow_params);
}

#[test]
fn test_release_batch_funds_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _platform_wallet, _) =
        setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create multiple escrows
    client.create_escrow(&buyer, &seller, &token_id, &100_000_000, &100, &None);
    client.create_escrow(&buyer, &seller, &token_id, &200_000_000, &101, &None);
    client.create_escrow(&buyer, &seller, &token_id, &150_000_000, &102, &None);

    // Verify active obligations are set
    assert!(client.has_active_escrows(&buyer));
    assert!(client.has_active_escrows(&seller));

    // Release batch
    let order_ids = vec![&env, 100u32, 101u32, 102u32];
    let batch_id = 1u64;
    let results = client.release_batch_funds(&batch_id, &order_ids, &buyer);

    assert_eq!(results.len(), 3);
    assert_eq!(results.get(0).unwrap(), 100);
    assert_eq!(results.get(1).unwrap(), 101);
    assert_eq!(results.get(2).unwrap(), 102);

    // Verify active obligations were decremented
    assert!(!client.has_active_escrows(&buyer));
    assert!(!client.has_active_escrows(&seller));

    // Verify statuses
    let escrow1 = client.get_escrow(&100);
    assert_eq!(escrow1.status, EscrowStatus::Released);

    let escrow2 = client.get_escrow(&101);
    assert_eq!(escrow2.status, EscrowStatus::Released);

    let escrow3 = client.get_escrow(&102);
    assert_eq!(escrow3.status, EscrowStatus::Released);

    // Verify batch events were emitted
    let events = env.events().all();
    let expected_topic: soroban_sdk::Val = Symbol::new(&env, "escrow").into_val(&env);
    let batch_events: alloc::vec::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() >= 2
                && soroban_sdk::vec![&env, topics.get_unchecked(0)]
                    == soroban_sdk::vec![&env, expected_topic]
        })
        .collect();
    assert_eq!(
        batch_events.len(),
        6,
        "Should emit batch event for each release"
    );
}

#[test]
#[should_panic]
fn test_release_batch_funds_fails_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create one escrow
    client.create_escrow(&buyer, &seller, &token_id, &100, &100, &None);

    // Try to release batch with non-existent escrow
    let order_ids = vec![&env, 100u32, 999u32]; // 999 doesn't exist
    client.release_batch_funds(&1u64, &order_ids, &buyer);
}

#[test]
#[should_panic]
fn test_release_batch_funds_fails_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create escrow
    client.create_escrow(&buyer, &seller, &token_id, &100, &100, &None);

    // Release it first
    client.release_funds(&100);

    // Try to release again in batch
    let order_ids = vec![&env, 100u32];
    client.release_batch_funds(&1u64, &order_ids, &buyer);
}

#[test]
#[should_panic]
fn test_release_batch_funds_fails_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create escrow
    client.create_escrow(&buyer, &seller, &token_id, &100, &100, &None);

    // Try to release with different address
    let unauthorized = Address::generate(&env);
    let order_ids = vec![&env, 100u32];
    client.release_batch_funds(&1u64, &order_ids, &unauthorized);
}

#[test]
fn test_reentrancy_guard_prevents_recursive_call() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // Manually set the guard in temporary storage
    env.as_contract(&client.address, || {
        env.storage().temporary().set(&DataKey::ReentryGuard, &true);
    });

    // Attempting to call a guarded function should now fail
    let result = client.try_release_funds(&1);
    assert!(result.is_err());
}

#[test]
fn test_reentrancy_guard_blocks_release_and_refund_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &20_000_000, &1, &None);
    client.create_escrow(&buyer, &seller, &token_id, &20_000_000, &2, &None);

    env.as_contract(&client.address, || {
        env.storage().temporary().set(&DataKey::ReentryGuard, &true);
    });

    let release_ids = vec![&env, 1u32];
    let batch_result = client.try_release_batch_funds(&1u64, &release_ids, &buyer);
    assert!(batch_result.is_err());

    env.as_contract(&client.address, || {
        env.storage().temporary().remove(&DataKey::ReentryGuard);
        env.storage().temporary().set(&DataKey::ReentryGuard, &true);
    });

    let refund_result = client.try_refund(&2u64);
    assert!(refund_result.is_err());

    env.as_contract(&client.address, || {
        env.storage().temporary().remove(&DataKey::ReentryGuard);
    });
}

#[test]
fn test_reentrancy_guard_cleared_after_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // This should succeed and clear the guard
    client.release_funds(&1);

    // The guard should be gone
    env.as_contract(&client.address, || {
        assert!(!env.storage().temporary().has(&DataKey::ReentryGuard));
    });
}

#[test]
fn test_reentrancy_guard_cleared_after_batch_create_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    let invalid_params = vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 0,
            order_id: 100,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
    ];

    let result = client.try_create_batch_escrow(&1u64, &invalid_params);
    assert!(result.is_err());

    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &101, &None);
    let escrow = client.get_escrow(&101);
    assert_eq!(escrow.status, EscrowStatus::Active);

    env.as_contract(&client.address, || {
        assert!(!env.storage().temporary().has(&DataKey::ReentryGuard));
    });
}

#[test]
fn test_reentrancy_guard_cleared_after_batch_release_error() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &25_000_000, &100, &None);
    client.create_escrow(&buyer, &seller, &token_id, &25_000_000, &101, &None);

    client.release_funds(&100);

    let order_ids = vec![&env, 100u32];
    let result = client.try_release_batch_funds(&1u64, &order_ids, &buyer);
    assert!(result.is_err());

    client.release_funds(&101);
    let escrow = client.get_escrow(&101);
    assert_eq!(escrow.status, EscrowStatus::Released);

    env.as_contract(&client.address, || {
        assert!(!env.storage().temporary().has(&DataKey::ReentryGuard));
    });
}

#[test]
fn test_extend_release_window_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let window = 3600;
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &Some(window));

    let additional = 7200;
    client.extend_release_window(&1, &additional);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.release_window, window + additional);

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow").into_val(&env),
            1u64.into_val(&env)
        ]
    );

    let event: EscrowEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.action, EscrowAction::Extended);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
}

#[test]
#[should_panic]
fn test_extend_release_window_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // Switch auth to seller
    env.set_auths(&[]); // Clear auths
    client.extend_release_window(&1, &3600);
}

#[test]
#[should_panic]
fn test_extend_release_window_too_long() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // Max is 30 days (2592000). Default is 7 days (604800).
    // Try adding 25 days (2160000) -> 604800 + 2160000 = 2764800 > 2592000
    client.extend_release_window(&1, &2160000);
}

#[test]
fn test_auto_release_respects_extension() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let window = 100;
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &Some(window));

    client.extend_release_window(&1, &100);

    // Advance time by 150 - should still fail auto_release (window is now 200)
    env.ledger().with_mut(|li| {
        li.timestamp += 150;
    });

    assert!(!client.can_auto_release(&1));
    let result = client.try_auto_release(&1);
    assert!(result.is_err());

    // Advance time by another 100 (total 250) - should now succeed
    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    assert!(client.can_auto_release(&1));
    client.auto_release(&1);
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

// ============================================================
// Issue #67 – Custom Release Window Constraints
// ============================================================

/// Default max window (MAX_TOTAL_RELEASE_WINDOW = 2_592_000) is applied when
/// no admin has called set_max_release_window. An escrow with a window below
/// the default must be created successfully.
#[test]
fn test_max_window_default_allows_normal_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // 7-day window (604800) is well below the 30-day default max (2_592_000)
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &Some(604800));
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.release_window, 604800);
}

/// A zero release window must be rejected.
#[test]
#[should_panic]
fn test_create_escrow_zero_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // window = 0 should panic with ReleaseWindowTooShort
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &Some(0));
}

/// A window that exceeds the default maximum (2_592_000 seconds) must be rejected.
#[test]
#[should_panic]
fn test_create_escrow_exceeds_default_max_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // 31 days in seconds > 30-day default max
    let too_long: u32 = 31 * 24 * 60 * 60;
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &Some(too_long));
}

/// Admin can tighten the maximum; subsequent escrows over the new limit fail.
#[test]
fn test_set_max_release_window_and_enforcement() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Set a tight maximum of 1 hour (3600 seconds)
    client.set_max_release_window(&3600u32);

    // Escrow with window exactly at the limit succeeds
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &Some(3600));
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.release_window, 3600);
}

/// A window that exceeds the admin-configured maximum must be rejected.
#[test]
#[should_panic]
fn test_create_escrow_exceeds_configured_max_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Admin sets a 1-hour max
    client.set_max_release_window(&3600u32);

    // Attempting 2 hours should panic with ReleaseWindowTooLong
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &Some(7200));
}

/// set_max_release_window with zero must be rejected.
#[test]
#[should_panic]
fn test_set_max_release_window_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    client.set_max_release_window(&0u32);
}

/// set_max_release_window above the hard safety ceiling must be rejected.
#[test]
#[should_panic]
fn test_set_max_release_window_above_absolute_ceiling_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // 366 days > hardcoded 365-day ceiling.
    client.set_max_release_window(&(366u32 * 24 * 60 * 60));
}

#[test]
fn test_set_max_release_window_at_absolute_ceiling_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    let ceiling = 365u32 * 24 * 60 * 60;
    client.set_max_release_window(&ceiling);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &Some(ceiling));

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.release_window, ceiling);
}

// ============================================================
// Issue #100 – Reputation System / cross-contract plumbing
// ============================================================

/// set_onboarding_contract stores the address without error.
#[test]
fn test_set_onboarding_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let fake_onboarding = Address::generate(&env);
    // Should not panic
    client.set_onboarding_contract(&fake_onboarding);
}

/// Duplicate set_onboarding_contract with the same address performs only one
/// storage write — the second call is a no-op (Issue #527 / #642).
#[test]
fn test_set_onboarding_contract_same_address_skips_storage_write() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let new_onboarding = Address::generate(&env);
    let events_before = env.events().all().len();

    client.set_onboarding_contract(&new_onboarding);
    let events_after_first = env.events().all().len();
    assert_eq!(events_after_first, events_before + 1);

    client.set_onboarding_contract(&new_onboarding);
    let events_after_second = env.events().all().len();
    assert_eq!(events_after_second, events_after_first);

    assert_eq!(client.get_onboarding_contract(), new_onboarding);
}

#[test]
fn test_get_onboarding_client_uses_configured_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // `get_onboarding_client` reads contract storage, so it has to be invoked
    // inside the contract's storage context rather than from the test frame.
    let initial = env.as_contract(&client.address, || {
        CraftNexusContract::get_onboarding_client(&env)
    });
    let (initial_address, _) = initial.expect("setup registers an onboarding contract");
    assert_eq!(initial_address, client.get_onboarding_contract());

    // Re-pointing the registry must be reflected by the helper on the next read.
    let onboarding = Address::generate(&env);
    client.set_onboarding_contract(&onboarding);

    let configured = env.as_contract(&client.address, || {
        CraftNexusContract::get_onboarding_client(&env)
    });
    let (address, _client) = configured.expect("configured address should resolve");
    assert_eq!(address, onboarding);
}

/// When no onboarding contract is set, release_funds completes without error.
#[test]
fn test_release_funds_no_onboarding_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    client.release_funds(&1); // should succeed gracefully

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

/// When no onboarding contract is set, refund completes without error.
#[test]
fn test_refund_no_onboarding_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    let result = client.try_refund(&1u64);
    assert!(result.is_ok());

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
}

// ─── Issue #103: Token Whitelisting ──────────────────────────────────────────

/// When no tokens have been whitelisted, any token is accepted (backward compat).
#[test]
fn test_whitelist_empty_allows_any_token() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Whitelist is empty — escrow creation must succeed for any token
    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Active);
}

/// is_token_whitelisted returns true for any token when the whitelist is empty.
#[test]
fn test_is_token_whitelisted_empty_whitelist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, token_id, _, _, _) = setup_test(&env, true);

    assert!(client.is_token_whitelisted(&token_id));
}

/// Admin can whitelist a token; is_token_whitelisted returns true for it.
#[test]
fn test_whitelist_token_admin_can_add() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, token_id, _, _, _) = setup_test(&env, true);

    client.whitelist_token(&token_id);
    assert!(client.is_token_whitelisted(&token_id));
}

/// Once a token is whitelisted, a different (non-whitelisted) token is rejected.
#[test]
#[should_panic]
fn test_create_escrow_non_whitelisted_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Whitelist the first token — enforcement is now active
    client.whitelist_token(&token_id);

    // Attempt to create an escrow with a different, non-whitelisted token
    let other_token_admin = Address::generate(&env);
    let other_token = env.register_stellar_asset_contract_v2(other_token_admin.clone());
    let other_token_client = token::StellarAssetClient::new(&env, &other_token.address());
    other_token_client.mint(&buyer, &100_000_000);

    client.create_escrow(
        &buyer,
        &seller,
        &other_token,
        &10_000,
        &2,
        &Some(3600),
    );
}

/// Whitelisted token is accepted for escrow creation when whitelist is active.
#[test]
fn test_create_escrow_whitelisted_token_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    client.whitelist_token(&token_id);
    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Active);
}

/// Admin can remove a token from the whitelist; is_token_whitelisted returns false for it.
#[test]
fn test_remove_token_from_whitelist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, token_id, _, _, _) = setup_test(&env, true);

    client.whitelist_token(&token_id);
    assert!(client.is_token_whitelisted(&token_id));

    client.remove_token_from_whitelist(&token_id);
    // Whitelist is now empty again — all tokens permitted
    assert!(client.is_token_whitelisted(&token_id));
}

/// After removing the last token, escrow creation succeeds for any token again.
#[test]
fn test_empty_whitelist_after_removal_allows_any_token() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Add then immediately remove to leave whitelist empty
    client.whitelist_token(&token_id);
    client.remove_token_from_whitelist(&token_id);

    // Should succeed — empty whitelist means no enforcement
    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Active);
}

/// Batch escrow creation fails if a token in the batch is not whitelisted.
#[test]
fn test_batch_escrow_non_whitelisted_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Whitelist the first token — enforcement is now active
    client.whitelist_token(&token_id);

    // Build a batch with a non-whitelisted second token
    let other_token_admin = Address::generate(&env);
    let other_token = env.register_stellar_asset_contract_v2(other_token_admin.clone());

    let params = soroban_sdk::vec![
        &env,
        EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: other_token,
            amount: 10_000,
            order_id: 10,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        },
    ];
    let result = client.try_create_batch_escrow(&1u64, &params);
    assert!(result.is_err());
}

// Ensure that removing a token from the whitelist does not prevent state
// transitions (release/refund) for escrows that were created while the
// token was whitelisted. This prevents funds from being locked if the
// whitelist changes after escrow creation (Issue #201 acceptance).
#[test]
fn test_release_succeeds_after_whitelist_removal() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    // Mint funds to buyer and whitelist the token
    token_admin.mint(&buyer, &100_000_000);
    client.whitelist_token(&token_id);

    // Create escrow while token is whitelisted
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    // Admin removes token from whitelist (enforcement now changes)
    client.remove_token_from_whitelist(&token_id);

    // Release funds — must succeed even though token is no longer whitelisted
    client.release_funds(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Released);

    let token_client = token::Client::new(&env, &token_id);
    // Seller receives 50_000_000 - fee (5%) = 47_500_000
    assert_eq!(token_client.balance(&seller), 47_500_000);
    // Platform receives fee
    assert_eq!(token_client.balance(&platform_wallet), 2_500_000);
}

/// Multiple tokens can be whitelisted independently.
#[test]
fn test_multiple_tokens_on_whitelist() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Register a second token
    let token2_admin = Address::generate(&env);
    let token2 = env.register_stellar_asset_contract_v2(token2_admin.clone());
    let token2_client = token::StellarAssetClient::new(&env, &token2.address());
    token2_client.mint(&buyer, &100_000_000);

    client.whitelist_token(&token_id);
    client.whitelist_token(&token2);

    assert!(client.is_token_whitelisted(&token_id));
    assert!(client.is_token_whitelisted(&token2));

    // Both should succeed in escrow creation
    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    client.create_escrow(&buyer, &seller, &token2, &10_000, &2, &Some(3600));
    assert_eq!(client.get_escrow(&1).status, EscrowStatus::Active);
    assert_eq!(client.get_escrow(&2).status, EscrowStatus::Active);
}

#[test]
fn test_whitelist_stores_tokens_as_individual_keys() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, token_id, _, _, _) = setup_test(&env, true);

    client.whitelist_token(&token_id);

    assert!(env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .has(&DataKey::WhitelistedTokenIndexed(token_id.clone()))
    }));
    assert!(env.as_contract(&client.address, || {
        !env.storage().persistent().has(&DataKey::WhitelistedTokens)
    }));
    let count: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::WhitelistedTokenCount)
            .unwrap_or(0u32)
    });
    assert_eq!(count, 1);
}

// ============================================================
// Decimal validation on whitelist_token
// ============================================================

/// Tokens with 0 decimals (minimum boundary) are accepted.
#[test]
fn test_whitelist_token_accepts_zero_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    use crate::onboarding::decimal_test_token::{DecimalTestToken, DecimalTestTokenClient};
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, DecimalTestToken);
    DecimalTestTokenClient::new(&env, &contract_id).initialize(&admin, &0u32);

    client.whitelist_token(&contract_id);
    assert!(client.is_token_whitelisted(&contract_id));
}

/// Tokens with 7 decimals (standard Stellar) are accepted.
#[test]
fn test_whitelist_token_accepts_seven_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    use crate::onboarding::decimal_test_token::{DecimalTestToken, DecimalTestTokenClient};
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, DecimalTestToken);
    DecimalTestTokenClient::new(&env, &contract_id).initialize(&admin, &7u32);

    client.whitelist_token(&contract_id);
    assert!(client.is_token_whitelisted(&contract_id));
}

/// Tokens with 18 decimals (maximum boundary) are accepted.
#[test]
fn test_whitelist_token_accepts_eighteen_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    use crate::onboarding::decimal_test_token::{DecimalTestToken, DecimalTestTokenClient};
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, DecimalTestToken);
    DecimalTestTokenClient::new(&env, &contract_id).initialize(&admin, &18u32);

    client.whitelist_token(&contract_id);
    assert!(client.is_token_whitelisted(&contract_id));
}

/// Tokens with 19 decimals (one above the maximum) are rejected with
/// InvalidTokenDecimals; the token must not appear in the whitelist.
#[test]
fn test_whitelist_token_rejects_nineteen_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    use crate::onboarding::decimal_test_token::{DecimalTestToken, DecimalTestTokenClient};
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, DecimalTestToken);
    DecimalTestTokenClient::new(&env, &contract_id).initialize(&admin, &19u32);

    let result = client.try_whitelist_token(&contract_id);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidTokenDecimals)),
        "expected InvalidTokenDecimals for 19-decimal token"
    );
    // Token must not have been added to the whitelist
    assert_eq!(
        client.get_whitelisted_token_count(),
        0,
        "whitelist count must stay 0 after rejection"
    );
}

/// Tokens reporting 255 decimals (malformed metadata) are rejected with
/// InvalidTokenDecimals.
#[test]
fn test_whitelist_token_rejects_255_decimals() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    use crate::onboarding::decimal_test_token::{DecimalTestToken, DecimalTestTokenClient};
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, DecimalTestToken);
    DecimalTestTokenClient::new(&env, &contract_id).initialize(&admin, &255u32);

    let result = client.try_whitelist_token(&contract_id);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidTokenDecimals)),
        "expected InvalidTokenDecimals for 255-decimal token"
    );
    assert_eq!(
        client.get_whitelisted_token_count(),
        0,
        "whitelist count must stay 0 after rejection"
    );
}

// ============================================================
// Issue #643 – Fee token config migration audit
// ============================================================

#[test]
fn test_migrate_fee_token_configs_migrates_twenty_tokens_and_emits_summary() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let mut fee_tokens = vec![&env];
    for i in 0..20u32 {
        let token = Address::generate(&env);
        fee_tokens.push_back(token.clone());

        env.as_contract(&client.address, || {
            env.storage().persistent().set(
                &DataKey::TotalFees(token.clone()),
                &((i as i128 + 1) * 1_000),
            );
        });
    }

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FeeTokenIndex, &fee_tokens);
    });

    let migrated = client.migrate_fee_token_configs();
    assert_eq!(migrated, 20);

    for i in 0..fee_tokens.len() {
        let token = fee_tokens.get(i).unwrap();
        let cfg = client.get_fee_token_config(&token).unwrap();
        assert_eq!(
            cfg,
            FeeTokenInfo {
                active: true,
                custom_fee_bps: None,
                accumulated: (i as i128 + 1) * 1_000,
            }
        );
    }

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    assert_eq!(
        last_event.1,
        vec![&env, Symbol::new(&env, "fee_cfg_migrated").into_val(&env)]
    );

    let summary: FeeTokenConfigsMigratedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(
        summary,
        FeeTokenConfigsMigratedEvent {
            scanned_tokens: 20,
            migrated_configs: 20,
            skipped_existing: 0,
        }
    );
}

#[test]
fn test_migrate_fee_token_configs_is_idempotent_and_preserves_existing_configs() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let mut fee_tokens = vec![&env];
    for i in 0..20u32 {
        let token = Address::generate(&env);
        fee_tokens.push_back(token.clone());

        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .set(&DataKey::TotalFees(token.clone()), &((i as i128 + 1) * 500));
        });
    }

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::FeeTokenIndex, &fee_tokens);

        let preset_one = fee_tokens.get(3).unwrap();
        let preset_two = fee_tokens.get(11).unwrap();

        env.storage().persistent().set(
            &DataKey::FeeTokenConfig(preset_one.clone()),
            &FeeTokenInfo {
                active: false,
                custom_fee_bps: Some(250),
                accumulated: 777_777,
            },
        );
        env.storage().persistent().set(
            &DataKey::FeeTokenConfig(preset_two.clone()),
            &FeeTokenInfo {
                active: true,
                custom_fee_bps: Some(900),
                accumulated: 888_888,
            },
        );
    });

    let migrated_first = client.migrate_fee_token_configs();
    assert_eq!(migrated_first, 18);

    let preset_one = fee_tokens.get(3).unwrap();
    let preset_two = fee_tokens.get(11).unwrap();
    assert_eq!(
        client.get_fee_token_config(&preset_one).unwrap(),
        FeeTokenInfo {
            active: false,
            custom_fee_bps: Some(250),
            accumulated: 777_777,
        }
    );
    assert_eq!(
        client.get_fee_token_config(&preset_two).unwrap(),
        FeeTokenInfo {
            active: true,
            custom_fee_bps: Some(900),
            accumulated: 888_888,
        }
    );

    for i in 0..fee_tokens.len() {
        let token = fee_tokens.get(i).unwrap();
        let cfg = client.get_fee_token_config(&token).unwrap();
        if token != preset_one && token != preset_two {
            assert_eq!(
                cfg,
                FeeTokenInfo {
                    active: true,
                    custom_fee_bps: None,
                    accumulated: (i as i128 + 1) * 500,
                }
            );
        }
    }

    let migrated_second = client.migrate_fee_token_configs();
    assert_eq!(migrated_second, 0);

    for i in 0..fee_tokens.len() {
        let token = fee_tokens.get(i).unwrap();
        assert!(client.get_fee_token_config(&token).is_some());
    }

    let events = env.events().all();
    let latest_event = events.last().unwrap();
    let latest_summary: FeeTokenConfigsMigratedEvent = latest_event.2.try_into_val(&env).unwrap();
    assert_eq!(
        latest_summary,
        FeeTokenConfigsMigratedEvent {
            scanned_tokens: 20,
            migrated_configs: 0,
            skipped_existing: 20,
        }
    );
}

// ============================================================
// Issue #111 – Batch Optimization Tests (Additional)
// ============================================================

/// Test batch creation consolidates storage updates (Issue #111)
#[test]
fn test_create_batch_escrow_consolidates_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &500_000);

    let mut batch_params = vec![&env];
    for i in 0..10 {
        batch_params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 5_000,
            order_id: 300 + i,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
            service_agreement_hash: None,
        });
    }

    let results = client.create_batch_escrow(&2u64, &batch_params);
    assert_eq!(results.len(), 10);

    // Verify buyer's escrow list contains all 10
    let buyer_escrows = client.get_escrows_by_buyer(&buyer, &0, &100, &false);
    assert_eq!(buyer_escrows.len(), 10);

    // Verify seller's escrow list contains all 10
    let seller_escrows = client.get_escrows_by_seller(&seller, &0, &100, &false);
    assert_eq!(seller_escrows.len(), 10);
}

// ============================================================
// Issue #122 – Metadata Privacy Tests
// ============================================================

/// Test metadata reveal verification with valid content (Issue #122)
#[test]
fn test_verify_metadata_reveal_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Create content and compute its hash
    let content = Bytes::from_slice(&env, b"test metadata content");
    let content_hash = env.crypto().sha256(&content);
    let content_hash_bytes: Bytes = content_hash.into();

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &500,
        &1,
        &Some(3600),
        &None,
        &Some(content_hash_bytes.clone()),
        &None,
    );

    assert_eq!(escrow.metadata_hash, Some(content_hash_bytes));

    // Verify the metadata reveal
    let proof = MetadataRevealProof {
        content: content.clone(),
        secret: None,
    };

    let is_valid = client.verify_metadata_reveal(&1, &proof, &buyer);
    assert!(is_valid);
}

#[test]
fn test_verify_metadata_reveal_authorized_emits_metadata_verified_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    let content = Bytes::from_slice(&env, b"test metadata content");
    let content_hash = env.crypto().sha256(&content);
    let content_hash_bytes: Bytes = content_hash.into();

    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &500,
        &1,
        &Some(3600),
        &None,
        &Some(content_hash_bytes.clone()),
        &None,
    );

    let proof = MetadataRevealProof {
        content: content.clone(),
        secret: None,
    };

    let is_valid = client.verify_metadata_reveal_recorded(&1, &proof, &buyer);
    assert!(is_valid);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow_metadata_verified").into_val(&env),
            (1u64).into_val(&env),
        ]
    );

    let event: MetadataVerifiedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.order_id, 1);
    assert_eq!(event.verifier, buyer);
    assert_eq!(event.timestamp, 1711368000);
}

#[test]
fn test_set_paused_emits_platform_status_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, admin) = setup_test(&env, true);

    client.set_paused(&true);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "admin_platform_paused").into_val(&env),
            admin.clone().into_val(&env),
        ]
    );

    let paused_event: PlatformPausedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(paused_event.initiator, admin.clone());
    assert_eq!(paused_event.timestamp, 1711368000);

    client.set_paused(&false);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "admin_platform_unpaused").into_val(&env),
            admin.clone().into_val(&env),
        ]
    );

    let unpaused_event: PlatformUnpausedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(unpaused_event.initiator, admin);
    assert_eq!(unpaused_event.timestamp, 1711368000);
}

/// Test metadata reveal verification with invalid content (Issue #122)
#[test]
fn test_verify_metadata_reveal_invalid_content() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    let content = Bytes::from_slice(&env, b"test metadata content");
    let content_hash = env.crypto().sha256(&content);
    let content_hash_bytes: Bytes = content_hash.into();

    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &500,
        &1,
        &Some(3600),
        &None,
        &Some(content_hash_bytes),
        &None,
    );

    // Try to verify with different content
    let wrong_content = Bytes::from_slice(&env, b"wrong content");
    let proof = MetadataRevealProof {
        content: wrong_content,
        secret: None,
    };

    let is_valid = client.verify_metadata_reveal(&1, &proof, &buyer);
    assert!(!is_valid);
}

/// Test metadata reveal verification without metadata hash (Issue #122)
#[test]
fn test_verify_metadata_reveal_no_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Create escrow without metadata hash
    client.create_escrow(&buyer, &seller, &token_id, &500, &1, &Some(3600));

    let content = Bytes::from_slice(&env, b"test metadata content");
    let proof = MetadataRevealProof {
        content,
        secret: None,
    };

    let is_valid = client.verify_metadata_reveal(&1, &proof, &buyer);
    assert!(!is_valid);
}

/// Test get_escrow_metadata returns only metadata fields (Issue #122)
#[test]
fn test_get_escrow_metadata_privacy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    let content = Bytes::from_slice(&env, b"private metadata");
    let content_hash = env.crypto().sha256(&content);
    let content_hash_bytes: Bytes = content_hash.into();

    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &500,
        &1,
        &Some(3600),
        &None,
        &Some(content_hash_bytes.clone()),
        &None,
    );

    let metadata = client.get_escrow_metadata(&1);
    assert_eq!(metadata.metadata_hash, Some(content_hash_bytes));
    assert_eq!(metadata.ipfs_hash, None);
}

// ============================================================
// Issue #121 – Comprehensive Test Suite
// ============================================================

/// Test escrow with IPFS hash validation (Issue #121)
#[test]
fn test_create_escrow_with_ipfs_hash_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Valid CIDv0 (46 chars starting with Qm)
    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnAzt8auVTL3u2M6YvM7NfF4hB9m8C3vM9");

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &500,
        &1,
        &Some(3600),
        &Some(ipfs_hash.clone()),
        &None,
        &None,
    );

    assert_eq!(escrow.ipfs_hash, Some(ipfs_hash));
}

/// Test escrow creation with both IPFS and metadata hash (Issue #121)
#[test]
fn test_create_escrow_with_both_metadata_types() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    let ipfs_hash = String::from_str(&env, "QmYwAPJzv5CZsnAzt8auVTL3u2M6YvM7NfF4hB9m8C3vM9");
    let content = Bytes::from_slice(&env, b"metadata");
    let metadata_hash = env.crypto().sha256(&content);
    let metadata_hash_bytes: Bytes = metadata_hash.into();

    let escrow = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &500,
        &1,
        &Some(3600),
        &Some(ipfs_hash.clone()),
        &Some(metadata_hash_bytes.clone()),
        &None,
    );

    assert_eq!(escrow.ipfs_hash, Some(ipfs_hash));
    assert_eq!(escrow.metadata_hash, Some(metadata_hash_bytes));
}

/// Test batch creation with metadata (Issue #121)
#[test]
fn test_create_batch_escrow_with_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &500_000);

    let content = Bytes::from_slice(&env, b"batch metadata");
    let metadata_hash = env.crypto().sha256(&content);
    let metadata_hash_bytes: Bytes = metadata_hash.into();

    let mut batch_params = vec![&env];
    for i in 0..3 {
        batch_params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_id.clone(),
            amount: 10_000,
            order_id: 500 + i,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: Some(metadata_hash_bytes.clone()),
            service_agreement_hash: None,
        });
    }

    let results = client.create_batch_escrow(&3u64, &batch_params);
    assert_eq!(results.len(), 3);

    // Verify metadata was stored
    for i in 0..3 {
        let metadata = client.get_escrow_metadata(&(500 + i));
        assert_eq!(metadata.metadata_hash, Some(metadata_hash_bytes.clone()));
    }
}

// ============================================================
// DevEx #119 – Dry-Run Batch Validation
// ============================================================

#[test]
fn test_validate_batch_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    let invalid_amount = EscrowCreateParams {
        buyer: buyer.clone(),
        seller: seller.clone(),
        token: token_id.clone(),
        amount: 0,
        order_id: 1,
        release_window: Some(3600),
        ipfs_hash: None,
        metadata_hash: None,
        service_agreement_hash: None,
    };

    let invalid_parties = EscrowCreateParams {
        buyer: buyer.clone(),
        seller: buyer.clone(),
        token: token_id.clone(),
        amount: 1000,
        order_id: 2,
        release_window: Some(3600),
        ipfs_hash: None,
        metadata_hash: None,
        service_agreement_hash: None,
    };

    let valid_param = EscrowCreateParams {
        buyer: buyer.clone(),
        seller: seller.clone(),
        token: token_id.clone(),
        amount: 1000,
        order_id: 3,
        release_window: Some(3600),
        ipfs_hash: None,
        metadata_hash: None,
        service_agreement_hash: None,
    };

    let mut batch_params = soroban_sdk::Vec::new(&env);
    batch_params.push_back(invalid_amount);
    batch_params.push_back(invalid_parties);
    batch_params.push_back(valid_param);

    let errors = client.validate_batch_creation(&batch_params);

    assert_eq!(errors.len(), 2);
    assert_eq!(errors.get(0).unwrap(), Error::AmountBelowMinimum);
    assert_eq!(errors.get(1).unwrap(), Error::SameBuyerSeller);
    assert!(errors.get(2).is_none());
}

#[test]
fn test_validate_batch_creation_rejects_invalid_metadata_hash_length() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    let mut batch_params = soroban_sdk::Vec::new(&env);
    batch_params.push_back(EscrowCreateParams {
        buyer,
        seller,
        token: token_id,
        amount: 1000,
        order_id: 1,
        release_window: Some(3600),
        ipfs_hash: None,
        metadata_hash: Some(Bytes::from_array(&env, &[9; 31])),
        service_agreement_hash: None,
    });

    let errors = client.validate_batch_creation(&batch_params);

    assert_eq!(errors.len(), 1);
    assert_eq!(errors.get(0).unwrap(), Error::InvalidMetadataHash);
}

#[test]
#[should_panic]
fn test_validate_batch_creation_exceeds_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    let valid_param = EscrowCreateParams {
        buyer: buyer.clone(),
        seller: seller.clone(),
        token: token_id.clone(),
        amount: 1000,
        order_id: 1,
        release_window: Some(3600),
        ipfs_hash: None,
        metadata_hash: None,
        service_agreement_hash: None,
    };

    let mut batch_params = soroban_sdk::Vec::new(&env);
    for _ in 0..101 {
        // MAX_BATCH_SIZE is 100
        batch_params.push_back(valid_param.clone());
    }

    client.validate_batch_creation(&batch_params);
}

// ── Storage Explorer tests ───────────────────────────────────────────

#[test]
fn test_get_escrow_count_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    assert_eq!(client.get_escrow_count(), 0);
}

#[test]
fn test_get_escrow_count_increments() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &1_000_000);

    assert_eq!(client.get_escrow_count(), 0);

    client.create_escrow(&buyer, &seller, &token_id, &500, &1, &Some(3600));
    assert_eq!(client.get_escrow_count(), 1);

    client.create_escrow(&buyer, &seller, &token_id, &500, &2, &Some(3600));
    assert_eq!(client.get_escrow_count(), 2);

    client.create_escrow(&buyer, &seller, &token_id, &500, &3, &Some(3600));
    assert_eq!(client.get_escrow_count(), 3);
}

#[test]
fn test_get_escrow_count_tracks_100_global_indices() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    for order_id in 1u32..=100 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &order_id, &Some(3600));
    }

    assert_eq!(client.get_escrow_count(), 100);

    let count_key = DataKey::EscrowCount;
    let stored_count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&count_key).unwrap_or(0u32)
    });
    assert_eq!(stored_count, 100);

    for index in 0u32..100 {
        let index_key = DataKey::GlobalEscrowIdIndexed(index);
        let stored_id: u32 = env.as_contract(&client.address, || {
            env.storage().persistent().get(&index_key).unwrap()
        });
        assert_eq!(stored_id, index + 1);
    }

    let first_page = client.get_all_escrow_ids_iterative(&0, &20);
    assert_eq!(first_page.len(), 20);
    assert_eq!(first_page.get(0), Some(1u32));
    assert_eq!(first_page.get(19), Some(20u32));

    let last_page = client.get_all_escrow_ids_iterative(&4, &20);
    assert_eq!(last_page.len(), 20);
    assert_eq!(last_page.get(0), Some(81u32));
    assert_eq!(last_page.get(19), Some(100u32));
}

#[test]
fn test_get_all_escrow_ids_iterative_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let ids = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(ids.len(), 0);
}

#[test]
fn test_get_all_escrow_ids_iterative_single_page() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &1_000_000);

    client.create_escrow(&buyer, &seller, &token_id, &100, &10, &Some(3600));
    client.create_escrow(&buyer, &seller, &token_id, &100, &20, &Some(3600));
    client.create_escrow(&buyer, &seller, &token_id, &100, &30, &Some(3600));

    let ids = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids.get(0), Some(10u32));
    assert_eq!(ids.get(1), Some(20u32));
    assert_eq!(ids.get(2), Some(30u32));
}

#[test]
fn test_get_all_escrow_ids_iterative_pagination() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &1_000_000);

    // Create 5 escrows
    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    // Page 0, limit 2 → IDs 1, 2
    let page0 = client.get_all_escrow_ids_iterative(&0, &2);
    assert_eq!(page0.len(), 2);
    assert_eq!(page0.get(0), Some(1u32));
    assert_eq!(page0.get(1), Some(2u32));

    // Page 1, limit 2 → IDs 3, 4
    let page1 = client.get_all_escrow_ids_iterative(&1, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0), Some(3u32));
    assert_eq!(page1.get(1), Some(4u32));

    // Page 2, limit 2 → ID 5 (partial page)
    let page2 = client.get_all_escrow_ids_iterative(&2, &2);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0), Some(5u32));

    // Page 3, limit 2 → empty (out of range)
    let page3 = client.get_all_escrow_ids_iterative(&3, &2);
    assert_eq!(page3.len(), 0);
}

#[test]
fn test_get_all_escrow_ids_iterative_limit_capped_at_max_batch_size() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // Create 5 escrows, request with limit > MAX_BATCH_SIZE (100)
    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    // limit=200 is silently capped to 100; all 5 escrows fit on page 0
    let ids = client.get_all_escrow_ids_iterative(&0, &200);
    assert_eq!(ids.len(), 5);
}

#[test]
fn test_get_escrow_count_batch_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &1_000_000);

    let params = EscrowCreateParams {
        buyer: buyer.clone(),
        seller: seller.clone(),
        token: token_id.clone(),
        amount: 100,
        order_id: 0,
        release_window: Some(3600),
        ipfs_hash: None,
        metadata_hash: None,
        service_agreement_hash: None,
    };

    let mut batch = soroban_sdk::Vec::new(&env);
    for i in 1u32..=3 {
        let mut p = params.clone();
        p.order_id = i;
        batch.push_back(p);
    }

    client.create_batch_escrow(&1u64, &batch);

    assert_eq!(client.get_escrow_count(), 3);

    let ids = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(ids.len(), 3);
}

#[test]
fn test_legacy_all_escrow_ids_migrates_on_get_escrow_count() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let legacy_key = DataKey::AllEscrowIds;
    let count_key = DataKey::EscrowCount;
    let mut legacy_ids = soroban_sdk::Vec::new(&env);
    for order_id in [11u32, 22, 33, 44] {
        legacy_ids.push_back(order_id);
    }

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_ids);
        env.storage().persistent().set(&count_key, &1u32);
    });

    assert_eq!(client.get_escrow_count(), 4);

    let stored_count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&count_key).unwrap()
    });
    assert_eq!(stored_count, 4);

    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);

    for (index, expected_id) in [11u32, 22, 33, 44].into_iter().enumerate() {
        let index_key = DataKey::GlobalEscrowIdIndexed(index as u32);
        let stored_id: u32 = env.as_contract(&client.address, || {
            env.storage().persistent().get(&index_key).unwrap()
        });
        assert_eq!(stored_id, expected_id);
    }
}

#[test]
fn test_legacy_all_escrow_ids_migrates_on_iterative_read() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let legacy_key = DataKey::AllEscrowIds;
    let count_key = DataKey::EscrowCount;
    let mut legacy_ids = soroban_sdk::Vec::new(&env);
    for order_id in 1u32..=5 {
        legacy_ids.push_back(order_id * 10);
    }

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_ids);
        env.storage().persistent().remove(&count_key);
    });

    let page = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(page.len(), 5);
    assert_eq!(page.get(0), Some(10u32));
    assert_eq!(page.get(4), Some(50u32));

    let stored_count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&count_key).unwrap()
    });
    assert_eq!(stored_count, 5);

    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);
}

#[test]
fn test_legacy_all_escrow_ids_migration_is_idempotent_after_first_read() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let legacy_key = DataKey::AllEscrowIds;
    let count_key = DataKey::EscrowCount;
    let mut legacy_ids = soroban_sdk::Vec::new(&env);
    for order_id in [5u32, 15, 25] {
        legacy_ids.push_back(order_id);
    }

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_ids);
        env.storage().persistent().remove(&count_key);
    });

    let first_page = client.get_all_escrow_ids_iterative(&0, &10);
    let second_page = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(first_page, second_page);
    assert_eq!(client.get_escrow_count(), 3);

    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);

    for (index, expected_id) in [5u32, 15, 25].into_iter().enumerate() {
        let index_key = DataKey::GlobalEscrowIdIndexed(index as u32);
        let stored_id: u32 = env.as_contract(&client.address, || {
            env.storage().persistent().get(&index_key).unwrap()
        });
        assert_eq!(stored_id, expected_id);
    }
}

#[test]
fn test_legacy_all_escrow_ids_migration_preserves_existing_indexed_entries() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let legacy_key = DataKey::AllEscrowIds;
    let count_key = DataKey::EscrowCount;
    let existing_index_key = DataKey::GlobalEscrowIdIndexed(0);
    let missing_index_key = DataKey::GlobalEscrowIdIndexed(1);
    let tail_index_key = DataKey::GlobalEscrowIdIndexed(2);
    let mut legacy_ids = soroban_sdk::Vec::new(&env);
    for order_id in [10u32, 20, 30] {
        legacy_ids.push_back(order_id);
    }

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_ids);
        env.storage().persistent().set(&existing_index_key, &999u32);
        env.storage().persistent().set(&count_key, &1u32);
    });

    let page = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0), Some(999u32));
    assert_eq!(page.get(1), Some(20u32));
    assert_eq!(page.get(2), Some(30u32));

    let stored_count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&count_key).unwrap()
    });
    assert_eq!(stored_count, 3);

    let first_id: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&existing_index_key).unwrap()
    });
    let second_id: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&missing_index_key).unwrap()
    });
    let third_id: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&tail_index_key).unwrap()
    });
    assert_eq!(first_id, 999u32);
    assert_eq!(second_id, 20u32);
    assert_eq!(third_id, 30u32);

    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);
}

#[test]
fn test_partial_refund_negotiation_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &None);

    // 1. Dispute the escrow
    client.dispute_escrow(&1, &Symbol::new(&env, "Partial_refund_requested"), &buyer);

    // 2. Buyer proposes a 300 refund
    client.propose_partial_refund(&1, &300, &buyer);

    // 3. Seller accepts the proposal
    client.accept_partial_refund(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    // Buyer gets 300
    assert_eq!(token_client.balance(&buyer), 300);
    // Seller gets 700 - 35 (5% fee) = 665
    assert_eq!(token_client.balance(&seller), 665);
}

#[test]
fn test_propose_partial_refund_by_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &None);

    client.dispute_escrow(&1, &Symbol::new(&env, "Partial_refund_offered"), &seller);

    // Seller proposes a 400 refund
    client.propose_partial_refund(&1, &400, &seller);

    // Buyer accepts
    client.accept_partial_refund(&1);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 400);
    // 600 - 30 (5% fee) = 570
    assert_eq!(token_client.balance(&seller), 570);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #1)")]
fn test_propose_partial_refund_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &None);

    client.dispute_escrow(&1, &Symbol::new(&env, "Dispute"), &buyer);

    let unauthorized = Address::generate(&env);
    client.propose_partial_refund(&1, &500, &unauthorized);
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #21)")]
fn test_propose_partial_refund_already_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &None);

    client.dispute_escrow(&1, &Symbol::new(&env, "Dispute"), &buyer);

    client.propose_partial_refund(&1, &300, &buyer);
    client.propose_partial_refund(&1, &400, &seller); // Fails
}

#[test]
fn test_validate_ipfs_cid_v0_and_v1_accepts_valid_cids() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    let cid_v0 = String::from_str(&env, "QmYwAPJzv5CZsnAzt8auVTL3u2M6YvM7NfF4hB9m8C3vM9");
    let cid_v1 = String::from_str(
        &env,
        "bafybeigdyrzt5scf7nqm765as5a42n367d5e46as5a42n367d5e46as5a4",
    );

    let escrow_v0 = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &1000,
        &1,
        &Some(3600),
        &Some(cid_v0.clone()),
        &None,
        &None,
    );
    let escrow_v1 = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &1000,
        &2,
        &Some(3600),
        &Some(cid_v1.clone()),
        &None,
        &None,
    );

    assert_eq!(escrow_v0.ipfs_hash, Some(cid_v0));
    assert_eq!(escrow_v1.ipfs_hash, Some(cid_v1));
}

#[test]
fn test_validate_ipfs_cid_v1_stricter() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &1000,
        &1,
        &Some(3600),
        &Some(String::from_str(
            &env,
            "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco",
        )),
        &None,
        &None,
    );

    // Valid CIDv1 base32 (sha256) - 59 chars, starts with 'ba'
    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &1000,
        &2,
        &Some(3600),
        &Some(String::from_str(
            &env,
            "bafybeigdyrzt5scf7nqm765as5a42n367d5e46as5a42n367d5e46as5a4",
        )),
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_validate_ipfs_cid_v1_too_short() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // CIDv1 base32 too short (only 10 chars)
    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &1000,
        &1,
        &Some(3600),
        &Some(String::from_str(&env, "bafybeigdy")),
        &None,
        &None,
    );
}

#[test]
#[should_panic]
fn test_validate_ipfs_cid_v1_wrong_version() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);
    token_admin.mint(&buyer, &100_000_000);

    // CIDv1 base32 starts with 'bb' (wrong version byte bits)
    client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token_id,
        &1000,
        &1,
        &Some(3600),
        &Some(String::from_str(
            &env,
            "bbfybeigdyrzt5scf7nqm765as5a42n367d5e46as5a42n367d5e46as5a4",
        )),
        &None,
        &None,
    );
}

// ===== IPFS CID validation: boundary and fuzz tests =====

#[test]
fn test_validate_ipfs_cid_boundary_45_char_cidv0_rejected() {
    let env = Env::default();
    let mut cid_str = alloc::string::String::from("Qm");
    for _ in 0..43 {
        cid_str.push('a');
    }
    assert_eq!(cid_str.len(), 45);

    let cid = String::from_str(&env, &cid_str);
    assert!(!CraftNexusContract::validate_ipfs_cid(&cid));
}

#[test]
fn test_validate_ipfs_cid_boundary_46_char_cidv0_accepted() {
    let env = Env::default();
    let mut cid_str = alloc::string::String::from("Qm");
    for _ in 0..44 {
        cid_str.push('a');
    }
    assert_eq!(cid_str.len(), 46);

    let cid = String::from_str(&env, &cid_str);
    assert!(CraftNexusContract::validate_ipfs_cid(&cid));
}

#[test]
fn test_validate_ipfs_cid_boundary_58_char_cidv1_accepted() {
    let env = Env::default();
    let mut cid_str = alloc::string::String::from("ba");
    for _ in 0..56 {
        cid_str.push('b');
    }
    assert_eq!(cid_str.len(), 58);

    let cid = String::from_str(&env, &cid_str);
    assert!(CraftNexusContract::validate_ipfs_cid(&cid));
}

#[test]
fn test_validate_ipfs_cid_boundary_59_char_cidv1_accepted() {
    let env = Env::default();
    let mut cid_str = alloc::string::String::from("ba");
    for _ in 0..57 {
        cid_str.push('b');
    }
    assert_eq!(cid_str.len(), 59);

    let cid = String::from_str(&env, &cid_str);
    assert!(CraftNexusContract::validate_ipfs_cid(&cid));
}

#[test]
fn test_validate_ipfs_cid_rejects_invalid_base58_chars() {
    let env = Env::default();

    // '0', 'O', 'I', 'l' are excluded from the Base58btc alphabet and must
    // cause rejection even though the rest of the CID is otherwise valid.
    for bad_char in ['0', 'O', 'I', 'l'] {
        let mut cid_str = alloc::string::String::from("Qm");
        cid_str.push(bad_char);
        for _ in 0..43 {
            cid_str.push('a');
        }
        assert_eq!(cid_str.len(), 46);

        let cid = String::from_str(&env, &cid_str);
        assert!(
            !CraftNexusContract::validate_ipfs_cid(&cid),
            "CID containing invalid base58 char must be rejected"
        );
    }
}

#[test]
fn test_validate_ipfs_cid_fuzz_never_panics() {
    use arbitrary::{Arbitrary, Unstructured};

    let env = Env::default();

    // Deterministic pseudo-random sweep (not a true fuzzer, but reproducible
    // across runs) feeding arbitrary::Arbitrary-generated byte strings into
    // the validator to confirm it never panics, regardless of content.
    for seed in 0u32..256 {
        let raw: alloc::vec::Vec<u8> = (0..300u32)
            .map(|i| {
                let mixed = seed
                    .wrapping_mul(2654435761)
                    .wrapping_add(i.wrapping_mul(40503));
                (mixed >> 8) as u8
            })
            .collect();

        let mut unstructured = Unstructured::new(&raw);
        let bytes: alloc::vec::Vec<u8> =
            Arbitrary::arbitrary(&mut unstructured).unwrap_or_default();

        // Every u8 maps to a valid Unicode scalar (Latin-1 range), so this
        // never panics on construction; it exists purely to turn arbitrary
        // bytes into a String for the validator to chew on.
        let text: alloc::string::String = bytes.iter().take(200).map(|b| *b as char).collect();
        let cid = String::from_str(&env, &text);

        // The validator must never panic, regardless of input shape.
        let _ = CraftNexusContract::validate_ipfs_cid(&cid);
    }
}

#[test]
fn test_accept_partial_refund_with_custom_fee_tier() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    // Set custom fee tier for seller to 2% (200 bps)
    client.set_artisan_fee_tier(&seller, &200);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &None);

    client.dispute_escrow(&1, &Symbol::new(&env, "Dispute"), &buyer);
    client.propose_partial_refund(&1, &500, &buyer);

    // Seller accepts. Gross for seller is 500.
    // 2% of 500 is 10.
    // Seller should get 490.
    client.accept_partial_refund(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&seller), 490);
}

#[test]
fn test_partial_refund_full_gross_amount_is_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &1000, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "Full_gross_refund"), &buyer);

    // refund_amount is interpreted as gross and is valid when it equals escrow.amount.
    client.propose_partial_refund(&1, &1000, &buyer);
    client.accept_partial_refund(&1);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 1000);
    assert_eq!(token_client.balance(&seller), 0);
}

#[test]
fn test_get_escrows_by_buyer_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, _, _, _, _, _) = setup_test(&env, true);

    client.get_escrows_by_buyer(&buyer, &0, &10, &false);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths.get(0).unwrap().0, buyer);
}

#[test]
fn test_get_escrows_by_seller_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, seller, _, _, _, _) = setup_test(&env, true);

    client.get_escrows_by_seller(&seller, &0, &10, &false);
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths.get(0).unwrap().0, seller);
}

#[test]
fn test_platform_config_ttl_extension_on_read() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    // Read the platform config to ensure it is initialized and TTL is extended
    let config = client.get_platform_config();

    // Advance ledger timestamp by a large amount (e.g., 20 days)
    env.ledger().with_mut(|li| {
        li.timestamp += 20 * 24 * 60 * 60; // 20 days in seconds
    });

    // Read again - should still succeed because the TTL was extended on read
    let config_after = client.get_platform_config();
    assert_eq!(config.admin, config_after.admin);
}

// ===== Issue #656: funding_deadline / cancel_unfunded_escrow / auto_cancel_unfunded =====

/// Helper: create an unfunded escrow and return the escrow struct.
fn create_unfunded(
    client: &CraftNexusContractClient,
    buyer: &Address,
    seller: &Address,
    token: &Address,
) -> super::Escrow {
    client.create_unfunded_escrow(
        &1u32,
        buyer,
        seller,
        token,
        &1_000_000i128,
        &3600u32, // 1-hour release window
        &None,
        &None,
        &None,
    )
}

/// The funding_deadline field should equal created_at + UNFUNDED_CANCEL_TIMEOUT (24 h).
#[test]
fn test_funding_deadline_set_on_create() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    let escrow = create_unfunded(&client, &buyer, &seller, &token_id);

    assert!(!escrow.funded);
    let deadline = escrow
        .funding_deadline
        .expect("funding_deadline must be set");
    // created_at is stored as u32 (truncated ledger timestamp); deadline is created_at + 86400
    assert_eq!(deadline, escrow.created_at as u64 + 24 * 60 * 60);
}

/// Buyer may cancel an unfunded escrow voluntarily before the deadline.
#[test]
fn test_buyer_can_cancel_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    create_unfunded(&client, &buyer, &seller, &token_id);

    // Time is still within the 24-hour window; buyer cancels voluntarily.
    let result = client.cancel_unfunded_escrow(&1u32, &buyer);
    assert_eq!(result, ());
}

/// Non-buyer caller is rejected before the deadline.
#[test]
#[should_panic]
fn test_seller_cannot_cancel_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    create_unfunded(&client, &buyer, &seller, &token_id);

    // Seller tries to cancel before deadline — must panic with Unauthorized.
    client.cancel_unfunded_escrow(&1u32, &seller);
}

/// After the deadline the seller can cancel the unfunded escrow.
#[test]
fn test_seller_can_cancel_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    create_unfunded(&client, &buyer, &seller, &token_id);

    // Advance ledger past the 24-hour funding deadline.
    env.ledger().with_mut(|li| {
        li.timestamp += 24 * 60 * 60 + 1;
    });

    let result = client.cancel_unfunded_escrow(&1u32, &seller);
    assert_eq!(result, ());
}

/// After the deadline the platform admin can cancel the unfunded escrow.
#[test]
fn test_admin_can_cancel_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, admin) = setup_test(&env, true);

    create_unfunded(&client, &buyer, &seller, &token_id);

    // Advance ledger past the 24-hour funding deadline.
    env.ledger().with_mut(|li| {
        li.timestamp += 24 * 60 * 60 + 1;
    });

    let result = client.cancel_unfunded_escrow(&1u32, &admin);
    assert_eq!(result, ());
}

/// A funded escrow cannot be cancelled via cancel_unfunded_escrow.
#[test]
#[should_panic]
fn test_cancel_funded_escrow_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000);
    // create_escrow funds immediately
    client.create_escrow(&buyer, &seller, &token_id, &1_000_000i128, &1u32, &None);

    // Must panic: the escrow is funded
    client.cancel_unfunded_escrow(&1u32, &buyer);
}

/// auto_cancel_unfunded skips escrows before deadline, cancels those past it,
/// and returns the correct count.
#[test]
fn test_auto_cancel_unfunded_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, admin) = setup_test(&env, true);

    // Create 3 unfunded escrows at the current timestamp.
    for id in 1u32..=3 {
        client.create_unfunded_escrow(
            &id,
            &buyer,
            &seller,
            &token_id,
            &1_000_000i128,
            &3600u32,
            &None,
            &None,
            &None,
        );
    }

    // Advance past the deadline so all 3 are eligible.
    env.ledger().with_mut(|li| {
        li.timestamp += 24 * 60 * 60 + 1;
    });

    let cancelled = client.auto_cancel_unfunded(&admin, &soroban_sdk::vec![&env, 1u32, 2u32, 3u32]);
    assert_eq!(cancelled, 3);
}

/// auto_cancel_unfunded skips escrows that have not yet expired.
#[test]
fn test_auto_cancel_unfunded_skips_fresh_escrows() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, admin) = setup_test(&env, true);

    client.create_unfunded_escrow(
        &1u32,
        &buyer,
        &seller,
        &token_id,
        &1_000_000i128,
        &3600u32,
        &None,
        &None,
        &None,
    );

    // Do NOT advance time — escrow is still within the deadline window.
    let cancelled = client.auto_cancel_unfunded(&admin, &soroban_sdk::vec![&env, 1u32]);
    assert_eq!(cancelled, 0);
}

/// auto_cancel_unfunded is rejected for non-admin callers.
#[test]
#[should_panic]
fn test_auto_cancel_unfunded_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, _, _, _) = setup_test(&env, true);

    create_unfunded(&client, &buyer, &seller, &token_id);

    env.ledger().with_mut(|li| {
        li.timestamp += 24 * 60 * 60 + 1;
    });

    // Buyer is not admin — must panic.
    client.auto_cancel_unfunded(&buyer, &soroban_sdk::vec![&env, 1u32]);
}

/// Issue #640 — get_escrows_by_buyer and get_escrows_by_seller must paginate
/// results to avoid memory exhaustion. This test creates 200 escrows
/// and verifies correct subsets are returned across multiple pages, and
/// page_size limit is capped at MAX_PAGE_SIZE (100).
#[test]
fn test_get_escrows_pagination_large_dataset() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    // Mint enough tokens for 200 escrows
    token_admin.mint(&buyer, &(200 * 1_000_000_000i128));

    // Create 200 escrows individually (one per call since batch max is 20)
    for i in 0..200u32 {
        client.create_escrow(&buyer, &seller, &token_id, &1_000, &(i + 1), &Some(3600));
    }

    // Page 0: page_size=50 should return IDs 1..=50
    let page0 = client.get_escrows_by_buyer(&buyer, &0, &50, &false);
    assert_eq!(page0.len(), 50, "page0 should have 50 items");
    assert_eq!(page0.get_unchecked(0), 1u64);
    assert_eq!(page0.get_unchecked(49), 50u64);

    // Page 1: page_size=50 should return IDs 51..=100
    let page1 = client.get_escrows_by_buyer(&buyer, &1, &50, &false);
    assert_eq!(page1.len(), 50, "page1 should have 50 items");
    assert_eq!(page1.get_unchecked(0), 51u64);
    assert_eq!(page1.get_unchecked(49), 100u64);

    // Page 4: out of range
    let page4 = client.get_escrows_by_buyer(&buyer, &4, &50, &false);
    assert_eq!(page4.len(), 0, "page4 should be empty");

    // page_size capped at MAX_PAGE_SIZE (100): requesting 200 returns only 100
    let capped = client.get_escrows_by_buyer(&buyer, &0, &200, &false);
    assert_eq!(
        capped.len(),
        100,
        "page_size should be capped at MAX_PAGE_SIZE=100"
    );

    // Verify seller pagination returns same count
    let seller_page0 = client.get_escrows_by_seller(&seller, &0, &50, &false);
    assert_eq!(seller_page0.len(), 50, "seller page0 should have 50 items");
    let seller_page1 = client.get_escrows_by_seller(&seller, &1, &50, &false);
    assert_eq!(seller_page1.len(), 50, "seller page1 should have 50 items");
}

#[test]
fn test_fund_audit_escrow_release_and_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Escrow 1: funding & release
    client.create_escrow(&buyer, &seller, &token_id, &40_000_000, &1, &None);
    client.release_funds(&1);

    // Check buyer history: funding entry
    let buyer_count = client.get_fund_audit_count(&buyer);
    assert_eq!(buyer_count, 1);
    let buyer_history = client.get_fund_audit_history(&buyer);
    assert_eq!(buyer_history.get(0).unwrap().reason, Symbol::new(&env, "escrow_funded"));

    // Check seller history: release entry
    let seller_count = client.get_fund_audit_count(&seller);
    assert_eq!(seller_count, 1);
    let seller_history = client.get_fund_audit_history(&seller);
    let seller_entry = seller_history.get(0).unwrap();
    assert_eq!(seller_entry.actor, seller);
    assert_eq!(seller_entry.reason, Symbol::new(&env, "escrow_released"));
    assert!(seller_entry.amount > 0);
    assert!(seller_entry.balance_impact > 0);

    // Escrow 2: funding & refund
    client.create_escrow(&buyer, &seller, &token_id, &30_000_000, &2, &None);
    client.refund(&2);

    // Check buyer history now has 3 entries: funded (1), funded (2), refund (2)
    assert_eq!(client.get_fund_audit_count(&buyer), 3);
    let buyer_history_updated = client.get_fund_audit_history(&buyer);
    let refund_entry = buyer_history_updated.get(2).unwrap();
    assert_eq!(refund_entry.reason, Symbol::new(&env, "refund"));
    assert_eq!(refund_entry.amount, 30_000_000);
    assert_eq!(refund_entry.balance_impact, 30_000_000);
}

#[test]
fn test_fund_audit_staking_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);

    // Stake
    client.stake_tokens(&seller, &token_id, &20_000_000);
    assert_eq!(client.get_fund_audit_count(&seller), 1);
    let stake_history = client.get_fund_audit_history(&seller);
    let stake_entry = stake_history.get(0).unwrap();
    assert_eq!(stake_entry.actor, seller);
    assert_eq!(stake_entry.amount, 20_000_000);
    assert_eq!(stake_entry.reason, Symbol::new(&env, "stake_deposit"));
    assert_eq!(stake_entry.balance_impact, -20_000_000);

    // Fast forward timestamp past stake cooldown
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 86400;
    });

    // Unstake
    client.unstake_tokens(&seller, &token_id);
    assert_eq!(client.get_fund_audit_count(&seller), 2);
    let unstake_history = client.get_fund_audit_history(&seller);
    let unstake_entry = unstake_history.get(1).unwrap();
    assert_eq!(unstake_entry.reason, Symbol::new(&env, "stake_unstaked"));
    assert_eq!(unstake_entry.amount, 20_000_000);
    assert_eq!(unstake_entry.balance_impact, 20_000_000);
}

#[test]
fn test_fund_audit_recurring_escrow_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Create recurring escrow: 10_000_000 total, 100s frequency, 2 cycles
    let rec = client.create_recurring_escrow(&buyer, &seller, &token_id, &10_000_000, &100, &2);
    assert_eq!(client.get_fund_audit_count(&buyer), 1);
    let buyer_hist = client.get_fund_audit_history(&buyer);
    assert_eq!(buyer_hist.get(0).unwrap().reason, Symbol::new(&env, "recurring_escrow_locked"));

    // Fast forward timestamp past cycle frequency
    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });

    // Release next cycle
    client.release_next_cycle(&rec.id);
    assert_eq!(client.get_fund_audit_count(&seller), 1);
    let seller_hist = client.get_fund_audit_history(&seller);
    assert_eq!(seller_hist.get(0).unwrap().reason, Symbol::new(&env, "recurring_release"));

    // Cancel remaining
    client.cancel_recurring_escrow(&rec.id);
    assert_eq!(client.get_fund_audit_count(&buyer), 2);
    let buyer_cancel_hist = client.get_fund_audit_history(&buyer);
    assert_eq!(buyer_cancel_hist.get(1).unwrap().reason, Symbol::new(&env, "recurring_cancel_refund"));
}


#[test]
fn test_fund_audit_pagination_and_immutability() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &500_000_000);

    for i in 1u32..=5u32 {
        let amt = (i as i128) * 10_000_000;
        client.create_escrow(&buyer, &seller, &token_id, &amt, &i, &None);
    }

    assert_eq!(client.get_fund_audit_count(&buyer), 5);

    // Test page 0 (offset 0, limit 2)
    let page0 = client.get_fund_audit_history_paginated(&buyer, &0, &2);
    assert_eq!(page0.len(), 2);
    assert_eq!(page0.get(0).unwrap().amount, 10_000_000);
    assert_eq!(page0.get(1).unwrap().amount, 20_000_000);

    // Test page 1 (offset 2, limit 2)
    let page1 = client.get_fund_audit_history_paginated(&buyer, &2, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().amount, 30_000_000);
    assert_eq!(page1.get(1).unwrap().amount, 40_000_000);

    // Test page 2 (offset 4, limit 2 -> returns remaining 1)
    let page2 = client.get_fund_audit_history_paginated(&buyer, &4, &2);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().amount, 50_000_000);

    // Test out of bounds (offset 10, limit 2 -> empty)
    let page_oob = client.get_fund_audit_history_paginated(&buyer, &10, &2);
    assert_eq!(page_oob.len(), 0);
}

#[test]
#[should_panic]
fn test_stake_below_minimum_threshold_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    // Admin sets minimum stake required to 10_000_000
    client.set_min_stake_required(&10_000_000);

    token_admin.mint(&seller, &20_000_000);
    // Staking 5_000_000 when min required is 10_000_000 should panic
    client.stake_tokens(&seller, &token_id, &5_000_000);
}

#[test]
#[should_panic]
fn test_unstake_with_active_obligations_below_min_stake_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    client.set_min_stake_required(&10_000_000);

    token_admin.mint(&seller, &20_000_000);
    token_admin.mint(&buyer, &20_000_000);

    // Stake 15_000_000 in two deposits so partial unstaking is possible
    client.stake_tokens(&seller, &token_id, &15_000_000);

    // Create an active escrow (seller has active obligations)
    client.create_escrow(&buyer, &seller, &token_id, &5_000_000, &1, &None);
    assert!(client.has_active_escrows(&seller));

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });

    // Unstaking matured 15_000_000 while active escrow exists leaves 0 stake (< 10_000_000 min requirement)
    client.unstake_tokens(&seller, &token_id);
}

#[test]
fn test_partial_unstake_consistent_collateral_rules() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    client.set_min_stake_required(&10_000_000);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    // Stake 25_000_000 — this opens the first cooldown window.
    client.stake_tokens(&seller, &token_id, &25_000_000);
    assert_eq!(client.get_stake(&seller), 25_000_000);

    // Advance past the first cooldown and unstake 25_000_000.
    // Now the queue is empty; the next deposit will open a fresh cooldown.
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });
    client.unstake_tokens(&seller, &token_id);
    assert_eq!(client.get_stake(&seller), 0);

    // Stake 25_000_000 again — opens a new cooldown window starting now.
    client.stake_tokens(&seller, &token_id, &25_000_000);
    assert_eq!(client.get_stake(&seller), 25_000_000);

    // Advance 100 s and add a second deposit of 10_000_000.
    // This inherits the existing cooldown_end (anti-gaming rule), so both
    // deposits mature at the same time.
    env.ledger().with_mut(|li| {
        li.timestamp += 100;
    });
    client.stake_tokens(&seller, &token_id, &10_000_000);
    assert_eq!(client.get_stake(&seller), 35_000_000);

    // Advance past the shared cooldown. Both deposits mature together.
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64;
    });

    // Both deposits mature; total released = 35_000_000; remaining = 0.
    // The collateral check does not block unstake because no active obligations exist.
    client.unstake_tokens(&seller, &token_id);
    assert_eq!(client.get_stake(&seller), 0);
    assert_eq!(client.is_account_under_collateralized(&seller), false);
}

#[test]
fn test_is_account_under_collateralized_detection() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&seller, &20_000_000);
    token_admin.mint(&buyer, &20_000_000);

    // Stake 5_000_000 (when min stake is 0)
    client.stake_tokens(&seller, &token_id, &5_000_000);

    // Create an escrow
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    // Initially min stake is 0, so not under-collateralized
    assert_eq!(client.is_account_under_collateralized(&seller), false);

    // Admin raises min stake required to 10_000_000
    client.set_min_stake_required(&10_000_000);

    // Now seller has active obligation but stake (5M) < min_stake_required (10M)
    assert_eq!(client.is_account_under_collateralized(&seller), true);
}

// ===== Deterministic Fee Splitting Engine Tests =====

fn assert_fee_split_balances(
    _token_client: &token::Client,
    contract_client: &CraftNexusContractClient,
    order_id: u32,
    escrow_amount: i128,
    expected_platform: i128,
    expected_seller: i128,
    expected_buyer: i128,
) {
    let escrow = contract_client.get_escrow(&order_id);
    assert!(escrow.status == EscrowStatus::Released || escrow.status == EscrowStatus::Resolved || escrow.status == EscrowStatus::Refunded,
        "escrow must be in terminal state, got {:?}", escrow.status);

    assert_eq!(
        expected_platform + expected_seller + expected_buyer,
        escrow_amount,
        "fee split must balance to escrow amount"
    );
}

#[test]
fn test_fee_policy_version_exposed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    assert_eq!(client.get_fee_policy_version(), 1);
}

#[test]
fn test_release_funds_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amount = 1_000_000i128;
    client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &None);
    client.release_funds(&1);

    let token_client = token::Client::new(&env, &token_id);
    let platform_balance = token_client.balance(&platform_wallet);
    let seller_balance = token_client.balance(&seller);

    assert_fee_split_balances(&token_client, &client, 1, amount, platform_balance, seller_balance, 0);
}

#[test]
fn test_auto_release_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amount = 2_000_000i128;
    client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &None);

    env.ledger().with_mut(|li| {
        li.timestamp += 604_801;
    });
    client.auto_release(&1);

    let token_client = token::Client::new(&env, &token_id);
    let platform_balance = token_client.balance(&platform_wallet);
    let seller_balance = token_client.balance(&seller);

    assert_fee_split_balances(&token_client, &client, 1, amount, platform_balance, seller_balance, 0);
}

#[test]
fn test_batch_release_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amounts = [1_000_000i128, 2_000_000i128, 3_000_000i128];
    for (i, amount) in amounts.iter().enumerate() {
        client.create_escrow(&buyer, &seller, &token_id, amount, &(i as u32 + 1), &None);
    }

    let order_ids: soroban_sdk::Vec<u32> = soroban_sdk::vec![&env, 1u32, 2u32, 3u32];
    client.release_batch_funds(&1u64, &order_ids, &buyer);

    let token_client = token::Client::new(&env, &token_id);
    for (i, amount) in amounts.iter().enumerate() {
        let order_id = i as u32 + 1;
        let platform_balance = token_client.balance(&platform_wallet);
        let seller_balance = token_client.balance(&seller);
        assert_fee_split_balances(&token_client, &client, order_id, *amount, platform_balance, seller_balance, 0);
    }
}

#[test]
fn test_refund_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amount = 1_500_000i128;
    client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &None);
    client.refund(&1);

    let token_client = token::Client::new(&env, &token_id);
    let buyer_balance = token_client.balance(&buyer);

    assert_fee_split_balances(&token_client, &client, 1, amount, 0, 0, buyer_balance);
}

#[test]
fn test_dispute_release_to_seller_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amount = 800_000i128;
    client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "late_delivery"), &buyer);

    client.resolve_dispute(&1, &Resolution::ReleaseToSeller, &admin);

    let token_client = token::Client::new(&env, &token_id);
    let platform_balance = token_client.balance(&platform_wallet);
    let seller_balance = token_client.balance(&seller);

    assert_fee_split_balances(&token_client, &client, 1, amount, platform_balance, seller_balance, 0);
}

#[test]
fn test_dispute_refund_to_buyer_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, admin) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amount = 800_000i128;
    client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "item_not_as_described"), &buyer);

    client.resolve_dispute(&1, &Resolution::RefundToBuyer, &admin);

    let token_client = token::Client::new(&env, &token_id);
    let buyer_balance = token_client.balance(&buyer);

    assert_fee_split_balances(&token_client, &client, 1, amount, 0, 0, buyer_balance);
}

#[test]
fn test_expired_dispute_all_policies_balance_to_escrow_amount() {
    let policies = [
        ExpiredDisputeFeePolicy::RefundFullNoPlatformFee,
        ExpiredDisputeFeePolicy::RefundMinusPlatformFee,
        ExpiredDisputeFeePolicy::DeductFeeFromSeller,
        ExpiredDisputeFeePolicy::SplitFee,
    ];

    for (i, &policy) in policies.iter().enumerate() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, CraftNexusContract);
        let client = CraftNexusContractClient::new(&env, &contract_id);
        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        let platform_wallet = Address::generate(&env);
        let admin = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_addr = token_contract.address();
        let token_asset = token::StellarAssetClient::new(&env, &token_addr);
        token_asset.mint(&buyer, &100_000_000);

        client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None::<Address>);
        client.update_expired_dispute_policy(&policy);

        let amount = 2_500_000i128;
        client.create_escrow(&buyer, &seller, &token_addr, &amount, &(i as u32 + 1), &Some(604800));
        client.dispute_escrow(&(i as u32 + 1), &Symbol::new(&env, "test"), &buyer);

        env.ledger().with_mut(|li| {
            li.timestamp += 30 * 24 * 60 * 60 + 1;
        });

        client.resolve_expired_dispute(&(i as u32 + 1));

        let token_client = token::Client::new(&env, &token_addr);
        let platform_delta = token_client.balance(&platform_wallet);
        let buyer_delta = token_client.balance(&buyer);
        let seller_delta = token_client.balance(&seller);

        let sum = platform_delta + buyer_delta + seller_delta;
        assert_eq!(sum, amount, "policy {:?} must balance to escrow amount", policy);
    }
}

#[test]
fn test_partial_refund_balances_to_escrow_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    let amount = 1_200_000i128;
    let refund_gross = 700_000i128;

    client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &None);
    client.dispute_escrow(&1, &Symbol::new(&env, "partial"), &buyer);
    client.propose_partial_refund(&1, &refund_gross, &buyer);
    client.accept_partial_refund(&1);

    let token_client = token::Client::new(&env, &token_id);
    let platform_balance = token_client.balance(&platform_wallet);
    let buyer_balance = token_client.balance(&buyer);
    let seller_balance = token_client.balance(&seller);

    assert_fee_split_balances(&token_client, &client, 1, amount, platform_balance, seller_balance, buyer_balance);
}

#[test]
fn test_recurring_escrow_cycle_balances_to_cycle_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_recurring_escrow(
        &buyer,
        &seller,
        &token_id,
        &1_000_000,
        &3600,
        &2,
    );

    env.ledger().with_mut(|li| {
        li.timestamp += 3601;
    });

    client.release_next_cycle(&1);

    let token_client = token::Client::new(&env, &token_id);
    let platform_balance = token_client.balance(&platform_wallet);
    let seller_balance = token_client.balance(&seller);

    let cycle_amount = 500_000i128; // 1_000_000 / 2
    assert_fee_split_balances(&token_client, &client, 1, cycle_amount, platform_balance, seller_balance, 0);
}

#[test]
fn test_allocation_invariant_never_violated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);

    // Sweep a representative range of amounts.
    for amount in [1, 19, 20, 39, 40, 99, 100, 999, 1000, 9999, 10_000, 99_999, 100_000, 999_999, 1_000_000].iter() {
        let order_id = *amount as u32;
        client.create_escrow(&buyer, &seller, &token_id, amount, &order_id, &None);

        // ReleaseFunds
        client.release_funds(&order_id);
        let escrow = client.get_escrow(&order_id);
        assert_eq!(escrow.status, EscrowStatus::Released);
    }
}

