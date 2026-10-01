//! # `get_archival_compaction_cursor` missing-storage coverage (#1390)
//!
//! `get_archival_compaction_cursor` is what a resumable compaction uses to find out
//! where to pick up. Before the first compaction — and after archival or a partial
//! migration has removed the key — it must answer `0` rather than trapping, and it
//! must extend the read TTL of the cursor on hit so an operator polling the cursor
//! cannot let it lapse into archival.

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
    // Small, deterministic batches so the cursor assertions below do not depend on
    // the default `DEFAULT_ARCHIVAL_COMPACTION_BATCH`.
    client.set_archival_policy(&1, &2).unwrap();

    Fixture { env, client, contract_id }
}

#[test]
fn test_get_archival_compaction_cursor_defaults_to_zero_when_missing() {
    let f = setup();

    // No compaction has run yet.
    assert_eq!(f.client.get_archival_compaction_cursor(), 0);
}

#[test]
fn test_get_archival_compaction_cursor_returns_zero_after_archival_removes_it() {
    let f = setup();
    f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ArchivalCompactionCursor, &11u32);
    });
    assert_eq!(f.client.get_archival_compaction_cursor(), 11);

    // Archival/partial migration removes the entry; the reader must not trap.
    f.env.as_contract(&f.contract_id, || {
        f.env.storage().persistent().remove(&DataKey::ArchivalCompactionCursor);
    });
    assert_eq!(f.client.get_archival_compaction_cursor(), 0);
}

#[test]
fn test_get_archival_compaction_cursor_tracks_compaction_progress() {
    let f = setup();
    // Five archived summaries to scan, with a batch size of two.
    f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ArchivalSummaryCount, &5u32);
    });

    let first = f.client.compact_archival_records(&0, &2).unwrap();
    assert_eq!(first.cursor, 2);
    assert_eq!(f.client.get_archival_compaction_cursor(), 2);

    let second = f.client.compact_archival_records(&2, &2).unwrap();
    assert_eq!(second.cursor, 4);
    assert_eq!(f.client.get_archival_compaction_cursor(), 4);
}

#[test]
fn test_get_archival_compaction_cursor_is_stable_at_the_terminal_cursor() {
    let f = setup();
    f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::ArchivalSummaryCount, &3u32);
    });

    // Drain the whole index.
    let drained = f.client.compact_archival_records(&0, &2).unwrap();
    assert_eq!(drained.cursor, 2);
    let terminal = f.client.compact_archival_records(&2, &2).unwrap();
    assert_eq!(terminal.cursor, 3);
    assert_eq!(f.client.get_archival_compaction_cursor(), 3);

    // Re-running at the terminal cursor is idempotent and leaves the cursor at the end.
    let again = f.client.compact_archival_records(&3, &2).unwrap();
    assert_eq!(again.cursor, 3);
    assert_eq!(f.client.get_archival_compaction_cursor(), 3);
}
