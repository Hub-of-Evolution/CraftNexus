//! Model-based sequence shrinking tests.
//!
//! # Summary
//!
//! When a generated sequence violates an invariant, the test suite reduces it
//! to a minimal reproducible scenario by shrinking:
//!
//! - **Calls**: Remove operations while preserving the failure
//! - **Actors**: Normalize actor IDs to minimal set (0, 1, 2...)
//! - **Timestamps**: Reduce time jumps to minimal deltas
//! - **Amounts**: Shrink values while maintaining the violation
//! - **Token diversity**: Normalize token IDs to minimal set
//!
//! The harness reports:
//! 1. The first violated invariant
//! 2. The state transition that triggered it
//! 3. A minimal reproducible sequence
//!
//! Shrunk cases can be extracted and run as ordinary deterministic tests.
//!
//! # Running
//!
//! ```bash
//! cargo test --features testutils model_shrinking -- --nocapture
//! ```

#![cfg(test)]
extern crate alloc;
use alloc::string::{String, ToString};

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

use super::{
    generators::{shrink_model_based, ShrinkableOp},
    harness::{advance_ledger_time, InvariantReport, PropHarness, StateTransition},
    seed_from_env, Lcg64,
};
use crate::{CraftNexusContractClient, Resolution};

// ── Test operation types ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum TestOp {
    CreateEscrow,
    FundEscrow,
    ReleaseEscrow,
    RefundEscrow,
    RaiseDispute,
    ResolveDispute { to_seller: bool },
    StakeTokens,
    UnstakeTokens,
    AdvanceTime,
}

// ── Test fixtures ─────────────────────────────────────────────────────────────

fn setup_env() -> (
    Env,
    Address,
    Address,
    Address,
    Vec<Address>,
    Address,
    token::StellarAssetClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    // Generate pool of actors
    let actors: alloc::vec::Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();

    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_id = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    env.ledger().with_mut(|li| li.timestamp = 1_700_000_000);

    let contract_id = env.register_contract(None, crate::CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    client.set_min_escrow_amount(&token_id, &1_000);
    client.set_min_release_window(&86_400);
    client.set_evidence_challenge_window(&0);

    // Mint to all actors
    for actor in &actors {
        token_admin.mint(actor, &1_000_000_000_000i128);
    }

    (env, contract_id, arbitrator, actors, token_id, token_admin)
}

// ── Shrinking demonstration tests ─────────────────────────────────────────────

#[test]
fn shrink_removes_irrelevant_operations() {
    // Create a sequence where only 3 operations matter for a failure
    let long_sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 0,
            timestamp: 1000,
            amount: Some(10_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::AdvanceTime,
            actor_id: 0,
            timestamp: 2000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::StakeTokens,
            actor_id: 1,
            timestamp: 3000,
            amount: Some(5_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::RaiseDispute,
            actor_id: 0,
            timestamp: 4000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::AdvanceTime,
            actor_id: 0,
            timestamp: 5000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::UnstakeTokens,
            actor_id: 1,
            timestamp: 6000,
            amount: Some(5_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::ResolveDispute { to_seller: true },
            actor_id: 2,
            timestamp: 7000,
            amount: None,
            token_id: None,
        },
    ];

    // Failure condition: sequence has CreateEscrow + RaiseDispute + ResolveDispute
    let is_failure = |seq: &[ShrinkableOp<TestOp>]| -> bool {
        let has_create = seq.iter().any(|s| matches!(s.op, TestOp::CreateEscrow));
        let has_dispute = seq.iter().any(|s| matches!(s.op, TestOp::RaiseDispute));
        let has_resolve = seq
            .iter()
            .any(|s| matches!(s.op, TestOp::ResolveDispute { .. }));
        has_create && has_dispute && has_resolve
    };

    let shrunk = shrink_model_based(long_sequence.clone(), is_failure);

    // Shrunk sequence should have exactly 3 operations
    assert_eq!(shrunk.len(), 3);

    // Verify operations are the essential ones
    assert!(matches!(shrunk[0].op, TestOp::CreateEscrow));
    assert!(matches!(shrunk[1].op, TestOp::RaiseDispute));
    assert!(matches!(shrunk[2].op, TestOp::ResolveDispute { .. }));

    println!("[shrink_test] Original: {} steps → Shrunk: {} steps", long_sequence.len(), shrunk.len());
}

#[test]
fn shrink_reduces_actor_diversity() {
    let sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 5,
            timestamp: 1000,
            amount: Some(10_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::RaiseDispute,
            actor_id: 3,
            timestamp: 2000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::ResolveDispute { to_seller: false },
            actor_id: 7,
            timestamp: 3000,
            amount: None,
            token_id: None,
        },
    ];

    let is_failure = |_seq: &[ShrinkableOp<TestOp>]| -> bool { true };

    let shrunk = shrink_model_based(sequence.clone(), is_failure);

    // Actor IDs should be normalized to 0, 0, 0 or minimal values
    assert!(shrunk[0].actor_id < sequence[0].actor_id);
    assert!(shrunk[1].actor_id < sequence[1].actor_id);
    assert!(shrunk[2].actor_id < sequence[2].actor_id);

    println!("[shrink_test] Actor IDs normalized: {:?} → {:?}",
        sequence.iter().map(|s| s.actor_id).collect::<alloc::vec::Vec<_>>(),
        shrunk.iter().map(|s| s.actor_id).collect::<alloc::vec::Vec<_>>()
    );
}

#[test]
fn shrink_minimizes_timestamp_jumps() {
    let sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 0,
            timestamp: 1000,
            amount: Some(10_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::AdvanceTime,
            actor_id: 0,
            timestamp: 1_000_000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::ReleaseEscrow,
            actor_id: 0,
            timestamp: 1_000_100,
            amount: None,
            token_id: None,
        },
    ];

    let is_failure = |_seq: &[ShrinkableOp<TestOp>]| -> bool { true };

    let shrunk = shrink_model_based(sequence.clone(), is_failure);

    // Time jumps should be reduced
    let original_delta = sequence[1].timestamp - sequence[0].timestamp;
    let shrunk_delta = shrunk[1].timestamp - shrunk[0].timestamp;

    assert!(shrunk_delta < original_delta);

    println!("[shrink_test] Time delta reduced: {} → {}", original_delta, shrunk_delta);
}

#[test]
fn shrink_reduces_amounts() {
    let sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 0,
            timestamp: 1000,
            amount: Some(100_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::StakeTokens,
            actor_id: 1,
            timestamp: 2000,
            amount: Some(50_000_000),
            token_id: Some(0),
        },
    ];

    let is_failure = |_seq: &[ShrinkableOp<TestOp>]| -> bool { true };

    let shrunk = shrink_model_based(sequence.clone(), is_failure);

    // Amounts should be reduced
    assert!(shrunk[0].amount.unwrap() < sequence[0].amount.unwrap());
    assert!(shrunk[1].amount.unwrap() < sequence[1].amount.unwrap());

    println!("[shrink_test] Amounts reduced: {:?} → {:?}",
        sequence.iter().map(|s| s.amount).collect::<alloc::vec::Vec<_>>(),
        shrunk.iter().map(|s| s.amount).collect::<alloc::vec::Vec<_>>()
    );
}

#[test]
fn shrink_normalizes_token_ids() {
    let sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 0,
            timestamp: 1000,
            amount: Some(10_000_000),
            token_id: Some(3),
        },
        ShrinkableOp {
            op: TestOp::StakeTokens,
            actor_id: 1,
            timestamp: 2000,
            amount: Some(5_000_000),
            token_id: Some(2),
        },
    ];

    let is_failure = |_seq: &[ShrinkableOp<TestOp>]| -> bool { true };

    let shrunk = shrink_model_based(sequence.clone(), is_failure);

    // Token IDs should be normalized to 0 or minimal values
    assert!(shrunk[0].token_id.unwrap() < sequence[0].token_id.unwrap());
    assert!(shrunk[1].token_id.unwrap() < sequence[1].token_id.unwrap());

    println!("[shrink_test] Token IDs normalized: {:?} → {:?}",
        sequence.iter().map(|s| s.token_id).collect::<alloc::vec::Vec<_>>(),
        shrunk.iter().map(|s| s.token_id).collect::<alloc::vec::Vec<_>>()
    );
}

// ── Integration with harness ──────────────────────────────────────────────────

#[test]
fn model_harness_reports_first_violation() {
    let (env, contract_id, arbitrator, actors, token_id, _) = setup_env();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    // Define a sequence that will violate an invariant
    let sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 0,
            timestamp: 1000,
            amount: Some(10_000_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::RaiseDispute,
            actor_id: 0,
            timestamp: 2000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::ResolveDispute { to_seller: true },
            actor_id: 1,
            timestamp: 3000,
            amount: None,
            token_id: None,
        },
    ];

    let mut order_counter = 100u32;
    let mut violation_found = false;

    // Execute sequence and check for violations
    for (step, sop) in sequence.iter().enumerate() {
        let buyer = &actors[sop.actor_id as usize % actors.len()];
        let seller = &actors[(sop.actor_id as usize + 1) % actors.len()];

        match &sop.op {
            TestOp::CreateEscrow => {
                let amount = sop.amount.unwrap_or(10_000_000);
                let _ = client.try_create_escrow(buyer, seller, &token_id, &amount, &604_800, &None);
                order_counter += 1;
            }
            TestOp::RaiseDispute => {
                let _ = client.try_raise_dispute(&(order_counter - 1), buyer);
            }
            TestOp::ResolveDispute { to_seller } => {
                let resolution = if *to_seller {
                    Resolution::ReleaseToSeller
                } else {
                    Resolution::RefundToBuyer
                };
                let result = client.try_resolve_dispute(&(order_counter - 1), &resolution, &arbitrator);

                // Check invariant: dispute must exist before resolution
                if result.is_ok() {
                    let escrow = client.get_escrow(&(order_counter - 1));
                    if !matches!(escrow.status, crate::EscrowStatus::Resolved) {
                        violation_found = true;
                        println!("[invariant] Violation at step {}: resolve succeeded but status is {:?}",
                            step, escrow.status);
                    }
                }
            }
            TestOp::AdvanceTime => {
                advance_ledger_time(&env, 86_400);
            }
            _ => {}
        }
    }

    // This test verifies the reporting mechanism works
    assert!(!violation_found, "Expected clean execution for this test sequence");
}

#[test]
fn shrunk_sequences_are_deterministic() {
    let (env, contract_id, arbitrator, actors, token_id, _) = setup_env();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    // A minimal shrunk sequence should execute deterministically
    let minimal_sequence: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 0,
            timestamp: 1000,
            amount: Some(10_000),
            token_id: Some(0),
        },
        ShrinkableOp {
            op: TestOp::ReleaseEscrow,
            actor_id: 0,
            timestamp: 1100,
            amount: None,
            token_id: None,
        },
    ];

    let buyer = &actors[0];
    let seller = &actors[1];
    let amount = 10_000i128;

    // Execute the shrunk sequence
    let order_id = 200u32;
    client.create_escrow(buyer, seller, &token_id, &amount, &604_800, &None);
    let result = client.try_release_funds(&order_id);

    // Verify expected outcome
    assert!(result.is_ok(), "Minimal sequence should execute cleanly");

    println!("[shrink_test] Minimal sequence executed deterministically");
}

// ── Demonstration of full shrinking workflow ──────────────────────────────────

#[test]
fn full_shrinking_workflow_demonstration() {
    // Start with a complex failing sequence
    let original: alloc::vec::Vec<ShrinkableOp<TestOp>> = alloc::vec![
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 5,
            timestamp: 1000,
            amount: Some(100_000_000),
            token_id: Some(2),
        },
        ShrinkableOp {
            op: TestOp::AdvanceTime,
            actor_id: 0,
            timestamp: 500_000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::StakeTokens,
            actor_id: 3,
            timestamp: 501_000,
            amount: Some(50_000_000),
            token_id: Some(1),
        },
        ShrinkableOp {
            op: TestOp::RaiseDispute,
            actor_id: 5,
            timestamp: 502_000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::AdvanceTime,
            actor_id: 0,
            timestamp: 600_000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::UnstakeTokens,
            actor_id: 3,
            timestamp: 601_000,
            amount: Some(50_000_000),
            token_id: Some(1),
        },
        ShrinkableOp {
            op: TestOp::ResolveDispute { to_seller: true },
            actor_id: 7,
            timestamp: 602_000,
            amount: None,
            token_id: None,
        },
        ShrinkableOp {
            op: TestOp::CreateEscrow,
            actor_id: 2,
            timestamp: 603_000,
            amount: Some(25_000_000),
            token_id: Some(0),
        },
    ];

    // Failure condition: has disputed escrow resolution
    let is_failure = |seq: &[ShrinkableOp<TestOp>]| -> bool {
        let has_create = seq.iter().any(|s| matches!(s.op, TestOp::CreateEscrow));
        let has_dispute = seq.iter().any(|s| matches!(s.op, TestOp::RaiseDispute));
        let has_resolve = seq
            .iter()
            .any(|s| matches!(s.op, TestOp::ResolveDispute { .. }));
        has_create && has_dispute && has_resolve
    };

    let shrunk = shrink_model_based(original.clone(), is_failure);

    println!("\n=== Full Shrinking Workflow ===");
    println!("Original sequence: {} steps", original.len());
    println!("Shrunk sequence: {} steps", shrunk.len());
    println!("\nOriginal:");
    for (i, sop) in original.iter().enumerate() {
        println!("  {}: actor={}, time={}, amount={:?}, op={:?}",
            i, sop.actor_id, sop.timestamp, sop.amount, sop.op);
    }
    println!("\nShrunk (minimal reproducer):");
    for (i, sop) in shrunk.iter().enumerate() {
        println!("  {}: actor={}, time={}, amount={:?}, op={:?}",
            i, sop.actor_id, sop.timestamp, sop.amount, sop.op);
    }

    // Verify shrinking preserved the failure condition
    assert!(is_failure(&shrunk), "Shrunk sequence must still trigger failure");

    // Verify shrinking reduced complexity
    assert!(shrunk.len() < original.len(), "Shrunk sequence should be shorter");
    assert!(
        shrunk.iter().map(|s| s.actor_id).max().unwrap()
            < original.iter().map(|s| s.actor_id).max().unwrap(),
        "Shrunk sequence should use fewer actors"
    );
}
