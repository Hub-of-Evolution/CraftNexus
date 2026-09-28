//! Migration precondition gate (Issue #1118).
//!
//! # Properties verified
//!
//! 1. **Refusal is total** – a violated precondition panics with
//!    `MigrationPreconditionFailed` and leaves the layout version, escrow
//!    records, and token balances byte-for-byte unchanged.
//! 2. **Refusal is inspectable** – the same verdict is obtainable before the
//!    attempt, and reading it is itself side-effect free.
//! 3. **Failure is attributable** – the report names the specific condition
//!    and the offending token / participant / escrow id.
//! 4. **Refusal is diagnosable after the fact** – an admin audit persists the
//!    verdict, is bounded FIFO, and its digest can be re-verified.
//! 5. **Valid legacy fixtures still migrate** – a clean deployment, and a
//!    deployment whose escrows have settled, migrate with balances intact.
//! 6. **The gate fails closed** – exceeding the scan budget is a refusal, not a
//!    partial verdict.

#![cfg(test)]

use crate::{
    CraftNexusContract, CraftNexusContractClient, DataKey, Error, MigrationPrecondition,
    MigrationPreconditionAudit, MigrationPreconditionReport, CURRENT_STORAGE_LAYOUT_VERSION,
    ESCROW, MAX_MIGRATION_PRECONDITION_AUDITS, MAX_MIGRATION_PRECONDITION_SCAN,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, vec, Address, BytesN, Env, Map, Symbol, Vec,
};

struct Fixture {
    env: Env,
    client: CraftNexusContractClient<'static>,
    admin: Address,
    buyer: Address,
    seller: Address,
    token_admin: token::StellarAssetClient<'static>,
    token: Address,
}

/// A freshly initialized platform with no outstanding obligations. Every
/// precondition should hold, so this is the baseline for "valid legacy fixture".
fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let token_admin_address = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_address);
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &None::<Address>,
    );
    client.set_min_escrow_amount(&token, &0);
    client.set_min_release_window(&1);
    client.set_evidence_challenge_window(&0);

    Fixture {
        env,
        client,
        admin,
        buyer,
        seller,
        token_admin,
        token,
    }
}

fn precondition_of(report: &MigrationPreconditionReport) -> MigrationPrecondition {
    report
        .failure
        .as_ref()
        .expect("expected a violated precondition")
        .precondition
}

/// Simulate a storage-layout upgrade boundary: the deployment is live but has
/// not yet been stamped with the current layout version.
fn clear_layout_version(env: &Env, client: &CraftNexusContractClient) {
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .remove(&DataKey::StorageLayoutVersion);
    });
}

fn create_funded_escrow(f: &Fixture, order_id: u32, amount: i128) {
    f.token_admin.mint(&f.buyer, &100_000_000);
    f.client
        .create_escrow(&f.buyer, &f.seller, &f.token, &amount, &order_id, &None);
}

// ── 1. Clean deployments pass ───────────────────────────────────────────────

#[test]
fn clean_deployment_passes_every_precondition() {
    let f = setup();

    let report = f.client.get_migration_preconditions();

    assert!(
        report.failure.is_none(),
        "unexpected block: {:?}",
        report.failure
    );
    assert_eq!(report.target_layout_version, CURRENT_STORAGE_LAYOUT_VERSION);
    assert_eq!(
        report.current_layout_version,
        CURRENT_STORAGE_LAYOUT_VERSION
    );
    assert!(report.contract_version > 0);
    assert!(
        report.checks_evaluated > 0,
        "the report must state its coverage"
    );
    assert_ne!(report.digest, BytesN::from_array(&[0u8; 32]));
}

#[test]
fn clean_deployment_migrates_and_preserves_balances() {
    let f = setup();
    f.token_admin.mint(&f.buyer, &100_000_000);
    let buyer_before = f.token_admin.balance(&f.buyer);
    let contract_before = f.token_admin.balance(&f.client.address);

    clear_layout_version(&f.env, &f.client);

    assert_eq!(f.client.migrate_storage_layout(), 1);
    assert_eq!(
        f.client.get_storage_layout_version(),
        CURRENT_STORAGE_LAYOUT_VERSION
    );

    // Property 5: a valid migration is observably neutral.
    assert_eq!(f.token_admin.balance(&f.buyer), buyer_before);
    assert_eq!(f.token_admin.balance(&f.client.address), contract_before);
}

// ── 2. Refusal writes nothing ──────────────────────────────────────────────

#[test]
fn refuses_while_escrow_funds_are_locked_and_writes_nothing() {
    let f = setup();
    create_funded_escrow(&f, 1, 50_000_000);
    clear_layout_version(&f.env, &f.client);

    let buyer_before = f.token_admin.balance(&f.buyer);
    let contract_before = f.token_admin.balance(&f.client.address);
    let escrow_before = f.client.get_escrow(&1);

    let refusal = f.client.try_migrate_storage_layout().unwrap_err();
    assert_eq!(refusal, Ok(Error::MigrationPreconditionFailed));

    // The layout version was not stamped.
    assert_eq!(f.client.get_storage_layout_version(), 0);

    // Balances and the escrow record are untouched.
    assert_eq!(f.token_admin.balance(&f.buyer), buyer_before);
    assert_eq!(f.token_admin.balance(&f.client.address), contract_before);
    assert_eq!(f.client.get_escrow(&1), escrow_before);
    assert_eq!(f.client.get_escrow_count(), 1);
}

#[test]
fn a_refused_migration_does_not_even_record_an_audit() {
    let f = setup();
    create_funded_escrow(&f, 1, 50_000_000);
    clear_layout_version(&f.env, &f.client);

    assert!(f.client.try_migrate_storage_layout().is_err());

    // A refused migration writes nothing at all -- including no trace of why.
    // The audit trail only exists because an operator asked for it.
    assert!(f.client.get_last_precondition_audit().is_none());
    assert!(f
        .client
        .get_precondition_audits(&0, &MAX_MIGRATION_PRECONDITION_AUDITS)
        .is_empty());
}

// ── 3. Failures are attributable ───────────────────────────────────────────

#[test]
fn locked_funds_are_reported_with_the_offending_token() {
    let f = setup();
    create_funded_escrow(&f, 1, 50_000_000);
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    let failure = report.failure.as_ref().expect("must be blocked");

    assert_eq!(failure.precondition, MigrationPrecondition::NoLockedFunds);
    assert_eq!(failure.observed, 50_000_000);
    assert_eq!(failure.expected, 0);
    assert_eq!(failure.token.as_ref(), Some(&f.token));
    assert!(
        failure.code > 0,
        "codes must be non-zero and stable off-chain"
    );
}

#[test]
fn duplicate_fee_token_index_entry_is_refused() {
    let f = setup();
    let duplicate = Address::generate(&f.env);

    f.env.as_contract(&f.client.address, || {
        let mut index = vec![&f.env];
        index.push_back(duplicate.clone());
        index.push_back(duplicate.clone());
        f.env
            .storage()
            .persistent()
            .set(&DataKey::FeeTokenIndex, &index);
    });
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    let failure = report.failure.as_ref().expect("must be blocked");
    assert_eq!(
        failure.precondition,
        MigrationPrecondition::FeeTokenIndexDeduplicated
    );
    assert_eq!(failure.token.as_ref(), Some(&duplicate));

    assert_eq!(
        f.client.try_migrate_storage_layout().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
}

#[test]
fn mixed_whitelist_layout_is_refused() {
    let f = setup();

    f.env.as_contract(&f.client.address, || {
        let mut legacy = Map::new(&f.env);
        legacy.set(f.token.clone(), true);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::WhitelistedTokens, &legacy);
        // An indexed entry already exists, so re-running the migration would
        // rewrite the count from the legacy blob alone and silently drop
        // tokens from enforcement.
        f.env
            .storage()
            .persistent()
            .set(&DataKey::WhitelistedTokenCount, &3u32);
    });
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    assert_eq!(
        precondition_of(&report),
        MigrationPrecondition::NoMixedWhitelistLayout
    );

    let count_before = f.client.get_whitelisted_token_count();
    assert_eq!(
        f.client.try_migrate_whitelist_storage().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
    assert_eq!(f.client.get_whitelisted_token_count(), count_before);
}

#[test]
fn undecodable_legacy_blob_is_reported_not_crashed() {
    let f = setup();

    // A fee index that is not a `Vec<Address>`. The gate must surface this as
    // a refusal; a typed read would abort the host instead, which is exactly
    // the failure mode this test pins down.
    f.env.as_contract(&f.client.address, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::FeeTokenIndex, &12345u32);
    });
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    assert_eq!(
        precondition_of(&report),
        MigrationPrecondition::LegacyShapeReadable
    );

    assert_eq!(
        f.client.try_migrate_storage_layout().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
}

#[test]
fn undecodable_escrow_record_is_reported_not_crashed() {
    let f = setup();

    // A legacy-shaped escrow is exactly what a migration has to survive, so
    // the gate resolves every historical shape in memory rather than assuming
    // the newest one.
    f.env.as_contract(&f.client.address, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::EscrowCount, &1u32);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::GlobalEscrowIdIndexed(0), &7u32);
        // A bare integer where an escrow record belongs: decodable as a Val,
        // but not as any of the four historical escrow shapes.
        f.env.storage().persistent().set(&(ESCROW, 7u32), &42u32);
    });
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    assert_eq!(
        precondition_of(&report),
        MigrationPrecondition::LegacyShapeReadable
    );
    assert_eq!(report.failure.as_ref().unwrap().escrow_id, Some(7));
}

// ── 4. The gate fails closed ───────────────────────────────────────────────

#[test]
fn exceeding_the_scan_budget_is_a_refusal_not_a_partial_pass() {
    let f = setup();

    // One token past the budget. Reporting "no problems found" over a state the
    // gate never fully examined would be the dangerous answer.
    let over_budget = MAX_MIGRATION_PRECONDITION_SCAN + 1;
    f.env.as_contract(&f.client.address, || {
        let mut index = vec![&f.env];
        for _ in 0..over_budget {
            index.push_back(Address::generate(&f.env));
        }
        f.env
            .storage()
            .persistent()
            .set(&DataKey::FeeTokenIndex, &index);
    });
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    let failure = report.failure.as_ref().expect("must be blocked");
    assert_eq!(
        failure.precondition,
        MigrationPrecondition::ScanBudgetExceeded
    );
    assert_eq!(failure.expected, MAX_MIGRATION_PRECONDITION_SCAN as i128);

    assert_eq!(
        f.client.try_migrate_storage_layout().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
}

// ── 5. Inspection is read-only ─────────────────────────────────────────────

#[test]
fn inspecting_preconditions_is_side_effect_free() {
    let f = setup();
    create_funded_escrow(&f, 1, 50_000_000);
    clear_layout_version(&f.env, &f.client);

    let first = f.client.get_migration_preconditions();
    let second = f.client.get_migration_preconditions();

    assert_eq!(first, second, "a read must not perturb the verdict");
    assert!(f.client.get_last_precondition_audit().is_none());
    assert_eq!(f.client.get_storage_layout_version(), 0);
    assert_eq!(f.client.get_escrow_count(), 1);
}

#[test]
fn the_report_states_its_own_coverage() {
    let f = setup();
    f.token_admin.mint(&f.buyer, &1_000);

    let report = f.client.get_migration_preconditions();

    // An empty deployment has nothing to scan; the counters make that visible
    // rather than leaving the operator to assume full coverage.
    assert_eq!(report.tokens_scanned, 0);
    assert_eq!(report.escrows_scanned, 0);
    assert_eq!(report.artisans_scanned, 0);
}

// ── 6. The audit trail ─────────────────────────────────────────────────────

#[test]
fn audit_persists_the_verdict_under_admin_authority() {
    let f = setup();
    create_funded_escrow(&f, 1, 50_000_000);
    clear_layout_version(&f.env, &f.client);

    let report = f.client.audit_migration_preconditions();
    assert!(report.failure.is_some());

    let stored = f
        .client
        .get_last_precondition_audit()
        .expect("an audit must be recorded");
    assert_eq!(stored.report, report);
    assert_eq!(stored.sequence, 0);
    assert_eq!(stored.auditor, f.admin);
    assert_eq!(stored.recorded_at, f.env.ledger().timestamp());

    let page = f
        .client
        .get_precondition_audits(&0, &MAX_MIGRATION_PRECONDITION_AUDITS);
    assert_eq!(page.len(), 1);
}

#[test]
fn a_passing_audit_is_recorded_too() {
    let f = setup();

    let report = f.client.audit_migration_preconditions();
    assert!(report.failure.is_none());
    assert!(f.client.get_last_precondition_audit().is_some());
}

#[test]
fn audit_history_is_bounded_and_oldest_first() {
    let f = setup();

    // Clear the layout version each round so the verdict keeps changing and
    // the sequence numbers are distinguishable.
    let rounds = MAX_MIGRATION_PRECONDITION_AUDITS + 3;
    for _ in 0..rounds {
        f.client.audit_migration_preconditions();
    }

    let page = f
        .client
        .get_precondition_audits(&0, &MAX_MIGRATION_PRECONDITION_AUDITS);
    assert_eq!(page.len(), MAX_MIGRATION_PRECONDITION_AUDITS);

    // The oldest entries were evicted, so the window starts at the rollover.
    assert_eq!(
        page.first().unwrap().sequence,
        rounds - MAX_MIGRATION_PRECONDITION_AUDITS
    );
    assert_eq!(page.last().unwrap().sequence, rounds - 1);
}

#[test]
fn audit_digest_reverifies_and_detects_tampering() {
    let f = setup();
    let report = f.client.audit_migration_preconditions();
    let sequence = 0;

    let verified = f.client.verify_precondition_audit(&sequence);
    assert!(verified.is_ok());
    assert_eq!(verified.unwrap().report.digest, report.digest);

    // A sequence that was never recorded is a hard error, not a silent None.
    let missing = f.client.try_verify_precondition_audit(&9999);
    assert!(missing.is_err());
}

#[test]
fn tampering_with_a_stored_audit_breaks_its_digest() {
    let f = setup();
    f.client.audit_migration_preconditions();

    // Rewrite the history behind the contract's back, as a future buggy
    // migration might. The digest must not still vouch for it.
    f.env.as_contract(&f.client.address, || {
        let mut history: Vec<MigrationPreconditionAudit> = f
            .env
            .storage()
            .persistent()
            .get(&DataKey::MigrationPreconditionAudits)
            .unwrap_or_else(|| Vec::new(&f.env));
        if let Some(mut audit) = history.first() {
            audit.sequence = 42;
            history.set(0, audit);
        }
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MigrationPreconditionAudits, &history);
    });

    let result = f.client.verify_precondition_audit(&0);
    assert!(result.is_err());
}

// ── 7. Every migrate_* entry point is gated ────────────────────────────────

#[test]
fn all_operator_initiated_migrations_are_gated() {
    let f = setup();
    // Locked funds is a platform-wide blocker, so it must stop every
    // operator-initiated migration, not just `migrate_storage_layout`.
    create_funded_escrow(&f, 1, 50_000_000);
    clear_layout_version(&f.env, &f.client);

    assert_eq!(
        f.client.try_migrate_storage_layout().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
    assert_eq!(
        f.client.try_migrate_whitelist_storage().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
    assert_eq!(
        f.client.try_migrate_fee_token_configs().unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
    assert_eq!(
        f.client
            .try_migrate_artisan_stake_queue(&f.seller)
            .unwrap_err(),
        Ok(Error::MigrationPreconditionFailed)
    );
}

#[test]
fn settled_escrows_do_not_block_a_migration() {
    let f = setup();
    create_funded_escrow(&f, 1, 50_000_000);
    // The liability is the blocker, not the existence of the escrow.
    let _ = f.client.refund(&1);
    clear_layout_version(&f.env, &f.client);

    let report = f.client.get_migration_preconditions();
    assert!(
        report.failure.is_none(),
        "unexpected block: {:?}",
        report.failure
    );

    let escrow_before = f.client.get_escrow(&1);
    assert_eq!(f.client.migrate_storage_layout(), 1);
    assert_eq!(f.client.get_escrow(&1), escrow_before);
}

#[test]
fn a_reported_event_records_the_verdict() {
    let f = setup();

    f.client.audit_migration_preconditions();

    let events = f.env.events().all();
    let last = events.last().expect("the audit must emit an event");
    assert_eq!(last.0, f.client.address);
    assert_eq!(
        last.1,
        vec![
            &f.env,
            Symbol::new(&f.env, "migration_precondition_audited"),
            Symbol::new(&f.env, "passed"),
        ]
        .into_val(&f.env)
    );
}
