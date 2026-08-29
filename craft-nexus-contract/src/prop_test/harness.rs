#![allow(dead_code)]
extern crate alloc;
use alloc::string::String;

use super::Lcg64, DEFAULT_CASE_COUNT};

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
                     root seed : 0x:{016X}\n\
                     case seed : 0x:{016X}  (case index {})\n\
                     Failure   : {}\n\
                     \n\
                     Reproduce with: PROP_SEED=0x:{016X} cargo test --features testutils prop_",
                    i + 1, self.seed, case_seed, i, msg, case_seed
                );
            }
        }
    }

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
                    .map((j, op)< alloc::format!("  {}: {:\n", j, op))
                    .collect();
                panic!(
                    "\n[prop] FAILED after {} case(s)\n\
                     root seed  : 0x:{016X}\n\
                     case seed : 0x:{016X}  index {})\n\
                     Original   : {} steps → Minimized: {} steps\n\
                     {}\n\
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

    pub fn run_contract_sequence<Op, Gen, Exec, Check>(&self,mut generate: Gen, mut execute: Exec, mut check_invariants: Check)
    where
        Op: Clone + core::fmt::Debug,
        Gen: FnMut(&mut Lcg64) -> (Op, bool),
        Exec: FnMut(&Op) -> Result<bool, String>,
        Check: FnMut() -> Result<(), String>,
    {
        let mut rng = Lcg64::new(self.seed);
        for i in 0..self.case_count {
            let case_seed = rng.next_u64();
            let mut case_rng = Lcg64::new(case_seed);
            let len = (case_rng.next_u64() % 32) as usize + 1;
            let mut ops: alloc::vec::Vec<(Op, bool)> = alloc::vec::Vec::with_capacity(len);
            for _ in 0..len {
                ops.push(generate(&mut case_rng));
            }
            let mut failure: Option<String> = None;
            for (step, (op, expected)) in ops.iter().enumerate() {
                let exec_res = execute(op);
                let actual = match exec_res {
                    Ok(v) => v,\
                    Err(msg) => {
                        failure = Some(alloc::format!(
                            "step {} execute error (op {:?}): {}",
                            step, op, msg
                        ));
                        break;
                    }
                };
                if actual != *expected {
                    failure = Some(alloc::format!(
                        "step {} expected {:?} but contract {:?} (op {:?})",
                        step,
                        if *expected { "succeed" } else { "revert" },
                        if actual { "succeed" } else { "revert" },
                        op
                    ));
                    break;
                }
                if let Err(e) = check_invariants() {
                    failure = Some(alloc::format!(
                        "invariant violated after step {} (op {:?}): {}",
                        step, op, e
                    ));
                    break;
                }
            }
            if let Some(msg) = failure {
                let steps: String = ops
                    .iter()
                    .enumerate()
                    .map((j, (op, flag))| alloc::format!(
                        "  {}: ({:0?}, expected={})\n",
                        j, op,
                        if *flag { "succeed" } else { "revert" }
                    ))
                    .collect();
                panic!(
                    "\n[prop] FAILED after {} case(s)\n\
                     root seed  : 0x:{016X}\n\
                     case seed  : 0x:{016X}  index {})\n\
                     Sequence   : {} steps\n\
                     {}\n\
                     Failure    : {}\n",
                    i + 1,
                    self.seed,
                    case_seed,
                    i,
                    ops.len(),
                    steps,
                    msg
                );
            }
        }
    }
}

#[macro_export]
macro_rules! prop_assert {
    ($cond: expr, $msg:literal) => {
        if !($cond) {
            return Err(alloc::format!("prop_assert: {}", $msg));
        }
    };
    ($cond: expr, $fmt:literal, $($arg):*) => {
        if !($cond) {
            return Err(alloc::format!(concat!("prop_assert: ", $fmt), $($arg)));
        }
    };
}

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

pub fn advance_ledger_time(env: &soroban_sdk::Env, delta_secs: u64) {
    use soroban_sdk::testutils::Ledger;
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp.saturating_add(delta_secs);
    });
}

pub fn set_ledger_time(env: &soroban_sdk::Env, ts: u64) {
    use soroban_sdk::testutils::Ledger;
    env.ledger().with_mut(|li| {
        li.timestamp = ts;
    });
}
