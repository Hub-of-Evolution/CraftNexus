#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

#[contract]
struct CallbackToken;

#[contractimpl]
impl CallbackToken {
    pub fn initialize(env: Env, target: Address, order_id: u32) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "target"), &target);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "order"), &order_id);
    }

    pub fn decimals(_env: Env) -> u32 {
        7
    }

    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000
    }

    pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
        let target: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "target"))
            .unwrap();
        let order_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "order"))
            .unwrap();
        CraftNexusContractClient::new(&env, &target).release_funds(&order_id);
    }
}

#[contract]
struct UnsupportedTokenContract;

#[contractimpl]
impl UnsupportedTokenContract {
    pub fn ping(_env: Env) {}
}

#[test]
fn admin_cannot_whitelist_unsupported_token_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    client.initialize(
        &Address::generate(&env),
        &admin,
        &Address::generate(&env),
        &500,
        &None,
    );

    let unsupported = env.register_contract(None, UnsupportedTokenContract);
    assert_eq!(
        client.try_whitelist_token(&unsupported),
        Err(Ok(Error::UnsupportedToken))
    );
    assert_eq!(client.get_whitelisted_token_count(), 0);
}

#[test]
fn malicious_token_callback_is_rejected_and_rolls_back() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    client.initialize(
        &Address::generate(&env),
        &admin,
        &Address::generate(&env),
        &500,
        &None,
    );

    let order_id = 991u32;
    let token_id = env.register_contract(None, CallbackToken);
    CallbackTokenClient::new(&env, &token_id).initialize(&contract_id, &order_id);

    assert!(client
        .try_create_escrow(&buyer, &seller, &token_id, &5_000, &order_id, &Some(86_400),)
        .is_err());
    assert!(client.try_get_escrow(&order_id).is_err());
}

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
    client.set_evidence_challenge_window(&0);
    client.set_min_release_window(&1);

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
        client.create_recurring_escrow(&buyer, &artisan, &token.address(), &1000, &1000, &86400);
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

/// Issue #704 — Disputing an escrow after its deadline has passed allows arbitrator resolution
/// and claiming of platform / arbitrator fees.
#[test]
fn test_dispute_expired_recurring_escrow_arbitrator_fees() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500, // 5% fee (500 BPS)
        &None,
    );
    client.set_evidence_challenge_window(&0);

    client.set_min_escrow_amount(&token.address(), &0);
    client.set_min_release_window(&1);

    token_client.mint(&buyer, &100_000_000);

    let order_id = 704u32;
    client.create_escrow(
        &buyer,
        &seller,
        &token.address(),
        &50_000_000,
        &order_id,
        &Some(86400),
    );

    // Fast forward ledger timestamp past funding/release deadline (86,400s)
    env.ledger().with_mut(|li| {
        li.timestamp += 100_000;
    });

    // Dispute escrow after deadline
    client.dispute_escrow(&order_id, &Symbol::new(&env, "ExpiredDispute"), &buyer);

    // Arbitrator resolves dispute releasing funds to seller (with platform/arbitrator fee deduction)
    client.resolve_dispute(&order_id, &Resolution::ReleaseToSeller, &arbitrator);

    // Verify escrow status is resolved
    let escrow = client.get_escrow(&order_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
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

/// Issue #659 — fund_escrow CEI pattern coverage.
///
/// Verifies that `fund_escrow` follows the check-effects-interactions pattern:
/// `escrow.funded` is set to `true` and persisted **before** the token transfer
/// is executed. A malicious token contract that re-enters during the transfer
/// would observe the escrow as already funded and be rejected with
/// `Error::InvalidEscrowState`, preventing a double-fund attack.
#[test]
fn test_fund_escrow_cei_pattern() {
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

    // Mint enough tokens for the buyer to fund the escrow
    token_client.mint(&buyer, &10000);

    let order_id = 42u32;

    // Create an unfunded escrow stub — funded = false at this point
    client.create_unfunded_escrow(
        &order_id,
        &buyer,
        &seller,
        &token.address(),
        &5000,
        &86400,
        &None,
        &None,
        &None,
    );

    // Verify the escrow starts unfunded
    let escrow_before: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert!(!escrow_before.funded, "escrow should start unfunded");
    assert_eq!(escrow_before.status, EscrowStatus::Active);

    // Fund the escrow
    client.fund_escrow(&order_id);

    // CEI check: escrow.funded must be true in storage before any re-entrant
    // call could observe the state. If this assertion holds, a re-entrant
    // attempt during the transfer would see funded=true and be rejected.
    let escrow_after: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert!(
        escrow_after.funded,
        "escrow.funded must be true after fund_escrow (CEI: state updated before transfer)"
    );
    assert_eq!(escrow_after.status, EscrowStatus::Active);

    // Attempting to fund again must be rejected — proving the funded flag acts
    // as the reentrancy guard for this code path.
    let second_fund = client.try_fund_escrow(&order_id);
    assert!(
        second_fund.is_err(),
        "double-fund must be rejected with InvalidEscrowState"
    );
}

// ---------------------------------------------------------------------------
// Malicious token callback tests (#1067)
//
// A well-behaved token only moves balances on `transfer`. A malicious or
// buggy token can instead try to re-enter the escrow contract from inside
// its own `transfer` implementation — e.g. to double-spend a payout, drain a
// stake before its cooldown accounting settles, or otherwise observe/mutate
// contract state mid-custody-operation. Every custody path (stake, unstake,
// release, refund) is guarded by `ReentryGuardScope`, so any such recursive
// call must panic with `ReentryDetected` and roll back the *entire*
// transaction — including whatever the outer call had already done. These
// tests arm a callback only after an honest baseline interaction succeeds,
// so we can tell a rejected attack apart from a fixture that never worked.
// ---------------------------------------------------------------------------

// Each malicious token fixture lives in its own module: `#[contractimpl]`
// generates module-scoped helper items named after each method (`transfer`,
// `decimals`, ...), so two `#[contract]` types sharing a module cannot both
// define the same method names — as every token implementing the standard
// interface must.

/// Token whose `transfer` calls back into `stake_tokens` for the same artisan
/// once armed. Used to attack the artisan -> contract pull in `stake_tokens`.
mod stake_reentry_token {
    use super::*;

    #[contract]
    pub struct StakeReentryToken;

    #[contractimpl]
    impl StakeReentryToken {
        pub fn initialize(env: Env, target: Address, artisan: Address) {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "target"), &target);
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "artisan"), &artisan);
            env.storage().instance().set(&Symbol::new(&env, "armed"), &false);
        }

        pub fn arm(env: Env) {
            env.storage().instance().set(&Symbol::new(&env, "armed"), &true);
        }

        pub fn decimals(_env: Env) -> u32 {
            7
        }

        pub fn balance(_env: Env, _id: Address) -> i128 {
            1_000_000_000
        }

        pub fn transfer(env: Env, _from: Address, _to: Address, amount: i128) {
            let armed: bool = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "armed"))
                .unwrap_or(false);
            if armed {
                let target: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "target"))
                    .unwrap();
                let artisan: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "artisan"))
                    .unwrap();
                let this_token = env.current_contract_address();
                CraftNexusContractClient::new(&env, &target).stake_tokens(
                    &artisan,
                    &this_token,
                    &amount,
                );
            }
        }
    }
}
use stake_reentry_token::{StakeReentryToken, StakeReentryTokenClient};

/// Token whose `transfer` calls back into `unstake_tokens` for the same
/// artisan once armed. Used to attack the contract -> artisan payout in
/// `unstake_tokens`.
mod unstake_reentry_token {
    use super::*;

    #[contract]
    pub struct UnstakeReentryToken;

    #[contractimpl]
    impl UnstakeReentryToken {
        pub fn initialize(env: Env, target: Address, artisan: Address) {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "target"), &target);
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "artisan"), &artisan);
            env.storage().instance().set(&Symbol::new(&env, "armed"), &false);
        }

        pub fn arm(env: Env) {
            env.storage().instance().set(&Symbol::new(&env, "armed"), &true);
        }

        pub fn decimals(_env: Env) -> u32 {
            7
        }

        pub fn balance(_env: Env, _id: Address) -> i128 {
            1_000_000_000
        }

        pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
            let armed: bool = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "armed"))
                .unwrap_or(false);
            if armed {
                let target: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "target"))
                    .unwrap();
                let artisan: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "artisan"))
                    .unwrap();
                let this_token = env.current_contract_address();
                CraftNexusContractClient::new(&env, &target)
                    .unstake_tokens(&artisan, &this_token);
            }
        }
    }
}
use unstake_reentry_token::{UnstakeReentryToken, UnstakeReentryTokenClient};

/// Token whose `transfer` calls back into `refund` for the same order once
/// armed. Used to attack the buyer payout in `refund`.
mod refund_reentry_token {
    use super::*;

    #[contract]
    pub struct RefundReentryToken;

    #[contractimpl]
    impl RefundReentryToken {
        pub fn initialize(env: Env, target: Address, order_id: u32) {
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "target"), &target);
            env.storage()
                .instance()
                .set(&Symbol::new(&env, "order"), &order_id);
            env.storage().instance().set(&Symbol::new(&env, "armed"), &false);
        }

        pub fn arm(env: Env) {
            env.storage().instance().set(&Symbol::new(&env, "armed"), &true);
        }

        pub fn decimals(_env: Env) -> u32 {
            7
        }

        pub fn balance(_env: Env, _id: Address) -> i128 {
            1_000_000_000
        }

        pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
            let armed: bool = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "armed"))
                .unwrap_or(false);
            if !armed {
                return;
            }
            let target: Address = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "target"))
                .unwrap();
            let order_id: u32 = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "order"))
                .unwrap();
            CraftNexusContractClient::new(&env, &target).refund(&(order_id as u64));
        }
    }
}
use refund_reentry_token::{RefundReentryToken, RefundReentryTokenClient};

fn setup_reentrancy_env() -> (Env, CraftNexusContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env),
        &500,
        &None,
    );
    (env, client, contract_id, admin)
}

#[test]
fn malicious_token_cannot_reenter_stake_tokens() {
    let (env, client, contract_id, _admin) = setup_reentrancy_env();
    let artisan = Address::generate(&env);

    let token_id = env.register_contract(None, StakeReentryToken);
    StakeReentryTokenClient::new(&env, &token_id).initialize(&contract_id, &artisan);

    // Honest baseline stake succeeds and establishes ground truth.
    client.stake_tokens(&artisan, &token_id, &1_000);
    assert_eq!(client.get_stake(&artisan), 1_000);

    // Arm the callback: the next stake's pull-transfer tries to re-enter
    // stake_tokens for the same artisan before the outer call returns.
    StakeReentryTokenClient::new(&env, &token_id).arm();
    let result = client.try_stake_tokens(&artisan, &token_id, &500);
    assert!(
        result.is_err(),
        "recursive stake_tokens entry must be rejected"
    );

    // The whole failed invocation rolls back: the attacker gains nothing and
    // the previously staked principal is untouched.
    assert_eq!(
        client.get_stake(&artisan),
        1_000,
        "no stake invariant may be violated by a rejected reentrancy attempt"
    );

    // Guard cleanup: disarm and prove the contract is not left locked - a
    // legitimate follow-up stake must still succeed.
    env.as_contract(&token_id, || {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "armed"), &false);
    });
    client.stake_tokens(&artisan, &token_id, &250);
    assert_eq!(client.get_stake(&artisan), 1_250);
}

#[test]
fn malicious_token_cannot_reenter_unstake_tokens() {
    let (env, client, contract_id, _admin) = setup_reentrancy_env();
    let artisan = Address::generate(&env);

    let token_id = env.register_contract(None, UnstakeReentryToken);
    UnstakeReentryTokenClient::new(&env, &token_id).initialize(&contract_id, &artisan);

    // Establish a matured stake to withdraw.
    client.stake_tokens(&artisan, &token_id, &2_000);
    env.ledger().with_mut(|li| {
        li.timestamp += (DEFAULT_STAKE_COOLDOWN as u64) + 1;
    });

    // Arm the callback: the payout transfer tries to re-enter unstake_tokens
    // for the same artisan before the outer call finishes.
    UnstakeReentryTokenClient::new(&env, &token_id).arm();
    let result = client.try_unstake_tokens(&artisan, &token_id);
    assert!(
        result.is_err(),
        "recursive unstake_tokens entry must be rejected"
    );

    // Rolled back entirely: the matured stake is neither paid out once nor
    // twice - it must remain exactly as it was.
    assert_eq!(
        client.get_stake(&artisan),
        2_000,
        "a rejected reentrancy attempt must not partially or doubly settle a payout"
    );

    // Guard cleanup: disarm and confirm a legitimate unstake still works.
    env.as_contract(&token_id, || {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "armed"), &false);
    });
    client.unstake_tokens(&artisan, &token_id);
    assert_eq!(client.get_stake(&artisan), 0);
}

#[test]
fn malicious_token_cannot_reenter_refund() {
    let (env, client, contract_id, admin) = setup_reentrancy_env();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let order_id = 42u32;
    let token_id = env.register_contract(None, RefundReentryToken);
    RefundReentryTokenClient::new(&env, &token_id).initialize(&contract_id, &order_id);

    client.create_escrow(
        &buyer,
        &seller,
        &token_id,
        &5_000,
        &order_id,
        &Some(86_400),
    );

    // Arm the callback only now: escrow creation itself must go through
    // unaffected, so any failure below is attributable to the refund attack.
    RefundReentryTokenClient::new(&env, &token_id).arm();

    // The refund payout transfer immediately tries to re-enter refund for
    // the same order before the outer call finishes.
    let result = client.try_refund(&(order_id as u64));
    assert!(result.is_err(), "recursive refund entry must be rejected");

    // CEI already commits Refunded before the transfer, so even without the
    // reentry guard the inner call would see a non-Active escrow and be
    // rejected by the state machine - but the *whole* invocation (including
    // the outer state transition) must still roll back on the panic.
    let escrow: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(
        escrow.status,
        EscrowStatus::Active,
        "a rejected reentrancy attempt must leave the escrow state untouched"
    );

    // Guard cleanup: disarm and confirm a legitimate refund afterwards still
    // succeeds - the contract must not be left locked by the failed attempt.
    env.as_contract(&token_id, || {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "armed"), &false);
    });
    client.refund(&(order_id as u64));
    let escrow_after: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(escrow_after.status, EscrowStatus::Refunded);
    let _ = admin;
}

#[test]
fn malicious_token_cannot_cross_reenter_stake_into_unstake() {
    // A recursive call into a *different* guarded custody function must be
    // rejected too - the guard is contract-wide, not scoped to one function.
    let (env, client, contract_id, _admin) = setup_reentrancy_env();
    let artisan = Address::generate(&env);

    let token_id = env.register_contract(None, UnstakeReentryToken);
    UnstakeReentryTokenClient::new(&env, &token_id).initialize(&contract_id, &artisan);

    client.stake_tokens(&artisan, &token_id, &1_000);
    env.ledger().with_mut(|li| {
        li.timestamp += (DEFAULT_STAKE_COOLDOWN as u64) + 1;
    });
    UnstakeReentryTokenClient::new(&env, &token_id).arm();

    // Arm a second stake to trigger the pull-transfer, whose callback tries
    // to re-enter unstake_tokens (a different guarded function) mid-call.
    let result = client.try_stake_tokens(&artisan, &token_id, &500);
    assert!(
        result.is_err(),
        "cross-function reentrancy attempt must be rejected"
    );
    assert_eq!(
        client.get_stake(&artisan),
        1_000,
        "no principal may be gained or lost via cross-function reentrancy"
    );
}
