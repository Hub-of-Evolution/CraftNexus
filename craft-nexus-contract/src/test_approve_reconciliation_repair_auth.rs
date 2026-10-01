//! # `approve_reconciliation_repair` auth / pause / terminal coverage (#1383)
//!
//! `approve_reconciliation_repair` mutates the approval set of a value-bearing
//! repair plan, so every rejection path must run before the first write. These cases
//! pin down that a missing plan, a paused platform, a missing admin authorization and
//! a terminal (applied / cancelled) plan are all rejected without changing the plan,
//! and that re-approving is deduplicated.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

struct Fixture {
    env: Env,
    client: CraftNexusContractClient<'static>,
    buyer: Address,
    seller: Address,
    token: Address,
    token_admin: token::StellarAssetClient<'static>,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.budget().reset_unlimited();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let onboarding = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());

    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000;
    });

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &Some(onboarding));
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

    Fixture {
        env,
        client,
        buyer,
        seller,
        token: token_contract.address(),
        token_admin: token_admin_client,
    }
}

/// Drive the contract to a state where a repair plan exists.
fn propose_plan(f: &Fixture) -> ReconciliationRepairPlan {
    f.token_admin.mint(&f.buyer, &500_000);
    f.client.create_escrow(&f.buyer, &f.seller, &f.token, &100_000, &1, &Some(3600));

    f.env.as_contract(&f.client.address, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::TotalLocked(f.token.clone()), &200_000i128);
    });
    f.client.reconcile_token(&f.token, &0, &1).unwrap();

    f.client.propose_reconciliation_repair(&f.token).unwrap()
}

/// Overwrite one flag on the stored plan, bypassing the entrypoints. Used to reach
/// terminal states that no public call can undo.
fn force_plan_flag(f: &Fixture, plan_id: u64, mutate: impl FnOnce(&mut ReconciliationRepairPlan)) {
    f.env.as_contract(&f.client.address, || {
        let key = DataKey::ReconciliationRepairPlan(plan_id);
        let mut plan: ReconciliationRepairPlan =
            f.env.storage().persistent().get(&key).unwrap();
        mutate(&mut plan);
        f.env.storage().persistent().set(&key, &plan);
    });
}

#[test]
fn test_approve_reconciliation_repair_unknown_plan_reports_not_found() {
    let f = setup();

    let result = f.client.try_approve_reconciliation_repair(&4_242);

    assert!(matches!(result, Err(Ok(Error::RepairPlanNotFound))));
}

#[test]
fn test_approve_reconciliation_repair_rejected_while_paused_leaves_plan_unchanged() {
    let f = setup();
    let plan = propose_plan(&f);

    f.client.set_paused(&true);
    let result = f.client.try_approve_reconciliation_repair(&plan.id);

    assert!(matches!(result, Err(Ok(Error::ContractPaused))));
    assert_eq!(
        f.client.get_reconciliation_repair_plan(&plan.id).unwrap(),
        plan,
        "a paused approval must not touch the plan"
    );
}

#[test]
fn test_approve_reconciliation_repair_without_admin_auth_leaves_plan_unchanged() {
    let f = setup();
    let plan = propose_plan(&f);

    // Drop every mocked authorization: the admin's `require_auth` must now fail.
    f.env.set_auths(&[]);
    let result = f.client.try_approve_reconciliation_repair(&plan.id);

    assert!(result.is_err(), "approval must require the admin's authorization");
    assert_eq!(f.client.get_reconciliation_repair_plan(&plan.id).unwrap(), plan);
}

#[test]
fn test_approve_reconciliation_repair_is_deduplicated() {
    let f = setup();
    let plan = propose_plan(&f);
    let approvals_after_proposal = plan.approvals.len();
    assert_eq!(approvals_after_proposal, 1, "the proposer approves on creation");

    let first = f.client.approve_reconciliation_repair(&plan.id).unwrap();
    let second = f.client.approve_reconciliation_repair(&plan.id).unwrap();
    let third = f.client.approve_reconciliation_repair(&plan.id).unwrap();

    assert_eq!(first.approvals.len(), approvals_after_proposal);
    assert_eq!(second.approvals.len(), approvals_after_proposal);
    assert_eq!(third.approvals.len(), approvals_after_proposal);
    assert_eq!(first, second);
    assert_eq!(second, third);
    // The stored plan matches what the entrypoint returned.
    assert_eq!(
        f.client.get_reconciliation_repair_plan(&plan.id).unwrap(),
        third
    );
}

#[test]
fn test_approve_reconciliation_repair_rejects_an_applied_plan_unchanged() {
    let f = setup();
    let plan = propose_plan(&f);
    force_plan_flag(&f, plan.id, |p| p.applied = true);
    let terminal = f.client.get_reconciliation_repair_plan(&plan.id).unwrap();
    assert!(terminal.applied);

    let result = f.client.try_approve_reconciliation_repair(&plan.id);

    assert!(matches!(result, Err(Ok(Error::RepairPlanTerminal))));
    assert_eq!(
        f.client.get_reconciliation_repair_plan(&plan.id).unwrap(),
        terminal
    );
}

#[test]
fn test_approve_reconciliation_repair_rejects_a_cancelled_plan_unchanged() {
    let f = setup();
    let plan = propose_plan(&f);
    force_plan_flag(&f, plan.id, |p| p.cancelled = true);
    let terminal = f.client.get_reconciliation_repair_plan(&plan.id).unwrap();
    assert!(terminal.cancelled);

    let result = f.client.try_approve_reconciliation_repair(&plan.id);

    assert!(matches!(result, Err(Ok(Error::RepairPlanTerminal))));
    assert_eq!(
        f.client.get_reconciliation_repair_plan(&plan.id).unwrap(),
        terminal
    );
}

#[test]
fn test_approve_reconciliation_repair_leaves_a_neighbouring_plan_untouched() {
    let f = setup();
    let plan = propose_plan(&f);
    let neighbour = f.client.get_reconciliation_repair_plan(&(plan.id + 1));

    f.client.approve_reconciliation_repair(&plan.id).unwrap();

    // Only the targeted plan is affected; a missing neighbour stays missing.
    assert!(neighbour.is_none());
    assert!(f.client.get_reconciliation_repair_plan(&(plan.id + 1)).is_none());
}
