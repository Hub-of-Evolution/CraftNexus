use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

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
    client.update_user_metrics(&user, &5u32, &10_000_000_000i128);

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
    client.update_user_metrics(&user, &1u32, &1i128);
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
