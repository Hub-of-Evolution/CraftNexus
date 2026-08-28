#![cfg(test)]

//! Sweep allowance checks (#1069).
//!
//! `sweep_unallocated_funds` recovers stray balance that is not backing any
//! customer or artisan obligation. Before this fix it trusted the incremental
//! `TotalLocked`/`TotalStaked` counters directly; any bug that under-counted
//! a liability turned straight into stealable "unallocated" balance, and the
//! only safety net was an *optional* reconciliation report that had to
//! already exist and already be flagged unresolved. These tests exercise the
//! new precondition: a sweep must be backed by a complete, resolved, and
//! current `reconcile_token` report that canonically re-derives locked/staked
//! totals from the real escrow and stake records.

use super::*;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup_sweep_env() -> (
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

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &None,
    );
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

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
fn sweep_without_reconciliation_report_is_rejected() {
    let (_env, client, buyer, seller, token, token_admin, wallet, _admin) = setup_sweep_env();

    token_admin.mint(&buyer, &1_000_000);
    client.create_escrow(&buyer, &seller, &token, &100_000, &1, &Some(3_600));
    // Dust that is genuinely unallocated - but nobody has proven it yet.
    token_admin.mint(&client.address, &25_000);

    let allocation = client.get_fund_allocation(&token);
    assert_eq!(allocation.unallocated, 25_000);

    // No reconcile_token call has ever run for this token: the sweep must be
    // rejected rather than trusting the raw counters.
    let result = client.try_sweep_unallocated_funds(&token, &wallet);
    assert!(matches!(result, Err(Ok(Error::ReconciliationRequired))));
    assert_eq!(
        token::Client::new(&_env, &token).balance(&wallet),
        0,
        "nothing may move without proof the amount is safe to sweep"
    );
}

#[test]
fn sweep_succeeds_once_reconciliation_proves_the_amount_is_safe() {
    let (env, client, buyer, seller, token, token_admin, wallet, _admin) = setup_sweep_env();

    token_admin.mint(&buyer, &1_000_000);
    client.create_escrow(&buyer, &seller, &token, &100_000, &1, &Some(3_600));
    token_admin.mint(&client.address, &25_000);

    let report = client.reconcile_token(&token, &0, &20);
    assert!(report.complete);
    assert!(!report.unresolved);

    let swept = client.sweep_unallocated_funds(&token, &wallet);
    assert_eq!(swept, 25_000);

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&wallet), 25_000);
    assert_eq!(token_client.balance(&client.address), 100_000);
}

#[test]
fn sweep_rejects_stale_reconciliation_after_balance_changes() {
    let (_env, client, buyer, seller, token, token_admin, wallet, _admin) = setup_sweep_env();

    token_admin.mint(&buyer, &1_000_000);
    client.create_escrow(&buyer, &seller, &token, &100_000, &1, &Some(3_600));
    token_admin.mint(&client.address, &25_000);

    let report = client.reconcile_token(&token, &0, &20);
    assert!(report.complete && !report.unresolved);

    // The canonical state moves after reconciliation: a second escrow locks
    // more of the balance that the stale report never saw.
    token_admin.mint(&buyer, &1_000_000);
    client.create_escrow(&buyer, &seller, &token, &200_000, &2, &Some(3_600));

    let result = client.try_sweep_unallocated_funds(&token, &wallet);
    assert!(
        matches!(result, Err(Ok(Error::ReconciliationRequired))),
        "a reconciliation report that predates a balance-affecting operation must not vouch for a sweep"
    );
}

#[test]
fn sweep_rejects_reconciliation_that_found_a_mismatch() {
    let (_env, client, buyer, seller, token, token_admin, wallet, _admin) = setup_sweep_env();

    token_admin.mint(&buyer, &500_000);
    client.create_escrow(&buyer, &seller, &token, &200_000, &1, &Some(3_600));

    // Corrupt the tracked counter so it diverges from the canonical escrow
    // records reconcile_token recomputes.
    _env.as_contract(&client.address, || {
        _env.storage()
            .persistent()
            .set(&DataKey::TotalLocked(token.clone()), &50_000i128);
    });

    let report = client.reconcile_token(&token, &0, &20);
    assert!(report.complete);
    assert!(
        report.unresolved,
        "reconciliation must detect the tracked/expected mismatch"
    );

    let result = client.try_sweep_unallocated_funds(&token, &wallet);
    assert!(matches!(result, Err(Ok(Error::ReconciliationRequired))));
}

#[test]
fn sweep_rejects_accounting_invariant_breach_even_with_report() {
    let (_env, client, buyer, seller, token, token_admin, wallet, _admin) = setup_sweep_env();

    token_admin.mint(&buyer, &500_000);
    client.create_escrow(&buyer, &seller, &token, &200_000, &1, &Some(3_600));

    // A reconciliation report exists and matches tracked state...
    let report = client.reconcile_token(&token, &0, &20);
    assert!(report.complete && !report.unresolved);

    // ...but the tracked counter is corrupted upward *after* reconciling,
    // so tracked liabilities now exceed the actual balance held.
    _env.as_contract(&client.address, || {
        _env.storage()
            .persistent()
            .set(&DataKey::TotalLocked(token.clone()), &500_000i128);
    });

    let result = client.try_sweep_unallocated_funds(&token, &wallet);
    assert!(matches!(
        result,
        Err(Ok(Error::EmergencyAccountingInvariant))
            | Err(Ok(Error::ReconciliationRequired))
    ));
}

#[test]
fn sweep_via_admin_action_enforces_the_same_reconciliation_precondition() {
    let (_env, client, buyer, seller, token, token_admin, wallet, admin) = setup_sweep_env();

    token_admin.mint(&buyer, &1_000_000);
    client.create_escrow(&buyer, &seller, &token, &100_000, &1, &Some(3_600));
    token_admin.mint(&client.address, &25_000);

    client.set_admin_action_timelock_delay(&0);
    let action = client.propose_admin_action(
        &admin,
        &AdminActionKind::SweepUnallocatedFunds(token.clone(), wallet.clone()),
    );

    // No reconciliation has run yet for this token.
    let result = client.try_execute_admin_action(&action.id);
    assert!(matches!(result, Err(Ok(Error::ReconciliationRequired))));

    // Once reconciled, the same proposed action succeeds.
    let report = client.reconcile_token(&token, &0, &20);
    assert!(report.complete && !report.unresolved);
    client.execute_admin_action(&action.id);

    assert_eq!(
        token::Client::new(&_env, &token).balance(&wallet),
        25_000
    );
}
