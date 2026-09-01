#![cfg(test)]
extern crate alloc;

use crate::{
    onboarding::{self, OnboardingContract, OnboardingContractClient},
    CraftNexusContract, CraftNexusContractClient, DataKey, Error,
};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env,
};

struct TestRig {
    env: Env,
    client: CraftNexusContractClient<'static>,
    contract_id: Address,
    buyer: Address,
    seller: Address,
    token_addr: Address,
}

fn setup_rig() -> TestRig {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_id.address();
    let token_asset = token::StellarAssetClient::new(&env, &token_addr);
    token_asset.mint(&buyer, &1_000_000_000_000);
    token_asset.mint(&seller, &1_000_000_000_000);

    let onboarding_contract = Address::generate(&env);

    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    client.set_min_escrow_amount(&token_addr, &0);
    client.set_min_release_window(&1);
    client.whitelist_token(&token_addr);

    TestRig {
        env,
        client,
        contract_id,
        buyer,
        seller,
        token_addr,
    }
}

#[test]
fn test_total_locked_overflow_rejected() {
    let rig = setup_rig();
    let env = &rig.env;

    // Simulate existing TotalLocked at maximum representable value
    env.as_contract(&rig.contract_id, || {
        let key = DataKey::TotalLocked(rig.token_addr.clone());
        env.storage().persistent().set(&key, &i128::MAX);
    });

    // Attempt to create an escrow which updates TotalLocked with +1000
    let res = rig.client.try_create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &1000,
        &101,
        &Some(86400),
    );

    // Must be rejected with CounterOverflow (83) and not commit partial state
    assert_eq!(res.unwrap_err().unwrap(), Error::CounterOverflow.into());

    // Verify state was not modified
    env.as_contract(&rig.contract_id, || {
        let key = DataKey::TotalLocked(rig.token_addr.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap();
        assert_eq!(current, i128::MAX);
    });
}

#[test]
#[should_panic]
fn test_total_locked_underflow_rejected() {
    let rig = setup_rig();
    let env = &rig.env;

    // Set TotalLocked to 50
    env.as_contract(&rig.contract_id, || {
        let key = DataKey::TotalLocked(rig.token_addr.clone());
        env.storage().persistent().set(&key, &50i128);
        CraftNexusContract::update_total_locked(env, &rig.token_addr, -100);
    });
}

#[test]
#[should_panic]
fn test_total_staked_overflow_panics() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let key = DataKey::TotalStaked(rig.token_addr.clone());
        env.storage().persistent().set(&key, &i128::MAX);
        CraftNexusContract::update_total_staked(env, &rig.token_addr, 1);
    });
}

#[test]
#[should_panic]
fn test_total_staked_underflow_panics() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let key = DataKey::TotalStaked(rig.token_addr.clone());
        env.storage().persistent().set(&key, &10i128);
        CraftNexusContract::update_total_staked(env, &rig.token_addr, -20);
    });
}

#[test]
fn test_escrow_count_overflow_rejected() {
    let rig = setup_rig();
    let env = &rig.env;

    // Set EscrowCount to u32::MAX
    env.as_contract(&rig.contract_id, || {
        let key = DataKey::EscrowCount;
        env.storage().persistent().set(&key, &u32::MAX);
    });

    // Attempt to create an escrow
    let res = rig.client.try_create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &1000,
        &202,
        &Some(86400),
    );

    assert_eq!(res.unwrap_err().unwrap(), Error::CounterOverflow.into());
}

#[test]
fn test_buyer_and_seller_escrow_count_overflow_rejected() {
    let rig = setup_rig();
    let env = &rig.env;

    // Set BuyerEscrowCount to u32::MAX
    env.as_contract(&rig.contract_id, || {
        let key = DataKey::BuyerEscrowCount(rig.buyer.clone());
        env.storage().persistent().set(&key, &u32::MAX);
    });

    let res = rig.client.try_create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &1000,
        &303,
        &Some(86400),
    );

    assert_eq!(res.unwrap_err().unwrap(), Error::CounterOverflow.into());
}

#[test]
#[should_panic]
fn test_active_obligations_overflow_panics() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let user = rig.buyer.clone();
        let key = DataKey::ActiveObligations(user.clone());
        env.storage().persistent().set(&key, &u32::MAX);
        CraftNexusContract::update_active_obligations(env, &user, 1);
    });
}

#[test]
#[should_panic]
fn test_active_obligations_underflow_panics() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let user = rig.buyer.clone();
        let key = DataKey::ActiveObligations(user.clone());
        env.storage().persistent().set(&key, &2u32);
        CraftNexusContract::update_active_obligations(env, &user, -5);
    });
}

#[test]
#[should_panic]
fn test_active_dispute_count_overflow_panics() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let dispute_key = DataKey::ActiveDisputeCount;
        env.storage().persistent().set(&dispute_key, &u32::MAX);
        CraftNexusContract::update_active_dispute_count(env, 1);
    });
}

#[test]
#[should_panic]
fn test_active_dispute_count_underflow_panics() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let dispute_key = DataKey::ActiveDisputeCount;
        env.storage().persistent().set(&dispute_key, &1u32);
        CraftNexusContract::update_active_dispute_count(env, -3);
    });
}

#[test]
fn test_recurring_escrow_count_overflow_rejected() {
    let rig = setup_rig();
    let env = &rig.env;

    env.as_contract(&rig.contract_id, || {
        let key = DataKey::RecurringEscrowCount;
        env.storage().persistent().set(&key, &u64::MAX);
    });

    let res = rig.client.try_create_recurring_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &1000,
        &86400,
        &30,
    );

    assert_eq!(res.unwrap_err().unwrap(), Error::CounterOverflow.into());
}

#[test]
fn test_onboarding_metrics_and_active_contracts_overflow_underflow() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let ob_id = env.register_contract(None, OnboardingContract);
    let ob_client = OnboardingContractClient::new(&env, &ob_id);

    ob_client.initialize(&admin);

    // Test active contract count underflow
    let res = ob_client.try_update_active_contracts(&user, &-5);
    assert_eq!(res.unwrap_err().unwrap(), onboarding::Error::ActiveContractUnderflow.into());

    // Test active contract count overflow
    env.as_contract(&ob_id, || {
        let key = onboarding::DataKey::ActiveContractCount(user.clone());
        env.storage().persistent().set(&key, &u32::MAX);
    });

    let res_overflow = ob_client.try_update_active_contracts(&user, &1);
    assert_eq!(res_overflow.unwrap_err().unwrap(), onboarding::Error::ActiveContractOverflow.into());
}
