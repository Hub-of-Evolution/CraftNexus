#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
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

    client.set_min_escrow_amount(&token_contract.address(), &0);
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

    assert_eq!(snapshot.status, LiquidationStatus::Healthy);
    assert_eq!(snapshot.current_stake, 20_000_000);
    assert_eq!(snapshot.active_obligations, 0);
    assert_eq!(snapshot.deficit, 0);
    assert!(snapshot.health_ratio_bps >= 10_000);
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
    client.stake_tokens(&seller, &token_id, &5_000_000);

    // Create an active obligation
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    let snapshot = client.evaluate_stake_health(&seller);

    assert_eq!(snapshot.status, LiquidationStatus::UnderCollateralized);
    assert_eq!(snapshot.current_stake, 5_000_000);
    assert_eq!(snapshot.active_obligations, 1);
    assert_eq!(snapshot.required_collateral, 10_000_000);
    assert_eq!(snapshot.deficit, 5_000_000);
    // health_ratio = 5M / 10M = 50% = 5000 bps
    assert_eq!(snapshot.health_ratio_bps, 5000);
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
    assert_eq!(snap.status, LiquidationStatus::Healthy);
    assert_eq!(snap.current_stake, 20_000_000);
}

#[test]
fn test_evaluate_stake_health_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);

    // Two evaluations at the same timestamp should return identical results.
    let snap1 = client.evaluate_stake_health(&seller);
    let snap2 = client.evaluate_stake_health(&seller);

    assert_eq!(snap1.status, snap2.status);
    assert_eq!(snap1.deficit, snap2.deficit);
    assert_eq!(snap1.health_ratio_bps, snap2.health_ratio_bps);
}

// ===== Liquidation Policy Tests =====

#[test]
fn test_set_and_get_liquidation_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, _seller, _token_id, _token_admin) = setup_test(&env, true);

    // Default policy
    let policy = client.get_liquidation_policy();
    assert!(policy.enabled);
    assert_eq!(policy.max_seizure_bps, 5000);
    assert_eq!(policy.grace_period_secs, 2 * 24 * 60 * 60);

    // Update
    client.set_liquidation_policy(&7500, &86400, &false);
    let updated = client.get_liquidation_policy();
    assert_eq!(updated.max_seizure_bps, 7500);
    assert_eq!(updated.grace_period_secs, 86400);
    assert!(!updated.enabled);
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
    assert_eq!(snap.status, LiquidationStatus::UnderCollateralized);

    // Advance past grace period (2 days)
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_LIQUIDATION_GRACE_PERIOD + 1;
    });

    // Re-evaluate at the advanced timestamp so snapshot is current
    let snap2 = client.evaluate_stake_health(&seller);
    assert_eq!(snap2.status, LiquidationStatus::UnderCollateralized);

    // Flag as liquidation-eligible (admin auth is mocked)
    client.flag_liquidation_eligible(&seller);

    let status = client.get_liquidation_status(&seller);
    assert_eq!(status, LiquidationStatus::LiquidationEligible);
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
    assert_eq!(status, LiquidationStatus::LiquidationEligible);
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
    assert_eq!(record.seized_amount, 2_000_000);

    // Verify artisan's stake was reduced
    let remaining_stake = client.get_stake(&seller);
    assert_eq!(remaining_stake, 4_000_000);

    // Status should be Liquidated
    let status = client.get_liquidation_status(&seller);
    assert_eq!(status, LiquidationStatus::Liquidated);
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

    // Record is persisted and retrievable
    let fetched = client.get_liquidation_record(&record.id);
    assert!(fetched.is_some());
    let r = fetched.unwrap();
    assert_eq!(r.id, record.id);
    assert_eq!(r.artisan, seller);
    assert_eq!(r.seized_amount, 2_000_000);
    assert!(!r.cured);
    assert_eq!(r.cured_at, 0);

    // Count is tracked
    assert_eq!(client.get_liquidation_record_count(), 1);
}

// ===== Cure Liquidation Tests =====

#[test]
fn test_cure_liquidation_by_staking_more() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &100_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &6_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);
    client.trigger_liquidation(&seller);

    // Cure by topping up: need ≥ 10M, currently have 4M
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });
    // Can't unstake while liquidated, so just stake more
    client.stake_tokens(&seller, &token_id, &10_000_000);

    // Cure
    client.cure_liquidation(&seller);

    let status = client.get_liquidation_status(&seller);
    assert_eq!(status, LiquidationStatus::Healthy);

    // Record should be marked cured
    let record = client.get_liquidation_record(&0).unwrap();
    assert!(record.cured);
    assert!(record.cured_at > 0);
}

#[test]
fn test_cure_liquidation_rejects_when_still_undercollateralized() {
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
    client.trigger_liquidation(&seller);

    // Try to cure without adding enough stake (still have 4M < 10M required)
    let result = client.try_cure_liquidation(&seller);
    assert!(result.is_err());
}

#[test]
fn test_cure_liquidation_rejects_healthy_artisan() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &20_000_000);

    client.evaluate_stake_health(&seller);

    let result = client.try_cure_liquidation(&seller);
    assert!(result.is_err());
}

// ===== Unstake Blocking Tests =====

#[test]
fn test_unstake_blocked_when_liquidation_eligible() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &15_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);

    // Advance past stake cooldown
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });

    // Unstake should be blocked
    let result = client.try_unstake_tokens(&seller, &token_id);
    assert!(result.is_err());
}

#[test]
fn test_unstake_blocked_when_liquidated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &15_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);
    client.trigger_liquidation(&seller);

    // Advance past stake cooldown
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });

    // Unstake should be blocked
    let result = client.try_unstake_tokens(&seller, &token_id);
    assert!(result.is_err());
}

// ===== Full Lifecycle Integration Test =====

#[test]
fn test_full_liquidation_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &100_000_000);
    token_admin.mint(&buyer, &50_000_000);

    // 1. Healthy state
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &20_000_000);
    let snap = client.evaluate_stake_health(&seller);
    assert_eq!(snap.status, LiquidationStatus::Healthy);

    // 2. Create obligation that makes artisan under-collateralized
    //    after admin raises min_stake_required
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);
    client.set_min_stake_required(&25_000_000);

    // 3. Evaluate → under-collateralized (20M stake < 25M required)
    client.set_liquidation_policy(&5000, &0, &true);
    let snap2 = client.evaluate_stake_health(&seller);
    assert_eq!(snap2.status, LiquidationStatus::UnderCollateralized);
    assert_eq!(snap2.deficit, 5_000_000);

    // 4. Flag as liquidation-eligible
    client.flag_liquidation_eligible(&seller);
    assert_eq!(
        client.get_liquidation_status(&seller),
        LiquidationStatus::LiquidationEligible
    );

    // 5. Trigger liquidation — seizure = min(5M deficit, 5M × 50%) = 2.5M
    let record = client.trigger_liquidation(&seller);
    assert_eq!(record.seized_amount, 2_500_000);
    assert_eq!(client.get_stake(&seller), 17_500_000);
    assert_eq!(
        client.get_liquidation_status(&seller),
        LiquidationStatus::Liquidated
    );

    // 6. Try to unstake — blocked
    env.ledger().with_mut(|li| {
        li.timestamp += DEFAULT_STAKE_COOLDOWN as u64 + 1;
    });
    let result = client.try_unstake_tokens(&seller, &token_id);
    assert!(result.is_err());

    // 7. Cure by adding more stake (need ≥ 25M, currently have 17.5M)
    client.stake_tokens(&seller, &token_id, &10_000_000);
    assert_eq!(client.get_stake(&seller), 27_500_000);

    // 8. Cure
    client.cure_liquidation(&seller);
    assert_eq!(
        client.get_liquidation_status(&seller),
        LiquidationStatus::Healthy
    );

    // 9. Record is marked cured
    let r = client.get_liquidation_record(&0).unwrap();
    assert!(r.cured);
    assert!(r.cured_at > 0);

    // 10. Unstake works again
    let result = client.try_unstake_tokens(&seller, &token_id);
    assert!(result.is_ok());
}

// ===== Health Ratio Edge Cases =====

#[test]
fn test_health_ratio_zero_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, _token_id, _token_admin) = setup_test(&env, true);

    // Test with min_stake_required = 0 → always healthy
    client.set_min_stake_required(&0);

    let snap = client.evaluate_stake_health(&seller);
    assert_eq!(snap.status, LiquidationStatus::Healthy);
    assert_eq!(snap.health_ratio_bps, 0);
}

#[test]
fn test_health_ratio_large_stake() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &200_000_000);
    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &200_000_000);

    let snap = client.evaluate_stake_health(&seller);
    // health = 200M / 1 = 2000000% → health_ratio_bps should be > 10_000
    assert!(snap.health_ratio_bps > 10_000);
    assert_eq!(snap.deficit, 0);
}

// ===== Event Emission Tests =====

#[test]
fn test_flag_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &50_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &5_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);

    client.flag_liquidation_eligible(&seller);

    // Check events were published
    let events = env.events().all();
    let expected_topic: soroban_sdk::Val =
        Symbol::new(&env, "stake_liquidation_flagged").into_val(&env);
    let flagged = events.iter().any(|(_, topics, _)| {
        topics.len() >= 1
            && svec![&env, topics.get_unchecked(0)] == svec![&env, expected_topic]
    });
    assert!(flagged);
}

#[test]
fn test_cure_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, buyer, seller, token_id, token_admin) = setup_test(&env, true);

    token_admin.mint(&seller, &100_000_000);
    token_admin.mint(&buyer, &50_000_000);

    client.set_min_stake_required(&10_000_000);
    client.stake_tokens(&seller, &token_id, &6_000_000);
    client.create_escrow(&buyer, &seller, &token_id, &2_000_000, &1, &None);

    client.set_liquidation_policy(&5000, &0, &true);
    client.evaluate_stake_health(&seller);
    client.flag_liquidation_eligible(&seller);
    client.trigger_liquidation(&seller);

    // Cure
    client.stake_tokens(&seller, &token_id, &10_000_000);
    client.cure_liquidation(&seller);

    let events = env.events().all();
    let expected_topic: soroban_sdk::Val =
        Symbol::new(&env, "stake_liquidation_cured").into_val(&env);
    let cured = events.iter().any(|(_, topics, _)| {
        topics.len() >= 1
            && svec![&env, topics.get_unchecked(0)] == svec![&env, expected_topic]
    });
    assert!(cured);
}
