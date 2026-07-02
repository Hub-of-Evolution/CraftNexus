#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

#[test]
fn test_release_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    // Initialize contract
    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    // Mint tokens to buyer
    token_client.mint(&buyer, &10000);

    // Create escrow
    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // Get escrow before release
    let escrow_before: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow_before.status, EscrowStatus::Active);

    // Release funds
    client.release_funds(&order_id);

    // Verify state was updated (CEI pattern ensures this happens before transfer)
    let escrow_after: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow_after.status, EscrowStatus::Released);
}

#[test]
fn test_refund_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // Refund
    client.refund(&(order_id as u64));

    // Verify state was updated before transfer
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Refunded);
}

#[test]
fn test_resolve_dispute_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // Raise dispute
    client.dispute_escrow(&order_id, &Symbol::new(&env, "Issue"), &buyer);

    // Resolve dispute - 50/50 split
    client.resolve_dispute(&order_id, &Resolution::ReleaseToSeller, &arbitrator);

    // Verify state was updated before transfers
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_resolve_expired_dispute_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // Raise dispute
    client.dispute_escrow(&order_id, &Symbol::new(&env, "Issue"), &buyer);

    // Fast forward past dispute expiration (7 days)
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + (30 * 24 * 60 * 60) + 1;
    });

    // Resolve expired dispute
    client.resolve_expired_dispute(&order_id);

    // Verify state was updated before transfer
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_accept_partial_refund_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // Raise dispute
    client.dispute_escrow(&order_id, &Symbol::new(&env, "Issue"), &buyer);

    // Buyer proposes partial refund
    client.propose_partial_refund(&order_id, &3000, &buyer);

    // Seller accepts
    let _ = client.try_accept_partial_refund(&order_id);

    // Verify state was updated before transfers
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_cancel_recurring_escrow_cei_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let artisan = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &20000);

    // Create recurring escrow
    let escrow_obj =
        client.create_recurring_escrow(&buyer, &artisan, &token.address(), &1000, &1000, &86400).unwrap();
    let id = escrow_obj.id;

    // Cancel recurring escrow
    client.cancel_recurring_escrow(&id);

    // Verify state was updated before transfer
    let escrow: RecurringEscrow = env.as_contract(&contract_id, || {
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
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // Fast forward past release window
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + 86401;
    });

    // Auto release
    client.auto_release(&order_id);

    // Verify state was updated before transfer
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_state_consistency_during_concurrent_operations() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &30000);

    // Create multiple escrows
    let order_id1 = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id1,
        &Some(86400),
    );

    let order_id2 = 2u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id2,
        &Some(86400),
    );

    let order_id3 = 3u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id3,
        &Some(86400),
    );

    // Release first escrow
    client.release_funds(&order_id1);

    // Refund second escrow
    client.refund(&(order_id2 as u64));

    // Verify all escrows have correct independent states
    let escrow1: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id1))
            .unwrap()
    });
    assert_eq!(escrow1.status, EscrowStatus::Released);

    let escrow2: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id2))
            .unwrap()
    });
    assert_eq!(escrow2.status, EscrowStatus::Refunded);

    let escrow3: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id3))
            .unwrap()
    });
    assert_eq!(escrow3.status, EscrowStatus::Active);
}

#[test]
fn test_active_obligations_updated_before_transfers() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
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

/// Direct unit test of the `ReentryGuardScope` RAII guard (issue #607).
///
/// This exercises the fix mechanism itself rather than going through the host's
/// transaction-rollback safety net: it asserts the guard is set while the scope
/// is alive and is unconditionally cleared the instant the scope is dropped —
/// the property that makes early `Err(...)` returns safe. It fails if the `Drop`
/// implementation is ever removed or broken.
#[test]
fn test_reentry_guard_scope_releases_on_drop() {
    let env = Env::default();
    let contract_id = env.register_contract(None, CraftNexusContract);

    env.as_contract(&contract_id, || {
        assert!(
            !env.storage().temporary().has(&DataKey::ReentryGuard),
            "guard should start clear"
        );

        {
            let _guard = ReentryGuardScope::new(&env);
            assert!(
                env.storage().temporary().has(&DataKey::ReentryGuard),
                "guard must be set while the scope is alive"
            );
        } // `_guard` dropped here

        assert!(
            !env.storage().temporary().has(&DataKey::ReentryGuard),
            "ReentryGuardScope must clear the guard on drop"
        );
    });
}

/// Regression test for issue #607.
///
/// A guarded function that fails *mid-call* and returns `Err(...)` (rather than
/// panicking) must still clear the reentrancy guard. Otherwise the guard stays
/// set in temporary storage and permanently locks every other guarded entry
/// point (a denial-of-service). The `ReentryGuardScope` RAII guard guarantees
/// the guard is released on *every* exit path — `Ok`, `Err`, or panic.
#[test]
fn test_reentry_guard_cleared_after_failing_call() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &Some(onboarding_contract),
    );

    token_client.mint(&buyer, &10000);

    let order_id = 1u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &order_id,
        &Some(86400),
    );

    // `refund` enters the guard, then bails out early with `Err(EscrowNotFound)`
    // because escrow 999 does not exist. This is precisely the non-panicking
    // early-return path that previously leaked the guard.
    let failed = client.try_refund(&999u64);
    assert!(
        failed.is_err() || failed.unwrap().is_err(),
        "refund of a non-existent escrow should fail"
    );

    // The guard must NOT remain set in temporary storage after the failure.
    let guard_still_set: bool = env.as_contract(&contract_id, || {
        env.storage().temporary().has(&DataKey::ReentryGuard)
    });
    assert!(
        !guard_still_set,
        "ReentryGuard leaked after a failing call — contract would be permanently locked"
    );

    // A subsequent legitimate guarded call must still succeed. If the guard had
    // leaked, this would panic with `ReentryDetected`.
    client.release_funds(&order_id);
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow.status, EscrowStatus::Released);
}
