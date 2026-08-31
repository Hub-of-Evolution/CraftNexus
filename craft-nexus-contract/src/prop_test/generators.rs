//! Generators for random (but shrinkable) call sequences.
#![allow(dead_code)]
//!
//! Each `generate_*` function returns a `Vec` of typed `Op` variants that
//! represent one possible sequence of contract interactions. The generators
//! use `Lcg64` so that a seed can be replayed deterministically.
//!
//! # Design choices
//!
//! - **Valid + invalid intermixed**: every sequence contains both valid
//!   operations (that should succeed) and occasionally invalid ones
//!   (wrong caller, bad amount, wrong state) so the harness exercises
//!   rejection paths.
//! - **Time advances**: some operations insert a `AdvanceTime(delta)` op
//!   so cooldown and window expiry are tested.
//! - **No SDK types**: all parameters are plain Rust scalars; the property
//!   tests wire them into SDK calls.

extern crate alloc;
use alloc::vec::Vec;

use super::{Lcg64, MAX_SEQUENCE_LEN};

// ── Escrow operations ─────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum EscrowOp {
    CreateEscrow {
        order_id: u32,
        amount: i128,
        release_window: u32,
        /// If true, use the same address for buyer and seller (should fail).
        same_party: bool,
    },
    FundEscrow {
        order_id: u32,
    },
    ReleaseEscrow {
        order_id: u32,
        /// If true, use admin caller instead of buyer.
        by_admin: bool,
    },
    RefundEscrow {
        order_id: u32,
        /// If true, use an unauthorized caller.
        unauthorized: bool,
    },
    DisputeEscrow {
        order_id: u32,
        /// 0 = buyer, 1 = seller, 2 = unauthorized
        initiator: u8,
    },
    ResolveDispute {
        order_id: u32,
        release_to_seller: bool,
    },
    ResolveExpiredDispute {
        order_id: u32,
    },
    AutoRelease {
        order_id: u32,
    },
    /// Advance ledger time by `seconds`.
    AdvanceTime {
        seconds: u64,
    },
    /// Attempt an operation on a non-existent escrow.
    OperateOnMissingEscrow,
}

/// Generate a sequence of escrow operations seeded by `rng`.
/// `order_ids` is a pool of IDs to reference in operations.
pub fn generate_escrow_sequence(rng: &mut Lcg64, order_ids: &[u32]) -> Vec<EscrowOp> {
    let len = 1 + rng.next_usize(MAX_SEQUENCE_LEN);
    let mut ops = Vec::with_capacity(len);

    // Always start with at least one valid creation
    let seed_id = if order_ids.is_empty() {
        1
    } else {
        order_ids[rng.next_usize(order_ids.len())]
    };
    ops.push(EscrowOp::CreateEscrow {
        order_id: seed_id,
        amount: rng.next_i128_range(1_000, 100_000_000),
        release_window: rng.next_u64_range(1, 604_800) as u32,
        same_party: false,
    });

    for _ in 1..len {
        let pick = rng.next_usize(12);
        let id = if order_ids.is_empty() {
            seed_id
        } else {
            order_ids[rng.next_usize(order_ids.len())]
        };
        let op = match pick {
            0 => EscrowOp::CreateEscrow {
                order_id: rng.next_u64_range(100, 999) as u32,
                amount: rng.next_i128_range(1_000, 100_000_000),
                release_window: rng.next_u64_range(1, 604_800) as u32,
                same_party: rng.next_usize(8) == 0, // 1/8 chance of same-party
            },
            1 => EscrowOp::ReleaseEscrow {
                order_id: id,
                by_admin: rng.next_bool(),
            },
            2 => EscrowOp::RefundEscrow {
                order_id: id,
                unauthorized: rng.next_usize(5) == 0,
            },
            3 => EscrowOp::DisputeEscrow {
                order_id: id,
                initiator: rng.next_usize(3) as u8,
            },
            4 => EscrowOp::ResolveDispute {
                order_id: id,
                release_to_seller: rng.next_bool(),
            },
            5 => EscrowOp::ResolveExpiredDispute { order_id: id },
            6 => EscrowOp::AutoRelease { order_id: id },
            7 => EscrowOp::AdvanceTime {
                // Mix of short and long advances
                seconds: [1, 3600, 86400, 604_800, 30 * 86400]
                    [rng.next_usize(5)],
            },
            8 => EscrowOp::OperateOnMissingEscrow,
            9 => EscrowOp::FundEscrow { order_id: id },
            10 => EscrowOp::ReleaseEscrow {
                order_id: id,
                by_admin: false,
            },
            _ => EscrowOp::AdvanceTime {
                seconds: rng.next_u64_range(1, 7 * 86400),
            },
        };
        ops.push(op);
    }
    ops
}

// ── Staking operations ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum StakingOp {
    Stake {
        amount: i128,
        /// If true, stake a mismatching token (should fail if prior stake exists).
        wrong_token: bool,
    },
    Unstake {
        amount: i128,
        /// If true, attempt before cooldown (should fail).
        before_cooldown: bool,
        wrong_token: bool,
    },
    AdvanceTime {
        seconds: u64,
    },
    /// Attempt unstake with no prior stake.
    UnstakeEmpty,
}

pub fn generate_staking_sequence(rng: &mut Lcg64) -> Vec<StakingOp> {
    let len = 2 + rng.next_usize(MAX_SEQUENCE_LEN);
    let mut ops = Vec::with_capacity(len);

    // Always start with a valid stake
    ops.push(StakingOp::Stake {
        amount: rng.next_i128_range(1_000, 50_000_000),
        wrong_token: false,
    });

    for _ in 1..len {
        let pick = rng.next_usize(8);
        let op = match pick {
            0 => StakingOp::Stake {
                amount: rng.next_i128_range(1_000, 50_000_000),
                wrong_token: rng.next_usize(6) == 0,
            },
            1 => StakingOp::Unstake {
                amount: rng.next_i128_range(1_000, 50_000_000),
                before_cooldown: rng.next_bool(),
                wrong_token: rng.next_usize(8) == 0,
            },
            2 => StakingOp::AdvanceTime {
                seconds: [1, 3600, 86400, 7 * 86400, 30 * 86400][rng.next_usize(5)],
            },
            3 => StakingOp::UnstakeEmpty,
            4 => {
                // Multi-stake then fast advance then unstake
                ops.push(StakingOp::Stake {
                    amount: rng.next_i128_range(500, 5_000_000),
                    wrong_token: false,
                });
                StakingOp::AdvanceTime {
                    seconds: 7 * 86400 + 1,
                }
            }
            5 => StakingOp::Unstake {
                amount: rng.next_i128_range(100, 1_000),
                before_cooldown: false,
                wrong_token: false,
            },
            _ => StakingOp::AdvanceTime {
                seconds: rng.next_u64_range(1, 7 * 86400),
            },
        };
        ops.push(op);
    }
    ops
}

// ── Onboarding operations ─────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum OnboardingOp {
    OnboardBuyer,
    OnboardArtisan,
    /// Attempt to onboard same user twice.
    OnboardDuplicate,
    VerifyUser,
    DeactivateProfile,
    ReactivateProfile,
    /// Attempt to deactivate with a fake active contract count.
    DeactivateWithActiveContracts,
    UpdateRole,
}

pub fn generate_onboarding_sequence(rng: &mut Lcg64) -> Vec<OnboardingOp> {
    let len = 2 + rng.next_usize(MAX_SEQUENCE_LEN);
    let mut ops = Vec::with_capacity(len);

    // Always start with a valid onboard
    ops.push(if rng.next_bool() {
        OnboardingOp::OnboardBuyer
    } else {
        OnboardingOp::OnboardArtisan
    });

    for _ in 1..len {
        let pick = rng.next_usize(8);
        let op = match pick {
            0 => OnboardingOp::OnboardBuyer,
            1 => OnboardingOp::OnboardArtisan,
            2 => OnboardingOp::OnboardDuplicate,
            3 => OnboardingOp::VerifyUser,
            4 => OnboardingOp::DeactivateProfile,
            5 => OnboardingOp::ReactivateProfile,
            6 => OnboardingOp::DeactivateWithActiveContracts,
            _ => OnboardingOp::UpdateRole,
        };
        ops.push(op);
    }
    ops
}

// ── Upgrade operations ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum UpgradeOp {
    ProposeUpgrade,
    ApproveUpgrade,
    CancelUpgrade,
    ExecuteUpgrade,
    ProposeUpgradeWhileActive,
    ExecuteBeforeCooldown,
    CancelThenRepropose,
    AdvanceTime { seconds: u64 },
    PausePlatform,
    UnpausePlatform,
}

pub fn generate_upgrade_sequence(rng: &mut Lcg64) -> Vec<UpgradeOp> {
    let len = 2 + rng.next_usize(MAX_SEQUENCE_LEN);
    let mut ops = Vec::with_capacity(len);

    ops.push(UpgradeOp::ProposeUpgrade);

    for _ in 1..len {
        let pick = rng.next_usize(10);
        let op = match pick {
            0 => UpgradeOp::ProposeUpgrade,
            1 => UpgradeOp::ApproveUpgrade,
            2 => UpgradeOp::CancelUpgrade,
            3 => UpgradeOp::ExecuteUpgrade,
            4 => UpgradeOp::ProposeUpgradeWhileActive,
            5 => UpgradeOp::ExecuteBeforeCooldown,
            6 => UpgradeOp::CancelThenRepropose,
            7 => UpgradeOp::AdvanceTime {
                seconds: [1, 3600, 86400, 7 * 86400, 30 * 86400][rng.next_usize(5)],
            },
            8 => UpgradeOp::PausePlatform,
            _ => UpgradeOp::UnpausePlatform,
        };
        ops.push(op);
    }
    ops
}

// ── Sequence shrinking ────────────────────────────────────────────────────────

/// Attempt to shrink a failing sequence by removing individual ops while
/// maintaining the failure. Returns the shortest sub-sequence found.
///
/// `is_failure` should return `true` if the sequence still triggers the bug.
pub fn shrink_sequence<T: Clone>(
    seq: Vec<T>,
    mut is_failure: impl FnMut(&[T]) -> bool,
) -> Vec<T> {
    let mut current = seq;

    // One-deletion pass (repeat until stable)
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < current.len() {
            let mut candidate = current.clone();
            candidate.remove(i);
            if candidate.is_empty() {
                i += 1;
                continue;
            }
            if is_failure(&candidate) {
                current = candidate;
                changed = true;
                // Don't advance i; the element at i is now what was at i+1.
            } else {
                i += 1;
            }
        }
    }

    current
}
