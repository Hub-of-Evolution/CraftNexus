#![cfg(test)]

//! Admin-mutation idempotency and revision binding (#1071).
//!
//! Retries of configuration, pause, recovery, and governance actions must not
//! produce duplicate effects. Callers bind mutations to a monotonic revision;
//! stale or already-applied requests fail without writing.

use crate::{
    AdminActionKind, CraftNexusContract, CraftNexusContractClient, Error, ExpiredDisputeFeePolicy,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, IntoVal, Symbol, TryIntoVal,
};

fn setup() -> (Env, CraftNexusContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    (env, client, admin)
}

#[test]
fn test_admin_revision_starts_at_zero() {
    let (_, client, _) = setup();
    assert_eq!(client.get_admin_revision(), 0);
}

#[test]
fn test_successful_admin_mutation_exposes_revision_in_event() {
    let (env, client, _) = setup();
    assert_eq!(client.get_admin_revision(), 0);

    client.set_paused(&true);
    assert_eq!(client.get_admin_revision(), 1);
    assert!(client.is_paused());

    let events = env.events().all();
    let last = events.last().unwrap();
    let paused: crate::PlatformPausedEvent = last.2.try_into_val(&env).unwrap();
    assert_eq!(paused.revision, 0);
}

#[test]
fn test_replay_of_applied_pause_does_not_repeat_effect() {
    let (_, client, _) = setup();
    client.set_paused(&true);
    assert_eq!(client.get_admin_revision(), 1);

    let result = client.try_set_paused(&true);
    assert!(result.is_err());
    assert!(client.is_paused());
    assert_eq!(client.get_admin_revision(), 1);
}

#[test]
fn test_stale_revision_fails_without_mutation() {
    let (_, client, _) = setup();
    let first = client.get_admin_revision();
    client.apply_admin_mutation(&first, &AdminActionKind::SetPlatformFee(800));
    assert_eq!(client.get_platform_fee(), 800);
    assert_eq!(client.get_admin_revision(), first + 1);

    let stale = client.try_apply_admin_mutation(&first, &AdminActionKind::SetPlatformFee(200));
    assert_eq!(stale, Err(Ok(Error::StaleAdminRevision)));
    assert_eq!(client.get_platform_fee(), 800);
    assert_eq!(client.get_admin_revision(), first + 1);
}

#[test]
fn test_replay_applied_mutation_returns_already_applied() {
    let (_, client, _) = setup();
    let rev = client.get_admin_revision();
    client.apply_admin_mutation(&rev, &AdminActionKind::PausePlatform(true));
    assert!(client.is_paused());

    let replay = client.try_apply_admin_mutation(&rev, &AdminActionKind::PausePlatform(true));
    assert_eq!(replay, Err(Ok(Error::AdminActionAlreadyApplied)));
    assert!(client.is_paused());
    assert_eq!(client.get_admin_revision(), rev + 1);
}

#[test]
fn test_config_event_carries_applied_revision() {
    let (env, client, _) = setup();
    let rev = client.get_admin_revision();
    client.update_platform_fee(&800);

    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(
        last.1,
        soroban_sdk::vec![
            &env,
            Symbol::new(&env, "admin_config_updated").into_val(&env),
            Symbol::new(&env, "platform_fee_bps").into_val(&env),
        ]
    );
    let event: crate::ConfigUpdatedEvent = last.2.try_into_val(&env).unwrap();
    assert_eq!(event.revision, rev);
    assert_eq!(event.new_value, crate::ConfigValue::U32(800));
}

#[test]
fn test_execute_admin_action_records_applied_revision() {
    let (_, client, admin) = setup();
    client.set_admin_action_timelock_delay(&0);
    let before = client.get_admin_revision();
    let action = client.propose_admin_action(&admin, &AdminActionKind::PausePlatform(true));
    client.execute_admin_action(&action.id);

    assert!(client.is_paused());
    assert_eq!(client.get_admin_revision(), before + 1);
}

#[test]
fn test_expired_dispute_policy_update_is_revision_bound() {
    let (_, client, _) = setup();
    client.update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee);
    let replay = client.try_update_expired_dispute_policy(&ExpiredDisputeFeePolicy::SplitFee);
    assert_eq!(replay, Err(Ok(Error::AdminActionAlreadyApplied)));
    assert_eq!(
        client.get_expired_dispute_policy(),
        ExpiredDisputeFeePolicy::SplitFee
    );
}

#[test]
fn test_pause_then_unpause_consumes_two_revisions() {
    let (_, client, _) = setup();
    let start = client.get_admin_revision();
    client.set_paused(&true);
    client.set_paused(&false);
    assert!(!client.is_paused());
    assert_eq!(client.get_admin_revision(), start + 2);
}
