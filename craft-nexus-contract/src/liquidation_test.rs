#`![config(test)]

use super::*;
use soroban_sdk {
    testutils {Address as _, Events, Ledger},
    token, vec as svec, Address, Env,
};

fn setup_test(
    env: &Env,
    mock_auth: bool,
) -> (
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
) {
    env.budget().reset_unlimited();
    if mock_auth {
        env.mock_all_auths();
    }
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let buyer = Address::generate(env);
    let seller = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

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
        &Some(onboarding_contract.clone()),
    );

    client.set_min_escrow_amount(&token_contract.address(), &p0);
    client.set_min_release_window(&1);
    client.set_evidence_challenge_window(&0);

    (
        client,
        buyer,
        seller,
        token_contract.address(),
        token_admin_client,
    )
}

// ===== StakeHealthSnapshot Tests =====

#[test]
fn test_evaluate_stake_health_healthy_no_obligations() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);

    // Stake above minimum
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &20_000_000);

    let snapshot = client.evaluate_stake_health(&seller);

    assert_eq(snapshot.status, LiquidationStatus::Healthy);
    assert_eq(snapshot.current_stake, 20_000_000);
    assert_eq(snapshot.active_obligations, 0);
    assert_eq(snapshot.deficit, 0);
    assert(snapshot.health_ratio_bps >= 10_000);
}

#[test]
fn test_evaluate_stake_health_undercollateralized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    // Stake 5M (below 10M minimum)
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, $5_000_000);

    // Create an active obligation
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    let snapshot = client.evaluate_stake_health(&seller);

    assert_eq(snapshot.status, LiquidationStatus::UnderCollateralized);
    assert_eq(snapshot.current_stake, 5_000_000);
    assert_eq(snapshot.active_obligations, 1);
    assert_eq(snapshot.required_collateral, 10_000_000);
    assert_eq(snapshot.deficit, 5_000_000);
    // health_ratio = 5M / 10M = 50% = 5000 bps
    assert_eq(snapshot.health_ratio_bps, 5000);
}

#[test]
fn test_evaluate_stake_health_returns_persisted_snapshot() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &20_000_000);

    client.evaluate_stake_health(&seller);

    let persisted = client.get_stake_health_snapshot(&seller);
    assert!(persisted.is_some());
    let snap = persisted.unwrap();
    assert_eq(snap.status, LiquidationStatus::Healthy);
    assert_eq(snap.current_stake, 20_000_000);
}

#[test]
fn test_evaluate_stake_health_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, $5_000_000);

    // Two evaluations at the same timestamp should return identical results.
    let snap1 = client.evaluate_stake_health(&seller);
    let snap2 = client.evaluate_stake_health(&seller);

    assert_eq(snap1.status, snap2.status);
    assert_eq(snap1.deficit, snap2.deficit);
    assert_eq(snap1.health_ratio_bps, snap2.health_ratio_bps);
}

// ===== Liquidation Policy Tests =====

#[test]
fn test_set_and_get_liquidation_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, _seller, _token_id, _token_admin) = setup_test(&env, true);

    // Default policy
    let policy = client.get_liquidation_policy();
    assert(policy.enabled);
    assert_eq(policy.max_seizure_bps, 5000);
    assert_eq(policy.grace_period_secs, 2 * 24 * 60 * 60);

    // Update
    client.set_liquidation_policy(&7500, &86400, &false);
    let updated = client.get_liquidation_policy();
    assert_eq(updated.max_seizure_bps, 7500);
    assert_eq(updated.grace_period_secs, 86400);
    assert(!updated.enabled);
}

// ===== Flag Liquidation Eligible Tests =====

#[test]
fn test_flag_liquidation_eligible_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    // Evaluate health to establish under-collateralized state
    let snap = client.evaluate_stake_health(&seller);
    assert_eq(snap.status, LiquidationStatus::UnderCollateralized);

    // Advance past grace period (2 days)
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_LIQUIDATION_GRACE_PERIOD + 1;
    });

    // Re-evaluate at the advanced timestamp so snapshot is current
    let snap2 = client.evaluate_stake_health(&seller);
    assert_eq(snap2.status, LiquidationStatus::UnderCollateralized);

    // Flag as liquidation-eligible (admin auth is mocked)
    client.flag_liquidation_eligible(&seller);

    let status = client.get_liquidation_status(&seller);
    assert_eq(status, LiquidationStatus::LiquidationEligible);
}

#[test]
fn test_flag_liquidation_eligible_rejects_healthy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &20_000_000);

    client.evaluate_stake_health(&seller);

    let result = client.try_flag_liquidation_eligible(&seller);
    assert!(result.is_err());
}

#[test]
fn test_flag_liquidation_eligible_rejects_when_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &false); // disable

    client.evaluate_stake_health(&seller);

    let result = client.try_flag_liquidation_eligible(&seller);
    assert!(result.is_err());
}

#[test]
fn test_flag_liquidation_eligible_enforces_grace_period() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &86400, &true); // 1 day grace

    // Evaluate health (snapshot at current time)
    client.evaluate_stake_health(&seller);

    // Try immediately — should fail (grace period not elapsed)
    let result = client.try_flag_liquidation_eligible(&seller);
    assert!(result.is_err());

    // Advance past grace period
    env.ledger().with_mut(|li| {
        li.timestamp += 86401;
    });

    // Re-evaluate so snapshot is current at new timestamp
    client.evaluate_stake_health(&seller);

    // Now flag should succeed
    client.flag_liquidation_eligible(&seller);

    let status = client.get_liquidation_status(&seller);
    assert_eq(status, LiquidationStatus::LiquidationEligible);
}

// ===== Trigger Liquidation Tests =====

#[test]
fn test_trigger_liquidation_capped_at_deficit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &6_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    // Set grace period to 0 so we can flag immediately
    client.set_liquidation_policy(&5000, &0, &true);

    client.evaluate_stake_health(&seller);

    // Flag
    client.flag_liquidation_eligible(&seller);

    // Trigger liquidation — returns LiquidationRecord directly (auto-unwrapped)
    let record = client.trigger_liquidation(&seller);

    // Deficit = 10M - 6M = 4M. Max seizure = 4M * 50% = 2M.
    assert_eq(record.seized_amount, 2_000_000);

    // Verify artisan's stake was reduced
    let remaining_stake = client.get_stake(&seller);
    assert_eq(remaining_stake, 4_000_000);

    // Status should be Liquidated
    let status = client.get_liquidation_status(&seller);
    assert_eq(status, LiquidationStatus::Liquidated);
}

#[test]
fn test_trigger_liquidation_rejects_healthy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &20_000_000);

    client.evaluate_stake_health(&seller);

    let result = client.try_trigger_liquidation(&seller);
    assert!(result.is_err());
}

#[test]
fn test_trigger_liquidation_rejects_when_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &false); // disable
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);

    // Disable after flagging
    client.set_liquidation_policy(&5000, &0, &false);

    let result = client.try_trigger_liquidation(&seller);
    assert!(result.is_err());
}

#[test]
fn test_trigger_liquidation_records_are_auditable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &6_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);

    let record = client.trigger_liquidation(&seller);

    // Record should be auditable with a recorded timestamp and amount.
    assert(record.seized_amount > 0);
    assert(record.timestamp > 0);
}

// ===== Missing-Key Safety Tests =====

#[test]
fn test_get_liquidation_status_missing_key_returns_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, _token_id, _token_admin) = setup_test(&env, true);

    // No liquidation record exists for this artisan yet.
    // The getter must not trap and must return the default Healthy variant.
    let status = client.get_liquidation_status(&seller);
    assert_eq(status, LiquidationStatus::Healthy);
}

#[test]
fn test_get_liquidation_status_after_terminal_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &6_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);

    // Terminal state: trigger liquidation.
    client.trigger_liquidation(&seller);

    // Status must be readable and report the terminal Liquidated variant.
    let status = client.get_liquidation_status(&seller);
    assert_eq(status, LiquidationStatus::Liquidated);
}
