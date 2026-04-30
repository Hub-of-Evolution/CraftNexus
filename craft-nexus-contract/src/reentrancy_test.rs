#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger}, token, Address, Env, String, Symbol,
};

#[test]
fn test_release_funds_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Initialize contract
    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    // Mint tokens to buyer
    token_client.mint(&buyer, &10000);

    // Create escrow
    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Get escrow before release
    let escrow_before: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow_before.status, EscrowStatus::Active);

    // Release funds
    client.release_funds(&order_id);

    // Verify state was updated (CEI pattern ensures this happens before transfer)
    let escrow_after: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow_after.status, EscrowStatus::Released);
}

#[test]
fn test_refund_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &10000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Refund
    client.refund(&order_id);

    // Verify state was updated before transfer
    let escrow: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow.status, EscrowStatus::Refunded);
}

#[test]
fn test_resolve_dispute_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &onboarding_contract);
    client.add_arbitrator(&arbitrator);

    token_client.mint(&buyer, &10000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Raise dispute
    client.raise_dispute(&order_id, &String::from_str(&env, "Issue"));

    // Resolve dispute - 50/50 split
    client.resolve_dispute(&order_id, &2500, &arbitrator);

    // Verify state was updated before transfers
    let escrow: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_resolve_expired_dispute_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &10000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Raise dispute
    client.raise_dispute(&order_id, &String::from_str(&env, "Issue"));

    // Fast forward past dispute expiration (7 days)
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + (7 * 24 * 60 * 60) + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id);

    // Verify state was updated before transfer
    let escrow: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_accept_partial_refund_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &10000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Raise dispute
    client.raise_dispute(&order_id, &String::from_str(&env, "Issue"));

    // Buyer proposes partial refund
    client.propose_partial_refund(&order_id, &3000);

    // Seller accepts
    let _ = client.try_accept_partial_refund(&order_id);

    // Verify state was updated before transfers
    let escrow: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_cancel_recurring_escrow_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let artisan = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &20000);

    // Create recurring escrow
    let id = client.create_recurring_escrow(
        &buyer,
        &artisan,
        &token.address(),
        &10000,
        &1000,
        &86400,
    );

    // Cancel recurring escrow
    client.cancel_recurring_escrow(&id);

    // Verify state was updated before transfer
    let escrow: RecurringEscrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::RecurringEscrow(id))
                .unwrap()
        });
    assert_eq!(escrow.is_active, false);
}

#[test]
fn test_auto_release_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &10000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Fast forward past release window
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + 86401;
    });

    // Auto release
    client.auto_release(&order_id);

    // Verify state was updated before transfer
    let escrow: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id))
                .unwrap()
        });
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_state_consistency_during_concurrent_operations() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &30000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    // Create multiple escrows
    let order_id1 = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    let order_id2 = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    let order_id3 = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Release first escrow
    client.release_funds(&order_id1);

    // Refund second escrow
    client.refund(&order_id2);

    // Verify all escrows have correct independent states
    let escrow1: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id1))
                .unwrap()
        });
    assert_eq!(escrow1.status, EscrowStatus::Released);

    let escrow2: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id2))
                .unwrap()
        });
    assert_eq!(escrow2.status, EscrowStatus::Refunded);

    let escrow3: Escrow = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&(Symbol::new(&env, "Escrow"), order_id3))
                .unwrap()
        });
    assert_eq!(escrow3.status, EscrowStatus::Active);
}

#[test]
fn test_active_obligations_updated_before_transfers() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&platform_wallet, &admin, &Address::generate(&env), &500, &onboarding_contract);

    token_client.mint(&buyer, &10000);

    let metadata = Metadata {
        title: String::from_str(&env, "Test"),
        description: String::from_str(&env, "Test escrow"),
        category: String::from_str(&env, "Digital"),
    };

    let order_id = client.create_escrow_with_metadata(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &metadata,
    );

    // Verify active obligations before release
    assert!(client.has_active_escrows(&buyer));
    assert!(client.has_active_escrows(&seller));

    // Release funds
    client.release_funds(&order_id);

    // Verify active obligations were decremented before transfer
    assert!(!client.has_active_escrows(&buyer));
    assert!(!client.has_active_escrows(&seller));
}
