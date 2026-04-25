#![cfg(test)]

use crate::{CraftNexusContract, CraftNexusContractClient, Error};
use soroban_sdk::{
    testutils::{Address as _},
    token, Address, Env,
};

const DEFAULT_MIN_RELEASE_WINDOW: u32 = 24 * 60 * 60; // 1 day
const ONE_HOUR: u32 = 60 * 60;
const ONE_DAY: u32 = 24 * 60 * 60;
const SEVEN_DAYS: u32 = 7 * 24 * 60 * 60;

/// Helper function to setup test environment
fn setup_test() -> (
    Env,
    CraftNexusContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let platform_wallet = Address::generate(&env);
    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);

    // Deploy token contract
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = token::Client::new(&env, &token_id.address());
    let token_addr = token_id.address();

    // Mint tokens to buyer
    token.mint(&buyer, &10_000_000);

    // Deploy mock onboarding contract
    let onboarding_contract = Address::generate(&env);

    // Initialize the escrow contract
    client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500, // 5% platform fee
        &onboarding_contract,
    );

    (env, client, buyer, seller, token_addr, admin, platform_wallet)
}

#[test]
fn test_default_min_release_window_is_one_day() {
    let (_, client, _, _, _, _, _) = setup_test();

    let min_window = client.get_min_release_window();
    assert_eq!(min_window, DEFAULT_MIN_RELEASE_WINDOW);
    assert_eq!(min_window, ONE_DAY);
}

#[test]
fn test_create_escrow_with_minimum_window() {
    let (_, client, buyer, seller, token_addr, _, _) = setup_test();

    // Create escrow with exactly the minimum window (1 day)
    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(ONE_DAY),
    );

    assert_eq!(escrow.release_window, ONE_DAY);
}

#[test]
fn test_create_escrow_with_above_minimum_window() {
    let (_, client, buyer, seller, token_addr, _, _) = setup_test();

    // Create escrow with window above minimum (7 days)
    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(SEVEN_DAYS),
    );

    assert_eq!(escrow.release_window, SEVEN_DAYS);
}

#[test]
#[should_panic(expected = "ReleaseWindowTooShort")]
fn test_create_escrow_below_minimum_fails() {
    let (_, client, buyer, seller, token_addr, _, _) = setup_test();

    // Try to create escrow with window below minimum (1 hour < 1 day)
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(ONE_HOUR),
    );
}

#[test]
#[should_panic(expected = "ReleaseWindowTooShort")]
fn test_create_escrow_with_one_second_fails() {
    let (_, client, buyer, seller, token_addr, _, _) = setup_test();

    // Try to create "flash" escrow with 1 second window
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(1),
    );
}

#[test]
#[should_panic(expected = "ReleaseWindowTooShort")]
fn test_create_escrow_with_zero_window_fails() {
    let (_, client, buyer, seller, token_addr, _, _) = setup_test();

    // Try to create escrow with 0 second window
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(0),
    );
}

#[test]
fn test_admin_can_update_min_release_window() {
    let (_, client, _, _, _, admin, _) = setup_test();

    // Update minimum to 1 hour
    client.set_min_release_window(&ONE_HOUR).unwrap();

    let min_window = client.get_min_release_window();
    assert_eq!(min_window, ONE_HOUR);
}

#[test]
fn test_create_escrow_after_lowering_minimum() {
    let (_, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Lower minimum to 1 hour
    client.set_min_release_window(&ONE_HOUR).unwrap();

    // Now 1 hour window should work
    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(ONE_HOUR),
    );

    assert_eq!(escrow.release_window, ONE_HOUR);
}

#[test]
fn test_create_escrow_after_raising_minimum() {
    let (_, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Raise minimum to 7 days
    client.set_min_release_window(&SEVEN_DAYS).unwrap();

    // Now 1 day window should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_escrow(
            &buyer,
            &seller,
            &token_addr,
            &1_000_000,
            &1,
            &Some(ONE_DAY),
        );
    }));

    assert!(result.is_err());

    // But 7 days should work
    let escrow = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &2,
        &Some(SEVEN_DAYS),
    );

    assert_eq!(escrow.release_window, SEVEN_DAYS);
}

#[test]
#[should_panic(expected = "ReleaseWindowTooShort")]
fn test_set_min_release_window_to_zero_fails() {
    let (_, client, _, _, _, admin, _) = setup_test();

    // Try to set minimum to 0 (should fail)
    client.set_min_release_window(&0).unwrap();
}

#[test]
fn test_set_min_release_window_cannot_exceed_max() {
    let (_, client, _, _, _, admin, _) = setup_test();

    // Set max to 7 days
    client.set_max_release_window(&SEVEN_DAYS);

    // Try to set min to 30 days (should fail)
    let result = client.set_min_release_window(&(30 * ONE_DAY));
    assert!(result.is_err());
}

#[test]
fn test_min_and_max_window_boundaries() {
    let (_, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Set min to 1 hour and max to 7 days
    client.set_min_release_window(&ONE_HOUR).unwrap();
    client.set_max_release_window(&SEVEN_DAYS);

    // Test at minimum boundary
    let escrow1 = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(ONE_HOUR),
    );
    assert_eq!(escrow1.release_window, ONE_HOUR);

    // Test at maximum boundary
    let escrow2 = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &2,
        &Some(SEVEN_DAYS),
    );
    assert_eq!(escrow2.release_window, SEVEN_DAYS);

    // Test in the middle
    let three_days = 3 * ONE_DAY;
    let escrow3 = client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &3,
        &Some(three_days),
    );
    assert_eq!(escrow3.release_window, three_days);
}

#[test]
fn test_default_window_respects_minimum() {
    let (_, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Raise minimum to 14 days
    let fourteen_days = 14 * ONE_DAY;
    client.set_min_release_window(&fourteen_days).unwrap();

    // Create escrow with default window (7 days) - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_escrow(
            &buyer,
            &seller,
            &token_addr,
            &1_000_000,
            &1,
            &None, // Uses default 7 days
        );
    }));

    assert!(result.is_err());
}

#[test]
fn test_prevents_flash_auto_release_attack() {
    let (env, client, buyer, seller, token_addr, _, _) = setup_test();

    // Attacker tries to create escrow with 1 second window
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_escrow(
            &buyer,
            &seller,
            &token_addr,
            &1_000_000,
            &1,
            &Some(1),
        );
    }));

    // Should fail due to minimum window constraint
    assert!(result.is_err());

    // Even with minimum set to 1 hour, very short windows are prevented
    client.set_min_release_window(&ONE_HOUR).unwrap();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.create_escrow(
            &buyer,
            &seller,
            &token_addr,
            &1_000_000,
            &2,
            &Some(10), // 10 seconds
        );
    }));

    assert!(result.is_err());
}

#[test]
fn test_multiple_escrows_with_different_windows() {
    let (_, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Set min to 1 hour
    client.set_min_release_window(&ONE_HOUR).unwrap();

    // Create escrows with various valid windows
    let windows = vec![
        ONE_HOUR,
        2 * ONE_HOUR,
        ONE_DAY,
        3 * ONE_DAY,
        SEVEN_DAYS,
    ];

    for (i, window) in windows.iter().enumerate() {
        let escrow = client.create_escrow(
            &buyer,
            &seller,
            &token_addr,
            &1_000_000,
            &((i + 1) as u32),
            &Some(*window),
        );
        assert_eq!(escrow.release_window, *window);
    }
}

#[test]
fn test_min_window_persists_across_config_updates() {
    let (_, client, _, _, _, admin, _) = setup_test();

    // Set min window to 2 days
    let two_days = 2 * ONE_DAY;
    client.set_min_release_window(&two_days).unwrap();

    // Update other config (platform fee)
    client.update_platform_fee(&600);

    // Min window should still be 2 days
    let min_window = client.get_min_release_window();
    assert_eq!(min_window, two_days);

    // Update platform wallet
    let new_wallet = Address::generate(&client.env);
    client.update_platform_wallet(&new_wallet);

    // Min window should still be 2 days
    let min_window = client.get_min_release_window();
    assert_eq!(min_window, two_days);
}

#[test]
fn test_batch_create_respects_minimum_window() {
    let (env, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Set min to 2 days
    let two_days = 2 * ONE_DAY;
    client.set_min_release_window(&two_days).unwrap();

    // Create batch with valid windows
    let mut batch_params = soroban_sdk::Vec::new(&env);
    for i in 0..3 {
        batch_params.push_back(crate::CreateEscrowParams {
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token_addr.clone(),
            amount: 1_000_000,
            order_id: i + 1,
            release_window: Some(two_days),
            ipfs_hash: None,
            metadata_hash: None,
        });
    }

    let results = client.create_escrows_batch(&batch_params).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn test_batch_create_fails_with_below_minimum_window() {
    let (env, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Create batch with window below minimum (1 hour < 1 day)
    let mut batch_params = soroban_sdk::Vec::new(&env);
    batch_params.push_back(crate::CreateEscrowParams {
        buyer: buyer.clone(),
        seller: seller.clone(),
        token: token_addr.clone(),
        amount: 1_000_000,
        order_id: 1,
        release_window: Some(ONE_HOUR),
        ipfs_hash: None,
        metadata_hash: None,
    });

    let result = client.create_escrows_batch(&batch_params);
    assert!(result.is_err());
}

#[test]
fn test_reasonable_minimum_windows() {
    let (_, client, buyer, seller, token_addr, admin, _) = setup_test();

    // Test various reasonable minimum windows
    let reasonable_minimums = vec![
        ONE_HOUR,           // 1 hour
        6 * ONE_HOUR,       // 6 hours
        12 * ONE_HOUR,      // 12 hours
        ONE_DAY,            // 1 day
        2 * ONE_DAY,        // 2 days
        7 * ONE_DAY,        // 1 week
    ];

    for min_window in reasonable_minimums {
        client.set_min_release_window(&min_window).unwrap();
        
        let retrieved_min = client.get_min_release_window();
        assert_eq!(retrieved_min, min_window);

        // Create escrow with this minimum
        let escrow = client.create_escrow(
            &buyer,
            &seller,
            &token_addr,
            &1_000_000,
            &1,
            &Some(min_window),
        );
        assert_eq!(escrow.release_window, min_window);
    }
}

#[test]
fn test_min_window_prevents_immediate_auto_release() {
    let (env, client, buyer, seller, token_addr, _, platform_wallet) = setup_test();
    let token = token::Client::new(&env, &token_addr);

    // Create escrow with minimum window (1 day)
    client.create_escrow(
        &buyer,
        &seller,
        &token_addr,
        &1_000_000,
        &1,
        &Some(ONE_DAY),
    );

    // Try to auto-release immediately (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.auto_release(&1);
    }));
    assert!(result.is_err());

    // Fast forward 1 second (still too early)
    env.ledger().with_mut(|li| {
        li.timestamp += 1;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.auto_release(&1);
    }));
    assert!(result.is_err());

    // Fast forward to just before the window (still too early)
    env.ledger().with_mut(|li| {
        li.timestamp += ONE_DAY as u64 - 2;
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.auto_release(&1);
    }));
    assert!(result.is_err());

    // Fast forward past the window (should work now)
    env.ledger().with_mut(|li| {
        li.timestamp += 2;
    });

    client.auto_release(&1);

    // Verify funds were released
    let expected_fee = 50_000i128; // 5% of 1,000,000
    let expected_seller_amount = 1_000_000 - expected_fee;
    assert_eq!(token.balance(&seller), expected_seller_amount);
    assert_eq!(token.balance(&platform_wallet), expected_fee);
}
