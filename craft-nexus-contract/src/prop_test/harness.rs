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

// ── Invariant reporting ───────────────────────────────────────────────────────

/// Report of invariant checks during model-based sequence execution.
#[derive(Clone, Debug)]
pub struct InvariantReport {
    /// Step index where the first violation occurred (if any)
    pub violation_step: usize,
    /// Description of the violated invariant
    pub violation_msg: String,
    /// State transition that triggered the violation
    pub state_transition: StateTransition,
}

impl InvariantReport {
    /// Create a clean report with no violations
    pub fn clean() -> Self {
        Self {
            violation_step: usize::MAX,
            violation_msg: String::new(),
            state_transition: StateTransition::None,
        }
    }

    /// Create a violation report
    pub fn violation(step: usize, msg: String, transition: StateTransition) -> Self {
        Self {
            violation_step: step,
            violation_msg: msg,
            state_transition: transition,
        }
    }

    /// Check if this report contains a violation
    pub fn has_violation(&self) -> bool {
        self.violation_step != usize::MAX
    }
}

/// Description of a state transition in the model.
#[derive(Clone, Debug)]
pub enum StateTransition {
    None,
    EscrowCreated {
        order_id: u32,
        amount: i128,
    },
    EscrowFunded {
        order_id: u32,
    },
    EscrowReleased {
        order_id: u32,
        to_seller: i128,
        fee: i128,
    },
    EscrowRefunded {
        order_id: u32,
        to_buyer: i128,
    },
    DisputeRaised {
        order_id: u32,
        initiator: String,
    },
    DisputeResolved {
        order_id: u32,
        resolution: String,
    },
    StakeAdded {
        artisan: String,
        amount: i128,
    },
    StakeWithdrawn {
        artisan: String,
        amount: i128,
    },
    TimeAdvanced {
        from: u64,
        to: u64,
    },
}

impl core::fmt::Display for StateTransition {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StateTransition::None => write!(f, "No transition"),
            StateTransition::EscrowCreated { order_id, amount } => {
                write!(f, "Escrow {} created with amount {}", order_id, amount)
            }
            StateTransition::EscrowFunded { order_id } => {
                write!(f, "Escrow {} funded", order_id)
            }
            StateTransition::EscrowReleased {
                order_id,
                to_seller,
                fee,
            } => write!(
                f,
                "Escrow {} released: seller={}, fee={}",
                order_id, to_seller, fee
            ),
            StateTransition::EscrowRefunded { order_id, to_buyer } => {
                write!(f, "Escrow {} refunded: buyer={}", order_id, to_buyer)
            }
            StateTransition::DisputeRaised {
                order_id,
                initiator,
            } => write!(f, "Dispute raised on escrow {} by {}", order_id, initiator),
            StateTransition::DisputeResolved {
                order_id,
                resolution,
            } => write!(
                f,
                "Dispute resolved on escrow {} with {}",
                order_id, resolution
            ),
            StateTransition::StakeAdded { artisan, amount } => {
                write!(f, "Stake added by {}: {}", artisan, amount)
            }
            StateTransition::StakeWithdrawn { artisan, amount } => {
                write!(f, "Stake withdrawn by {}: {}", artisan, amount)
            }
            StateTransition::TimeAdvanced { from, to } => {
                write!(f, "Time advanced: {} → {}", from, to)
            }
        }
    }
}

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

    /// Run `case_count` cases with model-based shrinking that reports the first
    /// violated invariant and state transition details.
    pub fn run_model_sequence<Op, Gen, Exec>(
        &self,
        mut generate: Gen,
        mut execute: Exec,
    ) where
        Op: Clone + core::fmt::Debug,
        Gen: FnMut(&mut Lcg64) -> alloc::vec::Vec<super::generators::ShrinkableOp<Op>>,
        Exec: FnMut(&[super::generators::ShrinkableOp<Op>]) -> Result<InvariantReport, String>,
    {
        let mut rng = Lcg64::new(self.seed);
        for i in 0..self.case_count {
            let case_seed = rng.next_u64();
            let mut case_rng = Lcg64::new(case_seed);
            let ops = generate(&mut case_rng);
            match execute(&ops) {
                Err(msg) => {
                    let minimized = super::generators::shrink_model_based(ops.clone(), |c| {
                        execute(c).is_err()
                    });
                    let steps: String = minimized
                        .iter()
                        .enumerate()
                        .map(|(j, sop)| {
                            alloc::format!(
                                "  {}: actor={}, time={}, op={:?}\n",
                                j, sop.actor_id, sop.timestamp, sop.op
                            )
                        })
                        .collect();
                    panic!(
                        "\n[prop] MODEL-BASED FAILURE after {} case(s)\n\
                         root seed  : 0x{:016X}\n\
                         case seed  : 0x{:016X}  (index {})\n\
                         Original   : {} steps → Minimized: {} steps\n\
                         Shrunk sequence:\n\
                         {}\
                         Failure    : {}\n\
                         \n\
                         Reproduce with: PROP_SEED=0x{:016X} cargo test --features testutils prop_",
                        i + 1,
                        self.seed,
                        case_seed,
                        i,
                        ops.len(),
                        minimized.len(),
                        steps,
                        msg,
                        case_seed
                    );
                }
                Ok(report) if report.has_violation() => {
                    let minimized = super::generators::shrink_model_based(ops.clone(), |c| {
                        matches!(execute(c), Ok(r) if r.has_violation())
                    });
                    let steps: String = minimized
                        .iter()
                        .enumerate()
                        .map(|(j, sop)| {
                            alloc::format!(
                                "  {}: actor={}, time={}, op={:?}\n",
                                j, sop.actor_id, sop.timestamp, sop.op
                            )
                        })
                        .collect();
                    panic!(
                        "\n[prop] INVARIANT VIOLATION after {} case(s)\n\
                         root seed     : 0x{:016X}\n\
                         case seed     : 0x{:016X}  (index {})\n\
                         Original      : {} steps → Minimized: {} steps\n\
                         First violation at step {}: {}\n\
                         State transition: {:?}\n\
                         Shrunk sequence:\n\
                         {}\
                         \n\
                         Reproduce with: PROP_SEED=0x{:016X} cargo test --features testutils prop_",
                        i + 1,
                        self.seed,
                        case_seed,
                        i,
                        ops.len(),
                        minimized.len(),
                        report.violation_step,
                        report.violation_msg,
                        report.state_transition,
                        steps,
                        case_seed
                    );
                }
                Ok(_) => {}
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
