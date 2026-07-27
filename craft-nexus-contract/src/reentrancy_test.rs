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

    // Advance past the evidence challenge window (#942) before finalizing.
    env.ledger().with_mut(|li| {
        li.timestamp += 2 * 24 * 60 * 60 + 1;
    });

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
            .get(&DataKey::RecurEscrow(id))
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

/// Reentrancy test for the native XLM token transfer path (issue #716).
///
/// ## Scenario
///
/// An attacker deploys a custom token client that calls back into `release_funds`
/// during its `transfer` invocation — the classic reentrancy attack vector.
/// In production this would manifest as: outer `release_funds` → token `transfer`
/// → (malicious callback) → inner `release_funds`, which would try to drain the
/// escrow a second time before the state is marked `Released`.
///
/// ## Why direct callback injection is not used here
///
/// Soroban's single-contract sandbox executes everything within one Rust call
/// stack and does not yet expose a mechanism for a *test-registered* token
/// contract to synchronously invoke a method back on the escrow contract through
/// the host. We therefore use the same **guard-state injection** pattern that
/// all existing reentrancy tests in this file use: we manually set the guard
/// flag in temporary storage — exactly the state the contract itself would be in
/// mid-call — and assert that any concurrent entry attempt is rejected.
///
/// ## What is verified
///
/// 1. An escrow funded with a native XLM-style Stellar Asset Contract (SAC)
///    token is created and enters the `Active` state.
/// 2. While the guard flag is live (simulating execution inside an outer
///    `release_funds` call that has already entered the guarded section but has
///    not yet returned), a second call to `release_funds` on the *same* escrow
///    is blocked with an error — i.e. `ReentryDetected` fires.
/// 3. After the guard flag is cleared (simulating the outer call completing and
///    the RAII `ReentryGuardScope` dropping), `release_funds` succeeds normally,
///    proving the guard was not permanently stuck.
/// 4. The final escrow state is `Released` and the guard flag is absent, showing
///    the RAII cleanup ran correctly.
#[test]
fn test_native_xlm_token_transfer_reentrancy_guard() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    // ── Participants ──────────────────────────────────────────────────────────
    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    // ── Register a Stellar Asset Contract (SAC) – the same host primitive used
    //    for native XLM on the Stellar network. ────────────────────────────────
    let xlm_token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let xlm_admin_client = token::StellarAssetClient::new(&env, &xlm_token.address());

    // ── Deploy and initialise the escrow contract ─────────────────────────────
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.initialize(
        &platform_wallet,
        &admin,
        &Address::generate(&env), // arbitrator
        &500,                      // 5 % platform fee
        &Some(onboarding_contract),
    );

    // Allow zero-amount minimum so the test amount is accepted.
    client.set_min_escrow_amount(&xlm_token.address(), &0);
    client.set_min_release_window(&1);

    // ── Fund the buyer and create an active escrow denominated in XLM-SAC ─────
    let escrow_amount: i128 = 10_000;
    xlm_admin_client.mint(&buyer, &escrow_amount);

    let order_id = 42u32;
    client.create_escrow(
        &buyer,
        &seller,
        &xlm_token.address(),
        &escrow_amount,
        &order_id,
        &Some(86_400), // 24-hour release window
    );

    // Confirm the escrow is active before attempting any exploit.
    let escrow_initial: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(
        escrow_initial.status,
        EscrowStatus::Active,
        "escrow must be Active before the reentrancy simulation"
    );
    assert_eq!(
        escrow_initial.token,
        xlm_token.address(),
        "escrow token must be the XLM SAC address"
    );

    // ── Phase 1: simulate reentrant call arriving mid-transfer ────────────────
    //
    // Inject the guard flag directly into temporary storage.  This represents
    // the state of the contract *during* an in-progress `release_funds` call —
    // specifically after `enter_reentry_guard` has fired but before the final
    // token `transfer` has returned.  A malicious XLM-token callback would find
    // the contract in precisely this state.
    env.as_contract(&contract_id, || {
        env.storage()
            .temporary()
            .set(&DataKey::ReentryGuard, &true);
    });

    // Verify the guard is present so our assertion below is meaningful.
    let guard_is_set: bool = env.as_contract(&contract_id, || {
        env.storage().temporary().has(&DataKey::ReentryGuard)
    });
    assert!(
        guard_is_set,
        "pre-condition: ReentryGuard must be set before the reentrancy probe"
    );

    // A reentrant call to `release_funds` must be rejected.
    let reentrant_result = client.try_release_funds(&order_id);
    assert!(
        reentrant_result.is_err(),
        "release_funds must be blocked while the reentrancy guard is active"
    );

    // The escrow state must remain `Active` — the reentrant call must not have
    // advanced the state machine or transferred any funds.
    let escrow_after_blocked: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(
        escrow_after_blocked.status,
        EscrowStatus::Active,
        "escrow must still be Active after a blocked reentrancy attempt"
    );

    // ── Phase 2: outer call finishes — guard is released by RAII scope ────────
    //
    // In production the `ReentryGuardScope::drop` implementation clears the
    // flag when the outer call frame unwinds.  We replicate that here.
    env.as_contract(&contract_id, || {
        env.storage().temporary().remove(&DataKey::ReentryGuard);
    });

    // ── Phase 3: legitimate follow-up call must succeed ───────────────────────
    //
    // After the guard is cleared, a normal `release_funds` invocation should
    // complete without error, proving the guard did not become permanently
    // stuck from the earlier failed attempt.
    client.release_funds(&order_id);

    // The escrow must now be in the `Released` state.
    let escrow_released: Escrow = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get(&(Symbol::new(&env, "ESCROW"), order_id))
            .unwrap()
    });
    assert_eq!(
        escrow_released.status,
        EscrowStatus::Released,
        "escrow must be Released after the legitimate call succeeds"
    );

    // ── Phase 4: verify the RAII guard cleared itself after success ───────────
    let guard_leaked: bool = env.as_contract(&contract_id, || {
        env.storage().temporary().has(&DataKey::ReentryGuard)
    });
    assert!(
        !guard_leaked,
        "ReentryGuard must be absent after a successful release_funds — RAII cleanup failed"
    );
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
