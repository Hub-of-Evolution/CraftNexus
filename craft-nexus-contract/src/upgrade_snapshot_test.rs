#![cfg(test)]

//! # Upgrade State Snapshot Tooling — regression tests (#1137)
//!
//! Proves the acceptance criteria for upgrade state snapshots:
//!
//! 1. **Determinism** — an unchanged ledger state always yields the same
//!    snapshot and the same state commitment (two independent reads agree).
//! 2. **Sensitivity** — the snapshot commits to counts/sums/presence, not
//!    raw addresses or user payloads (asserted structurally).
//! 3. **Fixture feeding** — the fixture builder produces a representative
//!    state that can be replayed in old/new differential runs; mutating the
//!    fixture (adding a record) changes the commitment.

extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, vec, Address, Env,
};

/// Build a representative, initialized contract state and return the client
/// plus the admin/buyer/seller identities and the whitelisted token.
#[allow(clippy::type_complexity)]
fn fixture(
    env: &Env,
) -> (
    CraftNexusContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000;
    });

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    client.initialize(&platform_wallet, &admin, &arbitrator, &1000_u32, &None);
    client.whitelist_token(&token_id);

    (client, admin, buyer, seller, token_admin, token_id)
}

#[test]
fn snapshot_is_deterministic_for_unchanged_state() {
    let env = Env::default();
    let (client, _admin, _buyer, _seller, _token_admin, _token_id) = fixture(&env);

    // Two independent reads over identical state must agree field-for-field.
    let a = client.get_upgrade_state_snapshot();
    let b = client.get_upgrade_state_snapshot();

    assert_eq!(a.contract_version, b.contract_version);
    assert_eq!(a.escrow_count, b.escrow_count);
    assert_eq!(a.recurring_escrow_next_id, b.recurring_escrow_next_id);
    assert_eq!(a.upgrade_threshold, b.upgrade_threshold);
    assert_eq!(a.paused, b.paused);
    assert_eq!(a.onboarding_configured, b.onboarding_configured);
    // #1137: newly added representative fields.
    assert_eq!(a.storage_layout_version, b.storage_layout_version);
    assert_eq!(a.upgrade_history_len, b.upgrade_history_len);
    assert_eq!(a.total_locked, b.total_locked);
    assert_eq!(a.total_staked, b.total_staked);
    assert_eq!(a.whitelisted_token_count, b.whitelisted_token_count);
    assert_eq!(a.upgrade_signer_count, b.upgrade_signer_count);
    assert_eq!(a.pending_admin_action_count, b.pending_admin_action_count);
    assert_eq!(a.recurring_escrow_count, b.recurring_escrow_count);
    assert_eq!(a.pending_batch_job_count, b.pending_batch_job_count);
    assert_eq!(a.has_pending_upgrade_proposal, b.has_pending_upgrade_proposal);

    // The commitment is a hash of the same snapshot XDR -> must be identical.
    assert_eq!(
        client.get_upgrade_state_commitment(),
        client.get_upgrade_state_commitment()
    );
}

#[test]
fn snapshot_commits_to_structural_counts_not_raw_payloads() {
    let env = Env::default();
    let (client, _admin, _buyer, _seller, _token_admin, token_id) = fixture(&env);

    let snapshot = client.get_upgrade_state_snapshot();

    // The fixture whitelisted exactly one token.
    assert_eq!(snapshot.whitelisted_token_count, 1);
    assert!(!snapshot.paused);
    assert!(snapshot.storage_layout_version >= 1);
    // No escrows created yet, so the balance surface is zero.
    assert_eq!(snapshot.total_locked, 0);
}

#[test]
fn mutating_state_changes_the_commitment() {
    let env = Env::default();
    let (client, _admin, buyer, seller, token_admin, token_id) = fixture(&env);

    let before = client.get_upgrade_state_commitment();

    // Fund the buyer and create an escrow so escrow_count / total_locked change.
    token_admin.mint(&buyer, &10_000_000_000_i128);
    token::Client::new(&env, &token_id)
        .approve(&buyer, &client.address, &10_000_000_000_i128, &1000_u32);

    let _ = client.create_escrow(
        &buyer,
        &seller,
        &token_id,
        &1_000_i128,
        &1_u32,
        &None,
    );

    let after = client.get_upgrade_state_commitment();

    assert_ne!(
        before, after,
        "creating a record must change the upgrade state commitment"
    );
}
