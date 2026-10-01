#![cfg(test)]

use crate::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup() -> (Env, CraftNexusContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let artisan = Address::generate(&env);
    let wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&wallet, &admin, &arbitrator, &500, &None);
    (env, client, admin, artisan)
}

#[test]
fn set_artisan_fee_tier_rejects_unauthorized_call_without_writing() {
    let (env, client, _admin, artisan) = setup();
    let before = client.get_effective_fee_bps(&artisan);

    env.set_auths(&[]);
    let result = client.try_set_artisan_fee_tier(&artisan, &200);

    assert!(result.is_err());
    assert_eq!(client.get_effective_fee_bps(&artisan), before);
}

#[test]
fn set_artisan_fee_tier_rejects_paused_contract_without_writing() {
    let (_env, client, _admin, artisan) = setup();
    let before = client.get_effective_fee_bps(&artisan);
    client.set_paused(&true);

    let result = client.try_set_artisan_fee_tier(&artisan, &200);

    assert_eq!(result.unwrap_err(), Ok(soroban_sdk::Error::from_contract_error(Error::ContractPaused as u32)));
    assert_eq!(client.get_effective_fee_bps(&artisan), before);
}
