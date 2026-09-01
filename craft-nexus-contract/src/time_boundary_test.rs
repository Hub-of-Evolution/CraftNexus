//! Time-boundary tests: clock progression around every deadline.
//!
//! These tests verify the normalised time policy documented in `time_policy`:
//!
//! ```text
//! window_open  = (now >= start)
//! window_closed = (now >= start + duration)
//! ```
//!
//! Every time-gated mechanism is tested at three points:
//!   1. `deadline - 1` → window is still OPEN (action blocked / evidence valid)
//!   2. `deadline`     → window is CLOSED (action allowed  / evidence expired)
//!   3. `deadline + 1` → window is CLOSED (action allowed  / evidence expired)

#![cfg(test)]

use crate::{CraftNexusContract, CraftNexusContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String, Symbol,
};

const ONE_DAY: u32 = 24 * 60 * 60;
const THREE_DAYS: u32 = 3 * ONE_DAY;
const SEVEN_DAYS: u32 = 7 * ONE_DAY;
const THIRTY_DAYS: u32 = 30 * ONE_DAY;

// ── Helpers ───────────────────────────────────────────────────────────────────

struct TestEnv {
    env: Env,
    client: CraftNexusContractClient<'static>,
    admin: Address,
    buyer: Address,
    seller: Address,
    token_addr: Address,
    platform_wallet: Address,
}

fn setup() -> TestEnv {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_id.address();
    let token_asset = token::StellarAssetClient::new(&env, &token_addr);
    token_asset.mint(&buyer, &100_000_000);

    let onboarding_contract = Address::generate(&env);

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
    client.set_min_escrow_amount(&token_addr, &0);
    client.set_min_release_window(&1);
    client.set_evidence_challenge_window(&0);

    TestEnv {
        env,
        client,
        admin,
        buyer,
        seller,
        token_addr,
        platform_wallet,
    }
}

fn create_and_fund_escrow(te: &TestEnv, order_id: u32, release_window: u32) {
    te.client.create_escrow(
        &te.buyer,
        &te.seller,
        &te.token_addr,
        &1_000_000,
        &order_id,
        &Some(release_window),
    );
}

fn create_and_dispute_escrow(te: &TestEnv, order_id: u32, release_window: u32) {
    create_and_fund_escrow(te, order_id, release_window);
    te.client.dispute_escrow(
        &order_id,
        &Symbol::new(&te.env, "item_not_received"),
        &te.buyer,
    );
}

fn get_timestamp(te: &TestEnv) -> u64 {
    te.env.ledger().timestamp()
}

fn set_timestamp(te: &TestEnv, ts: u64) {
    te.env.ledger().with_mut(|li| {
        li.timestamp = ts;
    });
}

// ── Release window boundary ───────────────────────────────────────────────────

#[test]
fn release_window_deadline_exact_boundary() {
    let te = setup();
    let order_id = 1u32;

    create_and_fund_escrow(&te, order_id, ONE_DAY);

    let created_at = get_timestamp(&te);

    // t = created_at + ONE_DAY - 1 → window still ACTIVE → auto_release should fail
    set_timestamp(&te, created_at + ONE_DAY as u64 - 1);
    assert!(te.client.try_auto_release(&order_id).is_err());

    // t = created_at + ONE_DAY → window ELAPSED → auto_release should succeed
    set_timestamp(&te, created_at + ONE_DAY as u64);
    te.client.auto_release(&order_id);
}

#[test]
fn release_window_one_second_before_boundary() {
    let te = setup();
    let order_id = 2u32;

    create_and_fund_escrow(&te, order_id, 3600); // 1-hour window

    let created_at = get_timestamp(&te);

    // 1 second before deadline → must fail
    set_timestamp(&te, created_at + 3600 - 1);
    assert!(te.client.try_auto_release(&order_id).is_err());
}

// ── Stake cooldown boundary ───────────────────────────────────────────────────

#[test]
fn stake_cooldown_exact_boundary() {
    let te = setup();

    // Configure 1-day stake cooldown
    te.client.set_stake_cooldown(&ONE_DAY);

    // Stake tokens at t=1000
    set_timestamp(&te, 1000);
    te.client.stake_tokens(&te.buyer, &te.token_addr, &1_000_000);

    let stake_time = get_timestamp(&te);

    // t = stake_time + ONE_DAY - 1 → cooldown still ACTIVE → unstake should fail
    set_timestamp(&te, stake_time + ONE_DAY as u64 - 1);
    assert!(te.client.try_unstake_tokens(&te.buyer, &te.token_addr).is_err());

    // t = stake_time + ONE_DAY → cooldown ELAPSED → unstake should succeed
    set_timestamp(&te, stake_time + ONE_DAY as u64);
    te.client.unstake_tokens(&te.buyer, &te.token_addr);
}

// ── Dispute max duration boundary ─────────────────────────────────────────────

#[test]
fn dispute_max_duration_exact_boundary() {
    let te = setup();
    let order_id = 10u32;

    // Set dispute duration to 30 days
    te.client.set_max_dispute_duration(&THIRTY_DAYS);

    create_and_dispute_escrow(&te, order_id, ONE_DAY);

    let dispute_time = get_timestamp(&te);

    // t = dispute_time + THIRTY_DAYS - 1 → dispute NOT expired → resolve_expired_dispute should fail
    set_timestamp(&te, dispute_time + THIRTY_DAYS as u64 - 1);
    assert!(te.client.try_resolve_expired_dispute(&order_id).is_err());

    // t = dispute_time + THIRTY_DAYS → dispute EXPIRED → resolve_expired_dispute should succeed
    set_timestamp(&te, dispute_time + THIRTY_DAYS as u64);
    te.client.resolve_expired_dispute(&order_id);
}

// ── Evidence challenge window boundary ────────────────────────────────────────

#[test]
fn evidence_challenge_window_exact_boundary() {
    let te = setup();
    let order_id = 20u32;

    // Set challenge window to 1 day
    te.client.set_evidence_challenge_window(&ONE_DAY);

    create_and_dispute_escrow(&te, order_id, ONE_DAY);

    let dispute_time = get_timestamp(&te);

    // t = dispute_time + ONE_DAY - 1 → challenge window ACTIVE → resolve_dispute should fail
    set_timestamp(&te, dispute_time + ONE_DAY as u64 - 1);
    assert!(te.client.try_resolve_dispute(
        &order_id,
        &crate::Resolution::ReleaseToSeller,
        &te.admin,
    ).is_err());

    // t = dispute_time + ONE_DAY → challenge window CLOSED → resolve_dispute should proceed
    set_timestamp(&te, dispute_time + ONE_DAY as u64);
    te.client.resolve_dispute(
        &order_id,
        &crate::Resolution::ReleaseToSeller,
        &te.admin,
    );
}

// ── Dispute escalation window boundary ────────────────────────────────────────

#[test]
fn escalation_window_exact_boundary() {
    let te = setup();
    let order_id = 30u32;

    // Set escalation window to 3 days
    te.client.set_dispute_escalation_window(&THREE_DAYS);

    create_and_dispute_escrow(&te, order_id, ONE_DAY);

    let dispute_time = get_timestamp(&te);

    // t = dispute_time + THREE_DAYS - 1 → escalation window ACTIVE → escalate should fail
    set_timestamp(&te, dispute_time + THREE_DAYS as u64 - 1);
    assert!(te.client.try_escalate_dispute(&order_id, &te.buyer).is_err());

    // t = dispute_time + THREE_DAYS → escalation window CLOSED → escalate should succeed
    set_timestamp(&te, dispute_time + THREE_DAYS as u64);
    te.client.escalate_dispute(&order_id, &te.buyer);
}

// ── Evidence expiry boundary ──────────────────────────────────────────────────

#[test]
fn evidence_expiry_exact_boundary() {
    let te = setup();
    let order_id = 40u32;

    create_and_dispute_escrow(&te, order_id, ONE_DAY);

    let dispute_time = get_timestamp(&te);

    // Submit evidence
    te.client.submit_evidence(
        &order_id,
        &te.buyer,
        &String::from_str(&te.env, "photo_evidence"),
    );

    // Evidence expires after 7 days (DEFAULT_EVIDENCE_EXPIRY_WINDOW)
    let evidence_expiry = SEVEN_DAYS as u64;

    // t = dispute_time + evidence_expiry - 1 → evidence STILL VALID
    set_timestamp(&te, dispute_time + evidence_expiry - 1);
    let valid = te.client.get_valid_evidence(&order_id);
    assert_eq!(valid.len(), 1);

    // t = dispute_time + evidence_expiry → evidence EXPIRED
    set_timestamp(&te, dispute_time + evidence_expiry);
    let valid = te.client.get_valid_evidence(&order_id);
    assert_eq!(valid.len(), 0);
}

// ── WASM upgrade cooldown boundary ────────────────────────────────────────────

#[test]
fn wasm_upgrade_cooldown_exact_boundary() {
    let te = setup();

    let wasm_hash = soroban_sdk::BytesN::<32>::from_array(&te.env, &[1u8; 32]);
    te.client.propose_upgrade_wasm(&te.admin, &wasm_hash);

    set_timestamp(&te, 2000);
    te.client.cancel_upgrade_wasm();

    let cancel_time = get_timestamp(&te);
    let cooldown = crate::time_policy::CANCEL_REPROPOSE_COOLDOWN;

    // t = cancel_time + cooldown - 1 → cooldown ACTIVE → repropose should fail
    set_timestamp(&te, cancel_time + cooldown - 1);
    assert!(te.client.try_propose_upgrade_wasm(&te.admin, &wasm_hash).is_err());

    // t = cancel_time + cooldown → cooldown ELAPSED → repropose should succeed
    set_timestamp(&te, cancel_time + cooldown);
    te.client.propose_upgrade_wasm(&te.admin, &wasm_hash);
}

// ── Time regression (clock going backward) ────────────────────────────────────

#[test]
fn time_regression_does_not_reopen_closed_window() {
    let te = setup();
    let order_id = 60u32;

    create_and_fund_escrow(&te, order_id, ONE_DAY);

    let created_at = get_timestamp(&te);

    // Advance past deadline → auto_release succeeds
    set_timestamp(&te, created_at + ONE_DAY as u64);
    te.client.auto_release(&order_id);

    // Clock goes backward — but escrow is already Released, no re-entrance possible
    set_timestamp(&te, created_at);
    // Should fail because escrow is Released, not Active
    assert!(te.client.try_auto_release(&order_id).is_err());
}

// ── time_policy unit tests (supplementary) ────────────────────────────────────

#[test]
fn policy_is_window_elapsed_matches_original_semantics() {
    let now = 1000u64;
    let start = 100u64;
    let duration = 800u64;

    // Original: now >= start + duration → 1000 >= 900 → true
    assert!(crate::time_policy::is_window_elapsed(now, start, duration));
    // Original: now < start + duration → 1000 < 900 → false
    assert!(!crate::time_policy::is_window_active(now, start, duration));
}

#[test]
fn policy_inclusive_end_convention() {
    let d = crate::time_policy::deadline(100, 50); // 150
    assert!(crate::time_policy::is_deadline_reached(150, d));
    assert!(!crate::time_policy::is_deadline_reached(149, d));
    assert!(crate::time_policy::is_deadline_pending(149, d));
    assert!(!crate::time_policy::is_deadline_pending(150, d));
}

#[test]
fn policy_rate_limit_bucket_consistency() {
    let window = 3600u64;
    assert_eq!(crate::time_policy::rate_limit_bucket(0, window), 0);
    assert_eq!(crate::time_policy::rate_limit_bucket(3599, window), 0);
    assert_eq!(crate::time_policy::rate_limit_bucket(3600, window), 1);
    assert_eq!(crate::time_policy::rate_limit_bucket(7199, window), 1);
    assert_eq!(crate::time_policy::rate_limit_bucket(7200, window), 2);
}

// ── Multiple deadline progression test ────────────────────────────────────────

#[test]
fn full_lifecycle_deadline_progression() {
    let te = setup();
    let order_id = 100u32;

    create_and_fund_escrow(&te, order_id, ONE_DAY);

    let created_at = get_timestamp(&te);

    // Test progression: t=created_at → t=created_at+86399 → t=created_at+86400
    let offsets: &[u64] = &[0, 1, ONE_DAY as u64 - 1, ONE_DAY as u64, ONE_DAY as u64 + 1];

    for &offset in offsets {
        set_timestamp(&te, created_at + offset);
        if offset < ONE_DAY as u64 {
            assert!(te.client.try_auto_release(&order_id).is_err(),
                "auto_release should fail at offset={}", offset);
        } else {
            te.client.auto_release(&order_id);
            break;
        }
    }
}
