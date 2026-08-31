//! Test harness engine: runs deterministic sequences and shrinks failures.
#![allow(dead_code)]
//!
//! # Usage
//!
//! ```rust
//! use crate::prop_test::harness::PropHarness;
//!
//! let harness = PropHarness::new(seed_from_env(), 64);
//! harness.run(|rng| {
//!     // generate + execute one case; return Ok(()) or Err(description)
//! });
//! ```
//!
//! When a case fails, `PropHarness::run` panics with:
//!
//! ```text
//! [prop] FAILED after N cases
//! seed: 0xCAFEF00DDEADBEEF  case: 0x…
//! Failure: "fund_conservation violation …"
//! ```

extern crate alloc;
use alloc::string::String;

use super::{Lcg64, DEFAULT_CASE_COUNT};

/// The core harness struct.
pub struct PropHarness {
    pub seed: u64,
    pub case_count: u32,
}

impl PropHarness {
    pub fn new(seed: u64, case_count: u32) -> Self {
        Self { seed, case_count }
    }

    pub fn default_harness() -> Self {
        Self::new(super::seed_from_env(), DEFAULT_CASE_COUNT)
    }

    /// Run `case_count` iterations of `f`. Each call receives a forked `Lcg64`.
    /// Returns `Ok(())` or Err(message)` per case; panics on first failure.
    pub fn run<F>(&self, mut f: F)
    where
        F: FnMut(&mut Lcg64) -> Result<(), String>,
    {
        let mut rng = Lcg64::new(self.seed);
        for i in 0..self.case_count {
            let case_seed = rng.next_u64();
            let mut case_rng = Lcg64::new(case_seed);
            if let Err(msg) = f(&mut case_rng) {
                panic!(
                    "\n[prop] FAILED after {} case(s)\n\
                     root seed : 0x{:016X}\n\
                     case seed : 0x{:016X}  (case index {})\n\
                     Failure   : {}\n\
                     \n\
                     Reproduce with: PROP_SEED=0x{:016X} cargo test --features testutils prop_",
                    i + 1, self.seed, case_seed, i, msg, case_seed
                );
            }
        }
    }

    /// Run `case_count` cases with a generated sequence, shrinking on failure.
    pub fn run_sequence<Op, Gen, Exec>(&self, mut generate: Gen, mut execute: Exec)
    where
        Op: Clone + core::fmt::Debug,
        Gen: FnMut(&mut Lcg64) -> alloc::vec::Vec<Op>,
        Exec: FnMut(&[Op]) -> Result<(), String>,
    {
        let mut rng = Lcg64::new(self.seed);
        for i in 0..self.case_count {
            let case_seed = rng.next_u64();
            let mut case_rng = Lcg64::new(case_seed);
            let ops = generate(&mut case_rng);
            if let Err(msg) = execute(&ops) {
                let minimized =
                    super::generators::shrink_sequence(ops.clone(), |c| execute(c).is_err());
                let steps: String = minimized
                    .iter()
                    .enumerate()
                    .map(|(j, op)| alloc::format!("  {}: {:?}\n", j, op))
                    .collect();
                panic!(
                    "\n[prop] FAILED after {} case(s)\n\
                     root seed  : 0x{:016X}\n\
                     case seed  : 0x{:016X}  (index {})\n\
                     Original   : {} steps → Minimized: {} steps\n\
                     {}\
                     Failure    : {}\n",
                    i + 1,
                    self.seed,
                    case_seed,
                    i,
                    ops.len(),
                    minimized.len(),
                    steps,
                    msg
                );
            }
        }
    }
}

// ── Convenience macros ────────────────────────────────────────────────────────

/// Assert condition inside a property test closure, returning `Err` on failure.
#[macro_export]
macro_rules! prop_assert {
    ($cond:expr, $msg:literal) => {
        if !($cond) {
            return Err(alloc::format!("prop_assert: {}", $msg));
        }
    };
    ($cond:expr, $fmt:literal, $($arg:tt)*) => {
        if !($cond) {
            return Err(alloc::format!(concat!("prop_assert: ", $fmt), $($arg)*));
        }
    };
}

/// Assert equality inside a property test closure, returning `Err` on failure.
#[macro_export]
macro_rules! prop_assert_eq {
    ($left:expr, $right:expr) => {
        if $left != $right {
            return Err(alloc::format!(
                "prop_assert_eq: {:?} != {:?}",
                $left,
                $right
            ));
        }
    };
}

// ── Ledger time helpers ───────────────────────────────────────────────────────

/// Advance the Soroban test environment ledger timestamp by `delta_secs`.
pub fn advance_ledger_time(env: &soroban_sdk::Env, delta_secs: u64) {
    use soroban_sdk::testutils::Ledger;
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(delta_secs);
    });
}

/// Set the Soroban test environment ledger timestamp to an absolute value.
pub fn set_ledger_time(env: &soroban_sdk::Env, ts: u64) {
    use soroban_sdk::testutils::Ledger;
    env.ledger().with_mut(|li| {
        li.timestamp = ts;
    });
}
