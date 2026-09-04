#![cfg(test)]
extern crate alloc;

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (CraftNexusContractClient<'static>, Address, Address) {
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);

    let platform_wallet = Address::generate(env);
    let admin = Address::generate(env);
    let arbitrator = Address::generate(env);
    let onboarding_contract = Address::generate(env);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    (client, admin, platform_wallet)
}

#[test]
fn test_pre_migration_check_matches_current_version() {
    let env = Env::default();
    let (client, _admin, _wallet) = setup(&env);

    assert_eq!(client.get_version(), 1);
    client.pre_migration_check(&1);

    let result = client.try_pre_migration_check(&2);
    assert!(result.is_err());
}

#[test]
fn test_backup_and_rollback_platform_config() {
    let env = Env::default();
    let (client, _admin, _wallet) = setup(&env);

    let original_fee = client.get_platform_config().platform_fee_bps;
    let backup_id = client.backup_platform_config();

    // Mutate config after the backup was taken.
    client.update_platform_fee(&(original_fee + 100));
    assert_eq!(
        client.get_platform_config().platform_fee_bps,
        original_fee + 100
    );

    // Roll back and confirm the pre-migration snapshot is restored.
    client.rollback_platform_config(&backup_id);
    assert_eq!(client.get_platform_config().platform_fee_bps, original_fee);
}

#[test]
fn test_rollback_unknown_backup_fails() {
    let env = Env::default();
    let (client, _admin, _wallet) = setup(&env);

    let result = client.try_rollback_platform_config(&999);
    assert!(result.is_err());
}

#[test]
fn test_backup_log_is_bounded_fifo() {
    let env = Env::default();
    let (client, _admin, _wallet) = setup(&env);

    let mut last_id = 0u32;
    for _ in 0..(MAX_CONFIG_BACKUPS + 5) {
        last_id = client.backup_platform_config();
    }

    let backups = client.get_platform_config_backups();
    assert_eq!(backups.len(), MAX_CONFIG_BACKUPS);
    // Oldest entries should have been trimmed FIFO; only the most recent
    // MAX_CONFIG_BACKUPS ids remain, ending at `last_id`.
    assert_eq!(backups.get(backups.len() - 1).unwrap().id, last_id);
    assert!(client.get_platform_config_backup(&0).is_none());
}

#[test]
fn test_backup_platform_config_requires_admin() {
    let env = Env::default();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let onboarding_contract = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    // Without mocked auth, a non-admin backup attempt must fail.
    env.set_auths(&[]);
    let result = client.try_backup_platform_config();
    assert!(result.is_err());
}
