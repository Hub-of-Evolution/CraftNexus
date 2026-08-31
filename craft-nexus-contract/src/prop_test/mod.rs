//! Property-based and model-based verification of CraftNexus contract state machines.
#![allow(dead_code)]
//!
//! # Architecture
//!
//! ```text
//! harness.rs          – deterministic seed runner + sequence shrinking
//! model.rs            – pure reference state machine (no SDK side-effects)
//! generators.rs       – `arbitrary`-backed call sequence generators
//! invariants.rs       – reusable invariant assertions (conservation, auth, …)
//! escrow_props.rs     – escrow lifecycle property tests
//! onboarding_props.rs – onboarding lifecycle property tests
//! staking_props.rs    – staking deposit / cooldown / withdrawal property tests
//! upgrade_props.rs    – upgrade / pause / recovery interaction tests
//! ```
//!
//! # Model vs. ledger semantics
//!
//! The reference model (`model.rs`) is a pure Rust state machine that mirrors
//! the documented transitions. **Deliberate differences** that are tested but
//! NOT modelled:
//!
//! | Ledger behaviour | Model simplification | Rationale |
//! |---|---|---|
//! | `funded` flag set by token transfer | Model assumes atomic fund-on-create | Token callbacks are host-level details not part of the state machine |
//! | CEI pending states (ReleasePending etc.) | Model skips to terminal immediately | Pending states are host-level re-entrancy guards, not externally observable |
//! | TTL / storage archival | Not modelled | No ledger time concept in the pure model |
//! | Multi-sig upgrade threshold snapshots | Simplified to single-admin | Full multi-sig tested separately in upgrade_props.rs |
//!
//! # Running
//!
//! ```bash
//! # Native host (fast, default CI path):
//! cargo test --features testutils prop_ -- --nocapture
//!
//! # With a fixed seed to reproduce a failure:
//! PROP_SEED=0xdeadbeef cargo test --features testutils prop_ -- --nocapture
//! ```

pub mod escrow_props;
pub mod generators;
pub mod harness;
pub mod invariants;
pub mod model;
pub mod onboarding_props;
pub mod staking_props;
pub mod upgrade_props;
extern crate alloc;

// ── Shared constants ─────────────────────────────────────────────────────────

/// Default seed used when `PROP_SEED` env var is not set at compile time.
pub const DEFAULT_SEED: u64 = 0xCAFE_F00D_DEAD_BEEF;

/// Number of random call sequences generated per property.
pub const DEFAULT_CASE_COUNT: u32 = 64;

/// Maximum number of operations in a single generated sequence.
pub const MAX_SEQUENCE_LEN: usize = 16;

/// Read the optional `PROP_SEED` environment variable (compile time).
/// Falls back to `DEFAULT_SEED`.
#[inline]
pub fn seed_from_env() -> u64 {
    match option_env!("PROP_SEED") {
        Some(s) => u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(DEFAULT_SEED),
        None => DEFAULT_SEED,
    }
}

// ── Minimal LCG PRNG ─────────────────────────────────────────────────────────

/// A minimal linear-congruential PRNG sufficient for test generation.
/// All state is in a `u64`; no heap allocation required.
#[derive(Clone, Debug)]
pub struct Lcg64 {
    pub state: u64,
}

impl Lcg64 {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { DEFAULT_SEED } else { seed },
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    #[inline]
    pub fn next_usize(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() as usize) % n
    }

    #[inline]
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }

    #[inline]
    pub fn next_u64_range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        lo + self.next_u64() % (hi - lo + 1)
    }

    #[inline]
    pub fn next_i128_range(&mut self, lo: i128, hi: i128) -> i128 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() as i128) % (hi - lo + 1)
    }

    /// Derive a child seed without advancing the parent state.
    #[inline]
    pub fn fork(&self, salt: u64) -> Self {
        Self::new(self.state ^ salt)
    }
}

// ── Time cursor ───────────────────────────────────────────────────────────────

/// A deterministic time cursor used in property tests to advance ledger time.
#[derive(Clone, Debug)]
pub struct TimeCursor {
    pub now: u64,
}

impl TimeCursor {
    pub fn new(start: u64) -> Self {
        Self { now: start }
    }

    pub fn advance(&mut self, delta: u64) {
        self.now = self.now.saturating_add(delta);
    }

    pub fn advance_to(&mut self, t: u64) {
        if t > self.now {
            self.now = t;
        }
    }
}

// ── Step outcome ──────────────────────────────────────────────────────────────

/// Outcome of executing one step in a property-based sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// The call succeeded as expected.
    Ok,
    /// The call was rejected with the expected error (precondition guarded).
    RejectedExpected,
    /// The call was rejected with an *unexpected* error — property failure.
    RejectedUnexpected(alloc::string::String),
    /// An invariant assertion failed after the call.
    InvariantViolation(alloc::string::String),
}

impl StepOutcome {
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            StepOutcome::RejectedUnexpected(_) | StepOutcome::InvariantViolation(_)
        )
    }
}
