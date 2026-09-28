#![cfg(test)]

//! Tests for the resumable migration state machine (Issue #1117).
//!
//! The three acceptance criteria from the issue are covered explicitly:
//!
//! - AC1 *a failed chunk leaves prior records valid* —
//!   [`failed_chunk_leaves_prior_records_valid`],
//!   [`failed_chunk_leaves_prior_chunks_valid_across_many_resumes`],
//!   [`resume_after_failure_completes_the_migration`]
//! - AC2 *retrying a completed chunk is harmless* —
//!   [`retrying_a_completed_chunk_is_harmless`],
//!   [`rewound_cursor_skips_already_converted_records`],
//!   [`chunk_after_completion_changes_nothing`]
//! - AC3 *completion is explicit and blocks incompatible business calls* —
//!   [`completion_is_explicit_and_blocks_business_calls`],
//!   [`gate_blocks_unstarted_version`],
//!   [`failed_migration_keeps_the_gate_closed`]
//!
//! Persistent storage is only reachable from inside a contract frame, so
//! every call goes through [`Harness::in_contract`], mirroring the
//! `env.as_contract(...)` pattern the contract's own tests use to fabricate
//! pre-migration deployments.

use crate::resumable_migration::{
    self, ChunkConverter, MigrationError, MigrationKey, MigrationPhase, MigrationProgress,
    MigrationStatus, MAX_MIGRATION_CHUNK,
};
use soroban_sdk::{
    contracttype, testutils::Address as _, Address, Env, Vec as SorobanVec,
};

use crate::CraftNexusContract;

/// Target version used across most tests.
const VER: u32 = 2;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum TestKey {
    /// Append-only log of every record the converter actually touched.
    ///
    /// A duplicated entry is the observable symptom of a non-idempotent
    /// conversion, which is exactly what AC2 forbids.
    ConversionLog,
    /// Record id the converter should fail on; `u32::MAX` disables failure.
    FailAt,
}

/// Converts every record, appending to the log so re-conversion is visible.
struct CountingConverter;

impl ChunkConverter for CountingConverter {
    fn record_id(_env: &Env, index: u32) -> u64 {
        index as u64
    }

    fn convert(env: &Env, id: u64) -> Result<(), MigrationError> {
        let mut log: SorobanVec<u64> = env
            .storage()
            .persistent()
            .get(&TestKey::ConversionLog)
            .unwrap_or(SorobanVec::new(env));
        log.push_back(id);
        env.storage()
            .persistent()
            .set(&TestKey::ConversionLog, &log);
        Ok(())
    }
}

/// Same as [`CountingConverter`], but refuses to convert one specific record.
struct FallibleConverter;

impl ChunkConverter for FallibleConverter {
    fn record_id(_env: &Env, index: u32) -> u64 {
        index as u64
    }

    fn convert(env: &Env, id: u64) -> Result<(), MigrationError> {
        let fail_at: u32 = env
            .storage()
            .persistent()
            .get(&TestKey::FailAt)
            .unwrap_or(u32::MAX);
        if id == fail_at as u64 {
            return Err(MigrationError::RecordConversionFailed);
        }
        let mut log: SorobanVec<u64> = env
            .storage()
            .persistent()
            .get(&TestKey::ConversionLog)
            .unwrap_or(SorobanVec::new(env));
        log.push_back(id);
        env.storage()
            .persistent()
            .set(&TestKey::ConversionLog, &log);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Owns the [`Env`] and wraps every storage-touching call in a contract frame.
///
/// `as_contract` refuses to run against an unregistered address, so the main
/// contract is registered purely to obtain one. It is never called and the
/// state machine under test does not depend on it.
struct Harness {
    env: Env,
    contract: Address,
}

impl Harness {
    fn new() -> Self {
        let env = Env::default();
        let contract = env.register_contract(None, CraftNexusContract);
        Self { env, contract }
    }

    /// Run `f` with persistent storage accessible.
    fn in_contract<R>(&self, f: impl FnOnce(&Env) -> R) -> R {
        self.env.as_contract(&self.contract, || f(&self.env))
    }

    fn owner(&self) -> Address {
        Address::generate(&self.env)
    }

    // ── Module surface ───────────────────────────────────────────────

    fn begin(&self, owner: &Address, version: u32, total: u32) -> MigrationStatus {
        self.in_contract(|env| resumable_migration::begin(env, owner, version, total).unwrap())
    }

    fn run_chunk<C: ChunkConverter>(
        &self,
        owner: &Address,
        version: u32,
        chunk: u32,
    ) -> Result<MigrationProgress, MigrationError> {
        self.in_contract(|env| resumable_migration::run_chunk::<C>(env, owner, version, chunk))
    }

    fn status(&self, version: u32) -> Result<MigrationStatus, MigrationError> {
        self.in_contract(|env| resumable_migration::status(env, version))
    }

    fn is_complete(&self, version: u32) -> bool {
        self.in_contract(|env| resumable_migration::is_complete(env, version))
    }

    fn is_in_progress(&self, version: u32) -> bool {
        self.in_contract(|env| resumable_migration::is_in_progress(env, version))
    }

    fn ensure_complete(&self, version: u32) -> Result<(), MigrationError> {
        self.in_contract(|env| resumable_migration::ensure_migration_complete(env, version))
    }

    fn abandon(&self, owner: &Address, version: u32) -> Result<MigrationStatus, MigrationError> {
        self.in_contract(|env| resumable_migration::abandon(env, owner, version))
    }

    fn record_is_done(&self, version: u32, id: u64) -> bool {
        self.in_contract(|env| resumable_migration::record_is_done(env, version, id))
    }

    fn record_is_blocked(&self, version: u32, id: u64) -> bool {
        self.in_contract(|env| resumable_migration::record_is_blocked(env, version, id))
    }

    // ── Fixture state ────────────────────────────────────────────────

    /// Make the converter fail on `id`; [`u64::MAX`] disables failure.
    fn set_fail_at(&self, id: u64) {
        self.in_contract(|env| env.storage().persistent().set(&TestKey::FailAt, &(id as u32)));
    }

    /// Number of conversions actually performed, per record id.
    fn conversion_count(&self, id: u64) -> u32 {
        self.in_contract(|env| {
            let log: SorobanVec<u64> = env
                .storage()
                .persistent()
                .get(&TestKey::ConversionLog)
                .unwrap_or(SorobanVec::new(env));
            (0..log.len())
                .filter(|i| log.get(*i) == Some(id))
                .count() as u32
        })
    }

    fn log_len(&self) -> u32 {
        self.in_contract(|env| {
            let log: SorobanVec<u64> = env
                .storage()
                .persistent()
                .get(&TestKey::ConversionLog)
                .unwrap_or(SorobanVec::new(env));
            log.len()
        })
    }

    fn log_contains(&self, id: u64) -> bool {
        self.conversion_count(id) > 0
    }

    /// Force the cursor backwards to simulate a rewound or replayed run.
    fn rewind_cursor(&self, version: u32, cursor: u32) {
        self.in_contract(|env| {
            let key = MigrationKey::Status(version);
            let mut current: MigrationStatus = env.storage().persistent().get(&key).unwrap();
            current.cursor = cursor;
            current.state = MigrationPhase::InProgress;
            env.storage().persistent().set(&key, &current);
        });
    }
}

// ---------------------------------------------------------------------------
// Claiming
// ---------------------------------------------------------------------------

#[test]
fn begin_persists_version_cursor_and_owner() {
    let h = Harness::new();
    let o = h.owner();

    let started = h.begin(&o, VER, 100);

    assert_eq!(started.version, VER);
    assert_eq!(started.owner, o);
    assert_eq!(started.cursor, 0);
    assert_eq!(started.total, 100);
    assert_eq!(started.state, MigrationPhase::InProgress);
    assert!(!started.is_complete());

    // And it survives a round-trip through storage.
    let persisted = h.status(VER).unwrap();
    assert_eq!(persisted.version, VER);
    assert_eq!(persisted.owner, o);
    assert_eq!(persisted.cursor, 0);
    assert_eq!(persisted.total, 100);
}

#[test]
fn begin_is_idempotent_and_preserves_progress() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);
    h.run_chunk::<CountingConverter>(&o, VER, 4).unwrap();

    // Re-claiming a runbook step must not reset the cursor.
    let again = h.begin(&o, VER, 10);

    assert_eq!(again.cursor, 4);
    assert_eq!(again.total, 10);
}

#[test]
fn begin_with_zero_total_completes_immediately() {
    let h = Harness::new();
    let o = h.owner();

    let started = h.begin(&o, VER, 0);

    assert_eq!(started.state, MigrationPhase::Complete);
    assert!(h.is_complete(VER));
}

#[test]
fn status_for_unstarted_version_errors() {
    let h = Harness::new();
    assert_eq!(h.status(99), Err(MigrationError::MigrationNotStarted));
}

// ---------------------------------------------------------------------------
// Bounded chunks
// ---------------------------------------------------------------------------

#[test]
fn zero_chunk_size_is_rejected() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);

    assert_eq!(
        h.run_chunk::<CountingConverter>(&o, VER, 0),
        Err(MigrationError::ZeroChunkSize)
    );
}

#[test]
fn chunk_size_is_clamped_to_max() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10_000);

    let progress = h.run_chunk::<CountingConverter>(&o, VER, 10_000).unwrap();

    assert_eq!(progress.cursor, MAX_MIGRATION_CHUNK);
    assert_eq!(progress.scanned, MAX_MIGRATION_CHUNK);
}

#[test]
fn chunk_converts_only_the_bounded_window() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 100);

    let progress = h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();

    assert_eq!(progress.cursor, 3);
    assert_eq!(progress.scanned, 3);
    assert_eq!(progress.converted, 3);
    assert_eq!(progress.total, 100);
    assert!(!progress.complete);
    assert_eq!(progress.state, MigrationPhase::InProgress);
    assert_eq!(h.log_len(), 3);
}

#[test]
fn successive_chunks_walk_the_full_snapshot() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);

    for expected in [2u32, 4, 6, 8, 10] {
        let progress = h.run_chunk::<CountingConverter>(&o, VER, 2).unwrap();
        assert_eq!(progress.cursor, expected);
    }

    let final_status = h.status(VER).unwrap();
    assert_eq!(final_status.cursor, 10);
    assert_eq!(final_status.records_converted, 10);
    assert_eq!(h.log_len(), 10);
}

// ---------------------------------------------------------------------------
// AC2: retrying a completed chunk is harmless
// ---------------------------------------------------------------------------

#[test]
fn retrying_a_completed_chunk_is_harmless() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);

    // First chunk converts records 0..=2.
    let first = h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();
    assert_eq!(first.converted, 3);
    let log_after_first = h.log_len();

    // Simulate a rewind and replay the very same window.
    h.rewind_cursor(VER, 0);
    let replay = h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();

    assert_eq!(replay.converted, 0, "replay must not re-convert");
    assert_eq!(replay.skipped, 3, "replay must skip stamped records");
    assert_eq!(replay.cursor, 3);
    assert_eq!(
        h.log_len(),
        log_after_first,
        "conversion log must be unchanged by a replay"
    );
    for id in 0..3u64 {
        assert_eq!(h.conversion_count(id), 1, "record {id} converted twice");
    }
}

#[test]
fn rewound_cursor_skips_already_converted_records() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);
    h.run_chunk::<CountingConverter>(&o, VER, 6).unwrap();
    assert_eq!(h.log_len(), 6);

    h.rewind_cursor(VER, 2);
    let progress = h.run_chunk::<CountingConverter>(&o, VER, 2).unwrap();

    // Records 2 and 3 were already converted; the window is spent skipping.
    assert_eq!(progress.skipped, 2);
    assert_eq!(progress.converted, 0);
    assert_eq!(progress.cursor, 4);
    assert_eq!(h.log_len(), 6, "no duplicate conversions");
}

#[test]
fn chunk_after_completion_changes_nothing() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 4);
    h.run_chunk::<CountingConverter>(&o, VER, 4).unwrap();
    let before = h.status(VER).unwrap();

    let after = h.run_chunk::<CountingConverter>(&o, VER, 4).unwrap();

    assert!(after.complete);
    assert_eq!(after.scanned, 0);
    assert_eq!(after.converted, 0);
    assert_eq!(after.skipped, 0);
    assert_eq!(after.cursor, before.cursor);
    assert_eq!(h.log_len(), 4, "no work redone after completion");

    // No bookkeeping field moves at all, not even the chunk counter.
    let now = h.status(VER).unwrap();
    assert_eq!(now.cursor, before.cursor);
    assert_eq!(now.records_converted, before.records_converted);
    assert_eq!(now.chunks_completed, before.chunks_completed);
    assert_eq!(now.state, MigrationPhase::Complete);
}

// ---------------------------------------------------------------------------
// AC1: a failed chunk leaves prior records valid
// ---------------------------------------------------------------------------

#[test]
fn failed_chunk_leaves_prior_records_valid() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);

    // Records 0..=3 convert cleanly.
    let first = h.run_chunk::<FallibleConverter>(&o, VER, 4).unwrap();
    assert_eq!(first.converted, 4);
    assert_eq!(first.cursor, 4);
    for id in 0..4u64 {
        assert!(h.log_contains(id), "record {id} must be valid");
        assert_eq!(h.conversion_count(id), 1);
        assert!(h.record_is_done(VER, id));
    }

    // Record 4 now fails partway through the next chunk.
    h.set_fail_at(4);
    let failed = h.run_chunk::<FallibleConverter>(&o, VER, 4).unwrap();

    // The chunk reports the blocker rather than erroring out, so the work it
    // already committed is not rolled back with it.
    assert_eq!(failed.blocked_at, Some(4));
    assert!(!failed.complete);
    assert_eq!(failed.state, MigrationPhase::InProgress);

    // Cursor parked on the failing record: progress before it is preserved.
    assert_eq!(failed.cursor, 4);
    assert_eq!(failed.scanned, 1);
    assert_eq!(failed.converted, 0);

    // AC1: everything converted before the failure is still valid.
    for id in 0..4u64 {
        assert!(h.log_contains(id), "record {id} must survive");
        assert_eq!(h.conversion_count(id), 1, "record {id} re-converted");
        assert!(h.record_is_done(VER, id));
    }
    assert!(h.record_is_blocked(VER, 4));
    assert!(!h.is_complete(VER));
}

#[test]
fn failed_chunk_leaves_prior_chunks_valid_across_many_resumes() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);

    // Convert 0..=1, then repeatedly fail on the next record.
    h.run_chunk::<FallibleConverter>(&o, VER, 2).unwrap();
    h.set_fail_at(2);

    for _ in 0..3 {
        let blocked = h.run_chunk::<FallibleConverter>(&o, VER, 5).unwrap();
        assert_eq!(blocked.blocked_at, Some(2));
        assert_eq!(blocked.cursor, 2, "cursor must not drift past the failure");
        assert_eq!(h.log_len(), 2, "no partial records committed");
        for id in 0..2u64 {
            assert_eq!(h.conversion_count(id), 1);
        }
    }
}

#[test]
fn resume_after_failure_completes_the_migration() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 6);

    h.run_chunk::<FallibleConverter>(&o, VER, 2).unwrap();
    h.set_fail_at(2);
    let blocked = h.run_chunk::<FallibleConverter>(&o, VER, 4).unwrap();
    assert_eq!(blocked.blocked_at, Some(2));
    assert_eq!(blocked.cursor, 2, "cursor stays parked on the failure");

    // Operator repairs the underlying cause and resumes. The window from the
    // parked cursor is records 2..=5, so this chunk drains the remainder and
    // is the one that flips the migration to Complete.
    h.set_fail_at(u64::MAX);
    let resumed = h.run_chunk::<FallibleConverter>(&o, VER, 4).unwrap();

    assert_eq!(resumed.blocked_at, None);
    assert_eq!(resumed.converted, 4, "records 2, 3, 4 and 5");
    assert_eq!(resumed.cursor, 6);
    assert!(resumed.complete);
    assert!(!h.record_is_blocked(VER, 2));
    assert!(h.is_complete(VER));

    // Every record converted exactly once across the whole interrupted run.
    for id in 0..6u64 {
        assert_eq!(h.conversion_count(id), 1, "record {id} converted twice");
    }
    assert_eq!(h.log_len(), 6);
}

// ---------------------------------------------------------------------------
// AC3: completion is explicit and blocks incompatible business calls
// ---------------------------------------------------------------------------

#[test]
fn completion_is_explicit_and_blocks_business_calls() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 5);

    // While in flight the gate rejects business calls.
    assert!(!h.is_complete(VER));
    assert!(h.is_in_progress(VER));
    assert_eq!(
        h.ensure_complete(VER),
        Err(MigrationError::MigrationNotStarted)
    );

    h.run_chunk::<CountingConverter>(&o, VER, 2).unwrap();
    assert_eq!(
        h.ensure_complete(VER),
        Err(MigrationError::MigrationNotStarted),
        "a partially migrated deployment must stay blocked"
    );

    // Completion is only reached on the chunk that consumes the last record.
    h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();
    assert!(h.is_complete(VER));
    assert!(!h.is_in_progress(VER));
    assert_eq!(h.ensure_complete(VER), Ok(()));
    assert_eq!(h.status(VER).unwrap().state, MigrationPhase::Complete);
}

#[test]
fn gate_blocks_unstarted_version() {
    let h = Harness::new();
    // A deployment that never ran the migration must not pass the gate.
    assert!(!h.is_complete(VER));
    assert_eq!(
        h.ensure_complete(VER),
        Err(MigrationError::MigrationNotStarted)
    );
}

#[test]
fn gate_is_scoped_per_version() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, 2, 2);
    h.run_chunk::<CountingConverter>(&o, 2, 2).unwrap();

    assert_eq!(h.ensure_complete(2), Ok(()));
    // A later, unrelated migration is independently gated.
    assert_eq!(
        h.ensure_complete(3),
        Err(MigrationError::MigrationNotStarted)
    );
}

#[test]
fn failed_migration_keeps_the_gate_closed() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 4);
    h.set_fail_at(1);
    h.run_chunk::<FallibleConverter>(&o, VER, 4).unwrap();

    assert_eq!(
        h.ensure_complete(VER),
        Err(MigrationError::MigrationNotStarted)
    );
}

// ---------------------------------------------------------------------------
// Ownership
// ---------------------------------------------------------------------------

#[test]
fn non_owner_cannot_advance_migration() {
    let h = Harness::new();
    let o = h.owner();
    let interloper = h.owner();
    h.begin(&o, VER, 10);

    assert_eq!(
        h.run_chunk::<CountingConverter>(&interloper, VER, 3),
        Err(MigrationError::NotMigrationOwner)
    );
    assert_eq!(h.status(VER).unwrap().cursor, 0, "cursor must not move");
    assert_eq!(h.log_len(), 0);
}

#[test]
fn non_owner_cannot_abandon_migration() {
    let h = Harness::new();
    let o = h.owner();
    let interloper = h.owner();
    h.begin(&o, VER, 10);

    assert_eq!(
        h.abandon(&interloper, VER),
        Err(MigrationError::NotMigrationOwner)
    );
    assert!(h.is_in_progress(VER));
}

#[test]
fn abandoned_migration_rejects_further_work() {
    let h = Harness::new();
    let o = h.owner();
    h.begin(&o, VER, 10);
    h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();
    h.abandon(&o, VER).unwrap();

    assert_eq!(h.status(VER).unwrap().state, MigrationPhase::Abandoned);
    assert_eq!(
        h.run_chunk::<CountingConverter>(&o, VER, 3),
        Err(MigrationError::MigrationAbandoned)
    );
    // Abandonment must not reopen the business-call gate.
    assert_eq!(
        h.ensure_complete(VER),
        Err(MigrationError::MigrationNotStarted)
    );
}

#[test]
fn abandoned_migration_can_be_reclaimed_by_a_new_owner() {
    let h = Harness::new();
    let o = h.owner();
    let successor = h.owner();
    h.begin(&o, VER, 10);
    h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();
    h.abandon(&o, VER).unwrap();

    // Re-claiming at a fresh version supersedes the abandoned run.
    let reclaimed = h.begin(&successor, 3, 10);
    assert_eq!(reclaimed.owner, successor);
    assert_eq!(reclaimed.cursor, 0);
    assert_eq!(reclaimed.state, MigrationPhase::InProgress);
}

#[test]
fn reclaiming_preserves_prior_conversion_markers() {
    let h = Harness::new();
    let o = h.owner();
    let successor = h.owner();
    h.begin(&o, VER, 10);
    h.run_chunk::<CountingConverter>(&o, VER, 3).unwrap();
    h.abandon(&o, VER).unwrap();

    // The successor's run is versioned independently; the old version's
    // markers remain, so a later resume of VER cannot re-convert.
    h.begin(&successor, 3, 10);
    assert!(h.record_is_done(VER, 0));
    assert!(h.record_is_done(VER, 2));
    assert!(!h.record_is_done(3, 0));
}
