#![cfg(test)]
extern crate alloc;

use crate::{
    CraftNexusContract, CraftNexusContractClient, Error, IdempotencyOp,
};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, BytesN, Env,
};

struct TestRig {
    env: Env,
    client: CraftNexusContractClient<'static>,
    admin: Address,
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
    token_asset.mint(&admin, &1_000_000_000_000);

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
        admin,
        buyer,
        seller,
        token_addr,
    }
}

#[test]
fn test_create_escrow_idempotent_success_and_replay() {
    let rig = setup_rig();
    let key = BytesN::from_array(&rig.env, &[42u8; 32]);

    let res1 = rig.client.create_escrow_idempotent(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &101,
        &Some(100),
        &Some(key.clone()),
    );
    assert_eq!(res1.id, 101);
    assert_eq!(res1.amount, 10_000);

    // Replay with the exact same key and parameters
    let res2 = rig.client.create_escrow_idempotent(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &101,
        &Some(100),
        &Some(key.clone()),
    );
    assert_eq!(res2.id, 101);
    assert_eq!(res2.amount, 10_000);

    // Verify record exists via query helper
    let record = rig.client.get_idempotency_record(&rig.buyer, &key).expect("record should exist");
    assert_eq!(record.order_id, 101);
    assert_eq!(record.op, IdempotencyOp::CreateEscrow);
}

#[test]
fn test_create_escrow_idempotent_mismatch_parameters() {
    let rig = setup_rig();
    let key = BytesN::from_array(&rig.env, &[7u8; 32]);

    let _res1 = rig.client.create_escrow_idempotent(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &201,
        &Some(100),
        &Some(key.clone()),
    );

    // Replay with same key but DIFFERENT amount
    let res2 = rig.client.try_create_escrow_idempotent(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &20_000,
        &201,
        &Some(100),
        &Some(key.clone()),
    );
    assert_eq!(res2, Err(Ok(Error::IdempotencyMismatch)));

    // Replay with same key but DIFFERENT order_id
    let res3 = rig.client.try_create_escrow_idempotent(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &202,
        &Some(100),
        &Some(key.clone()),
    );
    assert_eq!(res3, Err(Ok(Error::IdempotencyMismatch)));
}

#[test]
fn test_idempotency_key_isolated_by_caller() {
    let rig = setup_rig();
    let other_buyer = Address::generate(&rig.env);
    let token_asset = token::StellarAssetClient::new(&rig.env, &rig.token_addr);
    token_asset.mint(&other_buyer, &1_000_000_000);

    let key = BytesN::from_array(&rig.env, &[15u8; 32]);

    // Buyer 1 uses key for order 301
    let res1 = rig.client.create_escrow_idempotent(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &5_000,
        &301,
        &Some(100),
        &Some(key.clone()),
    );
    assert_eq!(res1.id, 301);

    // Other buyer uses the exact same key for a different order 302
    let res2 = rig.client.create_escrow_idempotent(
        &other_buyer,
        &rig.seller,
        &rig.token_addr,
        &5_000,
        &302,
        &Some(100),
        &Some(key.clone()),
    );
    assert_eq!(res2.id, 302);

    // Both records exist independently under their respective callers
    let rec1 = rig.client.get_idempotency_record(&rig.buyer, &key).unwrap();
    let rec2 = rig.client.get_idempotency_record(&other_buyer, &key).unwrap();
    assert_eq!(rec1.order_id, 301);
    assert_eq!(rec2.order_id, 302);
}

#[test]
fn test_release_funds_idempotent_success_and_replay() {
    let rig = setup_rig();
    let key = BytesN::from_array(&rig.env, &[88u8; 32]);

    // Create escrow
    let escrow = rig.client.create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &401,
        &Some(100),
    );
    assert_eq!(escrow.id, 401);

    // Release with idempotency key
    let res1 = rig.client.release_funds_idempotent(&401, &Some(key.clone()));
    assert_eq!(res1, ());

    // Replay release with same key
    let res2 = rig.client.release_funds_idempotent(&401, &Some(key.clone()));
    assert_eq!(res2, ());

    // Create second escrow to test key replay on a different order
    let _escrow2 = rig.client.create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &402,
        &Some(100),
    );

    // Replay release with same key on DIFFERENT order fails with IdempotencyMismatch
    let res3 = rig.client.try_release_funds_idempotent(&402, &Some(key.clone()));
    assert_eq!(res3, Err(Ok(Error::IdempotencyMismatch)));
}

#[test]
fn test_refund_idempotent_success_and_replay() {
    let rig = setup_rig();
    let key = BytesN::from_array(&rig.env, &[99u8; 32]);

    // Create escrow
    let escrow = rig.client.create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &501,
        &Some(100),
    );
    assert_eq!(escrow.id, 501);

    // Refund with idempotency key (admin only)
    let res1 = rig.client.refund_idempotent(&501, &Some(key.clone()));
    assert_eq!(res1, ());

    // Replay refund with same key
    let res2 = rig.client.refund_idempotent(&501, &Some(key.clone()));
    assert_eq!(res2, ());

    // Create second escrow to test key replay on different order
    let _escrow2 = rig.client.create_escrow(
        &rig.buyer,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &502,
        &Some(100),
    );

    // Replay refund with same key on DIFFERENT order fails with IdempotencyMismatch
    let res3 = rig.client.try_refund_idempotent(&502, &Some(key.clone()));
    assert_eq!(res3, Err(Ok(Error::IdempotencyMismatch)));
}

#[test]
fn test_idempotency_cross_operation_isolation() {
    let rig = setup_rig();
    let key = BytesN::from_array(&rig.env, &[111u8; 32]);

    // Admin creates escrow with key 111
    let _res1 = rig.client.create_escrow_idempotent(
        &rig.admin,
        &rig.seller,
        &rig.token_addr,
        &10_000,
        &601,
        &Some(100),
        &Some(key.clone()),
    );

    // Admin tries to refund another escrow using the SAME key (cross-operation collision attempt)
    let res2 = rig.client.try_refund_idempotent(&602, &Some(key.clone()));
    assert_eq!(res2, Err(Ok(Error::IdempotencyMismatch)));
}
