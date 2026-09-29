#![cfg(test)]

//! Upgrade State Snapshot Tooling - regression tests (#1137).
//!
//! Proves the acceptance criteria for the representative upgrade state snapshot:
//!
//! 1. **Determinism** - an unchanged ledger state always yields the same
//!    snapshot and the same SHA-256 commitment (two independent reads agree).
//! 2. **Sensitivity** - the snapshot commits to counts/sums/presence, not raw
//!    addresses or user payloads (asserted structurally).
//! 3. **Fixture feeding** - the fixture builder produces a representative state
//!    that can be replayed in old/new differential runs; mutating the fixture
//!    changes the commitment, which is the property differential tooling needs.

extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

/// Build a representative, initialized contract state and return the client,
/// the admin, and the single whitelisted token.
fn fixture(env: &Env) -> (CraftNexusContractClient<'_>, Address, Address) {
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000;
    });

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();

    client.initialize(&platform_wallet, &admin, &arbitrator, &1000_u32, &None);
    client.whitelist_token(&token_id).unwrap();

    (client, admin, token_id)
}

#[test]
fn snapshot_is_deterministic_for_unchanged_state() {
    let env = Env::default();
    let (client, _admin, _token_id) = fixture(&env);

    // Two independent reads over identical state must agree field-for-field.
    let first = client.get_upgrade_state_snapshot();
    let second = client.get_upgrade_state_snapshot();
    assert_eq!(first, second);

    // The commitment hashes the same snapshot XDR, so it must also match.
    assert_eq!(
        client.get_upgrade_state_commitment(),
        client.get_upgrade_state_commitment()
    );
}

#[test]
fn snapshot_commits_to_structural_counts_not_raw_payloads() {
    let env = Env::default();
    let (client, _admin, _token_id) = fixture(&env);

    let snapshot = client.get_upgrade_state_snapshot();

    // The fixture whitelists exactly one token and creates no other state, so
    // every representative surface is a known structural value.
    assert_eq!(snapshot.whitelisted_token_count, 1);
    assert_eq!(snapshot.escrow_count, 0);
    assert_eq!(snapshot.recurring_escrow_count, 0);
    assert_eq!(snapshot.pending_batch_job_count, 0);
    assert_eq!(snapshot.upgrade_history_len, 0);
    assert_eq!(snapshot.total_locked, 0);
    assert!(!snapshot.paused);
    assert!(!snapshot.has_pending_upgrade_proposal);
}

#[test]
fn mutating_state_changes_the_commitment() {
    let env = Env::default();
    let (client, _admin, _token_id) = fixture(&env);

    let before = client.get_upgrade_state_commitment();

    // Whitelisting a second asset changes the permission surface, so the
    // commitment must move - the property differential tooling relies on to
    // detect old/new divergence.
    let second_admin = Address::generate(&env);
    let second_token = env.register_stellar_asset_contract_v2(second_admin).address();
    client.whitelist_token(&second_token).unwrap();

    let after = client.get_upgrade_state_commitment();
    assert_ne!(before, after, "mutating state must change the commitment");
    assert_eq!(
        client.get_upgrade_state_snapshot().whitelisted_token_count,
        2
    );
}
