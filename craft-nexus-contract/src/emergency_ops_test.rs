#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, Symbol,
};

fn setup_emergency_env() -> (
    Env,
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.budget().reset_unlimited();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let onboarding = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000;
    });

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding),
    );
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

    // Seed fallback admin (not set by initialize; required for recovery tests).
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::FallbackAdmin, &admin);
    });

    (
        env,
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
fn test_sweep_moves_only_unallocated_funds() {
    let (env, client, buyer, seller, token, token_admin, platform_wallet, _admin) =
        setup_emergency_env();

    token_admin.mint(&buyer, &1_000_000);
    client.create_escrow(&buyer, &seller, &token, &100_000, &1, &Some(3600));

    // Inject dust that is not tracked as locked/staked.
    token_admin.mint(&client.address, &25_000);

    let allocation = client.get_fund_allocation(&token);
    assert_eq!(allocation.total_locked, 100_000);
    assert_eq!(allocation.unallocated, 25_000);

    let swept = client.sweep_unallocated_funds(&token, &platform_wallet);
    assert_eq!(swept, 25_000);

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&platform_wallet), 25_000);
    assert_eq!(token_client.balance(&client.address), 100_000);

    let op = client.get_emergency_operation().unwrap();
    assert_eq!(op.kind, EmergencyOpKind::Sweep);
    assert_eq!(op.phase, EmergencyOpPhase::Completed);
    assert!(op.success);
    assert_eq!(op.amount, 25_000);
}

#[test]
fn test_sweep_rejects_accounting_invariant_breach() {
    let (env, client, buyer, seller, token, token_admin, platform_wallet, _admin) =
        setup_emergency_env();

    token_admin.mint(&buyer, &500_000);
    client.create_escrow(&buyer, &seller, &token, &200_000, &1, &Some(3600));

    // Corrupt TotalLocked upward so reserved > balance.
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::TotalLocked(token.clone()), &500_000i128);
    });

    let result = client.try_sweep_unallocated_funds(&token, &platform_wallet);
    assert!(matches!(
        result,
        Err(Ok(Error::EmergencyAccountingInvariant))
    ));
}

#[test]
fn test_recovery_blocked_by_active_dispute() {
    let (env, client, buyer, seller, token, token_admin, _wallet, _admin) = setup_emergency_env();

    token_admin.mint(&buyer, &500_000);
    client.create_escrow(&buyer, &seller, &token, &50_000, &1, &Some(3600));
    client.dispute_escrow(&1, &Symbol::new(&env, "damaged"), &buyer);

    assert_eq!(client.get_active_dispute_count(), 1);

    let recovered = Address::generate(&env);
    let result = client.try_recover_admin_access(&recovered);
    assert!(matches!(result, Err(Ok(Error::EmergencyConflictActive))));
}

#[test]
fn test_recovery_blocked_by_active_recurring() {
    let (env, client, buyer, seller, token, token_admin, _wallet, _admin) = setup_emergency_env();

    token_admin.mint(&buyer, &1_000_000);
    client.create_recurring_escrow(&buyer, &seller, &token, &100_000, &86_400, &3);
    assert_eq!(client.get_active_recurring_count(), 1);

    let recovered = Address::generate(&env);
    let result = client.try_recover_admin_access(&recovered);
    assert!(matches!(result, Err(Ok(Error::EmergencyConflictActive))));
}

#[test]
fn test_emergency_ops_serialize_recovery_blocks_sweep() {
    let (env, client, _buyer, _seller, token, token_admin, wallet, _admin) = setup_emergency_env();

    token_admin.mint(&client.address, &10_000);

    let recovered = Address::generate(&env);
    // Initiation returns Ok so the timelock persists under Soroban semantics.
    client.recover_admin_access(&recovered);

    let op = client.get_emergency_operation().unwrap();
    assert_eq!(op.kind, EmergencyOpKind::AdminRecovery);
    assert_eq!(op.phase, EmergencyOpPhase::Executing);
    assert!(client.is_paused());

    let sweep = client.try_sweep_unallocated_funds(&token, &wallet);
    assert!(matches!(sweep, Err(Ok(Error::EmergencyOpInProgress))));
}

#[test]
fn test_abort_partial_recovery_allows_resume_path() {
    let (env, client, _buyer, _seller, token, token_admin, wallet, admin) = setup_emergency_env();

    token_admin.mint(&client.address, &40_000);

    let recovered = Address::generate(&env);
    client.recover_admin_access(&recovered);

    let aborted = client.abort_emergency_operation(&admin);
    assert_eq!(aborted.phase, EmergencyOpPhase::Failed);
    assert!(!aborted.success);

    // After abort, sweep can proceed.
    let swept = client.sweep_unallocated_funds(&token, &wallet);
    assert_eq!(swept, 40_000);

    let history = client.get_emergency_operation_history(&0, &10);
    assert!(history.len() >= 2);
}

#[test]
fn test_recovery_completes_after_timelock_with_audit() {
    let (env, client, _buyer, _seller, _token, _token_admin, _wallet, _admin) =
        setup_emergency_env();

    let recovered = Address::generate(&env);
    client.recover_admin_access(&recovered);

    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60 + 1;
    });

    client.recover_admin_access(&recovered);

    let op = client.get_emergency_operation().unwrap();
    assert_eq!(op.kind, EmergencyOpKind::AdminRecovery);
    assert_eq!(op.phase, EmergencyOpPhase::Completed);
    assert!(op.success);
    assert!(client.is_paused());

    let history = client.get_emergency_operation_history(&0, &5);
    assert!(!history.is_empty());
}

#[test]
fn test_pause_blocks_release_and_dispute_initiation() {
    let (env, client, buyer, seller, token, token_admin, _wallet, _admin) = setup_emergency_env();

    token_admin.mint(&buyer, &500_000);
    client.create_escrow(&buyer, &seller, &token, &50_000, &1, &Some(3600));
    client.set_paused(&true);

    let release = client.try_release_funds(&1);
    assert!(release.is_err());

    let dispute = client.try_dispute_escrow(&1, &Symbol::new(&env, "late"), &buyer);
    assert!(dispute.is_err());
}

#[test]
#[should_panic(expected = "HostError: Error(Contract, #45)")]
fn test_unpause_blocked_while_recovery_executing() {
    let (env, client, _buyer, _seller, _token, _token_admin, _wallet, _admin) =
        setup_emergency_env();

    let recovered = Address::generate(&env);
    client.recover_admin_access(&recovered);

    // EmergencyOpInProgress = 45
    client.set_paused(&false);
}

#[test]
fn test_propose_upgrade_blocked_during_recovery() {
    let (env, client, _buyer, _seller, _token, _token_admin, _wallet, admin) =
        setup_emergency_env();

    let recovered = Address::generate(&env);
    client.recover_admin_access(&recovered);

    let hash = BytesN::from_array(&env, &[9u8; 32]);
    let result = client.try_propose_upgrade_wasm(&admin, &hash);
    assert!(matches!(result, Err(Ok(Error::EmergencyOpInProgress))));
}

#[test]
fn test_cancel_upgrade_clears_conflict_for_recovery() {
    let (env, client, _buyer, _seller, _token, _token_admin, _wallet, admin) =
        setup_emergency_env();

    let hash = BytesN::from_array(&env, &[3u8; 32]);
    client.propose_upgrade_wasm(&admin, &hash);

    let recovered = Address::generate(&env);
    assert!(matches!(
        client.try_recover_admin_access(&recovered),
        Err(Ok(Error::EmergencyConflictActive))
    ));

    client.cancel_upgrade_wasm();

    // Cancel-repropose cooldown still blocks new proposes, but recovery can start.
    client.recover_admin_access(&recovered);

    let op = client.get_emergency_operation().unwrap();
    assert_eq!(op.kind, EmergencyOpKind::AdminRecovery);
    assert_eq!(op.phase, EmergencyOpPhase::Executing);
}
