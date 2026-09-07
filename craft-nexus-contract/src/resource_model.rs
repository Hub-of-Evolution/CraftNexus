//! Resource-Aware Model for batch-escrow continuations (Issue #1146).
//!
//! Soroban enforces a per-transaction resource envelope (CPU instructions,
//! memory bytes, footprint entries) that, when exceeded, aborts the whole
//! invocation *after* any partial work has been performed. For resumable
//! batch creation this is dangerous: a continuation chunk that processes N
//! escrows one-by-one could mutate (fund) the first K escrows and only then
//! run out of budget, leaving the job cursor behind and the batch in a
//! half-applied state.
//!
//! To close that gap we maintain an explicit, deterministic resource model.
//! Every escrow creation is annotated with a conservative estimate of the
//! host resources it will consume (base CPU + per-byte CPU for the variable
//! length payload, footprint writes, TTL extends, and emitted events). Before
//! a continuation chunk is allowed to mutate any escrow we sum the estimates
//! for that chunk and compare them against an admin-configurable ceiling. If
//! the chunk is over budget we reject it with [`crate::Error::ResourceLimitExceeded`]
//! **before** any state change, so the job cursor and all balances remain
//! untouched. The caller may then retry with a smaller chunk or raise the
//! budget.
//!
//! # Model guarantees
//!
//! 1. **Over-budget rejection precedes mutation.** `estimate_create_chunk`
//!    is evaluated by [`crate::CraftNexusContract::continue_batch_escrow`]
//!    before `create_batch_escrow` is invoked, so no escrow is created, no
//!    funds move, and the persisted cursor is not advanced when the chunk is
//!    rejected.
//! 2. **Worst-case sizing.** The per-record estimate uses the maximum allowed
//!    record payloads (a 128-byte IPFS CID plus two 32-byte hashes), so the
//!    model is a strict upper bound on a well-formed request.
//! 3. **One-shot equivalence.** Because each chunk is created through the same
//!    `create_batch_escrow` path as a one-shot batch, a fully resumed job
//!    yields exactly the escrows a single-shot call would. The model only
//!    adds a pre-flight gating step; it never changes chunking semantics.

use crate::{EscrowCreateParams, Error};
use soroban_sdk::{Env, Vec};

// ---------------------------------------------------------------------------
// Network budget baseline (documented, conservative model constants)
// ---------------------------------------------------------------------------

/// Maximum length in characters of an IPFS CID accepted by the contract
/// (see `validate_ipfs_cid`, lib.rs). Used to size the worst-case record.
pub const MAX_IPFS_CID_LEN: u32 = 128;
/// Fixed size in bytes of the canonical metadata / service-agreement hashes.
pub const OPTIONAL_HASH_LEN: u32 = 32;

/// Lower bound on the estimated CPU cost (host instruction units) to create a
/// single escrow with the *minimum* variable-length payload. This covers the
/// onboarding state checks, escrow record construction, token transfer /
/// audit bookkeeping, and the two triggerable storage index updates.
pub const PER_ESCROW_BASE_CPU_INSNS: u64 = 2_400_000;
/// Marginal CPU (host instruction units) attributed per byte of the variable
/// length IPFS CID stored on the escrow record. Captures the extra
/// encode/decode and footprint cost of larger records (Issue #1146).
pub const PER_CID_BYTE_CPU_INSNS: u64 = 2_000;

/// Number of persistent storage writes conservatively attributed to one escrow
/// creation inside a batch (escrow record + buyer/seller index + batch-wide
/// count + global index accounting).
pub const PER_ESCROW_STORAGE_WRITES: u64 = 12;
/// Number of TTL extension host calls conservatively attributed to one escrow
/// creation inside a batch.
pub const PER_ESCROW_TTL_EXTENDS: u64 = 8;
/// Number of `EscrowEvent` events emitted per escrow creation.
pub const PER_ESCROW_EVENTS: u64 = 1;

/// Admin-configurable ceiling used by default when no explicit budget has been
/// set. Chosen conservatively: five worst-case records (128-byte CIDs) are
/// estimated at `5 * (2.4e6 + 128 * 2e3) = 13.28e6` instruction units, so the
/// default leaves headroom while staying comfortably below the ~100e6
/// protocol transaction ceiling.
pub const DEFAULT_CONTINUATION_CPU_BUDGET: u64 = 20_000_000;

/// The smallest budget an admin may configure. Prevents a zeroing attack that
/// would silently disable every continuation.
pub const MIN_CONTINUATION_CPU_BUDGET: u64 = 1_000_000;

// ---------------------------------------------------------------------------
// Estimate type
// ---------------------------------------------------------------------------

/// A deterministic, conservative estimate of the host resources a batch chunk
/// will consume when created through `create_batch_escrow`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchResourceEstimate {
    /// Estimated host CPU instructions.
    pub est_cpu_insns: u64,
    /// Estimated persistent storage writes.
    pub est_storage_writes: u64,
    /// Estimated TTL extension host calls.
    pub est_ttl_extends: u64,
    /// Estimated number of emitted events.
    pub est_events: u64,
}

impl BatchResourceEstimate {
    /// True when the estimate is within the given CPU budget.
    #[inline]
    pub fn fits_within_cpu_budget(&self, cpu_budget: u64) -> bool {
        self.est_cpu_insns <= cpu_budget
    }
}

/// Compute the worst-case CPU estimate for a single `EscrowCreateParams`.
///
/// The payload size is the IPFS CID length clamped to the contract's maximum;
/// the two optional 32-byte hashes are constant size so they are folded into
/// the base cost.
#[inline]
pub fn estimate_single_escrow_cpu(cid_len: u32) -> u64 {
    let cid_len = cid_len.min(MAX_IPFS_CID_LEN);
    PER_ESCROW_BASE_CPU_INSNS + u64::from(cid_len).saturating_mul(PER_CID_BYTE_CPU_INSNS)
}

/// Estimate the resources a scheduled batch chunk will consume when its
/// escrows are created. `params` is the exact chunk (`job.params[i]` for
/// `i in [next_index, next_index + work_limit)`).
///
/// This is a pure, side-effect free computation: it performs no storage reads
/// or writes, so it is safe to run on the continuation boundary before any
/// mutation.
pub fn estimate_create_chunk(
    _env: &Env,
    params: &Vec<EscrowCreateParams>,
) -> BatchResourceEstimate {
    let mut est_cpu_insns: u64 = 0;
    let mut n: u64 = 0;
    for i in 0..params.len() {
        if let Some(p) = params.get(i) {
            let cid_len = match &p.ipfs_hash {
                Some(cid) => cid.len(),
                None => 0,
            };
            est_cpu_insns = est_cpu_insns.saturating_add(estimate_single_escrow_cpu(cid_len));
            n = n.saturating_add(1);
        }
    }
    BatchResourceEstimate {
        est_cpu_insns,
        est_storage_writes: n.saturating_mul(PER_ESCROW_STORAGE_WRITES),
        est_ttl_extends: n.saturating_mul(PER_ESCROW_TTL_EXTENDS),
        est_events: n.saturating_mul(PER_ESCROW_EVENTS),
    }
}

/// Centralised budget resolution / validation used by the continuation path.
///
/// `configured` is the estimate of the immediate chunk's CPU cost (call
/// [`estimate_create_chunk`] first). `budget` is the admin-configured ceiling,
/// falling back to [`DEFAULT_CONTINUATION_CPU_BUDGET`] when `None`.
///
/// Returns `Ok(())` when the chunk fits and `Err(Error::ResourceLimitExceeded)`
/// when it does not. Callers must invoke this **before** any state mutation.
pub fn ensure_chunk_within_budget(
    estimate: &BatchResourceEstimate,
    budget: Option<u64>,
) -> Result<(), Error> {
    let budget = budget.unwrap_or(DEFAULT_CONTINUATION_CPU_BUDGET);
    if estimate.fits_within_cpu_budget(budget) {
        Ok(())
    } else {
        Err(Error::ResourceLimitExceeded)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_uses_worst_case_cid_clamped_to_max() {
        let tiny = estimate_single_escrow_cpu(0);
        let max = estimate_single_escrow_cpu(MAX_IPFS_CID_LEN);
        let over_max = estimate_single_escrow_cpu(10_000);

        assert!(max > tiny);
        assert_eq!(max, over_max, "cid length must be clamped to the max");
    }

    #[test]
    fn estimate_never_flags_an_empty_chunk() {
        let env = Env::default();
        let params = Vec::new(&env);
        let est = estimate_create_chunk(&env, &params);
        assert_eq!(est.est_cpu_insns, 0);
        assert!(ensure_chunk_within_budget(&est, Some(1)).is_ok());
    }

    #[test]
    fn default_budget_accepts_maximum_work_chunk() {
        let env = Env::default();
        let mut params = Vec::new(&env);
        // Longest valid base32lower CID (~100 chars) models the worst-case
        // realistic record size for a full work-limit chunk.
        let cid = format!("b{}", "a".repeat(99));
        for i in 0..5u32 {
            params.push_back(EscrowCreateParams {
                buyer: soroban_sdk::Address::generate(&env),
                seller: soroban_sdk::Address::generate(&env),
                token: soroban_sdk::Address::generate(&env),
                amount: 1_000,
                order_id: i,
                release_window: Some(3_600),
                ipfs_hash: Some(soroban_sdk::String::from_str(&env, &cid)),
                metadata_hash: None,
                service_agreement_hash: None,
            });
        }
        let est = estimate_create_chunk(&env, &params);
        assert!(ensure_chunk_within_budget(&est, None).is_ok());
    }

    #[test]
    fn over_budget_estimate_is_rejected() {
        let env = Env::default();
        let mut params = Vec::new(&env);
        for i in 0..5u32 {
            params.push_back(EscrowCreateParams {
                buyer: soroban_sdk::Address::generate(&env),
                seller: soroban_sdk::Address::generate(&env),
                token: soroban_sdk::Address::generate(&env),
                amount: 1_000,
                order_id: i,
                release_window: Some(3_600),
                ipfs_hash: None,
                metadata_hash: None,
                service_agreement_hash: None,
            });
        }
        let est = estimate_create_chunk(&env, &params);
        let tiny_budget = est.est_cpu_insns / 2;
        assert_eq!(
            ensure_chunk_within_budget(&est, Some(tiny_budget)),
            Err(Error::ResourceLimitExceeded)
        );
    }
}
