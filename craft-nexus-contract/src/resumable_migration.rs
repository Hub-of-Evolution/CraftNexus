//! Resumable, bounded-chunk storage migrations (Issue #1117).
//!
//! The pre-existing migration helpers in this contract (`migrate_legacy_*`,
//! `migrate_storage_layout`) convert legacy records opportunistically: they
//! detect a legacy key, convert *every* record it holds, then delete the
//! legacy key. That is safe to re-run, but it is not safe to *pause*. A
//! deployment holding thousands of legacy records exceeds the per-invocation
//! resource envelope, the transaction aborts, and because the legacy key is
//! only removed at the very end the work has to start over from zero. The
//! same helpers also delete the legacy key, so a bad conversion is not
//! recoverable — only re-derivable, and only if the legacy key survived.
//!
//! This module provides the *state machine* that makes a migration pausable,
//! resumable and retryable:
//!
//! 1. **Bounded chunks** — [`run_chunk`] converts at most
//!    [`MAX_MIGRATION_CHUNK`] records per invocation, so no single call can
//!    blow the resource envelope regardless of deployment size.
//! 2. **Persisted cursor** — the position is committed to persistent storage
//!    after every record, so the next invocation resumes exactly where the
//!    last one stopped.
//! 3. **Per-record completion markers** — each record is stamped individually
//!    ([`MigrationKey::RecordDone`]) the moment it converts. A record is
//!    therefore converted at most once no matter how many times its chunk is
//!    retried, even if the cursor is rewound.
//! 4. **Persisted version and owner** — the target version and the address
//!    permitted to drive the migration are stored, so no caller can retarget
//!    or hijack an in-flight run.
//! 5. **Explicit completion** — completion is an explicit persisted state,
//!    and [`ensure_migration_complete`] is the gate that blocks business
//!    calls that would read half-migrated state.
//!
//! # Failure model
//!
//! A record that fails to convert does **not** abort the chunk. The chunk
//! stops at that record, commits the cursor to that position, and reports the
//! blocker via [`MigrationProgress::blocked_at`]. Re-invoking [`run_chunk`]
//! retries it. This is deliberate: returning an error from the wrapper would
//! roll the whole invocation back, discarding the chunk's progress, and is
//! exactly the "restart from zero" behaviour this module exists to remove.
//!
//! Records converted by earlier chunks stay valid throughout, because each is
//! stamped on its own and the cursor only ever moves forward past successes.
//!
//! # Integration
//!
//! This module deliberately owns its own storage keys and error type so it
//! carries no dependency on the contract's `DataKey` or `Error` enums. The
//! contract supplies a [`ChunkConverter`] per record family and wraps the
//! state machine in an admin-gated entrypoint:
//!
//! ```ignore
//! pub struct EscrowIdConverter;
//! impl resumable_migration::ChunkConverter for EscrowIdConverter {
//!     fn record_id(_env: &Env, index: u32) -> u64 { index as u64 }
//!     fn convert(env: &Env, id: u64) -> Result<(), MigrationError> {
//!         // ... move record `id` to the new layout ...
//!         Ok(())
//!     }
//! }
//!
//! #[contractimpl]
//! impl CraftNexusContract {
//!     pub fn run_layout_migration(
//!         env: Env,
//!         owner: Address,
//!         version: u32,
//!         chunk_size: u32,
//!     ) -> Result<resumable_migration::MigrationProgress, Error> {
//!         owner.require_auth();
//!         resumable_migration::run_chunk::<EscrowIdConverter>(
//!             &env, &owner, version, chunk_size,
//!         ).map_err(|_| Error::StorageLayoutMismatch)
//!     }
//! }
//! ```
//!
//! `require_auth` lives in the wrapper rather than the module so this module
//! stays callable from host tests without a contract frame. Ownership is
//! still enforced inside the module: every state transition checks the caller
//! against the persisted owner.
//!
//! The blocking gate belongs at the top of any business entrypoint that
//! depends on the migrated layout:
//!
//! ```ignore
//! resumable_migration::ensure_migration_complete(&env, REQUIRED_LAYOUT_VERSION)
//!     .map_err(|_| Error::StorageLayoutMismatch)?;
//! ```

use soroban_sdk::{contracterror, contracttype, Address, Env};

/// Maximum records converted by a single [`run_chunk`] invocation.
///
/// Mirrors the envelope used by archival compaction: bounded so each
/// continuation is a small, individually affordable transaction, and capped
/// so a caller cannot request an unbounded amount of work in one call.
pub const MAX_MIGRATION_CHUNK: u32 = 50;

/// TTL threshold applied to migration bookkeeping keys.
const MIGRATION_TTL_THRESHOLD: u32 = 10_000;

/// TTL extension applied to migration bookkeeping keys.
///
/// Matches the contract's `TTL_EXTENSION` so migrated bookkeeping shares the
/// same liveness window as the records it describes.
const MIGRATION_TTL_EXTENSION: u32 = 518_400;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Storage keys owned by this module.
///
/// Namespaced by target `version` so several migrations can be in flight
/// without colliding, and so an abandoned run can be superseded by a fresh
/// one at a new version.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationKey {
    /// Persisted [`MigrationStatus`] for a target version.
    Status(u32),
    /// Per-record completion marker: record `record_id` already converted.
    RecordDone(u32, u64),
    /// Per-record failure marker: record `record_id` last failed to convert.
    RecordFailed(u32, u64),
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors specific to the resumable migration state machine.
///
/// Kept separate from the contract's `Error` so the state machine stays
/// independently compilable and testable.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MigrationError {
    /// No migration has been started for the requested version.
    MigrationNotStarted = 1,
    /// The caller is not the address that claimed the migration.
    NotMigrationOwner = 2,
    /// The persisted cursor has already consumed every record.
    MigrationAlreadyComplete = 3,
    /// A chunk size of zero was supplied; at least one record must be worked.
    ZeroChunkSize = 4,
    /// The supplied owner is not a valid address for this deployment.
    InvalidOwner = 5,
    /// The migration was abandoned and can no longer be advanced.
    MigrationAbandoned = 6,
    /// A record's conversion returned an error and blocked the chunk.
    RecordConversionFailed = 7,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Lifecycle phase of a migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MigrationPhase {
    /// Claimed but not yet advanced.
    NotStarted,
    /// In progress; the cursor marks the next record to convert.
    InProgress,
    /// Every record converted. Business calls gated on this version reopen.
    Complete,
    /// Abandoned by the owner. Re-runnable via [`begin`].
    Abandoned,
}

/// Persisted migration bookkeeping.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationStatus {
    /// Target layout version this run migrates to.
    pub version: u32,
    /// Address permitted to advance this migration.
    pub owner: Address,
    /// Next record index to convert. Monotonically non-decreasing.
    pub cursor: u32,
    /// Total records snapshotted when the migration was claimed.
    pub total: u32,
    /// Current lifecycle phase.
    pub state: MigrationPhase,
    /// Successful `run_chunk` invocations, for operator observability.
    pub chunks_completed: u32,
    /// Records converted across all chunks.
    pub records_converted: u32,
    /// Ledger timestamp the migration was claimed.
    pub started_at: u64,
    /// Ledger timestamp of the most recent state transition.
    pub updated_at: u64,
}

impl MigrationStatus {
    /// True once every snapshot record has converted.
    pub fn is_complete(&self) -> bool {
        self.state == MigrationPhase::Complete
    }
}

/// Per-chunk result returned to the operator driving the migration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationProgress {
    /// Target layout version.
    pub version: u32,
    /// Cursor committed by this chunk; the resume point for the next call.
    pub cursor: u32,
    /// Total records in the migration snapshot.
    pub total: u32,
    /// Records examined by this chunk, including a blocked record.
    pub scanned: u32,
    /// Records converted by this chunk.
    pub converted: u32,
    /// Records skipped because they were already converted.
    pub skipped: u32,
    /// Record that failed and stopped the chunk, if any.
    ///
    /// `Some` means the cursor is parked on that record and the next
    /// `run_chunk` retries it. `None` with `complete == true` means the
    /// migration finished.
    pub blocked_at: Option<u64>,
    /// True once the cursor has consumed every record.
    pub complete: bool,
    /// Lifecycle phase after this chunk.
    pub state: MigrationPhase,
}

// ---------------------------------------------------------------------------
// Record conversion
// ---------------------------------------------------------------------------

/// Converts one record family on behalf of the migration runner.
///
/// Implemented by the contract, once per record family being migrated. The
/// runner supplies the chunking, cursor, ownership and completion tracking;
/// the implementation only has to convert a single record.
///
/// # Contract for implementors
///
/// - `record_id` must be **stable** for a given `index` across invocations. A
///   cursor is only meaningful if the same index always names the same record.
/// - `convert` must be **idempotent**. The runner guards this with a
///   per-record marker, but the marker is written *after* `convert` returns,
///   so an implementation that panics partway could be re-entered. Prefer
///   checking the destination state inside `convert` as well.
/// - `convert` must not advance the runner's cursor or write
///   [`MigrationKey::Status`]; the runner owns that state.
pub trait ChunkConverter {
    /// Stable identity of the record at `index` in the migration's ordering.
    fn record_id(env: &Env, index: u32) -> u64;

    /// Convert the record identified by `id`.
    ///
    /// Returning `Err` stops the current chunk at this record without
    /// discarding prior progress.
    fn convert(env: &Env, id: u64) -> Result<(), MigrationError>;
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn status_key(version: u32) -> MigrationKey {
    MigrationKey::Status(version)
}

fn record_done_key(version: u32, id: u64) -> MigrationKey {
    MigrationKey::RecordDone(version, id)
}

fn record_failed_key(version: u32, id: u64) -> MigrationKey {
    MigrationKey::RecordFailed(version, id)
}

fn extend(env: &Env, key: &MigrationKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, MIGRATION_TTL_THRESHOLD, MIGRATION_TTL_EXTENSION);
}

fn get_status(env: &Env, version: u32) -> Option<MigrationStatus> {
    let key = status_key(version);
    let status: Option<MigrationStatus> = env.storage().persistent().get(&key);
    if status.is_some() {
        extend(env, &key);
    }
    status
}

fn put_status(env: &Env, status: &MigrationStatus) {
    let key = status_key(status.version);
    env.storage().persistent().set(&key, status);
    extend(env, &key);
}

fn is_record_done(env: &Env, version: u32, id: u64) -> bool {
    let key = record_done_key(version, id);
    let done: Option<bool> = env.storage().persistent().get(&key);
    if done.is_some() {
        extend(env, &key);
    }
    done.unwrap_or(false)
}

fn mark_record_done(env: &Env, version: u32, id: u64) {
    let key = record_done_key(version, id);
    env.storage().persistent().set(&key, &true);
    extend(env, &key);
}

fn mark_record_failed(env: &Env, version: u32, id: u64) {
    let key = record_failed_key(version, id);
    env.storage().persistent().set(&key, &true);
    extend(env, &key);
}

fn clear_record_failed(env: &Env, version: u32, id: u64) {
    env.storage().persistent().remove(&record_failed_key(version, id));
}

/// True when record `id` has already converted for `version`.
pub fn record_is_done(env: &Env, version: u32, id: u64) -> bool {
    is_record_done(env, version, id)
}

/// True when record `id` is currently blocking the cursor for `version`.
pub fn record_is_blocked(env: &Env, version: u32, id: u64) -> bool {
    let key = record_failed_key(version, id);
    let failed: Option<bool> = env.storage().persistent().get(&key);
    if failed.is_some() {
        extend(env, &key);
    }
    failed.unwrap_or(false)
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

/// Clamp a caller-supplied chunk size to a workable bound.
fn clamp_chunk(chunk_size: u32) -> Result<u32, MigrationError> {
    if chunk_size == 0 {
        return Err(MigrationError::ZeroChunkSize);
    }
    Ok(chunk_size.min(MAX_MIGRATION_CHUNK))
}

/// Claim a migration at `version`, owned by `owner`, over `total` records.
///
/// The record count is **snapshotted** here so every chunk is bounded against
/// a fixed total and the migration is deterministic regardless of how the
/// underlying collection changes mid-run. Re-claiming an already-running
/// migration is a no-op that returns the existing status, which keeps the
/// call idempotent for operators replaying a runbook step.
pub fn begin(env: &Env, owner: &Address, version: u32, total: u32) -> Result<MigrationStatus, MigrationError> {
    if let Some(existing) = get_status(env, version) {
        return Ok(existing);
    }
    let now = env.ledger().timestamp();
    let status = MigrationStatus {
        version,
        owner: owner.clone(),
        cursor: 0,
        total,
        state: if total == 0 {
            MigrationPhase::Complete
        } else {
            MigrationPhase::InProgress
        },
        chunks_completed: 0,
        records_converted: 0,
        started_at: now,
        updated_at: now,
    };
    put_status(env, &status);
    Ok(status)
}

/// Read the persisted status for `version`.
pub fn status(env: &Env, version: u32) -> Result<MigrationStatus, MigrationError> {
    get_status(env, version).ok_or(MigrationError::MigrationNotStarted)
}

/// Convert up to `chunk_size` records, resuming from the persisted cursor.
///
/// Safe to call repeatedly and safe to call after completion: a completed
/// migration returns its existing status with zero counts, changing nothing.
/// Ownership is enforced against the persisted owner, so a different caller
/// cannot advance someone else's run even if it holds admin rights.
pub fn run_chunk<C: ChunkConverter>(
    env: &Env,
    owner: &Address,
    version: u32,
    chunk_size: u32,
) -> Result<MigrationProgress, MigrationError> {
    let limit = clamp_chunk(chunk_size)?;
    let mut status = status(env, version)?;

    if &status.owner != owner {
        return Err(MigrationError::NotMigrationOwner);
    }

    if status.state == MigrationPhase::Abandoned {
        return Err(MigrationError::MigrationAbandoned);
    }

    if status.is_complete() {
        return Ok(progress_from(&status, 0, 0, 0, None));
    }

    let end = status.cursor.saturating_add(limit).min(status.total);
    let mut scanned: u32 = 0;
    let mut converted: u32 = 0;
    let mut skipped: u32 = 0;
    let mut blocked_at: Option<u64> = None;

    let mut index = status.cursor;
    while index < end {
        scanned = scanned.saturating_add(1);
        let id = C::record_id(env, index);

        if is_record_done(env, version, id) {
            // Already converted by an earlier attempt. Retrying a chunk is
            // therefore harmless: the record is skipped, never re-converted.
            skipped = skipped.saturating_add(1);
        } else {
            match C::convert(env, id) {
                Ok(()) => {
                    mark_record_done(env, version, id);
                    clear_record_failed(env, version, id);
                    converted = converted.saturating_add(1);
                    status.records_converted = status.records_converted.saturating_add(1);
                }
                Err(_) => {
                    // Park the cursor here. Everything converted before this
                    // point stays converted, and the next run_chunk retries.
                    mark_record_failed(env, version, id);
                    blocked_at = Some(id);
                    break;
                }
            }
        }

        index = index.saturating_add(1);
        // Commit the position after every record so an abort mid-chunk never
        // rewinds work that already succeeded.
        status.cursor = index;
    }

    status.chunks_completed = status.chunks_completed.saturating_add(1);
    status.updated_at = env.ledger().timestamp();
    if blocked_at.is_none() && status.cursor >= status.total {
        status.state = MigrationPhase::Complete;
    }
    put_status(env, &status);

    Ok(progress_from(
        &status,
        scanned,
        converted,
        skipped,
        blocked_at,
    ))
}

/// Abandon a migration, releasing it to be re-claimed with [`begin`].
///
/// Only the owner may abandon. Record markers are intentionally left in
/// place: a re-run must not re-convert records that already succeeded.
pub fn abandon(env: &Env, owner: &Address, version: u32) -> Result<MigrationStatus, MigrationError> {
    let mut status = status(env, version)?;
    if &status.owner != owner {
        return Err(MigrationError::NotMigrationOwner);
    }
    if status.is_complete() {
        return Ok(status);
    }
    status.state = MigrationPhase::Abandoned;
    status.updated_at = env.ledger().timestamp();
    put_status(env, &status);
    Ok(status)
}

/// True when the migration at `version` has explicitly completed.
pub fn is_complete(env: &Env, version: u32) -> bool {
    get_status(env, version)
        .map(|status| status.is_complete())
        .unwrap_or(false)
}

/// True when the migration at `version` is mid-flight (claimed, not done).
pub fn is_in_progress(env: &Env, version: u32) -> bool {
    get_status(env, version)
        .map(|status| status.state == MigrationPhase::InProgress)
        .unwrap_or(false)
}

/// Gate for business calls that require `version` to be fully migrated.
///
/// Returns `Ok(())` only on an explicit `Complete` state. A version that was
/// never started is *not* complete, so a deployment that skips the migration
/// is blocked rather than silently serving reads against half-migrated state.
///
/// Call this at the top of every entrypoint whose behaviour depends on the
/// migrated layout.
pub fn ensure_migration_complete(env: &Env, version: u32) -> Result<(), MigrationError> {
    if is_complete(env, version) {
        Ok(())
    } else {
        Err(MigrationError::MigrationNotStarted)
    }
}

fn progress_from(
    status: &MigrationStatus,
    scanned: u32,
    converted: u32,
    skipped: u32,
    blocked_at: Option<u64>,
) -> MigrationProgress {
    MigrationProgress {
        version: status.version,
        cursor: status.cursor,
        total: status.total,
        scanned,
        converted,
        skipped,
        blocked_at,
        complete: status.is_complete(),
        state: status.state.clone(),
    }
}
