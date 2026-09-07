#![cfg(test)]
//! Tests for the bounded, resumable contract diagnostic scan (Issue #1135).

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::Env;

fn setup_test(
    env: &Env,
) -> (
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let onboarding_contract = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);

    (client, buyer, seller, token_contract.address(), token_admin_client)
}

/// A scan over an all-healthy set of records finds nothing and completes
/// in one page.
#[test]
fn scan_over_healthy_escrows_reports_no_findings() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env);
    token_admin.mint(&buyer, &100_000_000);

    for order_id in 1..=3u32 {
        client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &order_id, &None);
    }

    let report = client.run_diagnostic_scan(&0, &20);
    assert_eq!(report.scanned, 3);
    assert_eq!(report.findings_count, 0);
    assert!(report.complete);
    assert_eq!(report.next_cursor, 3);
    assert_eq!(client.get_diagnostic_findings(&0, &10).len(), 0);
}

/// The scan flags the same orphaned-transition issue that
/// `diagnose_escrow_state` already detects for a single record, and files
/// the finding under that escrow's own token as the accounting category.
#[test]
fn scan_flags_orphaned_transition_under_its_token() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env);
    token_admin.mint(&buyer, &100_000_000);

    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &None);
    client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &2, &None);

    // Corrupt order 1 into an orphaned in-flight transition, mirroring
    // `test_escrow_state_diagnostic_flags_pending_orphans`.
    env.as_contract(&client.address, || {
        let mut escrow: Escrow = env.storage().persistent().get(&(ESCROW, 1u32)).unwrap();
        escrow.status = EscrowStatus::ReleasePending;
        env.storage().persistent().set(&(ESCROW, 1u32), &escrow);
    });

    let report = client.run_diagnostic_scan(&0, &20);
    assert!(report.complete);
    assert_eq!(report.scanned, 2);
    assert_eq!(report.findings_count, 1);

    let findings = client.get_diagnostic_findings(&0, &10);
    assert_eq!(findings.len(), 1);
    let finding = findings.get(0).unwrap();
    assert_eq!(finding.order_id, 1);
    assert_eq!(finding.category, token_id);
    assert_eq!(finding.issue, EscrowStateIssue::PendingTransitionUnfinished);

    // Cross-check against the single-record diagnostic to confirm the two
    // never disagree.
    let single = client.diagnose_escrow_state(&1);
    assert_eq!(single.issue, finding.issue);
}

/// A scan can be resumed page by page and always reaches the same total
/// findings as a single unbounded pass, regardless of page size.
#[test]
fn scan_is_resumable_across_pages() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env);
    token_admin.mint(&buyer, &100_000_000);

    for order_id in 1..=6u32 {
        client.create_escrow(&buyer, &seller, &token_id, &1_000_000, &order_id, &None);
    }
    // Corrupt two of the six into inconsistent states.
    env.as_contract(&client.address, || {
        for id in [2u32, 5u32] {
            let mut escrow: Escrow = env.storage().persistent().get(&(ESCROW, id)).unwrap();
            escrow.status = EscrowStatus::RefundPending;
            env.storage().persistent().set(&(ESCROW, id), &escrow);
        }
    });

    // Page through 2 records at a time.
    let mut cursor = 0u32;
    let mut last_report = client.run_diagnostic_scan(&cursor, &2);
    cursor = last_report.next_cursor;
    while !last_report.complete {
        last_report = client.run_diagnostic_scan(&cursor, &2);
        cursor = last_report.next_cursor;
    }

    assert_eq!(last_report.scanned, 6);
    assert_eq!(last_report.findings_count, 2);
    assert_eq!(last_report.next_cursor, 6);

    let findings = client.get_diagnostic_findings(&0, &10);
    assert_eq!(findings.len(), 2);
    assert_eq!(findings.get(0).unwrap().order_id, 2);
    assert_eq!(findings.get(1).unwrap().order_id, 5);

    // The persisted report matches the final page.
    let stored = client.get_diagnostic_report().unwrap();
    assert_eq!(stored.findings_count, 2);
    assert!(stored.complete);
}

/// Starting a fresh scan (cursor 0) never mixes findings from a prior scan,
/// even if a record was fixed in between.
#[test]
fn restarting_a_scan_discards_stale_findings() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env);
    token_admin.mint(&buyer, &100_000_000);

    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &2, &None);

    env.as_contract(&client.address, || {
        let mut escrow: Escrow = env.storage().persistent().get(&(ESCROW, 1u32)).unwrap();
        escrow.status = EscrowStatus::DisputePending;
        env.storage().persistent().set(&(ESCROW, 1u32), &escrow);
        let mut escrow2: Escrow = env.storage().persistent().get(&(ESCROW, 2u32)).unwrap();
        escrow2.status = EscrowStatus::SettlementPending;
        env.storage().persistent().set(&(ESCROW, 2u32), &escrow2);
    });

    let first = client.run_diagnostic_scan(&0, &20);
    assert_eq!(first.findings_count, 2);

    // Fix order 1, then run a brand-new scan from cursor 0.
    env.as_contract(&client.address, || {
        let mut escrow: Escrow = env.storage().persistent().get(&(ESCROW, 1u32)).unwrap();
        escrow.status = EscrowStatus::Active;
        env.storage().persistent().set(&(ESCROW, 1u32), &escrow);
    });

    let second = client.run_diagnostic_scan(&0, &20);
    assert_eq!(
        second.findings_count, 1,
        "restarting the scan must not carry over the now-fixed finding for order 1"
    );
    let findings = client.get_diagnostic_findings(&0, &10);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings.get(0).unwrap().order_id, 2);
}

/// A diagnostic report, complete or not, cannot itself authorize a repair:
/// no admin action accepts one as input, and the existing repair-proposal
/// flow is unaffected by running (or not running) a scan.
#[test]
fn diagnostic_report_grants_no_repair_authority() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);

    // Running a scan alone must not let a repair be proposed — that still
    // requires a *complete and unresolved* reconciliation report, which a
    // diagnostic scan does not produce.
    let _report = client.run_diagnostic_scan(&0, &20);
    let repair_attempt = client.try_propose_reconciliation_repair(&token_id);
    assert!(
        repair_attempt.is_err(),
        "a diagnostic scan report must not unlock the repair-proposal path"
    );
}

/// Two independent scans over identical, unchanged contract state produce
/// identical digests; the digest reflects content, not call history.
#[test]
fn report_digest_is_deterministic_for_identical_state() {
    let env = Env::default();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env);
    token_admin.mint(&buyer, &100_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &10_000_000, &1, &None);

    let first = client.run_diagnostic_scan(&0, &20);
    let second = client.run_diagnostic_scan(&0, &20);
    assert_eq!(first.report_digest, second.report_digest);
}
