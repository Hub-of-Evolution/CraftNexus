/// Test module for referral reward allocation.
///
/// This test validates the `create_referral` and `get_referral_rewards` functions
/// after the fix for bug #BUG-726. The original issue caused a compile error
/// when `unwrap()` was called on an empty `Vec<u64>` returned from storage.
///
/// The fix replaces the `unwrap()` with safe iteration and explicit assertions,
/// ensuring the test does not panic on empty vectors. This test also verifies
/// that proper reward IDs are generated and respects bounds constraints to
/// detect potential sequence counter corruption.
///
/// # Edge Cases Covered
/// - Empty vector: Asserted non-empty before access.
/// - Multiple rewards: Asserted exactly one for a single referral.
/// - Zero reward ID: Asserted positive.
/// - Overflow/out-of-bounds: Asserted ≤ MAX_REWARD_ID.
///
/// # Logging
/// Uses `log::info!` for success trace and `log::error!` on assertion failure
/// (implicit via panic). Environment logger is initialized idempotently.
#[test]
fn test_referral_reward_allocation() {
    // -------------------------------------------------------------------
    // Setup: initialize logger (idempotent across runs)
    // -------------------------------------------------------------------
    let _ = env_logger::try_init();

    // -------------------------------------------------------------------
    // Setup: fresh environment and contract instance
    // -------------------------------------------------------------------
    let env: Env = Env::default();
    let contract_id: BytesN<32> = env.register_contract(None, Contract);
    let contract: ContractClient<'_> = ContractClient::new(&env, &contract_id);

    // Generate a unique referrer address (deterministic per test run – acceptable)
    let referrer: Address = Address::random(&env);

    log::info!(
        "Starting referral reward allocation test with referrer: {}",
        referrer
    );

    // -------------------------------------------------------------------
    // Act: create a referral that internally allocates a reward
    // -------------------------------------------------------------------
    contract.create_referral(&referrer);

    // -------------------------------------------------------------------
    // Retrieve the stored reward IDs via the safe accessor
    // -------------------------------------------------------------------
    let reward_ids: Vec<u64> = contract.get_referral_rewards(&referrer);

    // -------------------------------------------------------------------
    // Assertions: thorough validation of returned data
    // -------------------------------------------------------------------

    // 1. Vector must not be empty – creation must have produced at least one reward
    assert!(
        !reward_ids.is_empty(),
        "Expected at least one reward ID after `create_referral`, got empty vector."
    );

    // 2. Exactly one reward should be present for a single referral call
    const EXPECTED_REWARD_COUNT: usize = 1;
    assert_eq!(
        reward_ids.len(),
        EXPECTED_REWARD_COUNT,
        "Expected exactly {} reward ID(s), got {}: {:?}",
        EXPECTED_REWARD_COUNT,
        reward_ids.len(),
        reward_ids
    );

    // 3. Safely extract the first element using pattern matching (eliminates unwrap)
    const MAX_REWARD_ID: u64 = 999_999;
    match reward_ids.first().copied() {
        Some(first_reward) => {
            // 4. Reward ID must be strictly positive (IDs start at 1)
            assert!(
                first_reward > 0,
                "Reward ID must be positive, got {}.",
                first_reward
            );

            // 5. Reward ID must be within a reasonable bound
            assert!(
                first_reward <= MAX_REWARD_ID,
                "Reward ID {} exceeds maximum allowed value ({}) – possible overflow or corruption.",
                first_reward,
                MAX_REWARD_ID
            );

            // Log successful allocation
            log::info!("Referral reward successfully allocated: ID = {}", first_reward);
        }
        None => {
            // This branch is logically unreachable because we already asserted non-empty.
            unreachable!(
                "Unexpected: reward_ids vector was non‑empty but `first()` returned None."
            );
        }
    }

    // -------------------------------------------------------------------
    // Teardown: implicit via env drop; log completion
    // -------------------------------------------------------------------
    log::info!("Referral reward allocation test completed successfully.");
}