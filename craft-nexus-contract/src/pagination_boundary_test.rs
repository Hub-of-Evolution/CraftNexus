#![cfg(test)]
extern crate alloc;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

/// Integration tests for pagination boundary conditions (Issue #1022).
///
/// These tests verify that all pagination methods correctly handle:
/// - Zero limits (must return `PaginationLimitZero`)
/// - Oversized limits (silently capped or hard error depending on context)
/// - Out-of-range cursors (must return empty or `PaginationCursorInvalid`)
/// - Deterministic results across repeated calls

fn setup_pagination_test(
    env: &Env,
) -> (
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let arbitrator = Address::generate(env);
    let onboarding_contract = Address::generate(env);

    env.ledger().with_mut(|li| {
        li.timestamp = 1711368000;
    });

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());
    client.whitelist_token(&token_contract.address());
    client.set_min_escrow_amount(&token_contract.address(), &0);
    client.set_min_release_window(&1);
    client.set_evidence_challenge_window(&0);

    token_admin_client.mint(&buyer, &10_000_000);

    (
        client,
        admin,
        buyer,
        seller,
        platform_wallet,
        token_admin_client,
        token_contract.address(),
    )
}

// ── get_escrows_by_buyer ─────────────────────────────────────────────

#[test]
fn test_buyer_pagination_zero_page_size_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, _) = setup_pagination_test(&env);

    let result = client.try_get_escrows_by_buyer(&buyer, &0, &0, &false);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_buyer_pagination_oversized_page_size_is_capped() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    // limit=500 is capped to MAX_PAGE_SIZE (100)
    let result = client.get_escrows_by_buyer(&buyer, &0, &500, &false);
    assert_eq!(result.len(), 5);
}

#[test]
fn test_buyer_pagination_out_of_range_returns_empty() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    // Page 100 with page_size=10 → start=1000, way past 5 escrows
    let result = client.get_escrows_by_buyer(&buyer, &100, &10, &false);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_buyer_pagination_deterministic_across_repeated_calls() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=10 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    let page1a = client.get_escrows_by_buyer(&buyer, &0, &3, &false);
    let page1b = client.get_escrows_by_buyer(&buyer, &0, &3, &false);
    assert_eq!(page1a.len(), page1b.len());
    for i in 0..page1a.len() {
        assert_eq!(page1a.get(i), page1b.get(i));
    }
}

// ── get_escrows_by_seller ────────────────────────────────────────────

#[test]
fn test_seller_pagination_zero_page_size_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, _, seller, _, _, _) = setup_pagination_test(&env);

    let result = client.try_get_escrows_by_seller(&seller, &0, &0, &false);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_seller_pagination_oversized_page_size_is_capped() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=3 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    let result = client.get_escrows_by_seller(&seller, &0, &500, &false);
    assert_eq!(result.len(), 3);
}

#[test]
fn test_seller_pagination_deterministic_across_repeated_calls() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=8 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    let page1a = client.get_escrows_by_seller(&seller, &0, &4, &false);
    let page1b = client.get_escrows_by_seller(&seller, &0, &4, &false);
    assert_eq!(page1a.len(), page1b.len());
    for i in 0..page1a.len() {
        assert_eq!(page1a.get(i), page1b.get(i));
    }
}

// ── get_all_escrow_ids_iterative ─────────────────────────────────────

#[test]
fn test_iterative_pagination_zero_limit_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, _, _, _, _, _) = setup_pagination_test(&env);

    let result = client.try_get_all_escrow_ids_iterative(&0, &0);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_iterative_pagination_oversized_limit_is_capped() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    // limit=200 is capped to MAX_ITERATIVE_PAGE_SIZE (20)
    let result = client.get_all_escrow_ids_iterative(&0, &200);
    assert_eq!(result.len(), 5);
}

#[test]
fn test_iterative_pagination_out_of_range_returns_empty() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    let result = client.get_all_escrow_ids_iterative(&100, &20);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_iterative_pagination_deterministic_across_repeated_calls() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=15 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    let page1a = client.get_all_escrow_ids_iterative(&0, &10);
    let page1b = client.get_all_escrow_ids_iterative(&0, &10);
    assert_eq!(page1a.len(), page1b.len());
    for i in 0..page1a.len() {
        assert_eq!(page1a.get(i), page1b.get(i));
    }
}

// ── get_fund_audit_history_paginated ──────────────────────────────────

#[test]
fn test_fund_audit_pagination_zero_limit_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, _) = setup_pagination_test(&env);

    let result = client.try_get_fund_audit_history_paginated(&buyer, &0, &0);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_fund_audit_pagination_out_of_range_returns_empty() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    client.create_escrow(&buyer, &seller, &token_id, &100, &1, &Some(3600));

    let result = client.get_fund_audit_history_paginated(&buyer, &1000, &10);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fund_audit_pagination_deterministic_across_repeated_calls() {
    let env = Env::default();
    let (client, _, buyer, seller, _, _, token_id) = setup_pagination_test(&env);

    for i in 1u32..=5 {
        client.create_escrow(&buyer, &seller, &token_id, &100, &i, &Some(3600));
    }

    let page1a = client.get_fund_audit_history_paginated(&buyer, &0, &3);
    let page1b = client.get_fund_audit_history_paginated(&buyer, &0, &3);
    assert_eq!(page1a.len(), page1b.len());
}

// ── get_artisan_stake_deposits ───────────────────────────────────────

#[test]
fn test_stake_deposits_pagination_zero_limit_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, _) = setup_pagination_test(&env);

    let result = client.try_get_artisan_stake_deposits(&buyer, &0, &0);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_stake_deposits_pagination_oversized_limit_is_capped() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, token_id) = setup_pagination_test(&env);

    client.stake_tokens(&buyer, &token_id, &1000);

    // limit=500 is capped to MAX_ADMIN_PAGE_SIZE (200)
    let result = client.get_artisan_stake_deposits(&buyer, &0, &500);
    assert_eq!(result.len(), 1);
}

#[test]
fn test_stake_deposits_pagination_out_of_range_returns_empty() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, token_id) = setup_pagination_test(&env);

    client.stake_tokens(&buyer, &token_id, &1000);

    let result = client.get_artisan_stake_deposits(&buyer, &1000, &10);
    assert_eq!(result.len(), 0);
}

// ── reconcile_token ──────────────────────────────────────────────────

#[test]
fn test_reconcile_token_zero_limit_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, _, _, _, _, token_id) = setup_pagination_test(&env);

    let result = client.try_reconcile_token(&token_id, &0, &0);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_reconcile_token_oversized_limit_returns_batch_work_error() {
    let env = Env::default();
    let (client, _, _, _, _, _, token_id) = setup_pagination_test(&env);

    // limit=50 exceeds MAX_RECONCILE_LIMIT (20)
    let result = client.try_reconcile_token(&token_id, &0, &50);
    assert!(
        matches!(result, Err(Ok(Error::InvalidBatchWorkLimit))),
        "expected InvalidBatchWorkLimit"
    );
}

// ── continue_batch_escrow ────────────────────────────────────────────

#[test]
fn test_continue_batch_zero_work_limit_returns_limit_zero_error() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, _) = setup_pagination_test(&env);

    let result = client.try_continue_batch_escrow(&0, &buyer, &0);
    assert!(
        matches!(result, Err(Ok(Error::PaginationLimitZero))),
        "expected PaginationLimitZero"
    );
}

#[test]
fn test_continue_batch_oversized_work_limit_returns_batch_work_error() {
    let env = Env::default();
    let (client, _, buyer, _, _, _, _) = setup_pagination_test(&env);

    let result = client.try_continue_batch_escrow(&0, &buyer, &10);
    assert!(
        matches!(result, Err(Ok(Error::InvalidBatchWorkLimit))),
        "expected InvalidBatchWorkLimit"
    );
}
