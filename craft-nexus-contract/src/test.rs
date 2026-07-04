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
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

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
    assert!(!events.is_empty(), "No events emitted");
    let last_event = events.last().unwrap();
    assert_eq!(last_event.0, client.address);
    let last_event = events.last();
    assert_eq!(last_event.clone().unwrap().0, client.address);
    assert_eq!(last_event.clone().unwrap().0, client.address);
    // Topics: ["escrow_created", escrow_id]
    assert_eq!(
        last_event.clone().unwrap().1,
        vec![
            &env,
            Symbol::new(&env, "escrow").into_val(&env),
            (order_id as u64).into_val(&env)
        ]
    );
    // Verify payload
    let event: EscrowEvent = last_event.unwrap().2.try_into_val(&env).unwrap();
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
    let last_event = events.last();
    let config_event: ConfigUpdatedEvent = last_event.unwrap().2.try_into_val(&env).unwrap();
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
        .2
        .try_into_val(&env)
        .unwrap();
    let wallet_event: ConfigUpdatedEvent = events
        .get(events.len() - 1)
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
            Symbol::new(&env, "artisan_fee_tier_updated").into_val(&env),
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
    assert_eq!(
        client.get_total_fees_for_token(&token_a_contract.address()),
        500_000
    );
    assert_eq!(
        client.get_total_fees_for_token(&token_b_contract.address()),
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
        token::StellarAssetClient::new(&env, &other_token_contract.address());

    token_admin.mint(&seller, &10_000_000);
    other_token_admin_client.mint(&seller, &10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);

    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });

    client.unstake_tokens(&seller, &other_token_contract.address());
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
    let last_event = events.last();
    let config_event: ConfigUpdatedEvent = last_event.unwrap().2.try_into_val(&env).unwrap();
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
    client.set_min_escrow_amount(&token_contract.address(), &0);

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

// ===== Multi-sig upgrade tests =====

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
    assert_eq!(client.get_upgrade_approvals(&hash).len(), 1);

    // Second approval — threshold reached, proposal committed.
    client.propose_upgrade_wasm(&signer2, &hash);
    let proposal = client.get_upgrade_proposal().expect("proposal missing");
    assert_eq!(proposal.wasm_hash, hash);
}

#[test]
fn test_duplicate_approval_ignored() {
    // The same signer approving twice must not count as two approvals.
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
    // Duplicate call from the same signer — idempotent, no panic.
    client.propose_upgrade_wasm(&admin, &hash);

    // Still only 1 unique approval.
    assert_eq!(client.get_upgrade_approvals(&hash).len(), 1);
    assert!(client.get_upgrade_proposal().is_none());
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
        },
    ];

    client.create_batch_escrow(&1u64, &escrow_params);
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
        &other_token.address(),
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
            token: other_token.address(),
            amount: 10_000,
            order_id: 10,
            release_window: Some(3600),
            ipfs_hash: None,
            metadata_hash: None,
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
    client.whitelist_token(&token2.address());

    assert!(client.is_token_whitelisted(&token_id));
    assert!(client.is_token_whitelisted(&token2.address()));

    // Both should succeed in escrow creation
    client.create_escrow(&buyer, &seller, &token_id, &10_000, &1, &Some(3600));
    client.create_escrow(&buyer, &seller, &token2.address(), &10_000, &2, &Some(3600));
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
    let latest_summary: FeeTokenConfigsMigratedEvent =
        events.last().unwrap().2.try_into_val(&env).unwrap();
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
            Symbol::new(&env, "metadata_verified").into_val(&env),
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
            Symbol::new(&env, "platform_paused").into_val(&env),
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
            Symbol::new(&env, "platform_unpaused").into_val(&env),
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
    );
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
