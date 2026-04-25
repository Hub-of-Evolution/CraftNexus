#![cfg(test)]

use crate::{
    CraftNexusContract, CraftNexusContractClient, EscrowStatus, ExpiredDisputeFeePolicy,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

const DEFAULT_MAX_DISPUTE_DURATION: u32 = 30 * 24 * 60 * 60; // 30 days

/// Helper function to setup test environment
fn setup_test() -> (
    Env,
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);

    // Deploy token contract
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &token_id.address());
    let token_addr = token_id.address();

    // Mint tokens to buyer
    token.mint(&buyer, &10_000_000);

    // Deploy mock onboarding contract
    let onboarding_contract = Address::generate(&env);

    // Initialize the escrow contract
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500, // 5% platform fee
        &onboarding_contract,
    );

    (
        env,
        client,
        buyer,
        seller,
        token_addr,
        admin,
        platform_wallet,
        arbitrator,
        onboarding_contract,
    )
}

/// Helper to create and dispute an escrow
fn create_and_dispute_escrow(
    client: &CraftNexusContractClient,
    buyer: &Address,
    seller: &Address,
    token: &Address,
    amount: i128,
    order_id: u32,
) {
    client.create_escrow(buyer, seller, token, &amount, &order_id, &Some(604800));
    client.initiate_dispute(buyer, &order_id, &Some(soroban_sdk::String::from_str(
        &client.env,
        "Test dispute",
    )));
}

#[test]
fn test_default_policy_is_refund_full_no_fee() {
    let (_, client, _, _, _, _, _, _, _) = setup_test();

    let policy = client.get_expired_dispute_policy();
    assert_eq!(policy, ExpiredDisputeFeePolicy::RefundFullNoPlatformFee);
}

#[test]
fn test_update_expired_dispute_policy() {
    let (_, client, _, _, _, admin, _, _, _) = setup_test();

    // Update to RefundMinusPlatformFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::RefundMinusPlatformFee)
        .unwrap();

    let policy = client.get_expired_dispute_policy();
    assert_eq!(policy, ExpiredDisputeFeePolicy::RefundMinusPlatformFee);

    // Update to SplitFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee)
        .unwrap();

    let policy = client.get_expired_dispute_policy();
    assert_eq!(policy, ExpiredDisputeFeePolicy::SplitFee);
}

#[test]
fn test_policy_refund_full_no_platform_fee() {
    let (env, client, buyer, seller, token_addr, _, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    let amount = 1_000_000i128;
    let order_id = 1u32;

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    let buyer_balance_before = token.balance(&buyer);
    let platform_balance_before = token.balance(&platform_wallet);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve expired dispute with default policy (RefundFullNoPlatformFee)
    client.resolve_expired_dispute(&order_id).unwrap();

    // Buyer should receive full amount
    assert_eq!(token.balance(&buyer), buyer_balance_before + amount);

    // Platform should receive no fee
    assert_eq!(token.balance(&platform_wallet), platform_balance_before);

    // Escrow should be resolved
    let escrow = client.get_escrow(&order_id).unwrap();
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_policy_refund_minus_platform_fee() {
    let (env, client, buyer, seller, token_addr, admin, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    let amount = 1_000_000i128;
    let order_id = 1u32;
    let expected_fee = 50_000i128; // 5% of 1,000,000

    // Update policy
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::RefundMinusPlatformFee)
        .unwrap();

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    let buyer_balance_before = token.balance(&buyer);
    let platform_balance_before = token.balance(&platform_wallet);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id).unwrap();

    // Buyer should receive amount minus platform fee
    assert_eq!(
        token.balance(&buyer),
        buyer_balance_before + amount - expected_fee
    );

    // Platform should receive the fee
    assert_eq!(
        token.balance(&platform_wallet),
        platform_balance_before + expected_fee
    );

    // Total fees should be tracked
    assert_eq!(
        client.get_total_fees_for_token(&token_addr),
        expected_fee
    );
}

#[test]
fn test_policy_deduct_fee_from_seller() {
    let (env, client, buyer, seller, token_addr, admin, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    let amount = 1_000_000i128;
    let order_id = 1u32;

    // Update policy
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::DeductFeeFromSeller)
        .unwrap();

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    let buyer_balance_before = token.balance(&buyer);
    let platform_balance_before = token.balance(&platform_wallet);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id).unwrap();

    // Buyer should receive full amount
    assert_eq!(token.balance(&buyer), buyer_balance_before + amount);

    // Platform should NOT receive fee (conceptually deducted from seller)
    assert_eq!(token.balance(&platform_wallet), platform_balance_before);

    // No fees tracked (platform doesn't collect in this policy)
    assert_eq!(client.get_total_fees_for_token(&token_addr), 0);
}

#[test]
fn test_policy_split_fee() {
    let (env, client, buyer, seller, token_addr, admin, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    let amount = 1_000_000i128;
    let order_id = 1u32;
    let full_fee = 50_000i128; // 5% of 1,000,000
    let half_fee = full_fee / 2; // 25,000

    // Update policy
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee)
        .unwrap();

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    let buyer_balance_before = token.balance(&buyer);
    let platform_balance_before = token.balance(&platform_wallet);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id).unwrap();

    // Buyer should receive amount minus half the fee
    assert_eq!(
        token.balance(&buyer),
        buyer_balance_before + amount - half_fee
    );

    // Platform should receive half the fee
    assert_eq!(
        token.balance(&platform_wallet),
        platform_balance_before + half_fee
    );

    // Only half fee should be tracked
    assert_eq!(client.get_total_fees_for_token(&token_addr), half_fee);
}

#[test]
fn test_multiple_expired_disputes_with_different_policies() {
    let (env, client, buyer, seller, token_addr, admin, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    let amount = 1_000_000i128;
    let expected_fee = 50_000i128;

    // Create first escrow with default policy (RefundFullNoPlatformFee)
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, 1);

    // Change policy to RefundMinusPlatformFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::RefundMinusPlatformFee)
        .unwrap();

    // Create second escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, 2);

    // Change policy to SplitFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee)
        .unwrap();

    // Create third escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, 3);

    let buyer_balance_before = token.balance(&buyer);
    let platform_balance_before = token.balance(&platform_wallet);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve first escrow (RefundFullNoPlatformFee policy at creation)
    // Note: Policy is applied at resolution time, not creation time
    // So all three will use SplitFee policy
    client.resolve_expired_dispute(&1).unwrap();
    client.resolve_expired_dispute(&2).unwrap();
    client.resolve_expired_dispute(&3).unwrap();

    // All three use current policy (SplitFee)
    // Each escrow: buyer gets amount - half_fee, platform gets half_fee
    let half_fee = expected_fee / 2;
    let total_buyer_refund = 3 * (amount - half_fee);
    let total_platform_fee = 3 * half_fee;

    assert_eq!(
        token.balance(&buyer),
        buyer_balance_before + total_buyer_refund
    );
    assert_eq!(
        token.balance(&platform_wallet),
        platform_balance_before + total_platform_fee
    );
}

#[test]
fn test_expired_dispute_cannot_resolve_before_deadline() {
    let (env, client, buyer, seller, token_addr, _, _, _, _) = setup_test();

    let amount = 1_000_000i128;
    let order_id = 1u32;

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    // Try to resolve before deadline (should fail)
    let result = client.resolve_expired_dispute(&order_id);
    assert!(result.is_err());

    // Fast forward but not enough
    env.ledger().with_mut(|li| {
        li.timestamp += (DEFAULT_MAX_DISPUTE_DURATION / 2) as u64;
    });

    // Still should fail
    let result = client.resolve_expired_dispute(&order_id);
    assert!(result.is_err());
}

#[test]
fn test_expired_dispute_only_works_on_disputed_escrows() {
    let (env, client, buyer, seller, token_addr, _, _, _, _) = setup_test();

    let amount = 1_000_000i128;
    let order_id = 1u32;

    // Create escrow but don't dispute it
    client.create_escrow(&buyer, &seller, &token_addr, &amount, &order_id, &Some(604800));

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Try to resolve (should fail because not disputed)
    let result = client.resolve_expired_dispute(&order_id);
    assert!(result.is_err());
}

#[test]
fn test_policy_with_different_fee_percentages() {
    let (env, client, buyer, seller, token_addr, admin, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    // Update platform fee to 10% (1000 bps)
    client.update_platform_fee(&1000);

    // Update policy to RefundMinusPlatformFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::RefundMinusPlatformFee)
        .unwrap();

    let amount = 1_000_000i128;
    let order_id = 1u32;
    let expected_fee = 100_000i128; // 10% of 1,000,000

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    let buyer_balance_before = token.balance(&buyer);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id).unwrap();

    // Buyer should receive amount minus 10% fee
    assert_eq!(
        token.balance(&buyer),
        buyer_balance_before + amount - expected_fee
    );

    // Platform should receive 10% fee
    assert_eq!(token.balance(&platform_wallet), expected_fee);
}

#[test]
fn test_policy_with_small_amounts() {
    let (env, client, buyer, seller, token_addr, admin, platform_wallet, _, _) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    // Update policy to RefundMinusPlatformFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::RefundMinusPlatformFee)
        .unwrap();

    let amount = 100i128; // Small amount
    let order_id = 1u32;
    let expected_fee = 5i128; // 5% of 100

    // Create and dispute escrow
    create_and_dispute_escrow(&client, &buyer, &seller, &token_addr, amount, order_id);

    let buyer_balance_before = token.balance(&buyer);

    // Fast forward past dispute duration
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_MAX_DISPUTE_DURATION as u64 + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id).unwrap();

    // Buyer should receive amount minus fee
    assert_eq!(
        token.balance(&buyer),
        buyer_balance_before + amount - expected_fee
    );

    // Platform should receive the fee
    assert_eq!(token.balance(&platform_wallet), expected_fee);
}

#[test]
fn test_policy_persists_across_config_updates() {
    let (_, client, _, _, _, admin, _, _, _) = setup_test();

    // Set policy to SplitFee
    client
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee)
        .unwrap();

    // Update other config (platform fee)
    client.update_platform_fee(&600);

    // Policy should still be SplitFee
    let policy = client.get_expired_dispute_policy();
    assert_eq!(policy, ExpiredDisputeFeePolicy::SplitFee);

    // Update platform wallet
    let new_wallet = Address::generate(&client.env);
    client.update_platform_wallet(&new_wallet);

    // Policy should still be SplitFee
    let policy = client.get_expired_dispute_policy();
    assert_eq!(policy, ExpiredDisputeFeePolicy::SplitFee);
}
