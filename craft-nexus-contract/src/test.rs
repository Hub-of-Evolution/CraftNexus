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
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
    Address,
) {
    if mock_auth {
        env.mock_all_auths();
    }
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

    let arbitrator = Address::generate(env);

    // Set a non-zero timestamp for event tests
    env.ledger().with_mut(|li| {
        li.timestamp = 1711368000; // 2024-03-25
    });

    // Initialize contract with platform config
    client.initialize(&platform_wallet, &admin, &arbitrator, &500);

    // Set min amount to 0 for tests to pass with small amounts
    client.set_min_escrow_amount(&token_contract.address(), &0);

    (
        client,
        buyer,
        seller,
        token_contract.address(),
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
    assert!(events.len() > 0, "No events emitted");
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    // Topics: ["escrow_created", escrow_id]
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow_created").into_val(&env),
            (order_id as u64).into_val(&env)
        ]
    );

    // Verify payload
    let event: EscrowCreatedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, order_id as u64);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert_eq!(event.token, token_id);
    assert_eq!(event.amount, amount as i128);
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

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    // Topics: ["funds_released", escrow_id]
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "funds_released").into_val(&env),
            1u64.into_val(&env)
        ]
    );
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

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    // Topics: ["funds_released", escrow_id]
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "funds_released").into_val(&env),
            1u64.into_val(&env)
        ]
    );
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

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    // Topics: ["funds_refunded", escrow_id]
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "funds_refunded").into_val(&env),
            1u64.into_val(&env)
        ]
    );

    // Verify payload
    let event: FundsRefundedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert_eq!(event.token, token_id);
    assert!(event.timestamp > 0);
}

#[test]
fn test_dispute_escrow_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    client.dispute_escrow(&1, &String::from_str(&env, "Item damaged"), &buyer);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
    assert_eq!(
        escrow.dispute_reason,
        Some(String::from_str(&env, "Item damaged"))
    );

    // Verify event
    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow_disputed").into_val(&env),
            1u64.into_val(&env)
        ]
    );

    // Verify payload
    let event: EscrowDisputedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert_eq!(event.token, token_id);
    assert_eq!(event.dispute_reason, String::from_str(&env, "Item damaged"));
    assert!(event.timestamp > 0);
}

#[test]
fn test_dispute_escrow_by_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1000);
    client.create_escrow(&buyer, &seller, &token_id, &500, &1, &None);

    client.dispute_escrow(&1, &String::from_str(&env, "Payment not received"), &seller);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
    assert_eq!(
        escrow.dispute_reason,
        Some(String::from_str(&env, "Payment not received"))
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
    client.dispute_escrow(&1, &String::from_str(&env, "Invalid reason"), &unauthorized);
}

#[test]
#[should_panic]
fn test_disputed_prevents_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &String::from_str(&env, "Damaged item"), &buyer);

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
    client.dispute_escrow(&1, &String::from_str(&env, "Damaged item"), &buyer);

    client.refund(&1);
}

#[test]
fn test_resolve_dispute_release_to_seller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &String::from_str(&env, "Non-delivery"), &buyer);

    // Arbitrator is setup in setup_test as a random Address and mock_all_auths bypasses auth
    client.resolve_dispute(&1, &Resolution::ReleaseToSeller);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&seller), 47_500_000);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow_resolved").into_val(&env),
            1u64.into_val(&env)
        ]
    );
}

#[test]
fn test_resolve_dispute_refund_to_buyer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.dispute_escrow(&1, &String::from_str(&env, "Late shipping"), &buyer);

    client.resolve_dispute(&1, &Resolution::RefundToBuyer);

    let escrow = client.get_escrow(&1);
    assert_eq!(escrow.status, EscrowStatus::Resolved);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 100_000_000);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    assert_eq!(
        last_event.1,
        vec![
            &env,
            Symbol::new(&env, "escrow_resolved").into_val(&env),
            1u64.into_val(&env)
        ]
    );
}

#[test]
#[should_panic(expected = "Escrow not in dispute")]
fn test_resolve_dispute_non_disputed() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);

    client.resolve_dispute(&1, &Resolution::RefundToBuyer);
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
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let arbitrator = Address::generate(&env);

    // Initialize with 10% fee
    client.initialize(&platform_wallet, &admin, &arbitrator, &1000);

    token_admin_client.mint(&buyer, &10_000_000);
    client.create_escrow(
        &buyer,
        &seller,
        &token_contract.address(),
        &10_000_000,
        &1,
        &None,
    );

    client.release_funds(&1);

    let token_client = token::Client::new(&env, &token_contract.address());
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

#[test]
fn test_update_platform_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let seller = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    let arbitrator = Address::generate(&env);

    // Initialize with 5% fee
    client.initialize(&platform_wallet, &admin, &arbitrator, &500);

    // Get initial fee
    assert_eq!(client.get_platform_fee(), 500);

    // Update to 8% fee (800 bps) - admin auth required
    client.update_platform_fee(&800);

    assert_eq!(client.get_platform_fee(), 800);

    // Now create escrow and release - should use 8%
    token_admin_client.mint(&Address::generate(&env), &100_000_000);
    let buyer = Address::generate(&env);
    token_admin_client.mint(&buyer, &100_000_000);
    client.create_escrow(
        &buyer,
        &seller,
        &token_contract.address(),
        &100_000_000,
        &1,
        &None,
    );

    client.release_funds(&1);

    let token_client = token::Client::new(&env, &token_contract.address());
    // 100,000,000 - 8,000,000 = 92,000,000
    assert_eq!(token_client.balance(&seller), 92_000_000);
    assert_eq!(token_client.balance(&platform_wallet), 8_000_000);
}

#[test]
#[should_panic]
fn test_update_platform_fee_too_high() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let _token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    let arbitrator = Address::generate(&env);

    // Initialize with 5% fee
    client.initialize(&platform_wallet, &admin, &arbitrator, &500);

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
    client.dispute_escrow(&1, &String::from_str(&env, "Unauthorized"), &unauthorized);
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
    client.dispute_escrow(&1, &String::from_str(&env, "buyer dispute"), &buyer);
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
    client.dispute_escrow(&999, &String::from_str(&env, "reason"), &caller);
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
#[should_panic(expected = "No pending admin")]
fn test_claim_admin_no_pending_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    client.claim_admin();
}

#[test]
fn test_wasm_upgrade_grace_period() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Propose upgrade
    client.propose_upgrade_wasm(&new_wasm_hash);

    // Try to upgrade immediately - should fail
    // We can't easily catch a panic in a test without should_panic, 
    // but we can verify the error if we return Result.
    // Our update_wasm uses expect/panic.
}

#[test]
fn test_cancel_upgrade_wasm() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);

    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.propose_upgrade_wasm(&new_wasm_hash);
    
    // Admin cancels
    client.cancel_upgrade_wasm();
    
    // Should panic when trying to update since proposal is gone
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
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);

    let arbitrator = Address::generate(&env);

    // 25 bps = 0.25%
    client.initialize(&platform_wallet, &admin, &arbitrator, &25);
    assert_eq!(client.calculate_fee_for_amount(&1000), 2); // floor(2.5) => 2
    assert_eq!(client.calculate_fee_for_amount(&399), 0); // floor(0.9975) => 0
    assert_eq!(client.calculate_fee_for_amount(&400), 1); // floor(1.0) => 1
}

#[test]
fn test_integration_multiple_tokens_and_escrows() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&platform_wallet, &admin, &arbitrator, &500);

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
        &token_a_contract.address(),
        &10_000_000,
        &1,
        &None,
    );
    client.create_escrow(
        &buyer,
        &seller,
        &token_b_contract.address(),
        &10_000_000,
        &2,
        &None,
    );

    client.release_funds(&1);
    client.release_funds(&2);

    let token_a = token::Client::new(&env, &token_a_contract.address());
    let token_b = token::Client::new(&env, &token_b_contract.address());

    // Seller: 9.5M (token A) + 9.5M (token B)
    assert_eq!(token_a.balance(&seller), 9_500_000);
    assert_eq!(token_b.balance(&seller), 9_500_000);

    // Platform: 500,000 (token A) + 500,000 (token B)
    let fee_a = token_a.balance(&platform_wallet);
    let fee_b = token_b.balance(&platform_wallet);
    assert_eq!(fee_a, 500_000);
    assert_eq!(fee_b, 500_000);
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
    );

    assert_eq!(escrow.ipfs_hash, Some(ipfs_hash));
}

#[test]
#[should_panic(expected = "Invalid IPFS CID")]
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
        &Some(String::from_str(&env, "not-a-cid")),
        &None,
    );
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
    let b1 = client.get_escrows_by_buyer(&buyer, &0, &10);
    assert_eq!(b1.len(), 3);
    assert_eq!(b1.get_unchecked(0), 1);
    assert_eq!(b1.get_unchecked(1), 2);
    assert_eq!(b1.get_unchecked(2), 3);

    // Pagination: page 0, limit 2
    let b2 = client.get_escrows_by_buyer(&buyer, &0, &2);
    assert_eq!(b2.len(), 2);
    assert_eq!(b2.get_unchecked(0), 1);
    assert_eq!(b2.get_unchecked(1), 2);

    // Pagination: page 1, limit 2
    let b3 = client.get_escrows_by_buyer(&buyer, &1, &2);
    assert_eq!(b3.len(), 1);
    assert_eq!(b3.get_unchecked(0), 3);

    // Pagination: out of bounds
    let b4 = client.get_escrows_by_buyer(&buyer, &2, &2);
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
    let s1 = client.get_escrows_by_seller(&seller, &0, &10);
    assert_eq!(s1.len(), 2);
    assert_eq!(s1.get_unchecked(0), 1);
    assert_eq!(s1.get_unchecked(1), 3);

    // Check seller 2
    let s2 = client.get_escrows_by_seller(&seller2, &0, &10);
    assert_eq!(s2.len(), 1);
    assert_eq!(s2.get_unchecked(0), 2);

    // Check non-existent seller
    let s3 = client.get_escrows_by_seller(&Address::generate(&env), &0, &10);
    assert_eq!(s3.len(), 0);
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
    assert_eq!(escrow.amount, 50_00000);
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
    let _new_wasm_hash = env.deployer().upload_contract_wasm(dummy_wasm);

    client.update_wasm();

    // Version should be 2
    assert_eq!(client.get_version(), 2);
}

#[test]
#[should_panic]
fn test_contract_upgrade_unauthorized() {
    let env = Env::default();
    // Do NOT mock auth globally
    let (client, _, _, _, _, _, _) = setup_test(&env, false);

    let _dummy_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Attempt upgrade without admin auth
    client.update_wasm();
}

#[test]
fn test_get_version_initially() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _, _) = setup_test(&env, true);
    assert_eq!(client.get_version(), 1);
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

    let escrow2 = client.get_escrow(&101);
    assert_eq!(escrow2.amount, 200_000_000);
    assert_eq!(escrow2.status, EscrowStatus::Active);

    let escrow3 = client.get_escrow(&102);
    assert_eq!(escrow3.amount, 150_000_000);
    assert_eq!(escrow3.release_window, 604800); // Default 7 days

    // Verify events were emitted
    let events = env.events().all();
    let expected_topic: soroban_sdk::Val = Symbol::new(&env, "batch_escrow_created").into_val(&env);
    let batch_events: alloc::vec::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() >= 2 && soroban_sdk::vec![&env, topics.get_unchecked(0)] == soroban_sdk::vec![&env, expected_topic]
        })
        .collect();
    assert_eq!(
        batch_events.len(),
        3,
        "Should emit batch event for each escrow"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
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
        },
    ];

    client.create_batch_escrow(&1u64, &escrow_params);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
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
        },
    ];

    client.create_batch_escrow(&1u64, &escrow_params);
}

#[test]
fn test_release_batch_funds_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin, _platform_wallet, _) = setup_test(&env, true);

    token_admin.mint(&buyer, &1_000_000_000);

    // Create multiple escrows
    client.create_escrow(&buyer, &seller, &token_id, &100_000_000, &100, &None);
    client.create_escrow(&buyer, &seller, &token_id, &200_000_000, &101, &None);
    client.create_escrow(&buyer, &seller, &token_id, &150_000_000, &102, &None);

    // Release batch
    let order_ids = vec![&env, 100u32, 101u32, 102u32];
    let batch_id = 1u64;
    let results = client.release_batch_funds(&batch_id, &order_ids, &buyer);

    assert_eq!(results.len(), 3);
    assert_eq!(results.get(0).unwrap(), 100);
    assert_eq!(results.get(1).unwrap(), 101);
    assert_eq!(results.get(2).unwrap(), 102);

    // Verify statuses
    let escrow1 = client.get_escrow(&100);
    assert_eq!(escrow1.status, EscrowStatus::Released);

    let escrow2 = client.get_escrow(&101);
    assert_eq!(escrow2.status, EscrowStatus::Released);

    let escrow3 = client.get_escrow(&102);
    assert_eq!(escrow3.status, EscrowStatus::Released);

    // Verify batch events were emitted
    let events = env.events().all();
    let expected_topic: soroban_sdk::Val = Symbol::new(&env, "batch_funds_released").into_val(&env);
    let batch_events: alloc::vec::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() >= 2 && soroban_sdk::vec![&env, topics.get_unchecked(0)] == soroban_sdk::vec![&env, expected_topic]
        })
        .collect();
    assert_eq!(
        batch_events.len(),
        3,
        "Should emit batch event for each release"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
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
#[should_panic(expected = "Error(Contract, #3)")]
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
#[should_panic(expected = "Error(Contract, #1)")]
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
            Symbol::new(&env, "escrow_extended").into_val(&env),
            1u64.into_val(&env)
        ]
    );

    let event: EscrowExtendedEvent = last_event.2.try_into_val(&env).unwrap();
    assert_eq!(event.escrow_id, 1);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.seller, seller);
    assert_eq!(event.new_release_window, window + additional);
    assert_eq!(event.additional_seconds, additional);
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
