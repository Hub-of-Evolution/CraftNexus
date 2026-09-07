#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env,
};

// ---------------------------------------------------------------------------
// Test tokens for interface probing
// ---------------------------------------------------------------------------

#[contract]
struct FullySupportedToken;

#[contractimpl]
impl FullySupportedToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

#[contract]
struct MissingBalanceToken;

#[contractimpl]
impl MissingBalanceToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

#[contract]
struct MissingTransferToken;

#[contractimpl]
impl MissingTransferToken {
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000
    }
}

#[contract]
struct MalformedDecimalsToken;

#[contractimpl]
impl MalformedDecimalsToken {
    // Returns a value outside the allowed 0..=18 range
    pub fn decimals(_env: Env) -> u32 {
        42
    }
    pub fn balance(_env: Env, _id: Address) -> i128 {
        0
    }
    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {}
}

// Token that records whether transfer was called and with what amount
#[contract]
struct RecordingToken;

#[contractimpl]
impl RecordingToken {
    pub fn initialize(env: Env) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "transfer_calls"), &0u32);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "last_amount"), &0i128);
    }
    pub fn decimals(_env: Env) -> u32 {
        7
    }
    pub fn balance(_env: Env, _id: Address) -> i128 {
        5_000_000
    }
    pub fn transfer(env: Env, _from: Address, _to: Address, amount: i128) {
        let calls: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "transfer_calls"))
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "transfer_calls"), &(calls + 1));
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "last_amount"), &amount);
    }
    pub fn get_transfer_calls(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "transfer_calls"))
            .unwrap_or(0)
    }
    pub fn get_last_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "last_amount"))
            .unwrap_or(0)
    }
}

fn setup_client(env: &Env) -> (CraftNexusContractClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(env, &contract_id);
    client.initialize(
        &Address::generate(env),
        &admin,
        &Address::generate(env),
        &500,
        &None,
    );
    (client, contract_id)
}

#[test]
fn whitelist_rejects_missing_transfer() {
    let env = Env::default();
    let (client, _) = setup_client(&env);
    let token = env.register_contract(None, MissingTransferToken);
    assert_eq!(
        client.try_whitelist_token(&token),
        Err(Ok(Error::UnsupportedToken))
    );
    assert_eq!(client.get_whitelisted_token_count(), 0);
}

#[test]
fn whitelist_rejects_missing_balance() {
    let env = Env::default();
    let (client, _) = setup_client(&env);
    let token = env.register_contract(None, MissingBalanceToken);
    assert_eq!(
        client.try_whitelist_token(&token),
        Err(Ok(Error::UnsupportedToken))
    );
    assert_eq!(client.get_whitelisted_token_count(), 0);
}

#[test]
fn whitelist_accepts_fully_supported_token() {
    let env = Env::default();
    let (client, _) = setup_client(&env);
    let token = env.register_contract(None, FullySupportedToken);
    // Should succeed – all three entrypoints exist
    client.whitelist_token(&token);
    assert_eq!(client.get_whitelisted_token_count(), 1);
    assert!(client.is_token_whitelisted(&token));
}

#[test]
fn whitelist_rejects_malformed_decimals_via_unsupported_path() {
    let env = Env::default();
    let (client, _) = setup_client(&env);
    let token = env.register_contract(None, MalformedDecimalsToken);
    let result = client.try_whitelist_token(&token);
    // 42 decimals is outside 0..=18, so InvalidTokenDecimals takes precedence
    assert_eq!(result, Err(Ok(Error::InvalidTokenDecimals)));
    assert_eq!(client.get_whitelisted_token_count(), 0);
}

#[test]
fn validate_compatibility_does_not_mutate_funds() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (client, contract_id) = setup_client(&env);

    // Use StellarAssetContract (real token) to observe real balances
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::StellarAssetClient::new(&env, &token.address());
    let buyer = Address::generate(&env);
    token_client.mint(&buyer, &10_000);
    token_client.mint(&contract_id, &5_000);

    let token_balance_before = token_client.balance(&contract_id);
    let buyer_balance_before = token_client.balance(&buyer);

    // Validation is read-only: should succeed and not move any funds
    let token_addr = token.address();
    assert!(client.is_token_supported(&token_addr));
    client.validate_token_compatibility(&token_addr).unwrap();

    // Call whitelist_token as well – also must not mutate customer funds beyond
    // the zero self-transfer (which is internal to validation)
    client.whitelist_token(&token_addr);

    let token_balance_after = token_client.balance(&contract_id);
    let buyer_balance_after = token_client.balance(&buyer);

    assert_eq!(
        token_balance_before, token_balance_after,
        "validation must not change contract balance"
    );
    assert_eq!(
        buyer_balance_before, buyer_balance_after,
        "validation must not change customer balance"
    );
}

#[test]
fn validate_compatibility_zero_transfer_probe_is_non_mutating() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_client(&env);
    let token_id = env.register_contract(None, RecordingToken);
    RecordingTokenClient::new(&env, &token_id).initialize();

    // Before validation, no transfers recorded
    assert_eq!(
        RecordingTokenClient::new(&env, &token_id).get_transfer_calls(),
        0
    );

    client.validate_token_compatibility(&token_id).unwrap();

    // Transfer must have been probed exactly once with amount 0
    assert_eq!(
        RecordingTokenClient::new(&env, &token_id).get_transfer_calls(),
        1
    );
    assert_eq!(
        RecordingTokenClient::new(&env, &token_id).get_last_amount(),
        0
    );

    // is_token_supported also probes (should be another call)
    let supported = client.is_token_supported(&token_id);
    assert!(supported);
    assert_eq!(
        RecordingTokenClient::new(&env, &token_id).get_transfer_calls(),
        2
    );
}

#[test]
fn is_token_supported_returns_false_for_unsupported() {
    let env = Env::default();
    let (client, _) = setup_client(&env);
    let bad = env.register_contract(None, MissingTransferToken);
    let good = env.register_contract(None, FullySupportedToken);
    assert!(!client.is_token_supported(&bad));
    assert!(client.is_token_supported(&good));
}

#[test]
fn stellar_asset_contract_is_supported() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_client(&env);
    let admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(admin);
    let addr = token.address();
    // Real Stellar asset should pass all three probes
    assert!(client.is_token_supported(&addr));
    client.validate_token_compatibility(&addr).unwrap();
    // And whitelisting should succeed
    client.whitelist_token(&addr);
    assert!(client.is_token_whitelisted(&addr));
}

#[test]
fn missing_methods_are_rejected_with_stable_error_not_panic() {
    let env = Env::default();
    env.mock_all_auths();

    #[contract]
    struct EmptyToken;
    #[contractimpl]
    impl EmptyToken {
        pub fn ping(_env: Env) {}
    }

    let (client, _) = setup_client(&env);
    let empty = env.register_contract(None, EmptyToken);
    // Must be a stable contract error, not a host panic
    let result = client.try_validate_token_compatibility(&empty);
    assert_eq!(result, Err(Ok(Error::UnsupportedToken)));
    let result2 = client.try_whitelist_token(&empty);
    assert_eq!(result2, Err(Ok(Error::UnsupportedToken)));
}
