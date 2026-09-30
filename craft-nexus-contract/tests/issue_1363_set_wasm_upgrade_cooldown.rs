//! Regression tests for issue #1363 — harden `set_wasm_upgrade_cooldown`.
//!
//! The entrypoint mutates admin-controlled configuration, so every rejected
//! call (paused platform, below-minimum cooldown) must surface the expected
//! contract `Error` variant and leave persisted storage untouched.

use craft_nexus_contract::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Mirrors `lib.rs::MIN_WASM_UPGRADE_COOLDOWN` (24 hours).
const MIN_WASM_UPGRADE_COOLDOWN: u32 = 24 * 60 * 60;

/// Deploys and initializes the contract, returning the environment and the
/// deployed contract address so each test can build its own client handle.
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    {
        let client = CraftNexusContractClient::new(&env, &contract_id);
        client.initialize(&platform_wallet, &admin, &arbitrator, &100u32, &None);
    }

    (env, contract_id)
}

/// The core hardening: while the platform is paused, the entrypoint is rejected
/// with `ContractPaused` and the configured cooldown is not modified.
#[test]
fn set_wasm_upgrade_cooldown_rejected_while_paused_and_leaves_storage_unchanged() {
    let (env, contract_id) = setup();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let before = client.get_platform_config().wasm_upgrade_cooldown;

    client.set_paused(&true);

    let new_cooldown = MIN_WASM_UPGRADE_COOLDOWN + 1;
    let result = client.try_set_wasm_upgrade_cooldown(&new_cooldown);

    assert!(
        matches!(result, Err(Ok(Error::ContractPaused))),
        "a paused platform must reject set_wasm_upgrade_cooldown with ContractPaused"
    );

    let after = client.get_platform_config().wasm_upgrade_cooldown;
    assert_eq!(
        before, after,
        "rejection while paused must not mutate persisted config"
    );
}

/// The main rejected input: a cooldown below the floor must fail with
/// `UpgradeCooldownTooShort` before any storage write.
#[test]
fn set_wasm_upgrade_cooldown_below_minimum_leaves_storage_unchanged() {
    let (env, contract_id) = setup();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let before = client.get_platform_config().wasm_upgrade_cooldown;

    let too_short = MIN_WASM_UPGRADE_COOLDOWN - 1;
    let result = client.try_set_wasm_upgrade_cooldown(&too_short);

    assert!(
        matches!(result, Err(Ok(Error::UpgradeCooldownTooShort))),
        "a sub-minimum cooldown must be rejected with UpgradeCooldownTooShort"
    );

    let after = client.get_platform_config().wasm_upgrade_cooldown;
    assert_eq!(
        before, after,
        "rejected input must not mutate persisted config"
    );
}

/// Guard against over-hardening: when active and valid, the entrypoint still
/// persists the new value.
#[test]
fn set_wasm_upgrade_cooldown_persists_valid_value_when_active() {
    let (env, contract_id) = setup();
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let new_cooldown = MIN_WASM_UPGRADE_COOLDOWN + 3_600;
    client.set_wasm_upgrade_cooldown(&new_cooldown);

    assert_eq!(
        client.get_platform_config().wasm_upgrade_cooldown,
        new_cooldown
    );
}
