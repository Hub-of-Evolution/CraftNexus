//! # `reset_observability_metrics` auth / pause / overflow coverage (#1392)
//!
//! `reset_observability_metrics` moves the observability baseline that every
//! off-chain monitor compares against, and it is admin-gated. These cases pin down
//! that a rejected call (missing admin authorization, paused platform, epoch
//! overflow) leaves the stored epoch exactly as it was, and that the entrypoint is
//! usable again once the platform is unpaused.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

struct Fixture {
    env: Env,
    client: CraftNexusContractClient<'static>,
    contract_id: Address,
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

    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000;
    });

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &Some(onboarding));

    Fixture { env, client, contract_id }
}

#[test]
fn test_reset_observability_metrics_rejected_while_paused() {
    let f = setup();
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, 0);

    f.client.set_paused(&true);

    let result = f.client.try_reset_observability_metrics();

    assert!(matches!(result, Err(Ok(Error::ContractPaused))));
    // A rejected reset must not move the baseline.
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, 0);

    // Unpausing restores the entrypoint and the epoch advances exactly once.
    f.client.set_paused(&false);
    f.client.reset_observability_metrics().unwrap();
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, 1);
}

#[test]
fn test_reset_observability_metrics_without_admin_auth_changes_nothing() {
    let f = setup();
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, 0);

    // Drop every mocked authorization: the admin's `require_auth` must now fail.
    f.env.set_auths(&[]);

    let result = f.client.try_reset_observability_metrics();

    assert!(result.is_err(), "a caller without the admin's auth must not reset the epoch");
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, 0);
}

#[test]
fn test_reset_observability_metrics_rejects_epoch_overflow_without_writing() {
    let f = setup();
    f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .set(&OBSERVABILITY_RESET_EPOCH, &u64::MAX);
    });
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, u64::MAX);

    let result = f.client.try_reset_observability_metrics();

    assert!(matches!(result, Err(Ok(Error::CounterOverflow))));
    // `checked_add` runs before the write, so the overflow is not persisted.
    assert_eq!(f.client.get_observability_snapshot().reset_epoch, u64::MAX);
}

#[test]
fn test_reset_observability_metrics_advances_monotonically() {
    let f = setup();

    for expected in 1..=5u64 {
        f.client.reset_observability_metrics().unwrap();
        assert_eq!(f.client.get_observability_snapshot().reset_epoch, expected);
    }
}
