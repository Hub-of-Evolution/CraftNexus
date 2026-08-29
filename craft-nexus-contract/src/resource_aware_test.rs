#![cfg(test)]

//! Resource-Aware Model Verification (Issue #1146).
//!
//! These tests exercise the three acceptance criteria of the resource-aware
//! model added in `resource_model.rs`:
//!
//! 1. **Over-budget operations are rejected before mutation.** A continuation
//!    chunk whose estimated resource footprint exceeds the configured budget is
//!    rejected with `Error::ResourceLimitExceeded` *before* any escrow is
//!    created or funds move: the job cursor stays put and no escrow exists.
//! 2. **Resumed execution matches one-shot semantics.** Scheduling and fully
//!    resuming a batch through `continue_batch_escrow` yields exactly the same
//!    escrows (order IDs, amounts) as a single `create_escrows_batch` call.
//! 3. **Resource assumptions hold under worst-case record sizes.** A full
//!    work-limit chunk whose records carry the maximum IPFS CID and 32-byte
//!    hashes stays within the default Soroban ledger budget.

use super::*;
use resource_model::{self, BatchResourceEstimate};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env,
};

/// Shared fixture mirroring the other test modules.
fn setup_test() -> (
    Env,
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let onboarding = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_id.address();
    let token_asset = token::StellarAssetClient::new(&env, &token_addr);
    token_asset.mint(&buyer, &1_000_000_000);

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &Some(onboarding));
    client.set_min_escrow_amount(&token_addr, &0);
    client.set_min_release_window(&1);

    (env, client, buyer, seller, token_addr, admin)
}

/// Build `n` escrow-create params. `worst_case` pads each record with the
/// longest *valid* IPFS CID and 32-byte optional hashes.
fn build_params(
    env: &Env,
    buyer: &Address,
    seller: &Address,
    token: &Address,
    n: u32,
    offset: u32,
    worst_case: bool,
) -> soroban_sdk::Vec<EscrowCreateParams> {
    let mut params = soroban_sdk::Vec::new(env);
    // A valid CIDv1 base32lower CID of ~100 chars (prefix 'b', payload [a-z2-7]).
    // This is the longest payload `validate_ipfs_cid` accepts, so it exercises the
    // worst-case record size while still passing validation.
    let cid = format!("b{}", "a".repeat(99));
    let hash = soroban_sdk::Bytes::from_array(env, &[0xAB; 32]);
    for i in 0..n {
        params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount: 1_000,
            order_id: offset + i + 1,
            release_window: Some(3_600),
            ipfs_hash: if worst_case {
                Some(soroban_sdk::String::from_str(env, &cid))
            } else {
                None
            },
            metadata_hash: if worst_case { Some(hash.clone()) } else { None },
            service_agreement_hash: if worst_case {
                Some(hash.clone())
            } else {
                None
            },
        });
    }
    params
}

// ── Criterion 1: over-budget rejected before mutation ────────────────────────

#[test]
fn over_budget_continuation_is_rejected_before_mutation() {
    let (env, client, buyer, seller, token, _admin) = setup_test();

    let params = build_params(&env, &buyer, &seller, &token, 5, 1_000, false);
    let job_id = client.schedule_batch_escrow(&buyer, &params);

    // A budget that is below the estimated cost of even a single escrow makes
    // every continuation over budget. The setter is admin-gated.
    client.set_continuation_resource_budget(&2_000_000);
    assert_eq!(client.get_continuation_resource_budget(), 2_000_000);

    let result = client.try_continue_batch_escrow(&job_id, &buyer, &5);
    assert!(
        matches!(result, Err(Ok(Error::ResourceLimitExceeded))),
        "expected ResourceLimitExceeded, got {:?}",
        result
    );

    // Cursor must not have advanced...
    let progress = client.get_batch_escrow_progress(&job_id).unwrap();
    assert_eq!(progress.next_index, 0);
    assert_eq!(progress.status, BatchJobStatus::Pending);

    // ...no escrow may exist (nothing mutated, no funds moved)...
    assert!(client.try_get_escrow(&1_001).is_err());
    assert!(client.try_get_escrow(&1_005).is_err());
}

#[test]
fn over_budget_rejection_leaves_balances_untouched() {
    let (env, client, buyer, seller, token, _admin) = setup_test();

    let token_client = token::Client::new(&env, &token);
    let balance_before = token_client.balance(&buyer);
    let params = build_params(&env, &buyer, &seller, &token, 5, 2_000, false);
    let job_id = client.schedule_batch_escrow(&buyer, &params);

    client.set_continuation_resource_budget(&1_000_000);
    let result = client.try_continue_batch_escrow(&job_id, &buyer, &5);
    assert!(matches!(result, Err(Ok(Error::ResourceLimitExceeded))));

    let balance_after = token_client.balance(&buyer);
    assert_eq!(
        balance_before, balance_after,
        "buyer balance must not change when a continuation is rejected"
    );
}

// ── Criterion 2: resumed execution matches one-shot semantics ────────────────

#[test]
fn resumed_execution_matches_one_shot_semantics() {
    // One-shot path (its own env / client).
    let (env_one, client_one, buyer_one, seller_one, token_one, _) = setup_test();
    let one_shot_params = build_params(&env_one, &buyer_one, &seller_one, &token_one, 12, 3_000, false);
    let one_shot_ids = client_one.create_escrows_batch(&one_shot_params);

    // Scheduled + resumed path (separate env / client).
    let (env_two, client_two, buyer_two, seller_two, token_two, _) = setup_test();
    let scheduled_params = build_params(&env_two, &buyer_two, &seller_two, &token_two, 12, 3_000, false);
    let job_id = client_two.schedule_batch_escrow(&buyer_two, &scheduled_params);

    // Resume in 5,5,2 chunks (MAX_BATCH_WORK_LIMIT = 5).
    let p1 = client_two.continue_batch_escrow(&job_id, &buyer_two, &5);
    assert_eq!(p1.next_index, 5);
    let p2 = client_two.continue_batch_escrow(&job_id, &buyer_two, &5);
    assert_eq!(p2.next_index, 10);
    let p3 = client_two.continue_batch_escrow(&job_id, &buyer_two, &2);
    assert_eq!(p3.next_index, 12);
    assert_eq!(p3.status, BatchJobStatus::Completed);

    // Both paths created escrows for the exact same order IDs and amounts.
    assert_eq!(one_shot_ids.len(), 12);
    for i in 0..12u32 {
        let order_id = 3_001 + i;
        let resumed = client_two.get_escrow(&order_id);
        let one = client_one.get_escrow(&order_id);
        assert_eq!(resumed.amount, one.amount, "order {} amount mismatch", order_id);
    }
}

// ── Criterion 3: worst-case record sizes stay within budget ──────────────────

#[test]
fn worst_case_records_stay_within_default_budget() {
    let (env, client, buyer, seller, token, _) = setup_test();

    let params = build_params(
        &env,
        &buyer,
        &seller,
        &token,
        pagination_validation::MAX_BATCH_WORK_LIMIT,
        4_000,
        true,
    );
    let job_id = client.schedule_batch_escrow(&buyer, &params);

    // Default continuation budget must admit a full worst-case work chunk.
    let est = client.get_continuation_resource_budget();
    assert!(est >= resource_model::DEFAULT_CONTINUATION_CPU_BUDGET);

    // Reset to the *default* ledger budget so a breach aborts the test loudly.
    env.budget().reset_default();
    let result = client.try_continue_batch_escrow(&job_id, &buyer, &pagination_validation::MAX_BATCH_WORK_LIMIT);
    match result {
        Ok(Ok(progress)) => assert_eq!(progress.next_index, pagination_validation::MAX_BATCH_WORK_LIMIT),
        Ok(Err(e)) => panic!("worst-case continuation returned error: {:?}", e),
        Err(_) => panic!("worst-case continuation exceeded the default ledger budget"),
    }
}

// ── Model unit checks (pure, no ledger) ──────────────────────────────────────

#[test]
fn model_estimate_scales_with_cid_size() {
    let small = resource_model::estimate_single_escrow_cpu(0);
    let large = resource_model::estimate_single_escrow_cpu(resource_model::MAX_IPFS_CID_LEN);
    assert!(large > small);
    assert_eq!(
        resource_model::estimate_single_escrow_cpu(10_000),
        large,
        "cid length must be clamped to the model maximum"
    );
}

#[test]
fn model_estimate_carries_footprint_counts() {
    let env = Env::default();
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);

    // Known CID length (10 chars) so the expected CPU is exact.
    let cid = soroban_sdk::String::from_str(&env, "baaaaaaaaa");
    let mut params = soroban_sdk::Vec::new(&env);
    for i in 0..4u32 {
        params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount: 1_000,
            order_id: i,
            release_window: Some(3_600),
            ipfs_hash: Some(cid.clone()),
            metadata_hash: None,
            service_agreement_hash: None,
        });
    }

    let est = resource_model::estimate_create_chunk(&env, &params);
    let per_record = resource_model::estimate_single_escrow_cpu(10);
    let expected: BatchResourceEstimate = BatchResourceEstimate {
        est_cpu_insns: 4 * per_record,
        est_storage_writes: 4 * resource_model::PER_ESCROW_STORAGE_WRITES,
        est_ttl_extends: 4 * resource_model::PER_ESCROW_TTL_EXTENDS,
        est_events: 4 * resource_model::PER_ESCROW_EVENTS,
    };
    assert_eq!(est, expected);
}
