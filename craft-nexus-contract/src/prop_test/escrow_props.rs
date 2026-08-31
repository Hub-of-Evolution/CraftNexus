//! Escrow state machine property tests.
//!
//! # Properties verified
//!
//! 1. **Fund conservation** – contract balance >= total locked funds.
//! 2. **Terminal-state immutability** – once Released/Refunded/Resolved,
//!    no further settlement call succeeds.
//! 3. **One-time settlement** – release, refund, and resolve each succeed
//!    exactly once per escrow.
//! 4. **Authorization** – non-arbitrators cannot resolve a dispute.
//! 5. **Dispute-only resolution** – `resolve_dispute` requires `Disputed` status.
//! 6. **Window-based auto-release** – auto-release fails before window, succeeds after.
//! 7. **Pause gate** – `create_escrow` fails while paused.
//! 8. **Fee conservation** – platform_fee + seller == escrow amount on release.
//! 9. **All documented transitions reachable** – every status variant is visited.
//! 10. **Model-contract agreement** – model conservation invariants hold after each sequence.
//! 11. **Dispute blocked on terminal escrow**.

#![cfg(test)]
extern crate alloc;

use soroban_sdk::{testutils::{Address as _, Ledger}, token, Address, Env, Symbol};

use super::{
    generators::{generate_escrow_sequence, EscrowOp},
    harness::advance_ledger_time,
    model::ModelState,
    seed_from_env, Lcg64, DEFAULT_CASE_COUNT,
};
use crate::{CraftNexusContractClient, EscrowStatus, Resolution};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fresh_env() -> (
    Env,
    Address, // contract_id
    Address, // admin
    Address, // arbitrator
    Address, // buyer
    Address, // seller
    Address, // token_id
    Address, // platform_wallet
    token::StellarAssetClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);

    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_id = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

    let contract_id = env.register_contract(None, crate::CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    client.set_min_escrow_amount(&token_id, &0);
    client.set_min_release_window(&1);
    client.set_evidence_challenge_window(&0);

    token_admin.mint(&buyer, &1_000_000_000_000i128);
    token_admin.mint(&admin, &100_000_000i128);

    // Return env separately so we can borrow properly in tests
    (env, contract_id, admin, arbitrator, buyer, seller, token_id, platform_wallet, token_admin)
}

// ── Property 1: Fund conservation ────────────────────────────────────────────

/// The contract's token balance must be >= tracked locked funds after any sequence.
#[test]
fn prop_fund_conservation() {
    let mut rng = Lcg64::new(seed_from_env());

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, admin, arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let order_ids: alloc::vec::Vec<u32> = (1u32..=5).collect();
        let ops = generate_escrow_sequence(&mut crng, &order_ids);

        for op in &ops {
            run_op(&env, &client, op, &buyer, &seller, &admin, &arbitrator, &token_id);
        }

        // Invariant: tracked locked <= actual balance
        let allocation = client.get_fund_allocation(&token_id);
        let balance = token::Client::new(&env, &token_id).balance(&client.address);

        if allocation.total_locked > balance {
            panic!(
                "[prop_fund_conservation] locked({}) > balance({}) (seed=0x{:016X})",
                allocation.total_locked, balance, case_seed
            );
        }
    }
}

// ── Property 2 & 3: Terminal immutability + one-time settlement ───────────────

/// Once an escrow reaches a terminal state, no further settlement call succeeds.
#[test]
fn prop_terminal_state_immutable() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x1111);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, __admin, arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &Some(3600));

        let path = crng.next_usize(3);
        match path {
            0 => {
                advance_ledger_time(&env, 7 * 86400 + 1);
                let _ = client.try_release_funds(&1);
            }
            1 => {
                let _ = client.try_refund(&1u64);
            }
            _ => {
                client.dispute_escrow(&1, &Symbol::new(&env, "Damaged"), &buyer);
                let _ = client.try_resolve_dispute(&1, &Resolution::ReleaseToSeller, &arbitrator);
            }
        }

        let escrow = match client.try_get_escrow(&1) {
            Ok(Ok(e)) => e,
            _ => continue,
        };

        if !matches!(
            escrow.status,
            EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved
        ) {
            continue;
        }

        // All settlement paths must now fail
        let r1 = client.try_release_funds(&1);
        let r2 = client.try_refund(&1u64);
        let r3 = client.try_resolve_dispute(&1, &Resolution::ReleaseToSeller, &arbitrator);

        if r1.is_ok() && r1.unwrap().is_ok() {
            panic!(
                "[prop_terminal_state_immutable] release succeeded on {:?} escrow (seed=0x{:016X})",
                escrow.status, case_seed
            );
        }
        if r2.is_ok() && r2.unwrap().is_ok() {
            panic!(
                "[prop_terminal_state_immutable] refund succeeded on {:?} escrow (seed=0x{:016X})",
                escrow.status, case_seed
            );
        }
        if r3.is_ok() && r3.unwrap().is_ok() {
            panic!(
                "[prop_terminal_state_immutable] resolve succeeded on {:?} escrow (seed=0x{:016X})",
                escrow.status, case_seed
            );
        }
    }
}

// ── Property 4: Unauthorized cannot resolve dispute ───────────────────────────

#[test]
fn prop_unauthorized_cannot_resolve_dispute() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x2222);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin, _arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let unauthorized = Address::generate(&env);

        client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &42, &Some(3600));
        client.dispute_escrow(&42, &Symbol::new(&env, "Defect"), &buyer);

        let r = client.try_resolve_dispute(&42, &Resolution::ReleaseToSeller, &unauthorized);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_unauthorized_cannot_resolve_dispute] unauthorized resolve succeeded (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 5: resolve_dispute requires Disputed state ──────────────────────

#[test]
fn prop_resolve_requires_disputed_state() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x3333);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin, arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        client.create_escrow(&buyer, &seller, &token_id, &50_000_000, &1, &Some(3600));

        // Active state → resolve must fail
        let r = client.try_resolve_dispute(&1, &Resolution::ReleaseToSeller, &arbitrator);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_resolve_requires_disputed_state] resolve succeeded on Active escrow (seed=0x{:016X})",
                case_seed
            );
        }

        // Release it, then resolve must fail on Released too
        advance_ledger_time(&env, 7 * 86400 + 1);
        client.release_funds(&1);

        let r2 = client.try_resolve_dispute(&1, &Resolution::ReleaseToSeller, &arbitrator);
        if r2.is_ok() && r2.unwrap().is_ok() {
            panic!(
                "[prop_resolve_requires_disputed_state] resolve succeeded on Released escrow (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 6: Window-based auto-release ────────────────────────────────────

#[test]
fn prop_auto_release_window_boundary() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x4444);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let window_secs = crng.next_u64_range(60, 604_800) as u32;

        let (env, contract_id, _admin, _arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        client.create_escrow(&buyer, &seller, &token_id, &1_000_000, &7, &Some(window_secs));

        // Before window — must fail
        let r = client.try_auto_release(&7);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_auto_release_window_boundary] auto-release succeeded before window \
                 (window={}s, seed=0x{:016X})",
                window_secs, case_seed
            );
        }

        // Advance past window — must succeed
        advance_ledger_time(&env, window_secs as u64 + 1);
        let r2 = client.try_auto_release(&7);
        if r2.is_err() || r2.unwrap().is_err() {
            panic!(
                "[prop_auto_release_window_boundary] auto-release failed after window \
                 (window={}s, seed=0x{:016X})",
                window_secs, case_seed
            );
        }
    }
}

// ── Property 7: Pause blocks escrow creation ─────────────────────────────────

#[test]
fn prop_pause_blocks_escrow_creation() {
    let (env, contract_id, _admin, _arbitrator, buyer, seller, token_id, _, _) = fresh_env();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.set_paused(&true);

    let r = client.try_create_escrow(&buyer, &seller, &token_id, &1_000_000, &99, &None);
    if r.is_ok() && r.unwrap().is_ok() {
        panic!("[prop_pause_blocks_escrow_creation] create_escrow succeeded while paused");
    }

    client.set_paused(&false);
    let r2 = client.try_create_escrow(&buyer, &seller, &token_id, &1_000_000, &99, &None);
    if r2.is_err() || r2.unwrap().is_err() {
        panic!("[prop_pause_blocks_escrow_creation] create_escrow failed after unpause");
    }
}

// ── Property 8: Fee conservation on release ──────────────────────────────────

/// platform_fee + seller_received == escrow_amount for a standard release.
#[test]
fn prop_fee_conservation_on_release() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x5555);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let amount = crng.next_i128_range(10_000, 100_000_000);

        let (env, contract_id, _admin, _arbitrator, buyer, seller, token_id, platform_wallet, _) =
            fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);
        let token_client = token::Client::new(&env, &token_id);

        client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &Some(3600));

        let seller_before = token_client.balance(&seller);
        let platform_before = token_client.balance(&platform_wallet);

        advance_ledger_time(&env, 7 * 86400 + 1);
        client.release_funds(&1);

        let seller_received = token_client.balance(&seller) - seller_before;
        let fee_received = token_client.balance(&platform_wallet) - platform_before;

        if seller_received + fee_received != amount {
            panic!(
                "[prop_fee_conservation_on_release] seller({}) + fee({}) = {} != amount({}) \
                 (seed=0x{:016X})",
                seller_received,
                fee_received,
                seller_received + fee_received,
                amount,
                case_seed
            );
        }
    }
}

// ── Property 9: All documented transitions reachable ─────────────────────────

#[test]
fn prop_all_transitions_reachable() {
    use alloc::collections::BTreeSet;
    let mut observed: BTreeSet<alloc::string::String> = BTreeSet::new();
    let mut rng = Lcg64::new(seed_from_env() ^ 0x6666);

    for _ in 0..(DEFAULT_CASE_COUNT * 4) {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, admin, arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let order_ids: alloc::vec::Vec<u32> = (1u32..=3).collect();
        let ops = generate_escrow_sequence(&mut crng, &order_ids);
        for op in &ops {
            run_op(&env, &client, op, &buyer, &seller, &admin, &arbitrator, &token_id);
        }

        for id in 1u32..=10 {
            if let Ok(Ok(e)) = client.try_get_escrow(&id) {
                observed.insert(alloc::format!("{:?}", e.status));
            }
        }
    }

    for required in &["Active", "Released", "Refunded", "Disputed", "Resolved"] {
        if !observed.iter().any(|s| s.contains(required)) {
            panic!(
                "[prop_all_transitions_reachable] status '{}' never observed in {} cases",
                required,
                DEFAULT_CASE_COUNT * 4
            );
        }
    }
}

// ── Property 10: Model conservation invariants hold ──────────────────────────

#[test]
fn prop_model_conservation_invariants() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x7777);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, admin, arbitrator, buyer, seller, token_id, _, _) = fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let buyer_str = alloc::format!("{:?}", buyer);
        let seller_str = alloc::format!("{:?}", seller);
        let token_str = alloc::format!("{:?}", token_id);
        let admin_str = alloc::format!("{:?}", admin);

        let mut model = ModelState::new();
        let order_ids: alloc::vec::Vec<u32> = (1u32..=4).collect();
        let ops = generate_escrow_sequence(&mut crng, &order_ids);

        for op in &ops {
            match op {
                EscrowOp::CreateEscrow { order_id, amount, release_window, same_party } => {
                    let (b, s) = if *same_party {
                        (buyer_str.clone(), buyer_str.clone())
                    } else {
                        (buyer_str.clone(), seller_str.clone())
                    };
                    let now = env.ledger().timestamp();
                    let _ = model.create_escrow(
                        b, s, token_str.clone(), *amount, *order_id,
                        *release_window as u64, now,
                    );
                    run_op(&env, &client, op, &buyer, &seller, &admin, &arbitrator, &token_id);
                }
                EscrowOp::ReleaseEscrow { order_id, .. } => {
                    let now = env.ledger().timestamp();
                    let _ = model.release_escrow(*order_id, &buyer_str, now);
                    run_op(&env, &client, op, &buyer, &seller, &admin, &arbitrator, &token_id);
                }
                EscrowOp::RefundEscrow { order_id, .. } => {
                    let now = env.ledger().timestamp();
                    let _ = model.refund_escrow(*order_id, &admin_str, &admin_str, now);
                    run_op(&env, &client, op, &buyer, &seller, &admin, &arbitrator, &token_id);
                }
                EscrowOp::AdvanceTime { seconds } => {
                    advance_ledger_time(&env, *seconds);
                }
                _ => {
                    run_op(&env, &client, op, &buyer, &seller, &admin, &arbitrator, &token_id);
                }
            }
        }

        if let Err(msg) = model.check_fund_conservation() {
            panic!(
                "[prop_model_conservation_invariants] {} (seed=0x{:016X})",
                msg, case_seed
            );
        }
        if let Err(msg) = model.check_no_terminal_re_entry() {
            panic!(
                "[prop_model_conservation_invariants] {} (seed=0x{:016X})",
                msg, case_seed
            );
        }
    }
}

// ── Property 11: Dispute blocked on terminal escrow ──────────────────────────

#[test]
fn prop_dispute_blocked_on_terminal() {
    let (env, contract_id, _admin, _arbitrator, buyer, seller, token_id, _, _) = fresh_env();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    client.create_escrow(&buyer, &seller, &token_id, &1_000_000, &55, &Some(3600));
    advance_ledger_time(&env, 7 * 86400 + 1);
    client.release_funds(&55);

    let r = client.try_dispute_escrow(&55, &Symbol::new(&env, "Late"), &buyer);
    if r.is_ok() && r.unwrap().is_ok() {
        panic!("[prop_dispute_blocked_on_terminal] dispute succeeded on Released escrow");
    }
}

// ── Property 12: Partial refund conservation ─────────────────────────────────

/// After a partial refund is accepted, seller + buyer + platform == original amount.
#[test]
fn prop_partial_refund_conservation() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x8888);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let amount: i128 = crng.next_i128_range(100_000, 10_000_000);
        let refund_amount: i128 = crng.next_i128_range(1, amount - 1);

        let (env, contract_id, _admin, _arbitrator, buyer, seller, token_id, platform_wallet, _) =
            fresh_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);
        let token_client = token::Client::new(&env, &token_id);

        client.create_escrow(&buyer, &seller, &token_id, &amount, &1, &Some(3600));
        client.dispute_escrow(&1, &Symbol::new(&env, "Partial"), &buyer);

        let buyer_before = token_client.balance(&buyer);
        let seller_before = token_client.balance(&seller);
        let platform_before = token_client.balance(&platform_wallet);

        // Seller proposes, buyer accepts
        let _ = client.try_propose_partial_refund(&1, &refund_amount, &seller);
        let _ = client.try_accept_partial_refund(&1);

        let buyer_delta = token_client.balance(&buyer) - buyer_before;
        let seller_delta = token_client.balance(&seller) - seller_before;
        let platform_delta = token_client.balance(&platform_wallet) - platform_before;

        if buyer_delta + seller_delta + platform_delta != amount {
            panic!(
                "[prop_partial_refund_conservation] buyer({}) + seller({}) + platform({}) = {} != amount({}) \
                 (seed=0x{:016X})",
                buyer_delta, seller_delta, platform_delta,
                buyer_delta + seller_delta + platform_delta,
                amount, case_seed
            );
        }
    }
}

// ── Op executor ──────────────────────────────────────────────────────────────

fn run_op(
    env: &Env,
    client: &CraftNexusContractClient,
    op: &EscrowOp,
    buyer: &Address,
    seller: &Address,
    admin: &Address,
    arbitrator: &Address,
    token_id: &Address,
) {
    match op {
        EscrowOp::CreateEscrow { order_id, amount, release_window, same_party } => {
            let s = if *same_party { buyer } else { seller };
            let _ = client.try_create_escrow(buyer, s, token_id, amount, order_id, &Some(*release_window));
        }
        EscrowOp::FundEscrow { .. } => {
            // Escrows are funded at creation in the test environment.
        }
        EscrowOp::ReleaseEscrow { order_id, .. } => {
            let _ = client.try_release_funds(order_id);
        }
        EscrowOp::RefundEscrow { order_id, .. } => {
            let eid = *order_id as u64;
            let _ = client.try_refund(&eid);
        }
        EscrowOp::DisputeEscrow { order_id, initiator } => {
            let caller = match initiator {
                0 => buyer,
                1 => seller,
                _ => admin,
            };
            let _ = client.try_dispute_escrow(
                order_id,
                &Symbol::new(env, "Reason"),
                caller,
            );
        }
        EscrowOp::ResolveDispute { order_id, release_to_seller } => {
            let resolution = if *release_to_seller {
                Resolution::ReleaseToSeller
            } else {
                Resolution::RefundToBuyer
            };
            let _ = client.try_resolve_dispute(order_id, &resolution, arbitrator);
        }
        EscrowOp::ResolveExpiredDispute { order_id } => {
            let _ = client.try_resolve_expired_dispute(order_id);
        }
        EscrowOp::AutoRelease { order_id } => {
            let _ = client.try_auto_release(order_id);
        }
        EscrowOp::AdvanceTime { seconds } => {
            advance_ledger_time(env, *seconds);
        }
        EscrowOp::OperateOnMissingEscrow => {
            let _ = client.try_release_funds(&999_999u32);
        }
    }
}
