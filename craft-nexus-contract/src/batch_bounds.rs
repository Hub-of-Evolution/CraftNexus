//! Configurable bounds for batch escrow operations (Issue #1074).
//!
//! Batch methods must reject work sizes that exceed the configured contract
//! resource budgets **before** any escrow is created or any token moves.
//! Soroban aborts the whole invocation when its resource envelope
//! (instructions, footprint entries, TTL extends) is breached, but only after
//! the budget has actually been burnt. Rejecting a batch *before* mutation is
//! cheaper, deterministic, and gives callers a clean error instead of a host
//! abort.
//!
//! This module centralises the two explicitly-bounded axes introduced by
//! Issue #1074:
//!
//! 1. **Item-count ceiling** – the maximum number of escrows a single batch
//!    call may carry. Configurable per contract via
//!    [`crate::CraftNexusContract::set_batch_size_limit`]; defaults to the
//!    legacy `MAX_BATCH_SIZE` ceiling.
//! 2. **Expected storage-work ceiling** – a conservative estimate of the
//!    persistent storage writes a batch will perform (escrow record, buyer /
//!    seller indexes, batch counter and global index accounting). Configurable
//!    per contract via
//!    [`crate::CraftNexusContract::set_batch_resource_budget`].
//!
//! [`ensure_batch_within_bounds`] is a pure, side-effect-free pre-flight check:
//! it performs no storage reads or writes, so it is safe to run on every batch
//! boundary (one-shot creation, schedule, and continuations) before any
//! mutation. When it rejects a request, the error is returned *before* state
//! changes and the caller may retry with a smaller batch or ask the admin to
//! raise the configured limit / budget.

use crate::Error;

// ---------------------------------------------------------------------------
// Configurable bounds (documented defaults / floors / ceilings)
// ---------------------------------------------------------------------------

/// Default maximum number of escrows accepted per batch call.
///
/// Kept in lockstep with the legacy [`crate::MAX_BATCH_SIZE`] constant so the
/// out-of-the-box behaviour of valid batches is unchanged. Only admins can
/// raise or lower it through
/// [`crate::CraftNexusContract::set_batch_size_limit`].
pub const DEFAULT_BATCH_SIZE_LIMIT: u32 = crate::MAX_BATCH_SIZE;

/// Floor for the configurable batch size limit. Prevents a misconfiguration
/// that would silently disable batch creation entirely.
pub const MIN_BATCH_SIZE_LIMIT: u32 = 1;

/// Protocol ceiling for the configurable batch size limit. Keeps any single
/// batch call inside the Soroban ledger-entry envelope; raising the item-count
/// limit alone does **not** relax the storage-work budget below.
pub const MAX_BATCH_SIZE_LIMIT: u32 = 100;

/// Conservative estimate of the persistent storage writes required to create a
/// single escrow inside a batch (escrow record, buyer/seller indexed entries,
/// batch-wide counter, and global index accounting).
pub const STORAGE_WRITES_PER_ESCROW: u64 = 12;

/// Default ceiling for the expected storage work of a single batch call.
///
/// Sized to admit a full, default-sized batch (`20 * 12 = 240` writes) so that
/// existing valid batches keep working, while remaining far below the protocol
/// footprint envelope to leave headroom for TTL extends and emitted events.
/// Admins can tighten this ceiling (or raise it together with the item-count
/// limit) through [`crate::CraftNexusContract::set_batch_resource_budget`].
pub const DEFAULT_BATCH_STORAGE_WRITES_BUDGET: u64 =
    u64::from(DEFAULT_BATCH_SIZE_LIMIT).saturating_mul(STORAGE_WRITES_PER_ESCROW);

/// Floor for the configurable storage-work budget: at least one escrow's worth
/// of work must always be admitted so a misconfiguration cannot disable batch
/// creation.
pub const MIN_BATCH_STORAGE_WRITES_BUDGET: u64 = STORAGE_WRITES_PER_ESCROW;

// ---------------------------------------------------------------------------
// Expected-work estimation (pure, no ledger access)
// ---------------------------------------------------------------------------

/// Estimate the expected storage work (persistent writes) of a batch call that
/// carries `item_count` escrows.
#[inline]
pub fn expected_storage_writes(item_count: u32) -> u64 {
    u64::from(item_count).saturating_mul(STORAGE_WRITES_PER_ESCROW)
}

/// Reject a batch whose work size exceeds the configured bounds, **before**
/// any state mutation.
///
/// Checks, in order:
/// 1. `item_count > size_limit` → `Err(Error::BatchLimitExceeded)`.
/// 2. `expected_storage_writes(item_count) > storage_budget` →
///    `Err(Error::BatchResourceLimitExceeded)`.
///
/// The empty batch (`item_count == 0`) always passes, mirroring the existing
/// empty-batch fast path.
pub fn ensure_batch_within_bounds(
    item_count: u32,
    size_limit: u32,
    storage_budget: u64,
) -> Result<(), Error> {
    if item_count > size_limit {
        return Err(Error::BatchLimitExceeded);
    }
    if expected_storage_writes(item_count) > storage_budget {
        return Err(Error::BatchResourceLimitExceeded);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (pure, no ledger)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_batch_is_never_rejected() {
        assert!(ensure_batch_within_bounds(0, MIN_BATCH_SIZE_LIMIT, 0).is_ok());
        assert_eq!(expected_storage_writes(0), 0);
    }

    #[test]
    fn defaults_admit_a_full_default_size_batch() {
        assert!(ensure_batch_within_bounds(
            DEFAULT_BATCH_SIZE_LIMIT,
            DEFAULT_BATCH_SIZE_LIMIT,
            DEFAULT_BATCH_STORAGE_WRITES_BUDGET,
        )
        .is_ok());
        assert!(ensure_batch_within_bounds(
            DEFAULT_BATCH_SIZE_LIMIT + 1,
            DEFAULT_BATCH_SIZE_LIMIT,
            u64::MAX,
        )
        .is_err());
    }

    #[test]
    fn oversized_item_count_is_rejected() {
        assert_eq!(
            ensure_batch_within_bounds(6, 5, u64::MAX),
            Err(Error::BatchLimitExceeded)
        );
    }

    #[test]
    fn over_budget_expected_storage_work_is_rejected() {
        // 5 escrows * 12 writes = 60; a 48-write budget must reject already.
        assert_eq!(
            ensure_batch_within_bounds(5, 100, 48),
            Err(Error::BatchResourceLimitExceeded)
        );
    }

    #[test]
    fn expected_storage_work_scales_with_item_count() {
        assert_eq!(expected_storage_writes(1), STORAGE_WRITES_PER_ESCROW);
        assert_eq!(
            expected_storage_writes(20),
            20 * STORAGE_WRITES_PER_ESCROW
        );
    }
}