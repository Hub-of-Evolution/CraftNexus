//! Centralized pagination input validation (Issue #1022).
//!
//! All time-sensitive and query-related pagination methods should validate
//! cursors and limits **before** reading storage. This module provides the
//! single source of truth for maximum page sizes and helper functions that
//! return stable errors for invalid inputs.
//!
//! # Conventions
//!
//! | Term     | Definition                                                     |
//! |----------|----------------------------------------------------------------|
//! | cursor   | A zero-based index or page number marking where to start       |
//! | limit    | Maximum number of items to return in a single call              |
//! | page     | Zero-indexed page number (cursor / limit)                      |
//! | page_size| Items per page (alias for limit in page-based pagination)       |
//!
//! # Validation Rules
//!
//! 1. **Zero limits** – Return `Error::PaginationLimitZero` (caller must
//!    request at least one item).
//! 2. **Oversized limits** – Silently cap to the context-specific maximum
//!    to prevent resource exhaustion while keeping the API ergonomic.
//! 3. **Invalid cursors** – Return `Error::PaginationCursorInvalid` when
//!    the cursor exceeds the total count (caller should stop paginating).
//! 4. **Determinism** – Valid inputs always produce the same output for the
//!    same ledger state.

use crate::Error;

// ---------------------------------------------------------------------------
// Maximum page sizes
// ---------------------------------------------------------------------------

/// Maximum page size for user-facing escrow queries (`get_escrows_by_buyer`,
/// `get_escrows_by_seller`).  Sized to keep single-call read costs bounded
/// on mainnet while allowing reasonable bulk fetches.
pub const MAX_PAGE_SIZE: u32 = 100;

/// Maximum page size for the global iterative escrow ID reader
/// (`get_all_escrow_ids_iterative`).  Deliberately lower than
/// `MAX_PAGE_SIZE` because this path reads every escrow ID and was tuned
/// during the batch-size reduction (Issue #198).
pub const MAX_ITERATIVE_PAGE_SIZE: u32 = 20;

/// Maximum limit for `reconcile_token` – bounded to the same conservative
/// batch size used by other storage-intensive operations.
pub const MAX_RECONCILE_LIMIT: u32 = 20;

/// Maximum work limit for `continue_batch_escrow` – scheduled continuation
/// chunks are intentionally smaller to avoid instruction-limit overruns.
pub const MAX_BATCH_WORK_LIMIT: u32 = 5;

/// Maximum limit for admin/debug pagination helpers (`get_artisan_stake_deposits`,
/// `get_fund_audit_history_paginated`).  Allows larger fetches than user-facing
/// queries but still prevents unbounded reads.
pub const MAX_ADMIN_PAGE_SIZE: u32 = 200;

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// The result type returned by all pagination validators.
pub type PaginationResult = Result<u32, Error>;

/// Validate a page-based pagination limit (page_size / limit).
///
/// - Returns `Err(Error::PaginationLimitZero)` if `limit == 0`.
/// - Returns `Ok(clamped)` where `clamped = limit.min(max)`.
///
/// # Examples
///
/// ```ignore
/// let page_size = validate_limit(50, MAX_PAGE_SIZE)?;  // Ok(50)
/// let page_size = validate_limit(200, MAX_PAGE_SIZE)?; // Ok(100) – capped
/// let page_size = validate_limit(0, MAX_PAGE_SIZE)?;   // Err(PaginationLimitZero)
/// ```
pub fn validate_limit(limit: u32, max: u32) -> PaginationResult {
    if limit == 0 {
        return Err(Error::PaginationLimitZero);
    }
    Ok(limit.min(max))
}

/// Validate a page number (zero-indexed).
///
/// This is a lightweight guard – the caller should still handle the
/// out-of-range case after reading the total count, but catching obviously
/// invalid pages early avoids wasted storage reads.
///
/// Currently this always returns `Ok(page)` because page numbers are valid
/// for *any* total count (the empty-result case is handled after reading).
/// This function exists for future strictness and to mirror `validate_cursor`.
pub fn validate_page(page: u32) -> PaginationResult {
    Ok(page)
}

/// Validate a cursor (offset / start_index) against a known total.
///
/// - Returns `Err(Error::PaginationCursorInvalid)` if `cursor >= total`
///   **and** `total > 0` (the caller is past the end of the dataset).
/// - If `total == 0` (empty dataset), any cursor is treated as out-of-range
///   and returns `Err(Error::PaginationCursorInvalid)`.
/// - Returns `Ok(cursor)` otherwise.
///
/// # Examples
///
/// ```ignore
/// let c = validate_cursor(5, 10); // Ok(5)
/// let c = validate_cursor(10, 10); // Err(PaginationCursorInvalid) – at end
/// let c = validate_cursor(0, 0);  // Err(PaginationCursorInvalid) – empty
/// ```
pub fn validate_cursor(cursor: u32, total: u32) -> PaginationResult {
    if total == 0 || cursor >= total {
        return Err(Error::PaginationCursorInvalid);
    }
    Ok(cursor)
}

/// Compute the effective range `[start, end)` clamped to the total.
///
/// This is a pure helper – it does **not** validate the cursor (call
/// `validate_cursor` first if you want strict checking).  It simply
/// computes the safe slice bounds using saturating arithmetic.
///
/// # Returns
///
/// `(start, end, count)` where `count = end - start`.
pub fn page_bounds(cursor: u32, limit: u32, total: u32) -> (u32, u32, u32) {
    let start = cursor.min(total);
    let end = start.saturating_add(limit).min(total);
    (start, end, end - start)
}

/// Validate the limit for `reconcile_token` and `continue_batch_escrow`.
///
/// These functions use hard errors (not silent caps) because they perform
/// storage-intensive operations where an oversized request could be
/// expensive.
///
/// - Returns `Err(Error::PaginationLimitZero)` if `limit == 0`.
/// - Returns `Err(Error::InvalidBatchWorkLimit)` if `limit > max`.
/// - Returns `Ok(limit)` otherwise.
pub fn validate_strict_limit(limit: u32, max: u32) -> PaginationResult {
    if limit == 0 {
        return Err(Error::PaginationLimitZero);
    }
    if limit > max {
        return Err(Error::InvalidBatchWorkLimit);
    }
    Ok(limit)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_limit ───────────────────────────────────────────────

    #[test]
    fn limit_zero_returns_error() {
        assert_eq!(validate_limit(0, MAX_PAGE_SIZE), Err(Error::PaginationLimitZero));
    }

    #[test]
    fn limit_within_max_returns_self() {
        assert_eq!(validate_limit(1, MAX_PAGE_SIZE), Ok(1));
        assert_eq!(validate_limit(50, MAX_PAGE_SIZE), Ok(50));
        assert_eq!(validate_limit(100, MAX_PAGE_SIZE), Ok(100));
    }

    #[test]
    fn limit_above_max_is_capped() {
        assert_eq!(validate_limit(101, MAX_PAGE_SIZE), Ok(100));
        assert_eq!(validate_limit(200, MAX_PAGE_SIZE), Ok(100));
        assert_eq!(validate_limit(u32::MAX, MAX_PAGE_SIZE), Ok(100));
    }

    // ── validate_cursor ──────────────────────────────────────────────

    #[test]
    fn cursor_within_range_is_valid() {
        assert_eq!(validate_cursor(0, 10), Ok(0));
        assert_eq!(validate_cursor(5, 10), Ok(5));
        assert_eq!(validate_cursor(9, 10), Ok(9));
    }

    #[test]
    fn cursor_at_total_is_invalid() {
        assert_eq!(validate_cursor(10, 10), Err(Error::PaginationCursorInvalid));
    }

    #[test]
    fn cursor_beyond_total_is_invalid() {
        assert_eq!(validate_cursor(11, 10), Err(Error::PaginationCursorInvalid));
        assert_eq!(validate_cursor(u32::MAX, 10), Err(Error::PaginationCursorInvalid));
    }

    #[test]
    fn cursor_on_empty_dataset_is_invalid() {
        assert_eq!(validate_cursor(0, 0), Err(Error::PaginationCursorInvalid));
    }

    // ── validate_strict_limit ────────────────────────────────────────

    #[test]
    fn strict_limit_zero_returns_error() {
        assert_eq!(
            validate_strict_limit(0, MAX_RECONCILE_LIMIT),
            Err(Error::PaginationLimitZero)
        );
    }

    #[test]
    fn strict_limit_within_max_is_valid() {
        assert_eq!(validate_strict_limit(1, MAX_RECONCILE_LIMIT), Ok(1));
        assert_eq!(validate_strict_limit(20, MAX_RECONCILE_LIMIT), Ok(20));
    }

    #[test]
    fn strict_limit_above_max_returns_batch_error() {
        assert_eq!(
            validate_strict_limit(21, MAX_RECONCILE_LIMIT),
            Err(Error::InvalidBatchWorkLimit)
        );
    }

    // ── page_bounds ──────────────────────────────────────────────────

    #[test]
    fn page_bounds_basic() {
        let (s, e, c) = page_bounds(0, 10, 25);
        assert_eq!((s, e, c), (0, 10, 10));
    }

    #[test]
    fn page_bounds_clamped_at_total() {
        let (s, e, c) = page_bounds(20, 10, 25);
        assert_eq!((s, e, c), (20, 25, 5));
    }

    #[test]
    fn page_bounds_empty_total() {
        let (s, e, c) = page_bounds(0, 10, 0);
        assert_eq!((s, e, c), (0, 0, 0));
    }

    #[test]
    fn page_bounds_cursor_beyond_total() {
        let (s, e, c) = page_bounds(30, 10, 25);
        assert_eq!((s, e, c), (25, 25, 0));
    }

    #[test]
    fn page_bounds_zero_limit() {
        let (s, e, c) = page_bounds(5, 0, 10);
        assert_eq!((s, e, c), (5, 5, 0));
    }
}
