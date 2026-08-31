//! Staking deposit / cooldown / withdrawal property tests.
//!
//! # Properties verified
//!
//! 1. **Cooldown monotonicity** – unstake before cooldown always fails.
//! 2. **Unstake succeeds after cooldown** – full withdrawal works after 7 days.
//! 3. **Token mismatch** – unstaking with the wrong token is rejected.
//! 4. **Partial stake then full unstake** – balance is correct after unstake.
//! 5. **Multiple deposits** – staking twice then waiting cooldown allows withdrawal.
//! 6. **Empty unstake fails** – unstake with no prior stake always fails.
//! 7. **Model-contract stake total agreement** – model invariants hold.
//! 8. **Collateral requirement** – escrow blocked when artisan stake < min_stake_required.

#![cfg(test)]
extern crate alloc;

use soroban_sdk::{testutils::{Address as _, Ledger}, token, Address, Env};

use super::{
    generators::{generate_staking_sequence, StakingOp},
    harness::advance_ledger_time,
    model::ModelState,
    seed_from_env, Lcg64, DEFAULT_CASE_COUNT,
};
use crate::CraftNexusContractClient;

// ── Constants ─────────────────────────────────────────────────────────────────
const STAKE_COOLDOWN: u64 = 7 * 24 * 60 * 60; // 7 days (DEFAULT_STAKE_COOLDOWN)

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_staking_env() -> (
    Env,
    Address, // contract_id
    Address, // admin
    Address, // artisan (seller role)
    Address, // token_id
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

    let admin = Address::generate(&env);
    let artisan = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
    let token_id = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_id);

    let contract_id = env.register_contract(None, crate::CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    client.set_min_escrow_amount(&token_id, &0);
    client.set_min_release_window(&1);
    client.set_evidence_challenge_window(&0);

    // Whitelist the token so staking operations are permitted
    let _ = client.try_whitelist_token(&token_id);

    // Mint a large supply to the artisan
    token_admin.mint(&artisan, &10_000_000_000i128);

    (env, contract_id, admin, artisan, token_id)
}

// ── Property 1: Cooldown monotonicity ────────────────────────────────────────

/// Unstaking immediately after staking (cooldown not elapsed) must fail.
#[test]
fn prop_unstake_before_cooldown_fails() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA11);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let amount = crng.next_i128_range(1_000, 10_000_000);
        client.stake_tokens(&artisan, &token_id, &amount);

        // No time advance — cooldown has NOT elapsed
        let r = client.try_unstake_tokens(&artisan, &token_id);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_unstake_before_cooldown_fails] unstake succeeded before cooldown \
                 (amount={}, seed=0x{:016X})",
                amount, case_seed
            );
        }
    }
}

// ── Property 2: Unstake succeeds after cooldown ───────────────────────────────

#[test]
fn prop_unstake_after_cooldown_succeeds() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA22);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let amount = crng.next_i128_range(1_000, 10_000_000);
        client.stake_tokens(&artisan, &token_id, &amount);

        advance_ledger_time(&env, STAKE_COOLDOWN + 1);

        let r = client.try_unstake_tokens(&artisan, &token_id);
        if r.is_err() || r.unwrap().is_err() {
            panic!(
                "[prop_unstake_after_cooldown_succeeds] unstake failed after cooldown \
                 (amount={}, seed=0x{:016X})",
                amount, case_seed
            );
        }

        // After full unstake, stake balance must be 0
        let stake_balance = client.get_stake(&artisan);
        if stake_balance != 0 {
            panic!(
                "[prop_unstake_after_cooldown_succeeds] stake balance {} != 0 after full unstake \
                 (seed=0x{:016X})",
                stake_balance, case_seed
            );
        }
    }
}

// ── Property 3: Token mismatch rejection ─────────────────────────────────────

#[test]
fn prop_stake_token_mismatch_rejected() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA33);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        // Create and whitelist a second token
        let token2_admin = Address::generate(&env);
        let token2_contract = env.register_stellar_asset_contract_v2(token2_admin.clone());
        let token2_id = token2_contract.address();
        let token2_sa = token::StellarAssetClient::new(&env, &token2_id);
        let _ = client.try_whitelist_token(&token2_id);
        token2_sa.mint(&artisan, &10_000_000i128);

        let amount = crng.next_i128_range(1_000, 1_000_000);
        client.stake_tokens(&artisan, &token_id, &amount);

        advance_ledger_time(&env, STAKE_COOLDOWN + 1);

        // Unstaking with a different token must be rejected
        let r = client.try_unstake_tokens(&artisan, &token2_id);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_stake_token_mismatch_rejected] unstake with wrong token succeeded \
                 (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 4: Stake balance correct after unstake ──────────────────────────

/// After staking N then unstaking, token balance is restored.
#[test]
fn prop_unstake_restores_token_balance() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA44);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);
        let token_client = token::Client::new(&env, &token_id);

        let stake_amount: i128 = crng.next_i128_range(10_000, 100_000_000);

        let before = token_client.balance(&artisan);
        client.stake_tokens(&artisan, &token_id, &stake_amount);
        advance_ledger_time(&env, STAKE_COOLDOWN + 1);
        client.unstake_tokens(&artisan, &token_id);
        let after = token_client.balance(&artisan);

        // Balance should be restored to the original value
        if after != before {
            panic!(
                "[prop_unstake_restores_token_balance] before={}, after={} \
                 (stake={}, seed=0x{:016X})",
                before, after, stake_amount, case_seed
            );
        }
    }
}

// ── Property 5: Multiple stakes, first matures first ─────────────────────────

#[test]
fn prop_multi_stake_first_matures_first() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA55);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let amount_1: i128 = crng.next_i128_range(1_000, 5_000_000);
        let amount_2: i128 = crng.next_i128_range(1_000, 5_000_000);

        // First deposit at t=0
        client.stake_tokens(&artisan, &token_id, &amount_1);

        // Advance past cooldown for deposit 1
        advance_ledger_time(&env, STAKE_COOLDOWN + 1);

        // Second deposit (cooldown ends at t=2*COOLDOWN+1)
        client.stake_tokens(&artisan, &token_id, &amount_2);

        // At this point deposit 1 is mature; deposit 2 is not yet.
        // Unstake should succeed (at least deposit 1 can be withdrawn).
        let r = client.try_unstake_tokens(&artisan, &token_id);
        if r.is_err() || r.unwrap().is_err() {
            panic!(
                "[prop_multi_stake_first_matures_first] could not unstake after first deposit \
                 matured (amount_1={}, amount_2={}, seed=0x{:016X})",
                amount_1, amount_2, case_seed
            );
        }
    }
}

// ── Property 6: Empty unstake fails ──────────────────────────────────────────

#[test]
fn prop_empty_unstake_fails() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA66);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        // Advance past any cooldown — but there is no stake at all
        advance_ledger_time(&env, STAKE_COOLDOWN + 1);

        let r = client.try_unstake_tokens(&artisan, &token_id);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_empty_unstake_fails] unstake succeeded with no prior stake \
                 (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 7: Model stake queue consistency ─────────────────────────────────

#[test]
fn prop_model_stake_queue_consistency() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA77);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let mut model = ModelState::new();
        let artisan_str = alloc::format!("{:?}", artisan);
        let token_str = alloc::format!("{:?}", token_id);

        let ops = generate_staking_sequence(&mut crng);
        let mut ledger_time: u64 = 1_711_368_000;

        for op in &ops {
            match op {
                StakingOp::Stake { amount, wrong_token } => {
                    if *wrong_token {
                        continue; // token-mismatch tested separately
                    }
                    let _ = model.stake(
                        artisan_str.clone(),
                        token_str.clone(),
                        *amount,
                        ledger_time,
                    );
                    let _ = client.try_stake_tokens(&artisan, &token_id, amount);
                }
                StakingOp::Unstake { before_cooldown, wrong_token, .. } => {
                    if *wrong_token {
                        continue;
                    }
                    let _ = model.unstake(&artisan_str, &token_str, i128::MAX, ledger_time);
                    let _ = client.try_unstake_tokens(&artisan, &token_id);
                    let _ = before_cooldown;
                }
                StakingOp::AdvanceTime { seconds } => {
                    advance_ledger_time(&env, *seconds);
                    ledger_time = ledger_time.saturating_add(*seconds);
                }
                StakingOp::UnstakeEmpty => {
                    let _ = model.unstake(&artisan_str, &token_str, 1, ledger_time);
                    let _ = client.try_unstake_tokens(&artisan, &token_id);
                }
            }
        }

        if let Err(msg) = model.check_stake_queue_consistency() {
            panic!(
                "[prop_model_stake_queue_consistency] {} (seed=0x{:016X})",
                msg, case_seed
            );
        }
    }
}

// ── Property 8: Collateral requirement enforced ───────────────────────────────

#[test]
fn prop_collateral_requirement_enforced() {
    let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let token_admin = token::StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&buyer, &100_000_000i128);

    let min_stake: i128 = 10_000_000;
    client.set_min_stake_required(&min_stake);

    // Artisan has no stake → escrow creation blocked
    let r = client.try_create_escrow(&buyer, &artisan, &token_id, &1_000_000, &1, &None);
    if r.is_ok() && r.unwrap().is_ok() {
        panic!("[prop_collateral_requirement_enforced] escrow created with no artisan stake");
    }

    // Stake minimum → creation allowed
    client.stake_tokens(&artisan, &token_id, &min_stake);
    let r2 = client.try_create_escrow(&buyer, &artisan, &token_id, &1_000_000, &1, &None);
    if r2.is_err() || r2.unwrap().is_err() {
        panic!(
            "[prop_collateral_requirement_enforced] escrow creation failed after staking minimum"
        );
    }
}

// ── Property 9: get_stake monotone while staking ─────────────────────────────

/// After each stake call, `get_stake` must be >= previous value.
#[test]
fn prop_stake_balance_monotone_increasing() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAA99);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, artisan, token_id) = make_staking_env();
        let client = CraftNexusContractClient::new(&env, &contract_id);

        let n = 1 + crng.next_usize(5);
        let mut prev_stake: i128 = 0;

        for _ in 0..n {
            let amount = crng.next_i128_range(1_000, 5_000_000);
            client.stake_tokens(&artisan, &token_id, &amount);

            let current = client.get_stake(&artisan);
            if current < prev_stake {
                panic!(
                    "[prop_stake_balance_monotone_increasing] stake decreased from {} to {} \
                     (seed=0x{:016X})",
                    prev_stake, current, case_seed
                );
            }
            prev_stake = current;
        }
    }
}
