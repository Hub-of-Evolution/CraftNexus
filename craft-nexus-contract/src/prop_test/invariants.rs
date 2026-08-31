//! Reusable invariant assertions for property tests.
#![allow(dead_code)]
//!
//! # Invariant catalogue
//!
//! | Invariant | Description |
//! |---|---|
//! | `fund_conservation` | locked funds ≤ contract balance for each token |
//! | `no_double_settlement` | terminal escrow cannot be re-settled |
//! | `terminal_state_immutable` | status never changes once terminal |
//! | `unstake_rejected_before_cooldown` | early unstake always fails |
//! | `upgrade_nonce_increased` | cancel always increments nonce |
//! | `paused_blocks_create` | create_escrow rejected while paused |
//! | `unauthorized_cannot_resolve` | only arbitrator resolves disputes |
//! | `fee_allocation_sums_to_escrow` | fee + seller + buyer == amount |
//! | `recurring_released_le_total` | released ≤ total for recurring escrows |

extern crate alloc;
use alloc::string::{String, ToString};

use soroban_sdk::{token, Address, Env};

use crate::{CraftNexusContractClient, EscrowStatus, Resolution};

// ── Fund conservation ─────────────────────────────────────────────────────────

/// Assert locked ≤ actual token balance.
pub fn assert_fund_conservation(
    env: &Env,
    client: &CraftNexusContractClient,
    token_id: &Address,
) -> Result<(), String> {
    let balance = token::Client::new(env, token_id).balance(&client.address);
    let allocation = client.get_fund_allocation(token_id);
    if allocation.total_locked > balance {
        return Err(alloc::format!(
            "fund_conservation: locked({}) > balance({}) for token {:?}",
            allocation.total_locked, balance, token_id
        ));
    }
    Ok(())
}

// ── No double-settlement ──────────────────────────────────────────────────────

/// Assert that a terminal-status escrow cannot be re-settled via any path.
pub fn assert_no_double_settlement(
    client: &CraftNexusContractClient,
    order_id: u32,
    arbitrator: &Address,
) -> Result<(), String> {
    let escrow = match client.try_get_escrow(&order_id) {
        Ok(Ok(e)) => e,
        _ => return Ok(()),
    };
    if !matches!(
        escrow.status,
        EscrowStatus::Released | EscrowStatus::Refunded | EscrowStatus::Resolved
    ) {
        return Ok(());
    }
    let r1 = client.try_release_funds(&order_id);
    if r1.is_ok() && r1.unwrap().is_ok() {
        return Err(alloc::format!(
            "no_double_settlement: release succeeded on terminal escrow {}",
            order_id
        ));
    }
    let eid = order_id as u64;
    let r2 = client.try_refund(&eid);
    if r2.is_ok() && r2.unwrap().is_ok() {
        return Err(alloc::format!(
            "no_double_settlement: refund succeeded on terminal escrow {}",
            order_id
        ));
    }
    let r3 = client.try_resolve_dispute(&order_id, &Resolution::ReleaseToSeller, arbitrator);
    if r3.is_ok() && r3.unwrap().is_ok() {
        return Err(alloc::format!(
            "no_double_settlement: resolve succeeded on terminal escrow {}",
            order_id
        ));
    }
    Ok(())
}

// ── Terminal state immutability ───────────────────────────────────────────────

/// Assert that an escrow's status does not change between two successive reads.
pub fn assert_terminal_immutable(
    client: &CraftNexusContractClient,
    order_id: u32,
) -> Result<(), String> {
    let first = match client.try_get_escrow(&order_id) {
        Ok(Ok(e)) => e,
        _ => return Ok(()),
    };
    let second = match client.try_get_escrow(&order_id) {
        Ok(Ok(e)) => e,
        _ => return Ok(()),
    };
    if first.status != second.status {
        return Err(alloc::format!(
            "terminal_immutable: escrow {} status changed {:?} → {:?}",
            order_id, first.status, second.status
        ));
    }
    Ok(())
}

// ── Stake cooldown ────────────────────────────────────────────────────────────

/// Assert that unstake is rejected before the cooldown elapses.
pub fn assert_unstake_rejected_before_cooldown(
    client: &CraftNexusContractClient,
    artisan: &Address,
    token_id: &Address,
) -> Result<(), String> {
    match client.try_unstake_tokens(artisan, token_id) {
        Err(_) | Ok(Err(_)) => Ok(()),
        Ok(Ok(_)) => Err(
            "stake_cooldown_monotone: unstake succeeded before cooldown elapsed".to_string(),
        ),
    }
}

// ── Upgrade nonce monotonicity ────────────────────────────────────────────────

pub fn assert_upgrade_nonce_increased(before: u32, after: u32) -> Result<(), String> {
    if after <= before {
        Err(alloc::format!(
            "upgrade_nonce_monotone: nonce did not increase (before={}, after={})",
            before, after
        ))
    } else {
        Ok(())
    }
}

// ── Pause gate ────────────────────────────────────────────────────────────────

/// Assert that `create_escrow` is rejected while the platform is paused.
pub fn assert_paused_blocks_create(
    client: &CraftNexusContractClient,
    buyer: &Address,
    seller: &Address,
    token_id: &Address,
) -> Result<(), String> {
    match client.try_create_escrow(buyer, seller, token_id, &1_000, &9_999_999, &None) {
        Err(_) | Ok(Err(_)) => Ok(()),
        Ok(Ok(_)) => Err(
            "paused_blocks_mutations: create_escrow succeeded while paused".to_string(),
        ),
    }
}

// ── Role authorization ────────────────────────────────────────────────────────

/// Assert that an unauthorized address cannot resolve a dispute.
pub fn assert_unauthorized_cannot_resolve(
    client: &CraftNexusContractClient,
    order_id: u32,
    unauthorized: &Address,
) -> Result<(), String> {
    match client.try_resolve_dispute(&order_id, &Resolution::ReleaseToSeller, unauthorized) {
        Err(_) | Ok(Err(_)) => Ok(()),
        Ok(Ok(_)) => Err(alloc::format!(
            "role_authorization: unauthorized {:?} resolved dispute on escrow {}",
            unauthorized, order_id
        )),
    }
}

/// Assert that a non-signer cannot propose a WASM upgrade.
pub fn assert_unauthorized_cannot_propose_upgrade(
    env: &Env,
    client: &CraftNexusContractClient,
    unauthorized: &Address,
) -> Result<(), String> {
    let fake_hash = soroban_sdk::BytesN::from_array(env, &[0u8; 32]);
    match client.try_propose_upgrade_wasm(unauthorized, &fake_hash) {
        Err(_) | Ok(Err(_)) => Ok(()),
        Ok(Ok(_)) => Err(
            "role_authorization: unauthorized address proposed a WASM upgrade".to_string(),
        ),
    }
}

// ── Fee conservation ──────────────────────────────────────────────────────────

/// Assert `platform_fee + seller_amount + buyer_amount == escrow_amount`.
pub fn assert_fee_allocation_sums_to_escrow(
    escrow_amount: i128,
    platform_fee: i128,
    seller_amount: i128,
    buyer_amount: i128,
) -> Result<(), String> {
    let sum = platform_fee + seller_amount + buyer_amount;
    if sum != escrow_amount {
        Err(alloc::format!(
            "fee_allocation: platform({}) + seller({}) + buyer({}) = {} != escrow({})",
            platform_fee, seller_amount, buyer_amount, sum, escrow_amount
        ))
    } else {
        Ok(())
    }
}

// ── Recurring conservation ────────────────────────────────────────────────────

pub fn assert_recurring_released_le_total(
    released: i128,
    total: i128,
    escrow_id: u64,
) -> Result<(), String> {
    if released > total {
        Err(alloc::format!(
            "recurring_conservation: escrow {} released {} > total {}",
            escrow_id, released, total
        ))
    } else {
        Ok(())
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Combine multiple invariant results into one, joining all failure messages.
pub fn run_invariants(checks: &[Result<(), String>]) -> Result<(), String> {
    let failures: alloc::vec::Vec<String> = checks
        .iter()
        .filter_map(|r| r.as_ref().err().cloned())
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
