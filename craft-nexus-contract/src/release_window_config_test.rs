#![cfg(test)]

//! Focused boundary tests for release-window configuration validation (#1032).
//!
//! Administrators must not be able to persist a release-window configuration
//! that is impossible (a zero or inverted pair), overflowing (a maximum above
//! the absolute safety ceiling), or inconsistent with the dispute lifecycle
//! (a minimum that outlives the maximum dispute duration). Every
//! configuration-change path is covered:
//!
//! * the direct admin setters (`set_min_release_window`, `set_max_release_window`),
//! * the multi-sig proposal path (`propose_admin_action`), and
//! * the multi-sig execution path (`execute_admin_action`).
//!
//! Escrows that are already committed keep the release window they were created
//! with, even after the platform configuration changes.

use crate::{AdminActionKind, CraftNexusContract, CraftNexusContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

const ONE_HOUR: u32 = 60 * 60;
const ONE_DAY: u32 = 24 * 60 * 60;
const SEVEN_DAYS: u32 = 7 * 24 * 60 * 60;
const THIRTY_DAYS: u32 = 30 * 24 * 60 * 60;
/// Mirror of `ABSOLUTE_MAX_RELEASE_WINDOW` (365 days) from the contract.
const ABSOLUTE_MAX: u32 = 365 * 24 * 60 * 60;

fn setup() -> (
    Env,
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

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
    token_asset.mint(&buyer, &10_000_000);

    // Open mode: no onboarding contract, so privileged escrow flows are allowed.
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &None::<Address>,
    );

    (env, client, admin, buyer, seller, token_addr)
}

#[test]
fn test_default_release_window_config_is_valid() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    // The default minimum is one day and the default maximum is thirty days,
    // so the default configuration is correctly ordered.
    assert_eq!(client.get_min_release_window(), ONE_DAY);
    client.set_min_release_window(&THIRTY_DAYS);
    assert_eq!(client.get_min_release_window(), THIRTY_DAYS);
}

#[test]
fn test_zero_minimum_is_rejected_without_persisting() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    let result = client.try_set_min_release_window(&0);
    assert!(result.is_err());

    // The previous (valid) configuration is untouched.
    assert_eq!(client.get_min_release_window(), ONE_DAY);
}

#[test]
#[should_panic]
fn test_zero_maximum_is_rejected() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    client.set_max_release_window(&0);
}

#[test]
fn test_minimum_above_maximum_is_rejected() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    // Tighten the maximum to seven days; the default minimum (one day) is
    // still within bounds.
    client.set_max_release_window(&SEVEN_DAYS);

    // A minimum above the maximum leaves no window that satisfies the policy.
    let result = client.try_set_min_release_window(&THIRTY_DAYS);
    assert!(result.is_err());

    assert_eq!(client.get_min_release_window(), ONE_DAY);
}

#[test]
#[should_panic]
fn test_maximum_below_minimum_is_rejected() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    // The default minimum is one day, so a one-hour maximum is inverted.
    client.set_max_release_window(&ONE_HOUR);
}

#[test]
#[should_panic]
fn test_maximum_above_absolute_ceiling_is_rejected() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    // One second past the 365-day arithmetic ceiling.
    client.set_max_release_window(&(ABSOLUTE_MAX + 1));
}

#[test]
fn test_maximum_at_absolute_ceiling_is_accepted() {
    let (_env, client, _admin, buyer, seller, token) = setup();

    client.set_max_release_window(&ABSOLUTE_MAX);

    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token,
        &1_000_000,
        &1,
        &Some(ABSOLUTE_MAX),
    );
    assert_eq!(escrow.release_window, ABSOLUTE_MAX);
}

#[test]
fn test_minimum_equal_to_maximum_is_accepted() {
    let (_env, client, _admin, buyer, seller, token) = setup();

    client.set_max_release_window(&SEVEN_DAYS);
    client.set_min_release_window(&SEVEN_DAYS);
    assert_eq!(client.get_min_release_window(), SEVEN_DAYS);

    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token,
        &1_000_000,
        &1,
        &Some(SEVEN_DAYS),
    );
    assert_eq!(escrow.release_window, SEVEN_DAYS);
}

#[test]
fn test_minimum_inconsistent_with_dispute_period_is_rejected() {
    let (_env, client, _admin, _buyer, _seller, _token) = setup();

    // Raise the maximum so the ordering check passes; the value is still
    // rejected because it outlives the default 30-day max dispute duration.
    client.set_max_release_window(&ABSOLUTE_MAX);

    let result = client.try_set_min_release_window(&(THIRTY_DAYS + ONE_DAY));
    assert!(result.is_err());

    assert_eq!(client.get_min_release_window(), ONE_DAY);
}

#[test]
fn test_committed_escrow_retains_its_release_window_after_config_change() {
    let (_env, client, _admin, buyer, seller, token) = setup();

    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token,
        &1_000_000,
        &1,
        &Some(SEVEN_DAYS),
    );
    assert_eq!(escrow.release_window, SEVEN_DAYS);

    // Change the platform policy after the escrow was committed.
    client.set_min_release_window(&THIRTY_DAYS);
    client.set_max_release_window(&ABSOLUTE_MAX);

    // The escrow keeps the policy it was created with; configuration changes
    // are never applied retroactively.
    let stored = client.get_escrow(&1);
    assert_eq!(stored.release_window, SEVEN_DAYS);
}

#[test]
fn test_propose_admin_action_rejects_invalid_release_window_config() {
    let (_env, client, admin, _buyer, _seller, _token) = setup();

    let too_short_max = client.try_propose_admin_action(
        &admin,
        &AdminActionKind::SetMaxReleaseWindow(ONE_HOUR),
    );
    assert!(too_short_max.is_err());

    let too_long_min = client.try_propose_admin_action(
        &admin,
        &AdminActionKind::SetMinReleaseWindow(THIRTY_DAYS + ONE_DAY),
    );
    assert!(too_long_min.is_err());

    // Neither invalid action was persisted as a pending proposal.
    assert_eq!(client.get_pending_admin_actions().len(), 0);
}

#[test]
fn test_propose_admin_action_accepts_and_executes_valid_config() {
    let (_env, client, admin, buyer, seller, token) = setup();

    client.set_admin_action_timelock_delay(&0);

    let action = client.propose_admin_action(
        &admin,
        &AdminActionKind::SetMaxReleaseWindow(SEVEN_DAYS),
    );
    client.execute_admin_action(&action.id);

    // The new seven-day maximum is enforced: an eight-day window is rejected.
    let result = client.try_create_escrow(
        &buyer,
        &seller,
        &token,
        &1_000_000,
        &1,
        &Some(SEVEN_DAYS + ONE_DAY),
    );
    assert!(result.is_err());
}

#[test]
fn test_execute_admin_action_rejects_config_invalidated_by_drift() {
    let (_env, client, admin, buyer, seller, token) = setup();

    client.set_admin_action_timelock_delay(&0);

    // A proposal that is valid at creation time...
    let action = client.propose_admin_action(
        &admin,
        &AdminActionKind::SetMaxReleaseWindow(SEVEN_DAYS),
    );

    // ...is invalidated when the configuration drifts before execution.
    client.set_min_release_window(&THIRTY_DAYS);

    let result = client.try_execute_admin_action(&action.id);
    assert!(result.is_err());

    // The invalid maximum was never persisted: a thirty-day escrow still fits
    // within the default maximum.
    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token,
        &1_000_000,
        &1,
        &Some(THIRTY_DAYS),
    );
    assert_eq!(escrow.release_window, THIRTY_DAYS);
}
