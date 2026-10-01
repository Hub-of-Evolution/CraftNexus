//! # `get_reconciliation_repair_plan` missing-storage coverage (#1384)
//!
//! `get_reconciliation_repair_plan` is the reader treasury operators poll while a
//! repair is proposed, approved and applied. Callers hit it after archival, after a
//! partial migration, or simply for a plan id that never existed. It must return
//! `None` — never trap — in all of those cases, and must extend the read TTL of a
//! hot plan on hit so a polling operator cannot let it lapse into archival.

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

/// Drive the contract to a state where a repair plan exists, mirroring the setup
/// used by `emergency_ops_test::test_repair_plan_requires_approval_and_is_idempotent`.
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

#[test]
fn test_get_reconciliation_repair_plan_missing_key_returns_none() {
    let f = setup();

    // No plan has ever been proposed.
    assert!(f.client.get_reconciliation_repair_plan(&1).is_none());
    assert!(f.client.get_reconciliation_repair_plan(&u64::MAX).is_none());
}

#[test]
fn test_get_reconciliation_repair_plan_round_trips_the_proposed_plan() {
    let f = setup();
    let plan = propose_plan(&f);

    let loaded = f.client.get_reconciliation_repair_plan(&plan.id).unwrap();

    assert_eq!(loaded, plan);
    // A neighbouring id must still answer `None` rather than leaking the plan.
    assert!(f.client.get_reconciliation_repair_plan(&(plan.id + 1)).is_none());
}

#[test]
fn test_get_reconciliation_repair_plan_after_archival_returns_none_not_a_trap() {
    let f = setup();
    let plan = propose_plan(&f);
    assert!(f.client.get_reconciliation_repair_plan(&plan.id).is_some());

    // Archival evicts the persistent entry.
    f.env.as_contract(&f.client.address, || {
        f.env
            .storage()
            .persistent()
            .remove(&DataKey::ReconciliationRepairPlan(plan.id));
    });

    // The reader must answer `None`; a host panic here is not a usable client error.
    assert!(f.client.get_reconciliation_repair_plan(&plan.id).is_none());
}

#[test]
fn test_get_reconciliation_repair_plan_repeated_reads_are_stable() {
    let f = setup();
    let plan = propose_plan(&f);

    let first = f.client.get_reconciliation_repair_plan(&plan.id);
    let second = f.client.get_reconciliation_repair_plan(&plan.id);
    let third = f.client.get_reconciliation_repair_plan(&plan.id);

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(first.unwrap(), plan);
}
