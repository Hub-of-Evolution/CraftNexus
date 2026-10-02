
#![cfg(test)]

use crate::compatibility::Protocol;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, IntoVal, String, symbol_short};
use soroban_sdk::token::{StellarAssetClient, Client as TokenClient};

#[test]
fn test_ledger_timestamp() {
    let env = Env::default();
    env.ledger().with_mut(|l| {
        l.timestamp = 12345;
    });
    let ts = Protocol::get_ledger_timestamp(&env);
    assert_eq!(ts, 12345);
}

#[test]
fn test_token_operations() {
    let env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract_v2(admin.clone()).address();
    
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    
    let asset_client = StellarAssetClient::new(&env, &token_address);
    asset_client.mint(&sender, &1000);
    
    assert_eq!(Protocol::get_token_balance(&env, &token_address, &sender), 1000);
    assert_eq!(Protocol::get_token_balance(&env, &token_address, &recipient), 0);
    
    Protocol::transfer_tokens(&env, &token_address, &sender, &recipient, 400);
    
    assert_eq!(Protocol::get_token_balance(&env, &token_address, &sender), 600);
    assert_eq!(Protocol::get_token_balance(&env, &token_address, &recipient), 400);
    
    let decimals = Protocol::get_token_decimals(&env, &token_address).unwrap();
    assert_eq!(decimals, 7);
}

#[test]
fn test_ttl_extensions() {
    let env = Env::default();
    env.mock_all_auths();
    
    // We just verify the methods can be called without panicking.
    // Testing the actual TTL update in a unit test can be tricky,
    // but the abstraction passes it through to the env.
    
    let key = symbol_short!("test");
    
    env.storage().persistent().set(&key, &1_u32);
    Protocol::extend_persistent_ttl(&env, &key, 1000, 2000);
    
    env.storage().temporary().set(&key, &2_u32);
    Protocol::extend_temporary_ttl(&env, &key, 1000, 2000);
    
    env.storage().instance().set(&key, &3_u32);
    Protocol::extend_instance_ttl(&env, 1000, 2000);
}

