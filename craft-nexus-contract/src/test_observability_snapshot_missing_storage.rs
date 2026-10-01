//! # `get_observability_snapshot` missing-storage coverage (#1391)
//!
//! `get_observability_snapshot` is what off-chain monitoring polls. Callers hit it
//! after archival has removed persistent entries or after a partial migration has
//! only written some of the counters. The reader must answer with zeros instead of
//! trapping, and must extend the read TTL of the hot keys it touches so a
//! continuously polling dashboard keeps them from lapsing into archival.

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
fn test_snapshot_before_any_state_is_all_zero() {
    let f = setup();

    let snapshot = f.client.get_observability_snapshot();

    assert_eq!(snapshot.version, OBSERVABILITY_SNAPSHOT_VERSION);
    assert_eq!(snapshot.reset_epoch, 0);
    assert_eq!(snapshot.total_escrows, 0);
    assert_eq!(snapshot.total_volume, 0);
    assert_eq!(snapshot.active_disputes, 0);
    assert_eq!(snapshot.staked_artisans, 0);
    assert_eq!(snapshot.total_failures, 0);
    assert_eq!(snapshot.active_jobs, 0);
}

#[test]
fn test_snapshot_tolerates_partially_migrated_storage() {
    let f = setup();
    // A partial migration wrote only some of the counters.
    f.env.as_contract(&f.contract_id, || {
        f.env.storage().persistent().set(&DataKey::EscrowCount, &7u32);
    });

    let snapshot = f.client.get_observability_snapshot();

    assert_eq!(snapshot.total_escrows, 7);
    // The absent keys must read as zero rather than trapping the host.
    assert_eq!(snapshot.total_volume, 0);
    assert_eq!(snapshot.active_disputes, 0);
    assert_eq!(snapshot.staked_artisans, 0);
    assert_eq!(snapshot.reset_epoch, 0);
}

#[test]
fn test_snapshot_tolerates_archived_keys_being_removed() {
    let f = setup();
    f.env.as_contract(&f.contract_id, || {
        f.env.storage().persistent().set(&DataKey::EscrowCount, &3u32);
        f.env.storage().persistent().set(&DataKey::TotalVolume, &5_000i128);
        f.env.storage().persistent().set(&DataKey::ActiveDisputeCount, &2u32);
    });

    let populated = f.client.get_observability_snapshot();
    assert_eq!(populated.total_escrows, 3);
    assert_eq!(populated.total_volume, 5_000);
    assert_eq!(populated.active_disputes, 2);

    // Archival evicts the persistent entries; monitoring must still get an answer.
    f.env.as_contract(&f.contract_id, || {
        f.env.storage().persistent().remove(&DataKey::EscrowCount);
        f.env.storage().persistent().remove(&DataKey::TotalVolume);
        f.env.storage().persistent().remove(&DataKey::ActiveDisputeCount);
    });

    let evicted = f.client.get_observability_snapshot();
    assert_eq!(evicted.total_escrows, 0);
    assert_eq!(evicted.total_volume, 0);
    assert_eq!(evicted.active_disputes, 0);
    assert_eq!(evicted.version, OBSERVABILITY_SNAPSHOT_VERSION);
}

#[test]
fn test_snapshot_is_readable_without_authorization_and_after_terminal_reset() {
    let f = setup();

    f.client.reset_observability_metrics().unwrap();
    f.client.reset_observability_metrics().unwrap();

    // Monitoring is unauthenticated; the snapshot must never require auth.
    f.env.set_auths(&[]);
    let snapshot = f.client.get_observability_snapshot();

    assert_eq!(snapshot.reset_epoch, 2);
    assert_eq!(snapshot.total_escrows, 0);
}

#[test]
fn test_snapshot_is_repeatable() {
    let f = setup();
    f.env.as_contract(&f.contract_id, || {
        f.env.storage().persistent().set(&DataKey::TotalVolume, &1_234i128);
    });

    let first = f.client.get_observability_snapshot();
    let second = f.client.get_observability_snapshot();

    assert_eq!(first, second);
}
