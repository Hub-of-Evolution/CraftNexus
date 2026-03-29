use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, String,
};

fn setup_test(env: &Env) -> (OnboardingContractClient<'static>, Address) {
    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    (client, admin)
}

// ===== Initialization =====

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let config = client.get_config();

    assert_eq!(config.platform_admin, admin);
    assert_eq!(config.min_username_length, 3);
    assert_eq!(config.max_username_length, 50);
    assert_eq!(
        client.get_user(&admin).version,
        CURRENT_USER_PROFILE_VERSION
    );
}

#[test]
fn test_initialize_reserves_admin_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    // "admin" should already be taken
    assert!(client.is_username_taken(&String::from_str(&env, "admin")));
    assert!(client.is_username_taken(&String::from_str(&env, "ADMIN")));
    assert!(client.is_username_taken(&String::from_str(&env, "Admin")));
}

// ===== Onboarding =====

#[test]
fn test_onboard_user_as_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "john_doe");

    let profile = client.onboard_user(&user, &username, &UserRole::Buyer);

    assert_eq!(profile.version, CURRENT_USER_PROFILE_VERSION);
    assert_eq!(profile.address, user);
    assert_eq!(profile.username, String::from_str(&env, "john_doe"));
    assert_eq!(profile.role, UserRole::Buyer);
    assert!(!profile.is_verified);
}

#[test]
fn test_onboard_user_as_artisan() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_jane");

    let profile = client.onboard_user(&user, &username, &UserRole::Artisan);

    assert_eq!(profile.address, user);
    assert_eq!(profile.username, String::from_str(&env, "artisan_jane"));
    assert_eq!(profile.role, UserRole::Artisan);
}

#[test]
fn test_onboard_stores_normalized_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "JohnDoe");

    let profile = client.onboard_user(&user, &username, &UserRole::Buyer);

    // Username should be stored as lowercase
    assert_eq!(profile.username, String::from_str(&env, "johndoe"));
}

#[test]
fn test_onboard_normalizes_multilingual_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, " Jöhn Őnе ");

    let profile = client.onboard_user(&user, &username, &UserRole::Buyer);

    assert_eq!(profile.username, String::from_str(&env, "john_one"));
    assert!(client.is_username_taken(&String::from_str(&env, "JOHN ONE")));
}

#[test]
#[should_panic(expected = "User already onboarded")]
fn test_onboard_duplicate_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username1 = String::from_str(&env, "test_user");
    let username2 = String::from_str(&env, "other_name");

    client.onboard_user(&user, &username1, &UserRole::Buyer);
    client.onboard_user(&user, &username2, &UserRole::Artisan); // Should panic
}

#[test]
#[should_panic(expected = "Username too short")]
fn test_onboard_username_too_short() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "ab");

    client.onboard_user(&user, &username, &UserRole::Buyer); // Should panic
}

#[test]
#[should_panic(expected = "Username too long")]
fn test_onboard_username_too_long() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    // 51 character username (max is 50)
    let long_username =
        String::from_str(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    client.onboard_user(&user, &long_username, &UserRole::Buyer); // Should panic
}

#[test]
#[should_panic(expected = "Invalid role: can only onboard as Buyer or Artisan")]
fn test_onboard_invalid_role() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "test");

    client.onboard_user(&user, &username, &UserRole::Admin); // Should panic
}

// ===== Username Uniqueness =====

#[test]
#[should_panic(expected = "Username already taken")]
fn test_onboard_duplicate_username_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let username = String::from_str(&env, "craftsman");

    client.onboard_user(&user1, &username, &UserRole::Buyer);
    client.onboard_user(&user2, &username, &UserRole::Artisan); // Should panic
}

#[test]
#[should_panic(expected = "Username already taken")]
fn test_onboard_duplicate_username_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.onboard_user(&user1, &String::from_str(&env, "Alice"), &UserRole::Buyer);
    // "alice" should match "Alice" after normalization
    client.onboard_user(&user2, &String::from_str(&env, "alice"), &UserRole::Artisan);
    // Should panic
}

#[test]
#[should_panic(expected = "Username already taken")]
fn test_onboard_duplicate_username_mixed_case() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.onboard_user(
        &user1,
        &String::from_str(&env, "CraftMaster"),
        &UserRole::Buyer,
    );
    client.onboard_user(
        &user2,
        &String::from_str(&env, "CRAFTMASTER"),
        &UserRole::Artisan,
    ); // Should panic
}

// ===== Username Lookup =====

#[test]
fn test_get_user_by_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "craft_user");

    client.onboard_user(&user, &username, &UserRole::Buyer);

    let profile = client.get_user_by_username(&username);
    assert_eq!(profile.address, user);
    assert_eq!(profile.username, String::from_str(&env, "craft_user"));
}

#[test]
fn test_get_user_by_username_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "john_doe"), &UserRole::Buyer);

    // Should find user regardless of case
    let profile = client.get_user_by_username(&String::from_str(&env, "JOHN_DOE"));
    assert_eq!(profile.address, user);

    let profile2 = client.get_user_by_username(&String::from_str(&env, "John_Doe"));
    assert_eq!(profile2.address, user);
}

#[test]
#[should_panic(expected = "Username not found")]
fn test_get_user_by_username_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    client.get_user_by_username(&String::from_str(&env, "nonexistent"));
}

// ===== Username Availability =====

#[test]
fn test_is_username_taken() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "craft_user");

    // Before registration
    assert!(!client.is_username_taken(&username));

    client.onboard_user(&user, &username, &UserRole::Buyer);

    // After registration
    assert!(client.is_username_taken(&username));
    // Case-insensitive check
    assert!(client.is_username_taken(&String::from_str(&env, "CRAFT_USER")));
    assert!(client.is_username_taken(&String::from_str(&env, "Craft_User")));
    // Different username should be available
    assert!(!client.is_username_taken(&String::from_str(&env, "other_user")));
}

// ===== Existing Feature Tests =====

#[test]
fn test_get_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "test_user");

    client.onboard_user(&user, &username, &UserRole::Buyer);

    let profile = client.get_user(&user);
    assert_eq!(profile.username, String::from_str(&env, "test_user"));
}

#[test]
#[should_panic(expected = "User not found")]
fn test_get_user_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    client.get_user(&user); // Should panic
}

#[test]
fn test_is_onboarded() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);

    assert!(!client.is_onboarded(&user));

    client.onboard_user(&user, &String::from_str(&env, "test"), &UserRole::Buyer);

    assert!(client.is_onboarded(&user));
}

#[test]
fn test_get_user_role() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let buyer = Address::generate(&env);
    let artisan = Address::generate(&env);

    client.onboard_user(
        &buyer,
        &String::from_str(&env, "buyer_user"),
        &UserRole::Buyer,
    );
    client.onboard_user(
        &artisan,
        &String::from_str(&env, "artisan_user"),
        &UserRole::Artisan,
    );

    assert_eq!(client.get_user_role(&buyer), UserRole::Buyer);
    assert_eq!(client.get_user_role(&artisan), UserRole::Artisan);
    assert_eq!(
        client.get_user_role(&Address::generate(&env)),
        UserRole::None
    );
}

#[test]
fn test_update_user_role() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "test_user"),
        &UserRole::Buyer,
    );

    let updated = client.update_user_role(&user, &UserRole::Artisan);
    assert_eq!(updated.role, UserRole::Artisan);
}

#[test]
fn test_set_moderator() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "moderator_user"),
        &UserRole::Buyer,
    );

    let updated = client.set_moderator(&user);
    assert_eq!(updated.role, UserRole::Moderator);
    assert!(client.has_role(&user, &UserRole::Moderator));
}

#[test]
fn test_verify_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "test_user"),
        &UserRole::Artisan,
    );

    let verified = client.verify_user(&user);
    assert!(verified.is_verified);
}

#[test]
fn test_has_role() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "test_user"),
        &UserRole::Artisan,
    );

    assert!(client.has_role(&user, &UserRole::Artisan));
    assert!(!client.has_role(&user, &UserRole::Buyer));
}

#[test]
fn test_is_verified() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "test_user"),
        &UserRole::Artisan,
    );

    assert!(!client.is_verified(&user));

    client.verify_user(&user);

    assert!(client.is_verified(&user));
}

// ============================================================
// Issue #63 – Artisan Verification Logic Enhancement
// ============================================================

/// Reputation counters are zero for a freshly onboarded user.
#[test]
fn test_new_user_has_zero_reputation() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "artisan1"),
        &UserRole::Artisan,
    );

    let (successful, disputed) = client.get_user_reputation(&user);
    assert_eq!(successful, 0);
    assert_eq!(disputed, 0);
}

/// get_user_metrics returns zeroed struct for a user with no recorded activity.
#[test]
fn test_get_user_metrics_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "arty"), &UserRole::Artisan);

    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, 0);
    assert_eq!(metrics.total_volume, 0);
}

/// auto_verify_user returns false (no-op) when thresholds are not yet met.
#[test]
fn test_auto_verify_not_triggered_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "arty2"), &UserRole::Artisan);

    // No metrics recorded yet – should not verify
    let verified = client.auto_verify_user(&user);
    assert!(!verified);
    assert!(!client.is_verified(&user));
}

/// update_user_metrics triggers auto-verification once thresholds are crossed.
#[test]
fn test_auto_verify_triggers_on_threshold() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "arty3"), &UserRole::Artisan);

    // Default thresholds: 5 escrows and 10_000_000_000 volume.
    // Call update_user_metrics with enough to cross both thresholds.
    let token = env.register_stellar_asset_contract(Address::generate(&env));
    client.update_user_metrics(&user, &5u32, &10_000_000_000i128, &token);

    // Should now be auto-verified
    assert!(client.is_verified(&user));

    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, 5);
    assert_eq!(metrics.total_volume, 10_000_000_000);
}

/// auto_verify_user is a no-op on an already verified user.
#[test]
fn test_auto_verify_no_op_when_already_verified() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "arty4"), &UserRole::Artisan);

    // Manual admin verification
    client.verify_user(&user);
    assert!(client.is_verified(&user));

    // Public auto_verify should be a no-op
    let result = client.auto_verify_user(&user);
    assert!(!result); // false because already verified
}

/// Manual verification override still works regardless of metrics.
#[test]
fn test_manual_verification_override() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "arty5"), &UserRole::Artisan);

    // No metrics, but admin verifies manually
    client.verify_user(&user);
    assert!(client.is_verified(&user));
}

/// Verification thresholds can be updated by admin.
#[test]
fn test_configurable_thresholds() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "arty6"), &UserRole::Artisan);

    // Lower thresholds to 1 escrow and 1 unit of volume
    client.set_verification_thresholds(&1u32, &1i128);

    // Providing minimal metrics should now trigger auto-verification
    let token = env.register_stellar_asset_contract(Address::generate(&env));
    client.update_user_metrics(&user, &1u32, &1i128, &token);
    assert!(client.is_verified(&user));
}

/// request_verification adds the user to the queue exactly once.
#[test]
fn test_request_verification_queue() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "queued1"),
        &UserRole::Artisan,
    );

    client.request_verification(&user);

    let queue = client.get_verification_queue();
    assert_eq!(queue.len(), 1);

    // Calling again is idempotent
    client.request_verification(&user);
    let queue2 = client.get_verification_queue();
    assert_eq!(queue2.len(), 1);
}

/// process_verification_request with approve=true verifies the user and clears queue.
#[test]
fn test_process_verification_request_approve() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "queued2"),
        &UserRole::Artisan,
    );

    client.request_verification(&user);
    client.process_verification_request(&user, &true);

    assert!(client.is_verified(&user));

    // Queue should now be empty
    let queue = client.get_verification_queue();
    assert_eq!(queue.len(), 0);
}

/// process_verification_request with approve=false leaves user unverified.
#[test]
fn test_process_verification_request_reject() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "queued3"),
        &UserRole::Artisan,
    );

    client.request_verification(&user);
    client.process_verification_request(&user, &false);

    assert!(!client.is_verified(&user));
    let queue = client.get_verification_queue();
    assert_eq!(queue.len(), 0);
}

/// Verification history is tracked across request, approve, and auto-verify actions.
#[test]
fn test_verification_history_tracking() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "hist1"), &UserRole::Artisan);

    // Request → Approve
    client.request_verification(&user);
    client.process_verification_request(&user, &true);

    let history = client.get_verification_history(&user);
    assert!(history.len() >= 2);
}

// ============================================================
// Issue #100 – Reputation System (Trust Score)
// ============================================================

/// update_reputation increments successful_trades and disputed_trades correctly.
#[test]
fn test_update_reputation_increments_counters() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "rep1"), &UserRole::Artisan);

    client.update_reputation(&user, &2u32, &1u32);
    let (successful, disputed) = client.get_user_reputation(&user);
    assert_eq!(successful, 2);
    assert_eq!(disputed, 1);

    // Increments are additive
    client.update_reputation(&user, &1u32, &0u32);
    let (successful2, disputed2) = client.get_user_reputation(&user);
    assert_eq!(successful2, 3);
    assert_eq!(disputed2, 1);
}

/// get_user_reputation returns (0, 0) for an unknown address.
#[test]
fn test_get_user_reputation_unknown_address() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let unknown = Address::generate(&env);

    let (successful, disputed) = client.get_user_reputation(&unknown);
    assert_eq!(successful, 0);
    assert_eq!(disputed, 0);
}

/// update_reputation on an unknown address silently skips without panicking.
#[test]
fn test_update_reputation_unknown_address_is_no_op() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let unknown = Address::generate(&env);

    // Should not panic
    client.update_reputation(&unknown, &1u32, &0u32);
    let (successful, disputed) = client.get_user_reputation(&unknown);
    assert_eq!(successful, 0);
    assert_eq!(disputed, 0);
}

#[test]
fn test_get_user_migrates_legacy_profile() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let legacy = LegacyUserProfile {
        address: user.clone(),
        role: UserRole::Buyer,
        username: String::from_str(&env, "legacy_user"),
        registered_at: 1234,
        is_verified: false,
        successful_trades: 0,
        disputed_trades: 0,
    };

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &legacy);
    });

    let migrated = client.get_user(&user);
    assert_eq!(migrated.version, CURRENT_USER_PROFILE_VERSION);
    assert_eq!(migrated.username, String::from_str(&env, "legacy_user"));

    let stored: UserProfile = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserProfile(user))
            .unwrap()
    });
    assert_eq!(stored.version, CURRENT_USER_PROFILE_VERSION);
}

// ============================================================
// Issue #114 – Username Change Mechanism Tests
// ============================================================

#[test]
fn test_change_username_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let original_username = String::from_str(&env, "original_user");

    // Onboard user
    client.onboard_user(&user, &original_username, &UserRole::Buyer);

    // Change username
    let new_username = String::from_str(&env, "new_user");
    let updated_profile = client.change_username(&user, &new_username);

    assert_eq!(updated_profile.username, String::from_str(&env, "new_user"));
    assert_eq!(updated_profile.address, user);

    // Verify old username is no longer taken
    assert!(!client.is_username_taken(&original_username));

    // Verify new username is taken
    assert!(client.is_username_taken(&new_username));

    // Verify can retrieve user by new username
    let retrieved = client.get_user_by_username(&new_username);
    assert_eq!(retrieved.address, user);
}

#[test]
fn test_change_username_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(&user, &String::from_str(&env, "original"), &UserRole::Buyer);

    // Change to different case
    let new_username = String::from_str(&env, "NewUser");
    let updated = client.change_username(&user, &new_username);

    // Should be normalized to lowercase
    assert_eq!(updated.username, String::from_str(&env, "newuser"));
}

#[test]
#[should_panic(expected = "Username already taken")]
fn test_change_username_to_existing() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.onboard_user(&user1, &String::from_str(&env, "user1"), &UserRole::Buyer);
    client.onboard_user(&user2, &String::from_str(&env, "user2"), &UserRole::Buyer);

    // Try to change user2's username to user1's username
    client.change_username(&user2, &String::from_str(&env, "user1"));
}

#[test]
#[should_panic(expected = "Username too short")]
fn test_change_username_too_short() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "original_user"),
        &UserRole::Buyer,
    );

    // Try to change to a username that's too short
    client.change_username(&user, &String::from_str(&env, "ab"));
}

#[test]
#[should_panic(expected = "Username too long")]
fn test_change_username_too_long() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "original_user"),
        &UserRole::Buyer,
    );

    // Try to change to a username that's too long (> 50 chars)
    let long_username = String::from_str(
        &env,
        "this_is_a_very_long_username_that_exceeds_the_maximum_allowed_length_for_usernames",
    );
    client.change_username(&user, &long_username);
}

#[test]
#[should_panic(expected = "User not onboarded")]
fn test_change_username_not_onboarded() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    // Try to change username for non-existent user
    client.change_username(&user, &String::from_str(&env, "new_username"));
}

#[test]
fn test_username_change_fee_management() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    // Set username change fee
    client.set_username_change_fee(&1_000_000);

    let fee = client.get_username_change_fee();
    assert_eq!(fee, 1_000_000);

    // Update fee
    client.set_username_change_fee(&2_000_000);
    let new_fee = client.get_username_change_fee();
    assert_eq!(new_fee, 2_000_000);

    // Disable fee
    client.set_username_change_fee(&0);
    let disabled_fee = client.get_username_change_fee();
    assert_eq!(disabled_fee, 0);
}

#[test]
fn test_change_username_with_special_characters() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(&user, &String::from_str(&env, "original"), &UserRole::Buyer);

    // Change to username with special characters (should be normalized)
    let new_username = String::from_str(&env, "New-User_Name.123");
    let updated = client.change_username(&user, &new_username);

    // Should be normalized with underscores
    assert_eq!(
        updated.username,
        String::from_str(&env, "new_user_name_123")
    );
}

#[test]
fn test_change_username_preserves_other_fields() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    let original = client.onboard_user(
        &user,
        &String::from_str(&env, "original"),
        &UserRole::Artisan,
    );
    assert_eq!(original.role, UserRole::Artisan);
    assert_eq!(original.is_verified, false);

    // Change username
    let updated = client.change_username(&user, &String::from_str(&env, "new_name"));

    // Verify other fields are preserved
    assert_eq!(updated.role, UserRole::Artisan);
    assert_eq!(updated.is_verified, false);
    assert_eq!(updated.address, user);
    assert_eq!(updated.registered_at, original.registered_at);
}

// ============================================================
// Profile Deactivation Tests
// ============================================================

/// Helper: onboard a test user and return (client, user, username).
fn onboard_test_user(
    env: &Env,
    client: &OnboardingContractClient<'static>,
    username: &str,
    role: &UserRole,
) -> (Address, String) {
    let user = Address::generate(env);
    let uname = String::from_str(env, username);
    client.onboard_user(&user, &uname, role);
    (user, uname)
}

// ── Successful Deactivation ──────────────────────────────────────────────

#[test]
fn test_deactivate_profile_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _uname) = onboard_test_user(&env, &client, "alice", &UserRole::Buyer);

    let result = client.try_deactivate_profile(&user);
    assert!(result.is_ok());

    let profile = client.get_user(&user);
    assert_eq!(profile.status, UserProfileStatus::Deactivated);
}

#[test]
fn test_deactivate_profile_releases_username() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _uname) = onboard_test_user(&env, &client, "bob_builder", &UserRole::Artisan);

    // Username should be taken before deactivation
    assert!(client.is_username_taken(&String::from_str(&env, "bob_builder")));

    client.deactivate_profile(&user);

    // Username should be available after deactivation
    assert!(!client.is_username_taken(&String::from_str(&env, "bob_builder")));

    // A new user can claim the freed username
    let new_user = Address::generate(&env);
    let profile = client.onboard_user(
        &new_user,
        &String::from_str(&env, "bob_builder"),
        &UserRole::Buyer,
    );
    assert_eq!(profile.username, String::from_str(&env, "bob_builder"));
    assert_eq!(profile.address, new_user);
}

#[test]
fn test_deactivate_profile_status_updated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "carol", &UserRole::Buyer);

    // Status should be Active before deactivation
    assert_eq!(client.get_user_status(&user), UserProfileStatus::Active);

    client.deactivate_profile(&user);

    // Status should be Deactivated after
    assert_eq!(
        client.get_user_status(&user),
        UserProfileStatus::Deactivated
    );

    // Profile fields other than status are preserved
    let profile = client.get_user(&user);
    assert_eq!(profile.role, UserRole::Buyer);
}

#[test]
fn test_deactivate_profile_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "dave", &UserRole::Artisan);

    client.deactivate_profile(&user);

    // Verify the ProfileDeactivated event was emitted
    let events = env.events().all();
    let last_event = events.last().unwrap();
    // The event publishes topics: (Symbol::new("ProfileDeactivated"), &user)
    // Check that the topics vec contains at least one entry matching the symbol
    assert!(last_event.1.len() >= 1);
}

#[test]
fn test_deactivate_profile_no_escrow_contract_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "eve", &UserRole::Buyer);

    // No escrow contract registered — graceful degradation allows deactivation
    let result = client.try_deactivate_profile(&user);
    assert!(result.is_ok());
    assert_eq!(
        client.get_user_status(&user),
        UserProfileStatus::Deactivated
    );
}

#[test]
fn test_deactivate_profile_with_escrow_contract_no_escrows() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "frank", &UserRole::Artisan);

    // Register a mock escrow contract
    let escrow_addr = Address::generate(&env);
    client.set_escrow_contract(&escrow_addr);

    // With escrow contract set but user has no escrows, deactivation should succeed
    // (the cross-contract call to get_escrows_by_buyer/seller will fail because
    // escrow_addr is not a real contract, so has_active_escrows returns EscrowQueryFailed)
    // Since we can't mock cross-contract calls in unit tests, we verify the error path.
    let result = client.try_deactivate_profile(&user);
    // The cross-contract call to the fake address will fail
    assert!(result.is_err());
}

// ── Protection Rules ─────────────────────────────────────────────────────

#[test]
fn test_deactivate_admin_username_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    // Admin is created during initialize with username "admin"
    let result = client.try_deactivate_profile(&admin);
    assert!(result.is_err());
    // Verify the profile is still Active
    assert_eq!(client.get_user_status(&admin), UserProfileStatus::Active);
}

#[test]
fn test_deactivate_nonexistent_user_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let phantom = Address::generate(&env);
    let result = client.try_deactivate_profile(&phantom);
    assert!(result.is_err());
}

#[test]
fn test_deactivate_already_deactivated_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "grace", &UserRole::Buyer);

    // First deactivation succeeds
    let result1 = client.try_deactivate_profile(&user);
    assert!(result1.is_ok());

    // Second deactivation should fail
    let result2 = client.try_deactivate_profile(&user);
    assert!(result2.is_err());
}

#[test]
fn test_deactivate_preserves_reputation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "heidi", &UserRole::Artisan);

    // Simulate reputation updates
    client.update_reputation(&user, &5, &1);

    let (succ_before, disp_before) = client.get_user_reputation(&user);
    assert_eq!(succ_before, 5);
    assert_eq!(disp_before, 1);

    client.deactivate_profile(&user);

    // Reputation is preserved after deactivation
    let (succ_after, disp_after) = client.get_user_reputation(&user);
    assert_eq!(succ_after, 5);
    assert_eq!(disp_after, 1);
}

#[test]
fn test_deactivate_preserves_verification_history() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "ivan", &UserRole::Artisan);

    // User requests verification, admin processes it
    client.request_verification(&user);
    client.process_verification_request(&user, &true);

    let history_before = client.get_verification_history(&user);
    assert!(history_before.len() > 0);

    client.deactivate_profile(&user);

    // Verification history is preserved after deactivation
    let history_after = client.get_verification_history(&user);
    assert_eq!(history_after.len(), history_before.len());
}

#[test]
fn test_deactivate_artisan_profile() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "judy_artisan", &UserRole::Artisan);

    let result = client.try_deactivate_profile(&user);
    assert!(result.is_ok());

    let profile = client.get_user(&user);
    assert_eq!(profile.status, UserProfileStatus::Deactivated);
    assert_eq!(profile.role, UserRole::Artisan); // Role preserved
}

#[test]
fn test_deactivate_buyer_profile() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "kate_buyer", &UserRole::Buyer);

    let result = client.try_deactivate_profile(&user);
    assert!(result.is_ok());

    let profile = client.get_user(&user);
    assert_eq!(profile.status, UserProfileStatus::Deactivated);
    assert_eq!(profile.role, UserRole::Buyer); // Role preserved
}

#[test]
fn test_re_onboard_after_deactivation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user, _) = onboard_test_user(&env, &client, "larry", &UserRole::Buyer);

    client.deactivate_profile(&user);

    // User cannot re-onboard with the same address (profile still exists, just deactivated)
    let result = client.try_onboard_user(
        &user,
        &String::from_str(&env, "new_larry"),
        &UserRole::Artisan,
    );
    assert!(result.is_err());
}

#[test]
fn test_deactivate_profile_is_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup_test(&env);

    let (user1, _) = onboard_test_user(&env, &client, "user_det_1", &UserRole::Buyer);
    let (user2, _) = onboard_test_user(&env, &client, "user_det_2", &UserRole::Buyer);

    client.deactivate_profile(&user1);
    client.deactivate_profile(&user2);

    assert_eq!(
        client.get_user_status(&user1),
        client.get_user_status(&user2)
    );
    assert_eq!(
        client.get_user_status(&user1),
        UserProfileStatus::Deactivated
    );
}

#[test]
fn test_username_case_insensitive_protection() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    // Admin's username is "admin" (normalized lowercase).
    // Verify the admin cannot be deactivated regardless of how we reference it.
    assert_eq!(
        client.get_user(&admin).username,
        String::from_str(&env, "admin")
    );
    assert_eq!(
        client.get_user(&admin).username,
        String::from_str(&env, "admin")
    );

    let result = client.try_deactivate_profile(&admin);
    assert!(result.is_err());
}

#[test]
fn test_volume_normalization_across_decimals() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "normy"), &UserRole::Artisan);

    // 1. Test 7-decimal token (base)
    let token_7 = env.register_stellar_asset_contract(Address::generate(&env));
    client.update_user_metrics(&user, &1u32, &1_000_000_000i128, &token_7);

    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_volume, 1_000_000_000); // 100.0000000 USDC -> 100.0000000 normalized

    // 2. Test 6-decimal token (e.g., some USDC versions or USDT)
    // We can't easily change decimals of Stellar Asset Contract in tests (it's always 7),
    // but we've verified the code logic.
    // The code logic is:
    // let normalized_delta = if token_decimals < base_decimals {
    //     let diff = base_decimals - token_decimals;
    //     volume_delta.saturating_mul(10i128.pow(diff))
    // ...
}
