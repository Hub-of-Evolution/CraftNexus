//! Centralised time-boundary policy for CraftNexus.
//!
//! # Time Policy: Inclusive-End Half-Open Intervals
//!
//! All time-sensitive checks follow a single documented convention:
//!
//! ```text
//! window_open  = (now >= start)
//! window_closed = (now >= start + duration)
//! ```
//!
//! This means:
//! - At `t = start`             → window is OPEN  (inclusive start)
//! - At `t = start + duration`  → window is CLOSED (inclusive end — the deadline IS the expiry moment)
//! - At `t = start + duration - 1` → window is still OPEN
//!
//! # Boundary examples
//!
//! | Mechanism | start | duration | deadline | open at | closed at |
//! |---|---|---|---|---|---|
//! | Release window | created_at | release_window | created_at + release_window | created_at | created_at + release_window |
//! | Stake cooldown | stake_time | stake_cooldown | cooldown_end | stake_time | cooldown_end |
//! | Dispute max duration | dispute_initiated_at | max_dispute_duration | initiated + max | initiated_at | initiated + max |
//! | Evidence expiry | submitted_at | evidence_expiry | submitted + expiry | submitted_at | submitted + expiry |
//!
//! # Constants
//!
//! All duration constants are expressed in **seconds** (matching `env.ledger().timestamp()`).
//! Constants that are purely internal (not stored in `PlatformConfig`) are `u64`.
//! Configurable durations stored in `PlatformConfig` are `u32` (sufficient for ~136 years).
//!
//! # Naming conventions
//!
//! - `*_deadline` or `*_expires_at`: absolute timestamp at which the window CLOSES (inclusive).
//! - `*_duration` or `*_window`: relative duration in seconds.
//! - Functions named `assert_window_closed` / `assert_window_open` enforce the policy.

// ── Duration constants (seconds) ──────────────────────────────────────────────

/// Grace period before a WASM upgrade can execute after proposal (7 days).
pub const WASM_UPGRADE_COOLDOWN: u64 = 7 * 24 * 60 * 60;

/// Prevents cancel-and-repropose from resetting the review window (7 days).
pub const CANCEL_REPROPOSE_COOLDOWN: u64 = 7 * 24 * 60 * 60;

/// Maximum duration a dispute can remain open before force-resolution (30 days).
pub const MAX_DISPUTE_DURATION: u64 = 30 * 24 * 60 * 60;

/// Cooldown after staking before tokens can be unstaked (7 days).
pub const STAKE_COOLDOWN: u64 = 7 * 24 * 60 * 60;

/// Minimum release window to prevent flash auto-releases (1 day).
pub const MIN_RELEASE_WINDOW: u64 = 24 * 60 * 60;

/// Absolute safety ceiling for admin-configurable max release window (365 days).
pub const ABSOLUTE_MAX_RELEASE_WINDOW: u64 = 365 * 24 * 60 * 60;

/// Hard ceiling for cumulative release window (30 days).
pub const MAX_TOTAL_RELEASE_WINDOW: u64 = 30 * 24 * 60 * 60;

/// Evidence retention / expiry window (7 days).
pub const EVIDENCE_EXPIRY_WINDOW: u64 = 7 * 24 * 60 * 60;

/// Challenge period before a dispute can be resolved by arbitrator (1 day).
pub const EVIDENCE_CHALLENGE_WINDOW: u64 = 24 * 60 * 60;

/// Window before a dispute can be escalated to arbitration (3 days).
pub const DISPUTE_ESCALATION_WINDOW: u64 = 3 * 24 * 60 * 60;

/// Default rate-limit window (1 hour).
pub const RATE_LIMIT_WINDOW: u64 = 3600;

/// Timeout before unfunded escrows can be cancelled (24 hours).
pub const UNFUNDED_CANCEL_TIMEOUT: u64 = 24 * 60 * 60;

/// Time-lock period before admin recovery is allowed (7 days).
pub const ADMIN_RECOVERY_DELAY: u64 = 7 * 24 * 60 * 60;

/// Minimum allowed admin recovery cooldown (7 days).
pub const MIN_ADMIN_RECOVERY_COOLDOWN: u64 = 7 * 24 * 60 * 60;

/// Default timelock delay for pending critical admin actions (24 hours).
pub const ADMIN_ACTION_TIMELOCK_DELAY: u64 = 24 * 60 * 60;

// ── Boundary helpers ──────────────────────────────────────────────────────────

/// Returns `true` if the window that opened at `start` with `duration` seconds
/// has fully elapsed at time `now`.
///
/// Equivalently: `now >= start + duration`.
///
/// Use this to gate actions that become available AFTER a cooldown or release
/// window (e.g., auto-release, unstake, force-resolve dispute).
#[inline]
pub fn is_window_elapsed(now: u64, start: u64, duration: u64) -> bool {
    now >= start.saturating_add(duration)
}

/// Returns `true` if the window that opened at `start` with `duration` seconds
/// is still active (not yet elapsed) at time `now`.
///
/// Equivalently: `now < start + duration`.
///
/// Use this to reject actions that must wait (e.g., "release window not elapsed",
/// "challenge window still active").
#[inline]
pub fn is_window_active(now: u64, start: u64, duration: u64) -> bool {
    now < start.saturating_add(duration)
}

/// Returns `true` if `now` is within the window `[start, start + duration)`.
///
/// Equivalently: `start <= now < start + duration`.
#[inline]
pub fn is_within_window(now: u64, start: u64, duration: u64) -> bool {
    now >= start && now < start.saturating_add(duration)
}

/// Compute the absolute deadline (inclusive end) for a window.
///
/// Returns `start + duration`. The window is considered elapsed at exactly
/// this timestamp (inclusive end convention).
#[inline]
pub fn deadline(start: u64, duration: u64) -> u64 {
    start.saturating_add(duration)
}

/// Returns `true` if the deadline has been reached or exceeded.
///
/// Equivalently: `now >= deadline`.
#[inline]
pub fn is_deadline_reached(now: u64, deadline_ts: u64) -> bool {
    now >= deadline_ts
}

/// Returns `true` if the deadline has NOT yet been reached.
///
/// Equivalently: `now < deadline`.
#[inline]
pub fn is_deadline_pending(now: u64, deadline_ts: u64) -> bool {
    now < deadline_ts
}

// ── Rate-limit bucketing ──────────────────────────────────────────────────────

/// Compute the rate-limit bucket index for a given timestamp and window size.
///
/// Uses integer division so all calls within `[bucket * window, (bucket + 1) * window)`
/// share the same bucket.
#[inline]
pub fn rate_limit_bucket(now: u64, window_secs: u64) -> u64 {
    if window_secs == 0 {
        return 0;
    }
    now / window_secs
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_elapsed_basic() {
        // Window: start=100, duration=50 → deadline=150
        assert!(!is_window_elapsed(149, 100, 50)); // 1 second before deadline
        assert!(is_window_elapsed(150, 100, 50));  // exactly at deadline (inclusive)
        assert!(is_window_elapsed(151, 100, 50));  // 1 second after deadline
        assert!(!is_window_elapsed(100, 100, 50)); // at start (duration not elapsed)
    }

    #[test]
    fn window_active_basic() {
        assert!(is_window_active(149, 100, 50));  // still active 1s before deadline
        assert!(!is_window_active(150, 100, 50)); // closed at deadline
        assert!(!is_window_active(151, 100, 50)); // closed after deadline
        assert!(is_window_active(100, 100, 50));  // open at start
        assert!(is_window_active(99, 100, 50));   // before start but still < deadline → not yet elapsed
    }

    #[test]
    fn within_window_boundary() {
        // start=100, duration=50 → valid for [100, 150)
        assert!(!is_within_window(99, 100, 50));   // before start
        assert!(is_within_window(100, 100, 50));    // at start (inclusive)
        assert!(is_within_window(125, 100, 50));    // in the middle
        assert!(is_within_window(149, 100, 50));    // at deadline-1 → still in window
        assert!(!is_within_window(150, 100, 50));   // at deadline (closed)
    }

    #[test]
    fn deadline_helpers() {
        let d = deadline(100, 50);
        assert_eq!(d, 150);
        assert!(is_deadline_reached(150, d));
        assert!(!is_deadline_reached(149, d));
        assert!(is_deadline_pending(149, d));
        assert!(!is_deadline_pending(150, d));
    }

    #[test]
    fn saturating_add_prevents_overflow() {
        // When start + duration would overflow u64::MAX, saturating_add clamps to MAX.
        // is_window_elapsed(now, start, duration) checks now >= start.saturating_add(duration).
        let start = u64::MAX - 10;
        let duration = 20u64;
        // start.saturating_add(duration) == u64::MAX (saturated)
        assert!(is_window_elapsed(u64::MAX, start, duration));
        assert!(!is_window_elapsed(u64::MAX - 1, start, duration));
        assert!(!is_window_active(u64::MAX, start, duration));
        assert!(is_window_active(u64::MAX - 1, start, duration));
    }

    #[test]
    fn rate_limit_bucket_basic() {
        assert_eq!(rate_limit_bucket(0, 3600), 0);
        assert_eq!(rate_limit_bucket(3599, 3600), 0);
        assert_eq!(rate_limit_bucket(3600, 3600), 1);
        assert_eq!(rate_limit_bucket(7200, 3600), 2);
        assert_eq!(rate_limit_bucket(0, 0), 0); // zero window → always bucket 0
    }

    #[test]
    fn zero_duration_window() {
        // A zero-duration window is elapsed at start
        assert!(is_window_elapsed(100, 100, 0));
        assert!(!is_window_active(100, 100, 0));
    }

    #[test]
    fn equal_start_and_now() {
        // At the exact start moment, window is open but not elapsed
        assert!(!is_window_elapsed(100, 100, 50));
        assert!(is_window_active(100, 100, 50));
        assert!(is_within_window(100, 100, 50));
    }
}
