//! # Batch Escrow Creation – Soroban Budget Benchmarks
//!
//! Measures CPU-instruction and memory-byte consumption for `create_batch_escrow`
//! at batch sizes of 5, 10, 15, and 20 escrows.
//!
//! ## Running
//!
//! ```bash
//! cargo test --features testutils bench_ -- --nocapture
//! ```
//!
//! Each test prints a table that looks like:
//!
//! ```text
//! [BENCH] create_batch_escrow | batch_size=5  | cpu=123456 insns | mem=78910 bytes
//! ```
//!
//! ## Notes on interpretation
//!
//! * These numbers are measured against the Rust host, **not** the WASM runtime.
//!   Actual on-chain CPU/memory will be higher because the WASM VM adds its own
//!   instruction overhead on top of host-side costs.  The ratios between batch
//!   sizes are still meaningful and expose non-linear growth.
//! * The ledger budget is reset with `reset_default()` before every measured
//!   call so that setup work (token minting, contract initialisation) is
//!   excluded from the reported numbers.
//! * A separate `reset_tracker()` call zeroes the counters between the
//!   "validate-only" and "creation" phases when we need to isolate just the
//!   creation path.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Shared test fixture.  Returns everything the benchmark functions need.
fn setup_bench() -> (
    Env,
    EscrowContractClient<'static>,
    /// buyer
    Address,
    /// seller
    Address,
    /// whitelisted token address
    Address,
    /// token mint client (to fund buyers)
    token::StellarAssetClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    // Disable the budget ceiling during setup so that fixture construction
    // never trips a limit.
    env.budget().reset_unlimited();

    env.ledger().with_mut(|li| {
        li.timestamp = 1_711_368_000; // 2024-03-25 – arbitrary non-zero baseline
    });

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let onboarding = Address::generate(&env);

    // Deploy a Stellar asset contract and use it as the escrow token.
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_contract.address();
    let token_client = token::StellarAssetClient::new(&env, &token_addr);

    // Mint enough tokens to cover 20 escrows of 1 000 each plus platform fees.
    token_client.mint(&buyer, &1_000_000_000_i128);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500, // 5 % platform fee in bps
        &Some(onboarding),
    );

    // Accept any positive amount and any release window ≥ 1 second.
    client.set_min_escrow_amount(&token_addr, &0);
    client.set_min_release_window(&1);

    (env, client, buyer, seller, token_addr, token_client)
}

/// Build a `Vec<EscrowCreateParams>` of length `n`.
///
/// Every entry in the batch shares the same buyer/seller/token but has a
/// unique `order_id` so that each one maps to a distinct storage slot.
fn build_params(
    env: &Env,
    buyer: &Address,
    seller: &Address,
    token: &Address,
    n: u32,
    order_id_offset: u32,
) -> soroban_sdk::Vec<EscrowCreateParams> {
    let mut params = soroban_sdk::Vec::new(env);
    for i in 0..n {
        params.push_back(EscrowCreateParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount: 1_000_i128,
            order_id: order_id_offset + i + 1,
            release_window: Some(3_600), // 1 hour
            ipfs_hash: None,
            metadata_hash: None,
        });
    }
    params
}

/// Run the benchmark for a single batch size and print the result.
///
/// Internally:
/// 1. Fresh fixture is created (budget still unlimited from setup).
/// 2. Budget is reset to **default** limits immediately before the measured
///    invocation so that only the batch-creation work is counted.
/// 3. CPU and memory costs are read and printed.
/// 4. The function asserts that exactly `batch_size` escrow IDs were returned
///    so the benchmark also acts as a correctness regression.
fn bench_batch_size(batch_size: u32) {
    let (env, client, buyer, seller, token, _token_client) = setup_bench();

    let params = build_params(&env, &buyer, &seller, &token, batch_size, 0);

    // ── Measurement window begins here ──────────────────────────────────────
    env.budget().reset_default();

    let results = client
        .create_batch_escrow(&1_u64, &params)
        .expect("create_batch_escrow must succeed");

    let cpu = env.budget().cpu_instruction_cost();
    let mem = env.budget().memory_bytes_cost();
    // ── Measurement window ends here ─────────────────────────────────────────

    println!(
        "[BENCH] create_batch_escrow | batch_size={:<3} | cpu={:<10} insns | mem={:<10} bytes",
        batch_size, cpu, mem
    );

    // Correctness assertion – doubles as a smoke test.
    assert_eq!(
        results.len(),
        batch_size,
        "expected {} escrow IDs, got {}",
        batch_size,
        results.len()
    );
}

// ─── individual benchmark tests ──────────────────────────────────────────────

/// Benchmark: batch of 5 escrows.
#[test]
fn bench_create_batch_escrow_5() {
    bench_batch_size(5);
}

/// Benchmark: batch of 10 escrows.
#[test]
fn bench_create_batch_escrow_10() {
    bench_batch_size(10);
}

/// Benchmark: batch of 15 escrows.
#[test]
fn bench_create_batch_escrow_15() {
    bench_batch_size(15);
}

/// Benchmark: batch of 20 escrows (the current `MAX_BATCH_SIZE` ceiling).
#[test]
fn bench_create_batch_escrow_20() {
    bench_batch_size(20);
}

// ─── comparative summary ──────────────────────────────────────────────────────

/// Runs all four sizes in sequence and prints a formatted comparison table.
///
/// ```bash
/// cargo test --features testutils bench_create_batch_escrow_summary -- --nocapture
/// ```
#[test]
fn bench_create_batch_escrow_summary() {
    const SIZES: [u32; 4] = [5, 10, 15, 20];
    let mut rows: std::vec::Vec<(u32, u64, u64)> = std::vec::Vec::new();

    for &size in &SIZES {
        let (env, client, buyer, seller, token, _) = setup_bench();
        let params = build_params(&env, &buyer, &seller, &token, size, size * 100);

        env.budget().reset_default();
        let results = client
            .create_batch_escrow(&(size as u64), &params)
            .expect("batch creation must succeed");
        assert_eq!(results.len(), size);

        rows.push((size, env.budget().cpu_instruction_cost(), env.budget().memory_bytes_cost()));
    }

    // ── pretty-print table ────────────────────────────────────────────────────
    println!();
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║       CraftNexus – Batch Escrow Creation Budget Report       ║");
    println!("╠═════════════╦══════════════════╦══════════════════╦══════════╣");
    println!("║ batch_size  ║  cpu insns       ║  memory bytes    ║ cpu/item ║");
    println!("╠═════════════╬══════════════════╬══════════════════╬══════════╣");

    let (base_size, base_cpu, _) = rows[0];
    for &(size, cpu, mem) in &rows {
        let per_item = cpu / u64::from(size);
        println!(
            "║ {:>11} ║ {:>16} ║ {:>16} ║ {:>8} ║",
            size, cpu, mem, per_item
        );
    }

    println!("╠═════════════╩══════════════════╩══════════════════╩══════════╣");

    // Scaling factor vs the 5-escrow baseline
    println!("║  Scaling factors relative to batch_size={}:                   ║", base_size);
    for &(size, cpu, _mem) in &rows {
        let factor = cpu as f64 / base_cpu as f64;
        println!(
            "║    batch_size={:<3}  →  {:.3}×  ({} insns){}║",
            size,
            factor,
            cpu,
            " ".repeat(4_usize.saturating_sub(size.to_string().len()))
        );
    }

    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("NOTE: CPU/mem numbers are measured against the Rust host.");
    println!("      On-chain WASM costs will be higher but ratios remain indicative.");
}

// ─── marginal cost analysis ──────────────────────────────────────────────────

/// Measures the *marginal* cost of adding one more escrow to a batch by
/// comparing consecutive batch sizes.
///
/// ```bash
/// cargo test --features testutils bench_marginal_cost -- --nocapture
/// ```
#[test]
fn bench_marginal_cost_per_escrow() {
    const SIZES: [u32; 5] = [1, 5, 10, 15, 20];
    let mut cpu_costs: std::vec::Vec<(u32, u64)> = std::vec::Vec::new();

    for &size in &SIZES {
        let (env, client, buyer, seller, token, _) = setup_bench();
        let params = build_params(&env, &buyer, &seller, &token, size, size * 200);

        env.budget().reset_default();
        let _ = client
            .create_batch_escrow(&(size as u64), &params)
            .expect("batch creation must succeed");

        cpu_costs.push((size, env.budget().cpu_instruction_cost()));
    }

    println!();
    println!("── Marginal CPU cost per additional escrow ──────────────────────");
    for window in cpu_costs.windows(2) {
        let (n1, c1) = window[0];
        let (n2, c2) = window[1];
        let delta_n = n2 - n1;
        let delta_cpu = c2.saturating_sub(c1);
        let per_escrow = delta_cpu / u64::from(delta_n);
        println!(
            "  batch {}→{}: +{} insns total, ~{} insns/escrow",
            n1, n2, delta_cpu, per_escrow
        );
    }
    println!("─────────────────────────────────────────────────────────────────");
}

// ─── budget-limit smoke test ──────────────────────────────────────────────────

/// Verifies that a batch of 20 escrows stays within the default Soroban
/// network budget.  This test acts as a CI gate: it will start failing if a
/// future change causes the contract to exceed the protocol-enforced limits.
///
/// If this test fails, the contract needs optimisation before it can be
/// deployed to mainnet with a 20-escrow batch.
#[test]
fn bench_batch_20_fits_within_default_budget() {
    let (env, client, buyer, seller, token, _) = setup_bench();
    let params = build_params(&env, &buyer, &seller, &token, 20, 5_000);

    // reset_default() installs the real protocol limits; the call below will
    // panic (contract execution aborted) if either limit is breached.
    env.budget().reset_default();

    let result = client.try_create_batch_escrow(&1_u64, &params);

    match result {
        Ok(Ok(ids)) => {
            println!(
                "[BENCH] batch_size=20 fits in default budget: cpu={} mem={}",
                env.budget().cpu_instruction_cost(),
                env.budget().memory_bytes_cost()
            );
            assert_eq!(ids.len(), 20, "expected 20 escrow IDs");
        }
        Ok(Err(e)) => {
            panic!("contract returned error: {:?}", e);
        }
        Err(_) => {
            let cpu = env.budget().cpu_instruction_cost();
            let mem = env.budget().memory_bytes_cost();
            panic!(
                "batch_size=20 EXCEEDED default budget — cpu={} insns, mem={} bytes. \
                 Consider splitting the batch or optimising storage writes.",
                cpu, mem
            );
        }
    }
}
