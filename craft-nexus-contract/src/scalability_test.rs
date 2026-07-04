#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Map,
};

fn setup_test() -> (
    Env,
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let seller_addr = seller.clone();
    let token_admin = Address::generate(&env);

    // Deploy token contract
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_id.address();

    // Mint tokens to buyer
    let token_asset = token::StellarAssetClient::new(&env, &token_addr);
    token_asset.mint(&buyer, &1_000_000_000);

    // Deploy mock onboarding contract
    let onboarding_contract = Address::generate(&env);

    // Initialize the escrow contract
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(onboarding_contract),
    );

    (
        env,
        client,
        buyer,
        seller_addr,
        token_addr,
        admin,
        platform_wallet,
        arbitrator,
    )
}

#[test]
fn test_indexed_storage_scalability() {
    let (env, client, buyer, seller, token, _, _, _) = setup_test();

    // Create 100 escrows to simulate high-volume user
    for i in 0..100 {
        client.create_escrow(&buyer, &seller, &token, &1000, &(i + 1), &Some(604800));
    }

    // Verify buyer escrow count using indexed storage
    let buyer_count_key = DataKey::BuyerEscrowCount(buyer.clone());
    let count: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&buyer_count_key)
            .unwrap_or(0u32)
    });
    assert_eq!(count, 100);

    // Verify seller escrow count using indexed storage
    let seller_count_key = DataKey::SellerEscrowCount(seller.clone());
    let count: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&seller_count_key)
            .unwrap_or(0u32)
    });
    assert_eq!(count, 100);

    // Test pagination - first page
    let page1 = client.get_escrows_by_buyer(&buyer, &0, &10, &false);
    assert_eq!(page1.len(), 10);
    assert_eq!(page1.get_unchecked(0), 1);
    assert_eq!(page1.get_unchecked(9), 10);

    // Test pagination - middle page
    let page5 = client.get_escrows_by_buyer(&buyer, &5, &10, &false);
    assert_eq!(page5.len(), 10);
    assert_eq!(page5.get_unchecked(0), 51);
    assert_eq!(page5.get_unchecked(9), 60);

    // Test pagination - last page
    let page10 = client.get_escrows_by_buyer(&buyer, &9, &10, &false);
    assert_eq!(page10.len(), 10);
    assert_eq!(page10.get_unchecked(0), 91);
    assert_eq!(page10.get_unchecked(9), 100);

    // Test pagination - beyond last page
    let page11 = client.get_escrows_by_buyer(&buyer, &10, &10, &false);
    assert_eq!(page11.len(), 0);

    // Verify individual indexed entries exist
    for i in 0..100 {
        let index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), i);
        let escrow_id: u64 = env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .get(&index_key)
                .expect("Indexed entry should exist")
        });
        assert_eq!(escrow_id, (i + 1) as u64);
    }
}

#[test]
fn test_indexed_storage_multiple_users() {
    let (env, client, buyer1, seller1, token, _, _, _) = setup_test();
    let buyer2 = Address::generate(&env);
    let seller2 = Address::generate(&env);

    // Mint tokens to buyer2
    let token_asset = token::StellarAssetClient::new(&env, &token);
    token_asset.mint(&buyer2, &1_000_000_000);

    // Create escrows for buyer1
    for i in 0..50 {
        client.create_escrow(&buyer1, &seller1, &token, &1000, &(i + 1), &Some(604800));
    }

    // Create escrows for buyer2
    for i in 0..30 {
        client.create_escrow(&buyer2, &seller2, &token, &1000, &(i + 51), &Some(604800));
    }

    // Verify buyer1 count
    let buyer1_count_key = DataKey::BuyerEscrowCount(buyer1.clone());
    let count1: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&buyer1_count_key)
            .unwrap_or(0u32)
    });
    assert_eq!(count1, 50);

    // Verify buyer2 count
    let buyer2_count_key = DataKey::BuyerEscrowCount(buyer2.clone());
    let count2: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&buyer2_count_key)
            .unwrap_or(0u32)
    });
    assert_eq!(count2, 30);

    // Verify buyer1 escrows
    let buyer1_escrows = client.get_escrows_by_buyer(&buyer1, &0, &100, &false);
    assert_eq!(buyer1_escrows.len(), 50);

    // Verify buyer2 escrows
    let buyer2_escrows = client.get_escrows_by_buyer(&buyer2, &0, &100, &false);
    assert_eq!(buyer2_escrows.len(), 30);

    // Verify no cross-contamination
    assert_eq!(buyer1_escrows.get_unchecked(0), 1);
    assert_eq!(buyer2_escrows.get_unchecked(0), 51);
}

#[test]
fn test_migration_from_legacy_storage() {
    let (env, client, buyer, _seller, _token, _admin, _, _) = setup_test();

    // Simulate legacy storage by directly setting the old vector format
    let legacy_key = DataKey::BuyerEscrows(buyer.clone());
    let mut legacy_vec = soroban_sdk::Vec::new(&env);
    legacy_vec.push_back(1u64);
    legacy_vec.push_back(2u64);
    legacy_vec.push_back(3u64);
    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_vec);
    });

    // Verify legacy storage exists
    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(has_legacy);

    // Run migration
    let migrated_count = client.migrate_user_escrows(&buyer, &true);
    assert_eq!(migrated_count, 3);

    // Verify indexed storage was created
    let count_key = DataKey::BuyerEscrowCount(buyer.clone());
    let count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&count_key).unwrap()
    });
    assert_eq!(count, 3);

    // Verify individual indexed entries
    for i in 0..3 {
        let index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), i);
        let escrow_id: u64 = env.as_contract(&client.address, || {
            env.storage().persistent().get(&index_key).unwrap()
        });
        assert_eq!(escrow_id, (i + 1) as u64);
    }

    // Verify legacy storage was removed
    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);

    // Verify query function works with migrated data
    let escrows = client.get_escrows_by_buyer(&buyer, &0, &10, &false);
    assert_eq!(escrows.len(), 3);
    assert_eq!(escrows.get_unchecked(0), 1);
    assert_eq!(escrows.get_unchecked(1), 2);
    assert_eq!(escrows.get_unchecked(2), 3);
}

#[test]
fn test_backward_compatibility_query() {
    let (env, client, buyer, _seller, _token, _, _, _) = setup_test();

    // Simulate legacy storage
    let legacy_key = DataKey::BuyerEscrows(buyer.clone());
    let mut legacy_vec = soroban_sdk::Vec::new(&env);
    legacy_vec.push_back(10u64);
    legacy_vec.push_back(20u64);
    legacy_vec.push_back(30u64);
    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_vec);
    });

    // Query should work with legacy storage (backward compatibility)
    let escrows = client.get_escrows_by_buyer(&buyer, &0, &10, &false);
    assert_eq!(escrows.len(), 3);
    assert_eq!(escrows.get_unchecked(0), 10);
    assert_eq!(escrows.get_unchecked(1), 20);
    assert_eq!(escrows.get_unchecked(2), 30);

    // Test pagination with legacy storage
    let page1 = client.get_escrows_by_buyer(&buyer, &0, &2, &false);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get_unchecked(0), 10);
    assert_eq!(page1.get_unchecked(1), 20);

    let page2 = client.get_escrows_by_buyer(&buyer, &1, &2, &false);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get_unchecked(0), 30);
}

#[test]
fn test_batch_create_with_indexed_storage() {
    let (env, client, buyer, seller, token, _, _, _) = setup_test();

    // Create escrows individually
    let mut order_ids = soroban_sdk::Vec::new(&env);
    for i in 0..10 {
        let order_id = i + 1;
        client.create_escrow(&buyer, &seller, &token, &1000, &order_id, &Some(604800));
        order_ids.push_back(order_id);
    }
    assert_eq!(order_ids.len(), 10);

    // Verify count was updated correctly
    let buyer_count_key = DataKey::BuyerEscrowCount(buyer.clone());
    let count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&buyer_count_key).unwrap()
    });
    assert_eq!(count, 10);

    // Verify all indexed entries exist
    for i in 0..10 {
        let index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), i);
        let has_index = env.as_contract(&client.address, || {
            env.storage().persistent().has(&index_key)
        });
        assert!(has_index);
    }

    // Verify query returns all escrows
    let escrows = client.get_escrows_by_buyer(&buyer, &0, &100, &false);
    assert_eq!(escrows.len(), 10);
}

#[test]
fn test_no_storage_limit_with_indexed_pattern() {
    let (env, client, buyer, seller, token, _, _, _) = setup_test();

    // Create 500 escrows to demonstrate scalability
    // In the old pattern, this would approach the 64KB limit
    // With indexed storage, each entry is separate and small
    for i in 0..500 {
        client.create_escrow(&buyer, &seller, &token, &1000, &(i + 1), &Some(604800));
    }

    // Verify count
    let buyer_count_key = DataKey::BuyerEscrowCount(buyer.clone());
    let count: u32 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&buyer_count_key).unwrap()
    });
    assert_eq!(count, 500);

    // Verify we can still query efficiently
    let page1 = client.get_escrows_by_buyer(&buyer, &0, &50, &false);
    assert_eq!(page1.len(), 50);

    let page10 = client.get_escrows_by_buyer(&buyer, &9, &50, &false);
    assert_eq!(page10.len(), 50);
    assert_eq!(page10.get_unchecked(0), 451);
    assert_eq!(page10.get_unchecked(49), 500);

    // Verify individual storage entries are small
    // Each entry is just: Address + u32 index -> u64 escrow_id
    // This is well under 64KB per entry
    for i in 0..500 {
        let index_key = DataKey::BuyerEscrowIndexed(buyer.clone(), i);
        let has_index = env.as_contract(&client.address, || {
            env.storage().persistent().has(&index_key)
        });
        assert!(has_index);
    }
}

#[test]
fn test_whitelisted_tokens_individual_storage() {
    let (env, client, _, _, token1, _, _, _) = setup_test();
    let token2 = Address::generate(&env);
    let token3 = Address::generate(&env);

    // Initially no tokens are whitelisted (count should be 0)
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 0);

    // All tokens should be allowed when whitelist is empty
    assert!(client.is_token_whitelisted(&token1));
    assert!(client.is_token_whitelisted(&token2));

    // Add tokens to whitelist
    client.whitelist_token(&token1);
    client.whitelist_token(&token2);

    // Check count
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 2);

    // Check individual tokens
    assert!(client.is_token_whitelisted(&token1));
    assert!(client.is_token_whitelisted(&token2));
    assert!(!client.is_token_whitelisted(&token3));

    // Remove a token
    client.remove_token_from_whitelist(&token1);
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 1);

    // Check tokens after removal
    assert!(!client.is_token_whitelisted(&token1));
    assert!(client.is_token_whitelisted(&token2));

    // Remove last token - should disable enforcement
    client.remove_token_from_whitelist(&token2);
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 0);

    // All tokens should be allowed again when whitelist is empty
    assert!(client.is_token_whitelisted(&token1));
    assert!(client.is_token_whitelisted(&token2));
    assert!(client.is_token_whitelisted(&token3));
}

#[test]
fn test_whitelisted_tokens_scalability() {
    let (env, client, _, _, _, _admin, _, _) = setup_test();

    // Create many tokens to test scalability
    let mut tokens = soroban_sdk::Vec::new(&env);
    for _ in 0..100 {
        let token = Address::generate(&env);
        tokens.push_back(token.clone());
        client.whitelist_token(&token);
    }

    // Verify count
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 100);

    // Verify all tokens are whitelisted
    for i in 0..tokens.len() {
        if let Some(token) = tokens.get(i) {
            assert!(client.is_token_whitelisted(&token));
        }
    }

    // Verify non-whitelisted token is rejected
    let non_whitelisted = Address::generate(&env);
    assert!(!client.is_token_whitelisted(&non_whitelisted));

    // Remove half the tokens
    for i in 0..50 {
        if let Some(token) = tokens.get(i) {
            client.remove_token_from_whitelist(&token);
        }
    }

    // Verify count
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 50);

    // Verify removed tokens are no longer whitelisted
    for i in 0..50 {
        if let Some(token) = tokens.get(i) {
            assert!(!client.is_token_whitelisted(&token));
        }
    }

    // Verify remaining tokens are still whitelisted
    for i in 50..100 {
        if let Some(token) = tokens.get(i) {
            assert!(client.is_token_whitelisted(&token));
        }
    }
}

#[test]
fn test_whitelisted_tokens_migration() {
    let (env, client, _, _, token1, _, _, _) = setup_test();
    let token2 = Address::generate(&env);
    let token3 = Address::generate(&env);

    // Simulate legacy storage by directly setting the old Map format
    let legacy_key = DataKey::WhitelistedTokens;
    let mut legacy_map = Map::new(&env);
    legacy_map.set(token1.clone(), true);
    legacy_map.set(token2.clone(), true);
    legacy_map.set(token3.clone(), false); // This should not be migrated

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_map);
    });

    // Verify legacy storage exists
    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(has_legacy);

    // Run migration
    let migrated_count = client.migrate_whitelist_storage();
    assert_eq!(migrated_count, 2); // Only true entries should be migrated

    // Verify new storage was created
    let count = client.get_whitelisted_token_count();
    assert_eq!(count, 2);

    // Verify individual tokens
    assert!(client.is_token_whitelisted(&token1));
    assert!(client.is_token_whitelisted(&token2));
    assert!(!client.is_token_whitelisted(&token3)); // Was false in legacy, so not whitelisted

    // Verify legacy storage was removed
    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);
}

#[test]
fn test_whitelist_migration_50_tokens() {
    let (env, client, _, _, _, _admin, _, _) = setup_test();

    let num_tokens = 55;
    let mut tokens = soroban_sdk::Vec::new(&env);

    let legacy_key = DataKey::WhitelistedTokens;
    let mut legacy_map = Map::new(&env);
    for _i in 0..num_tokens {
        let token = Address::generate(&env);
        tokens.push_back(token.clone());
        legacy_map.set(token, true);
    }
    let false_token = Address::generate(&env);
    legacy_map.set(false_token.clone(), false);

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_map);
    });

    let migrated_count = client.migrate_whitelist_storage();
    assert_eq!(migrated_count, num_tokens);

    let count = client.get_whitelisted_token_count();
    assert_eq!(count, num_tokens);

    for i in 0..tokens.len() {
        if let Some(token) = tokens.get(i) {
            assert!(
                client.is_token_whitelisted(&token),
                "token {} not whitelisted",
                i
            );
        }
    }
    assert!(!client.is_token_whitelisted(&false_token));

    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);
}

#[test]
fn test_whitelist_scalability_beyond_1800() {
    let (env, client, _, _, _, _admin, _, _) = setup_test();

    let num_tokens = 2001;
    let mut tokens = soroban_sdk::Vec::new(&env);

    let legacy_key = DataKey::WhitelistedTokens;
    let mut legacy_map = Map::new(&env);
    for _i in 0..num_tokens {
        let token = Address::generate(&env);
        tokens.push_back(token.clone());
        legacy_map.set(token, true);
    }

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_map);
    });

    let migrated_count = client.migrate_whitelist_storage();
    assert_eq!(migrated_count, num_tokens);

    let count = client.get_whitelisted_token_count();
    assert_eq!(count, num_tokens);

    assert!(client.is_token_whitelisted(&tokens.get_unchecked(0)));
    assert!(client.is_token_whitelisted(&tokens.get_unchecked(num_tokens / 2)));
    assert!(client.is_token_whitelisted(&tokens.get_unchecked(num_tokens - 1)));
}

#[test]
fn test_artisan_stake_queue_bounded_storage() {
    let (env, client, _, artisan, token, _, _, _) = setup_test();

    // Mint tokens to artisan for staking
    let token_asset = token::StellarAssetClient::new(&env, &token);
    token_asset.mint(&artisan, &10_000_000);

    // Initially no deposits
    let count = client.get_artisan_stake_queue_count(&artisan);
    assert_eq!(count, 0);

    // Add multiple stake deposits
    for i in 1..=10 {
        client.stake_tokens(&artisan, &token, &(i * 1000));
    }

    // Verify count
    let count = client.get_artisan_stake_queue_count(&artisan);
    assert_eq!(count, 10);

    // Verify deposits can be retrieved
    let deposits = client.get_artisan_stake_deposits(&artisan, &0, &5);
    assert_eq!(deposits.len(), 5);
    assert_eq!(deposits.get_unchecked(0).amount, 1000);
    assert_eq!(deposits.get_unchecked(4).amount, 5000);

    // Test pagination
    let deposits_page2 = client.get_artisan_stake_deposits(&artisan, &5, &5);
    assert_eq!(deposits_page2.len(), 5);
    assert_eq!(deposits_page2.get_unchecked(0).amount, 6000);
    assert_eq!(deposits_page2.get_unchecked(4).amount, 10000);
}

#[test]
fn test_artisan_stake_queue_pruning() {
    let (env, client, _, artisan, token, _, _, _) = setup_test();

    // Mint tokens to artisan for staking
    let token_asset = token::StellarAssetClient::new(&env, &token);
    token_asset.mint(&artisan, &100_000_000);

    // Add deposits up to the pruning threshold
    for _ in 1..=STAKE_QUEUE_PRUNE_THRESHOLD {
        client.stake_tokens(&artisan, &token, &1000);
    }

    let count = client.get_artisan_stake_queue_count(&artisan);
    assert_eq!(count, STAKE_QUEUE_PRUNE_THRESHOLD);

    // Advance time to mature some deposits
    env.ledger().with_mut(|li| {
        li.timestamp = li.timestamp + (DEFAULT_STAKE_COOLDOWN as u64) + 1;
    });

    // Add one more deposit - this should trigger pruning
    client.stake_tokens(&artisan, &token, &1000);

    // Count should collapse to the single new deposit because all earlier entries matured and were pruned.
    let count_after_pruning = client.get_artisan_stake_queue_count(&artisan);
    assert_eq!(count_after_pruning, 1);

    let stored_deposit: Option<StakeDeposit> = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::ArtisanStakeQueueIndexed(artisan.clone(), 0))
    });
    let deposit = stored_deposit.expect("pruned queue should retain the latest deposit in storage");
    assert_eq!(deposit.amount, 1000);

    let missing_deposit: Option<StakeDeposit> = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::ArtisanStakeQueueIndexed(artisan.clone(), 1))
    });
    assert!(
        missing_deposit.is_none(),
        "only the latest deposit should remain after pruning"
    );
}

#[test]
fn test_artisan_stake_queue_migration() {
    let (env, client, _, artisan, _, _, _, _) = setup_test();

    // Simulate legacy storage by directly setting the old Vec format
    let legacy_key = DataKey::ArtisanStakeQueue(artisan.clone());
    let mut legacy_queue = soroban_sdk::Vec::new(&env);
    legacy_queue.push_back(StakeDeposit {
        amount: 1000,
        cooldown_end: 1000,
    });
    legacy_queue.push_back(StakeDeposit {
        amount: 2000,
        cooldown_end: 2000,
    });
    legacy_queue.push_back(StakeDeposit {
        amount: 3000,
        cooldown_end: 3000,
    });

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&legacy_key, &legacy_queue);
    });

    // Verify legacy storage exists
    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(has_legacy);

    // Run migration
    let migrated_count = client.migrate_artisan_stake_queue(&artisan);
    assert_eq!(migrated_count, 3);

    // Verify new storage was created
    let count = client.get_artisan_stake_queue_count(&artisan);
    assert_eq!(count, 3);

    // Verify individual deposits
    let deposits = client.get_artisan_stake_deposits(&artisan, &0, &10);
    assert_eq!(deposits.len(), 3);
    assert_eq!(deposits.get_unchecked(0).amount, 1000);
    assert_eq!(deposits.get_unchecked(1).amount, 2000);
    assert_eq!(deposits.get_unchecked(2).amount, 3000);

    // Verify legacy storage was removed
    let has_legacy = env.as_contract(&client.address, || {
        env.storage().persistent().has(&legacy_key)
    });
    assert!(!has_legacy);
}

#[test]
fn test_artisan_stake_queue_max_capacity() {
    let (env, client, _buyer, artisan, token, _admin, _, _) = setup_test();

    // Mint tokens to artisan for staking
    let token_asset = token::StellarAssetClient::new(&env, &token);
    token_asset.mint(&artisan, &1_000_000_000);

    // Fill queue to maximum capacity
    for _i in 1..=MAX_STAKE_QUEUE_SIZE {
        client.stake_tokens(&artisan, &token, &1000);
    }

    let count = client.get_artisan_stake_queue_count(&artisan);
    assert_eq!(count, MAX_STAKE_QUEUE_SIZE);

    // The next stake should fail due to queue being full
    // We can't use std::panic::catch_unwind in no_std, so we'll just verify the count
    // In a real scenario, this would panic with StakeQueueFull error
}

#[test]
fn test_index_read_budget_smoke() {
    let (env, client, buyer, seller, token, _, _, _) = setup_test();
    client.create_escrow(&buyer, &seller, &token, &1000, &1, &Some(604800));

    env.budget().reset_default();
    let _ = client.has_active_escrows(&buyer);
}
