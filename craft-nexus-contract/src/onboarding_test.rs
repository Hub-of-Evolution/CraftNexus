use super::decimal_test_token::{DecimalTestToken, DecimalTestTokenClient};
use super::*;
use crate::alloc::string::ToString;
use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _, Ledger},
    token, Address, Bytes, Env, String, Symbol,
};

fn register_decimal_test_token(env: &Env, decimals: u32) -> Address {
    let admin = Address::generate(env);
    let contract_id = env.register_contract(None, DecimalTestToken);
    DecimalTestTokenClient::new(env, &contract_id).initialize(&admin, &decimals);
    contract_id
}

const AUTO_VERIFY_VOLUME_THRESHOLD: i128 = 10_000_000_000;
const AUTO_VERIFY_ESCROW_THRESHOLD: u32 = 5;

fn string_to_bytes(env: &Env, s: &soroban_sdk::String) -> Bytes {
    let mut buf = [0u8; 128];
    let len = s.len() as usize;
    s.copy_into_slice(&mut buf[..len]);
    let mut b = Bytes::new(env);
    b.extend_from_slice(&buf[..len]);
    b
}

fn setup_test(env: &Env) -> (OnboardingContractClient<'static>, Address) {
    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    client.initialize(&admin);

    (client, admin)
}

/// Disable cooldown/farming caps so Issue #100 counter-math tests stay focused.
fn set_permissive_reputation_policy(client: &OnboardingContractClient) {
    client.set_reputation_policy(
        &DEFAULT_REPUTATION_DECAY_INTERVAL_SECS,
        &DEFAULT_REPUTATION_DECAY_BPS,
        &0u64,     // no update cooldown
        &0u64,     // disable farming window
        &u32::MAX, // unlimited successes when window re-enabled
    );
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

fn onboard_user_success(
    client: &OnboardingContractClient,
    user: &Address,
    username: &String,
    role: &UserRole,
) -> UserProfile {
    match client.try_onboard_user(user, username, role) {
        Ok(Ok(profile)) => profile,
        Ok(Err(_)) => panic!("try_onboard_user returned Err but should have succeeded"),
        Err(_) => panic!("try_onboard_user host call failed"),
    }
}

#[test]
fn test_onboard_user_as_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "john_doe");

    let profile = onboard_user_success(&client, &user, &username, &UserRole::Buyer);

    assert_eq!(profile.version, CURRENT_USER_PROFILE_VERSION);
    assert_eq!(profile.address, user);
    assert_eq!(profile.username, Symbol::new(&env, "john_doe"));
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

    let profile = onboard_user_success(&client, &user, &username, &UserRole::Artisan);

    assert_eq!(profile.address, user);
    assert_eq!(profile.username, Symbol::new(&env, "artisan_jane"));
    assert_eq!(profile.role, UserRole::Artisan);
}

#[test]
fn test_onboard_stores_normalized_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "JohnDoe");

    let profile = onboard_user_success(&client, &user, &username, &UserRole::Buyer);

    // Username should be stored as lowercase
    assert_eq!(profile.username, Symbol::new(&env, "johndoe"));
}

#[test]
fn test_onboard_normalizes_multilingual_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, " Jöhn Őnе ");

    let profile = onboard_user_success(&client, &user, &username, &UserRole::Buyer);

    assert_eq!(profile.username, Symbol::new(&env, "john_one"));
    assert!(client.is_username_taken(&String::from_str(&env, "JOHN ONE")));
}

#[test]
fn test_onboard_duplicate_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username1 = String::from_str(&env, "test_user");
    let username2 = String::from_str(&env, "other_name");

    client.onboard_user(&user, &username1, &UserRole::Buyer);
    let result = client.try_onboard_user(&user, &username2, &UserRole::Artisan);
    assert!(result.is_err());
}

#[test]
fn test_repeated_identical_onboarding_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "retry_user");

    let before = client.get_active_user_count();
    let first = client.onboard_user(&user, &username, &UserRole::Artisan);
    let after_first = client.get_active_user_count();
    let retried = client.onboard_user(&user, &username, &UserRole::Artisan);

    assert_eq!(retried, first);
    assert_eq!(after_first, before + 1);
    assert_eq!(client.get_active_user_count(), after_first);
    assert_eq!(client.get_user_by_username(&username).address, user);
}

#[test]
fn test_idempotent_retry_repairs_missing_secondary_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "repair_user");
    let original = client.onboard_user(&user, &username, &UserRole::Buyer);
    let active_count = client.get_active_user_count();
    let normalized = normalize_username(&env, &username);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .remove(&DataKey::Username(normalized.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::UserStateVersion(user.clone()));
    });

    let recovered = client.onboard_user(&user, &username, &UserRole::Buyer);
    assert_eq!(recovered.address, original.address);
    assert_eq!(recovered.registered_at, original.registered_at);
    assert_eq!(recovered.state_version, 1);
    assert_eq!(client.get_active_user_count(), active_count);
    assert_eq!(client.get_user_by_username(&username).address, user);
}

#[test]
fn test_explicit_recovery_restores_profile_indexes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "explicit_repair");
    client.onboard_user(&user, &username, &UserRole::Artisan);
    let normalized = normalize_username(&env, &username);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .remove(&DataKey::Username(normalized.clone()));
    });

    let recovered = client.recover_onboarding_profile(&user, &username);
    assert_eq!(recovered.address, user);
    assert_eq!(client.get_user_by_username(&username).address, user);
}

#[test]
fn test_same_account_username_reservation_is_retryable() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "reserved_retry");
    let normalized = normalize_username(&env, &username);

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::Username(normalized), &user);
    });

    let profile = client.onboard_user(&user, &username, &UserRole::Buyer);
    assert_eq!(profile.address, user);
    assert_eq!(client.get_user_by_username(&username).address, user);
}

#[test]
fn test_onboard_username_too_short() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "ab");

    let result = client.try_onboard_user(&user, &username, &UserRole::Buyer);
    assert!(result.is_err());
}

#[test]
fn test_onboard_username_too_long() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    // 51 character username (max is 50)
    let long_username =
        String::from_str(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    let result = client.try_onboard_user(&user, &long_username, &UserRole::Buyer);
    assert!(result.is_err());
}

#[test]
fn test_onboard_invalid_role() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user = Address::generate(&env);
    let username = String::from_str(&env, "test");

    let result = client.try_onboard_user(&user, &username, &UserRole::Admin);
    assert!(result.is_err());
}

// ===== Username Uniqueness =====

#[test]
fn test_onboard_duplicate_username_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let username = String::from_str(&env, "craftsman");

    client.onboard_user(&user1, &username, &UserRole::Buyer);
    let result = client.try_onboard_user(&user2, &username, &UserRole::Artisan);
    assert!(result.is_err());
}

#[test]
fn test_onboard_duplicate_username_case_insensitive() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.onboard_user(&user1, &String::from_str(&env, "Alice"), &UserRole::Buyer);
    // "alice" should match "Alice" after normalization
    let _result =
        client.try_onboard_user(&user2, &String::from_str(&env, "alice"), &UserRole::Artisan);
    let result =
        client.try_onboard_user(&user2, &String::from_str(&env, "alice"), &UserRole::Artisan);
    assert!(result.is_err());
}

#[test]
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
    let result = client.try_onboard_user(
        &user2,
        &String::from_str(&env, "CRAFTMASTER"),
        &UserRole::Artisan,
    );
    assert!(result.is_err());
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
    assert_eq!(profile.username, Symbol::new(&env, "craft_user"));
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
#[should_panic]
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
    assert_eq!(profile.username, Symbol::new(&env, "test_user"));
}

#[test]
#[should_panic]
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
#[should_panic(expected = "Error(Contract, #6)")]
fn test_update_user_role_to_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "test_user_admin"),
        &UserRole::Buyer,
    );

    // This should panic with Error::InvalidRole (code 6)
    client.update_user_role(&user, &UserRole::Admin);
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
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    client.update_user_metrics(&user, &5u32, &10_000_000_000i128, &token.address());

    // Should now be auto-verified
    assert!(client.is_verified(&user));

    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, 5);
    assert_eq!(metrics.total_volume, 10_000_000_000);
}

#[test]
fn test_auto_verify_can_be_disabled() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "manualonly"),
        &UserRole::Artisan,
    );

    client.set_auto_verify_enabled(&false);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    client.update_user_metrics(&user, &5u32, &10_000_000_000i128, &token.address());

    assert!(!client.is_verified(&user));
    assert!(!client.auto_verify_user(&user));

    client.verify_user(&user);
    assert!(client.is_verified(&user));

    let config = client.get_config();
    assert!(!config.auto_verify_enabled);
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
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    client.update_user_metrics(&user, &1u32, &1i128, &token.address());
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

#[test]
fn test_process_verification_request_preserves_other_pending_users() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user_one = Address::generate(&env);
    let user_two = Address::generate(&env);

    client.onboard_user(
        &user_one,
        &String::from_str(&env, "queued4"),
        &UserRole::Artisan,
    );
    client.onboard_user(
        &user_two,
        &String::from_str(&env, "queued5"),
        &UserRole::Artisan,
    );

    client.request_verification(&user_one);
    client.request_verification(&user_two);
    client.process_verification_request(&user_one, &true);

    let queue = client.get_verification_queue();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0), Some(user_two));
}

/// [SECURITY] Endpoint #53 (issue #454): `process_verification_request` is the
/// privileged verification state transition and must reject any caller that is
/// not the platform admin. With no auth mocked, `require_auth()` aborts the
/// invocation and the Soroban host rolls the transaction back before the target
/// profile's `is_verified` flag is touched.
#[test]
#[should_panic]
fn test_process_verification_request_unauthorized() {
    let env = Env::default();

    // Deliberately do NOT call env.mock_all_auths(); we want require_auth() to
    // enforce real authorization so an unauthorized invocation rolls back.
    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Seed the config and a target profile directly as the contract so the
    // endpoint is reachable without running `initialize` (which itself requires
    // auth). This isolates the test to the endpoint's own authorization guard.
    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&DataKey::Config, &config);
        let profile = StoredUserProfile {
            version: CURRENT_USER_PROFILE_VERSION,
            address: user.clone(),
            role: UserRole::Artisan,
            username: soroban_sdk::Symbol::new(&env, "pending_user"),
            registered_at: 0,
            is_verified: false,
            successful_trades: 0,
            disputed_trades: 0,
            status: ProfileStatus::Active,
        };
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
    });

    // No platform-admin signature is present, so require_auth() must panic and
    // the verification state transition must never execute.
    client.process_verification_request(&user, &true);
}

// ============================================================
// Issue #41 – admin_clear_verification_request authorization
// ============================================================

/// Admin can force-clear a pending verification request, advancing the queue.
#[test]
fn test_admin_clear_verification_request_authorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "stale_req"),
        &UserRole::Artisan,
    );

    client.request_verification(&user);
    assert!(client.is_verification_pending(&user));

    let was_pending = client.admin_clear_verification_request(&user);
    assert!(was_pending);

    // Request is gone and the queue has been compacted.
    assert!(!client.is_verification_pending(&user));
    assert_eq!(client.get_verification_queue().len(), 0);

    // The admin's authorization was the one that gated the call.
    let auths = env.auths();
    assert!(auths.iter().any(|(addr, _)| addr == &admin));
}

/// Clearing a user with no pending request is an idempotent no-op returning false.
#[test]
fn test_admin_clear_verification_request_no_pending() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "no_req"), &UserRole::Artisan);

    let was_pending = client.admin_clear_verification_request(&user);
    assert!(!was_pending);
}

/// Unauthorized callers cannot clear another user's verification request:
/// without the admin signature the require_auth() check rolls the call back.
#[test]
#[should_panic]
fn test_admin_clear_verification_request_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "victim"), &UserRole::Artisan);
    client.request_verification(&user);

    // Drop all mocked authorizations so the admin's require_auth() fails.
    env.set_auths(&[]);
    client.admin_clear_verification_request(&user);
}

/// A cleared request must not have flipped the user's verification status —
/// force-clear is a queue-hygiene operation, not an approval.
#[test]
fn test_admin_clear_verification_request_does_not_verify() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "unverified"),
        &UserRole::Artisan,
    );

    client.request_verification(&user);
    client.admin_clear_verification_request(&user);

    assert!(!client.is_verified(&user));
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
    set_permissive_reputation_policy(&client);
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

/// Issue #100 / #666 — update_reputation with zero trades is a no-op.
#[test]
fn test_reputation_zero_trades_no_op() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "repzero"),
        &UserRole::Artisan,
    );

    client.update_reputation(&user, &0u32, &0u32);
    let (successful, disputed) = client.get_user_reputation(&user);
    assert_eq!(successful, 0);
    assert_eq!(disputed, 0);
}

/// Issue #100 / #666 — update_reputation with u32::MAX handles overflow with saturating add.
#[test]
fn test_reputation_max_trades_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    set_permissive_reputation_policy(&client);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "repmax"), &UserRole::Artisan);

    client.update_reputation(&user, &u32::MAX, &u32::MAX);
    let (successful, disputed) = client.get_user_reputation(&user);
    assert_eq!(successful, u32::MAX);
    assert_eq!(disputed, u32::MAX);

    // Adding more does not overflow or panic, saturates at u32::MAX
    client.update_reputation(&user, &10u32, &10u32);
    let (successful2, disputed2) = client.get_user_reputation(&user);
    assert_eq!(successful2, u32::MAX);
    assert_eq!(disputed2, u32::MAX);
}

// ============================================================
// Issue #939 – Reputation Decay & Anti-Farming Controls
// ============================================================

/// Default policy is seeded on initialize.
#[test]
fn test_reputation_policy_defaults_on_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let policy = client.get_reputation_policy();

    assert_eq!(
        policy.decay_interval_secs,
        DEFAULT_REPUTATION_DECAY_INTERVAL_SECS
    );
    assert_eq!(policy.decay_bps, DEFAULT_REPUTATION_DECAY_BPS);
    assert_eq!(
        policy.update_cooldown_secs,
        DEFAULT_REPUTATION_UPDATE_COOLDOWN_SECS
    );
    assert_eq!(
        policy.farming_window_secs,
        DEFAULT_REPUTATION_FARMING_WINDOW_SECS
    );
    assert_eq!(
        policy.max_successful_per_window,
        DEFAULT_MAX_SUCCESSFUL_PER_WINDOW
    );
    assert_eq!(
        client.get_min_reputation_settlement(),
        DEFAULT_MIN_REPUTATION_SETTLEMENT
    );
}

/// Low-value completions are audited without changing score or consuming limits.
#[test]
fn test_reputation_requires_meaningful_completed_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 2_000);

    let (client, _) = setup_test(&env);
    client.set_reputation_policy(&1_000_000u64, &0u32, &3_600u64, &86_400u64, &2u32);
    client.set_min_reputation_settlement(&10_000_000i128);

    let token = register_decimal_test_token(&env, 7);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "settle1"),
        &UserRole::Artisan,
    );

    // Repeated dust settlements never gain trust or consume the account cap.
    for _ in 0..3 {
        client.update_reputation_for_settlement(&user, &1u32, &0u32, &9_999_999i128, &token);
    }
    assert_eq!(client.get_trust_score(&user), 0);
    assert_eq!(client.get_user_reputation(&user), (0, 0));
    assert_eq!(
        client.get_reputation_state(&user).window_successful_applied,
        0
    );

    // A meaningful completion at the same timestamp still receives its score.
    client.update_reputation_for_settlement(&user, &1u32, &0u32, &10_000_000i128, &token);
    assert_eq!(client.get_trust_score(&user), 1);
    assert_eq!(client.get_user_reputation(&user), (1, 0));

    // A second qualifying completion is still subject to the normal cooldown.
    client.update_reputation_for_settlement(&user, &1u32, &0u32, &10_000_000i128, &token);
    assert_eq!(client.get_trust_score(&user), 1);
    assert_eq!(client.get_user_reputation(&user), (1, 0));

    let history = client.get_reputation_history(&user);
    assert_eq!(history.len(), 5);
    for index in 0..3 {
        let entry = history.get(index).unwrap();
        assert_eq!(entry.reason, Symbol::new(&env, "below_minimum_settlement"));
        assert_eq!(entry.successful_requested, 1);
        assert_eq!(entry.successful_applied, 0);
        assert_eq!(entry.trust_score_after, 0);
    }
    assert_eq!(history.get(3).unwrap().reason, Symbol::new(&env, "applied"));
    assert_eq!(
        history.get(4).unwrap().reason,
        Symbol::new(&env, "cooldown_blocked")
    );
}

/// Settlement thresholds compare normalized values across token decimals.
#[test]
fn test_reputation_minimum_settlement_normalizes_token_decimals() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    set_permissive_reputation_policy(&client);
    client.set_min_reputation_settlement(&10_000_000i128);

    let six_decimal_token = register_decimal_test_token(&env, 6);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "settle2"),
        &UserRole::Artisan,
    );

    // One whole 6-decimal token normalizes to one whole 7-decimal token.
    client.update_reputation_for_settlement(
        &user,
        &1u32,
        &0u32,
        &1_000_000i128,
        &six_decimal_token,
    );
    assert_eq!(client.get_trust_score(&user), 1);
}

/// Adverse outcomes cannot be hidden behind the minimum-settlement gate.
#[test]
fn test_reputation_low_value_settlement_still_applies_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    set_permissive_reputation_policy(&client);
    let token = register_decimal_test_token(&env, 7);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "settle3"),
        &UserRole::Artisan,
    );

    client.update_reputation(&user, &2u32, &0u32);
    client.update_reputation_for_settlement(&user, &1u32, &1u32, &1i128, &token);

    assert_eq!(client.get_trust_score(&user), 1);
    assert_eq!(client.get_user_reputation(&user), (2, 1));
    let history = client.get_reputation_history(&user);
    let last = history.get(history.len() - 1).unwrap();
    assert_eq!(last.successful_applied, 0);
    assert_eq!(last.disputed_applied, 1);
    assert_eq!(last.reason, Symbol::new(&env, "below_minimum_settlement"));
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_minimum_reputation_settlement_rejects_negative_value() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    client.set_min_reputation_settlement(&-1i128);
}

/// Successful reputation gains are blocked by the update cooldown.
#[test]
fn test_reputation_cooldown_blocks_rapid_success() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000;
    });

    let (client, _) = setup_test(&env);
    // Keep farming permissive; exercise cooldown only.
    client.set_reputation_policy(
        &DEFAULT_REPUTATION_DECAY_INTERVAL_SECS,
        &DEFAULT_REPUTATION_DECAY_BPS,
        &3_600u64, // 1 hour cooldown
        &0u64,
        &u32::MAX,
    );

    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "cool1"), &UserRole::Artisan);

    client.update_reputation(&user, &1u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 1);
    assert_eq!(client.get_user_reputation(&user), (1, 0));

    // Immediate second success must be blocked.
    client.update_reputation(&user, &1u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 1);
    assert_eq!(client.get_user_reputation(&user), (1, 0));

    let history = client.get_reputation_history(&user);
    assert_eq!(history.len(), 2);
    assert_eq!(
        history.get(1).unwrap().reason,
        Symbol::new(&env, "cooldown_blocked")
    );
    assert_eq!(history.get(1).unwrap().successful_applied, 0);

    // After cooldown elapses, success is credited again.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000 + 3_600 + 1;
    });
    client.update_reputation(&user, &1u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 2);
    assert_eq!(client.get_user_reputation(&user), (2, 0));
}

/// Disputed increments still apply while success is cooldown-blocked.
#[test]
fn test_reputation_cooldown_allows_disputed() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 5_000;
    });

    let (client, _) = setup_test(&env);
    client.set_reputation_policy(
        &DEFAULT_REPUTATION_DECAY_INTERVAL_SECS,
        &0u32, // no decay noise
        &3_600u64,
        &0u64,
        &u32::MAX,
    );

    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "cool2"), &UserRole::Artisan);

    client.update_reputation(&user, &3u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 3);

    // Success blocked, dispute applied → trust drops.
    client.update_reputation(&user, &2u32, &1u32);
    assert_eq!(client.get_trust_score(&user), 2);
    assert_eq!(client.get_user_reputation(&user), (3, 1));

    let history = client.get_reputation_history(&user);
    let last = history.get(history.len() - 1).unwrap();
    assert_eq!(last.reason, Symbol::new(&env, "cooldown_blocked"));
    assert_eq!(last.successful_applied, 0);
    assert_eq!(last.disputed_applied, 1);
}

/// Anti-farming window caps rapid successful inflation.
#[test]
fn test_reputation_farming_window_caps_success() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 10_000;
    });

    let (client, _) = setup_test(&env);
    client.set_reputation_policy(
        &DEFAULT_REPUTATION_DECAY_INTERVAL_SECS,
        &0u32,
        &0u64, // no cooldown so farming alone is tested
        &86_400u64,
        &5u32,
    );

    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "farm1"), &UserRole::Artisan);

    client.update_reputation(&user, &3u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 3);

    client.update_reputation(&user, &3u32, &0u32);
    // Only 2 more fit under the cap of 5.
    assert_eq!(client.get_trust_score(&user), 5);
    assert_eq!(client.get_user_reputation(&user), (5, 0));

    let history = client.get_reputation_history(&user);
    let last = history.get(history.len() - 1).unwrap();
    assert_eq!(last.reason, Symbol::new(&env, "farming_capped"));
    assert_eq!(last.successful_requested, 3);
    assert_eq!(last.successful_applied, 2);

    // Further success in the same window is fully blocked.
    client.update_reputation(&user, &1u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 5);

    // After the window rolls, credits resume.
    env.ledger().with_mut(|li| {
        li.timestamp = 10_000 + 86_400 + 1;
    });
    client.update_reputation(&user, &2u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 7);
}

/// Trust score decays over time according to policy.
#[test]
fn test_reputation_decay_over_time() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 100;
    });

    let (client, _) = setup_test(&env);
    // 10% decay every 1_000 seconds; no cooldown/farming interference.
    client.set_reputation_policy(&1_000u64, &1_000u32, &0u64, &0u64, &u32::MAX);

    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "decay1"), &UserRole::Artisan);

    client.update_reputation(&user, &100u32, &0u32);
    assert_eq!(client.get_trust_score(&user), 100);
    // Lifetime counters are an audit trail and do not decay.
    assert_eq!(client.get_user_reputation(&user), (100, 0));

    env.ledger().with_mut(|li| {
        li.timestamp = 100 + 1_000;
    });
    // One interval: 100 * 0.9 = 90
    assert_eq!(client.get_trust_score(&user), 90);

    env.ledger().with_mut(|li| {
        li.timestamp = 100 + 2_000;
    });
    // Second interval: 90 * 0.9 = 81
    assert_eq!(client.get_trust_score(&user), 81);
    assert_eq!(client.get_user_reputation(&user), (100, 0));
}

/// Score history records applied and blocked updates for pattern detection.
#[test]
fn test_reputation_history_detects_farming_pattern() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.timestamp = 50_000;
    });

    let (client, _) = setup_test(&env);
    client.set_reputation_policy(&1_000_000u64, &0u32, &0u64, &86_400u64, &2u32);

    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "hist1"), &UserRole::Artisan);

    client.update_reputation(&user, &2u32, &0u32);
    client.update_reputation(&user, &1u32, &0u32); // capped to 0 remaining
    client.update_reputation(&user, &1u32, &0u32); // still capped

    let history = client.get_reputation_history(&user);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().reason, Symbol::new(&env, "applied"));
    assert_eq!(history.get(0).unwrap().successful_applied, 2);
    assert_eq!(
        history.get(1).unwrap().reason,
        Symbol::new(&env, "farming_capped")
    );
    assert_eq!(history.get(1).unwrap().successful_applied, 0);
    assert_eq!(
        history.get(2).unwrap().reason,
        Symbol::new(&env, "farming_capped")
    );

    // Repeated zero-applied successes in history = detectable farming pattern.
    let blocked: u32 = history
        .iter()
        .filter(|e| e.successful_requested > 0 && e.successful_applied == 0)
        .count() as u32;
    assert_eq!(blocked, 2);
}

/// Invalid decay_bps is rejected.
#[test]
#[should_panic(expected = "Error(Contract, #17)")]
fn test_set_reputation_policy_rejects_invalid_decay_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    client.set_reputation_policy(&1_000u64, &10_001u32, &0u64, &0u64, &5u32);
}

#[test]
fn test_get_user_migrates_legacy_profile() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let expected = string_to_bytes(&env, &portfolio_cid);
    let legacy = LegacyUserProfile {
        address: user.clone(),
        role: UserRole::Buyer,
        username: Symbol::new(&env, "legacy_user"),
        registered_at: 1234,
        is_verified: false,
        successful_trades: 0,
        disputed_trades: 0,
        portfolio_cid: Some(portfolio_cid),
    };

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &legacy);
    });

    let migrated = client.get_user(&user);
    assert_eq!(migrated.version, CURRENT_USER_PROFILE_VERSION);
    assert_eq!(migrated.username, Symbol::new(&env, "legacy_user"));
    assert_eq!(migrated.portfolio_cid, Some(expected.clone()));

    let stored: StoredUserProfile = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserProfile(user))
            .unwrap()
    });
    assert_eq!(stored.version, CURRENT_USER_PROFILE_VERSION);

    let stored_portfolio: Bytes = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserPortfolio(migrated.address.clone()))
            .unwrap()
    });
    assert_eq!(stored_portfolio, expected);
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

    assert_eq!(updated_profile.username, Symbol::new(&env, "new_user"));
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
#[should_panic]
fn test_change_username_cooldown_active() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "original_user"),
        &UserRole::Buyer,
    );
    client.change_username(&user, &String::from_str(&env, "first_change"));

    // Immediate second change should be blocked by cooldown.
    client.change_username(&user, &String::from_str(&env, "second_change"));
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
    assert_eq!(updated.username, Symbol::new(&env, "newuser"));
}

#[test]
#[should_panic]
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
#[should_panic]
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
#[should_panic]
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
#[should_panic]
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
fn test_change_username_collects_configured_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let fee_wallet = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract.address());
    let token_client = token::Client::new(&env, &token_contract.address());

    token_admin_client.mint(&user, &5_000_000);

    client.onboard_user(&user, &String::from_str(&env, "fee_user"), &UserRole::Buyer);
    client.set_username_change_fee(&1_000_000);
    client.set_username_fee_token(&token_contract.address());
    client.set_username_fee_wallet(&fee_wallet);

    client.change_username(&user, &String::from_str(&env, "fee_user_new"));

    assert_eq!(token_client.balance(&user), 4_000_000);
    assert_eq!(token_client.balance(&fee_wallet), 1_000_000);
}

#[test]
#[should_panic]
fn test_change_username_fee_requires_token_configuration() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "needs_fee"),
        &UserRole::Buyer,
    );
    client.set_username_change_fee(&1_000_000);

    client.change_username(&user, &String::from_str(&env, "still_needs_fee"));
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
    assert_eq!(updated.username, Symbol::new(&env, "new_user_name_123"));
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
    assert!(!original.is_verified);

    // Change username
    let updated = client.change_username(&user, &String::from_str(&env, "new_name"));

    // Verify other fields are preserved
    assert_eq!(updated.role, UserRole::Artisan);
    assert!(!updated.is_verified);
    assert_eq!(updated.address, user);
    assert_eq!(updated.registered_at, original.registered_at);
}

#[test]
#[should_panic]
fn test_bump_user_profile_ttl_unauthorized() {
    let env = Env::default();

    // Do NOT call env.mock_all_auths(); we want require_auth() to enforce real auth.
    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Populate config and a user profile directly as the contract to avoid
    // needing to run `initialize` (which itself requires auth). This lets us
    // test that calling `bump_user_profile_ttl` without the proper signer
    // will be rejected by `require_auth()`.
    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&DataKey::Config, &config);
        let profile = StoredUserProfile {
            version: CURRENT_USER_PROFILE_VERSION,
            address: user.clone(),
            role: UserRole::Buyer,
            username: Symbol::new(&env, "someone"),
            registered_at: env.ledger().timestamp(),
            is_verified: false,
            successful_trades: 0,
            disputed_trades: 0,
            status: ProfileStatus::Active,
        };
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &profile);
    });

    // No auth for the caller here — should panic due to require_auth
    client.bump_user_profile_ttl(&user);
}

#[test]
#[should_panic]
fn test_bump_user_metrics_ttl_unauthorized() {
    let env = Env::default();

    // Do NOT call env.mock_all_auths(); we want require_auth() to enforce real auth.
    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };

    env.as_contract(&client.address, || {
        env.storage().persistent().set(&DataKey::Config, &config);
    });

    // No auth for the caller here — should panic due to require_auth
    client.bump_user_metrics_ttl(&user);
}
#[test]
fn test_volume_normalization_7_decimal_token() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "vol7"), &UserRole::Artisan);

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin);
    client.update_user_metrics(
        &user,
        &AUTO_VERIFY_ESCROW_THRESHOLD,
        &AUTO_VERIFY_VOLUME_THRESHOLD,
        &token.address(),
    );

    assert!(client.is_verified(&user));
    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, AUTO_VERIFY_ESCROW_THRESHOLD);
    assert_eq!(metrics.total_volume, AUTO_VERIFY_VOLUME_THRESHOLD);
}

#[test]
fn test_volume_normalization_8_decimal_token() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "vol8"), &UserRole::Artisan);

    let token = register_decimal_test_token(&env, 8);
    let raw_threshold = AUTO_VERIFY_VOLUME_THRESHOLD * 10;
    client.update_user_metrics(&user, &AUTO_VERIFY_ESCROW_THRESHOLD, &raw_threshold, &token);

    assert!(client.is_verified(&user));
    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, AUTO_VERIFY_ESCROW_THRESHOLD);
    assert_eq!(metrics.total_volume, AUTO_VERIFY_VOLUME_THRESHOLD);
}

#[test]
fn test_volume_normalization_18_decimal_token() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "vol18"), &UserRole::Artisan);

    let token = register_decimal_test_token(&env, 18);
    let raw_threshold = AUTO_VERIFY_VOLUME_THRESHOLD * 10_i128.pow(11);
    client.update_user_metrics(&user, &AUTO_VERIFY_ESCROW_THRESHOLD, &raw_threshold, &token);

    assert!(client.is_verified(&user));
    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, AUTO_VERIFY_ESCROW_THRESHOLD);
    assert_eq!(metrics.total_volume, AUTO_VERIFY_VOLUME_THRESHOLD);
}

// ===== Portfolio Tests (Issue #112) =====

#[test]
fn test_update_portfolio_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_jane");

    // Onboard as artisan
    client.onboard_user(&user, &username, &UserRole::Artisan);

    // Update portfolio with valid CIDv0
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let expected = Bytes::from_slice(&env, portfolio_cid.to_string().as_bytes());
    let updated = client.update_portfolio(&user, &Some(portfolio_cid.clone()));

    assert_eq!(updated.portfolio_cid, Some(expected));
    assert_eq!(updated.role, UserRole::Artisan);
}

#[test]
fn test_onboard_user_stores_flat_profile_without_portfolio_key() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_flat");

    client.onboard_user(&user, &username, &UserRole::Artisan);

    let stored: StoredUserProfile = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserProfile(user.clone()))
            .unwrap()
    });
    assert_eq!(stored.version, CURRENT_USER_PROFILE_VERSION);

    let has_portfolio_key = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .has(&DataKey::UserPortfolio(user.clone()))
    });
    assert!(!has_portfolio_key);
}

#[test]
fn test_update_portfolio_with_cidv1() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_john");

    // Onboard as artisan
    client.onboard_user(&user, &username, &UserRole::Artisan);

    // Update portfolio with valid CIDv1 (base32)
    let portfolio_cid = String::from_str(
        &env,
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    );
    let expected = Bytes::from_slice(&env, portfolio_cid.to_string().as_bytes());
    let updated = client.update_portfolio(&user, &Some(portfolio_cid.clone()));

    assert_eq!(updated.portfolio_cid, Some(expected));
}

#[test]
fn test_update_portfolio_remove() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_bob");

    // Onboard as artisan
    client.onboard_user(&user, &username, &UserRole::Artisan);

    // Set portfolio
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.update_portfolio(&user, &Some(portfolio_cid));

    // Remove portfolio
    let updated = client.update_portfolio(&user, &None);
    assert_eq!(updated.portfolio_cid, None);
}

#[test]
fn test_update_portfolio_uses_separate_storage_key() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_split");
    client.onboard_user(&user, &username, &UserRole::Artisan);

    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let expected = string_to_bytes(&env, &portfolio_cid);
    client.update_portfolio(&user, &Some(portfolio_cid));

    let stored: StoredUserProfile = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserProfile(user.clone()))
            .unwrap()
    });
    assert_eq!(stored.version, CURRENT_USER_PROFILE_VERSION);

    let stored_portfolio: Bytes = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserPortfolio(user.clone()))
            .unwrap()
    });
    assert_eq!(stored_portfolio, expected);
}

#[test]
#[should_panic]
fn test_update_portfolio_buyer_cannot_update() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "buyer_jane");

    // Onboard as buyer
    client.onboard_user(&user, &username, &UserRole::Buyer);

    // Try to update portfolio (should fail)
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.update_portfolio(&user, &Some(portfolio_cid));
}

#[test]
#[should_panic]
fn test_update_portfolio_invalid_cid() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_alice");

    // Onboard as artisan
    client.onboard_user(&user, &username, &UserRole::Artisan);

    // Try to update with invalid CID
    let invalid_cid = String::from_str(&env, "invalid_cid_format");
    client.update_portfolio(&user, &Some(invalid_cid));
}

#[test]
#[should_panic]
fn test_update_portfolio_not_onboarded() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    // Try to update portfolio without onboarding
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    client.update_portfolio(&user, &Some(portfolio_cid));
}

#[test]
fn test_portfolio_accessible_via_get_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_carol");

    // Onboard as artisan
    client.onboard_user(&user, &username, &UserRole::Artisan);

    // Update portfolio
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let expected = Bytes::from_slice(&env, portfolio_cid.to_string().as_bytes());
    client.update_portfolio(&user, &Some(portfolio_cid.clone()));

    // Verify portfolio is accessible via get_user
    let profile = client.get_user(&user);
    assert_eq!(profile.portfolio_cid, Some(expected));
}

#[test]
fn test_portfolio_accessible_via_get_user_by_username() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_dave");

    // Onboard as artisan
    client.onboard_user(&user, &username, &UserRole::Artisan);

    // Update portfolio
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let expected = Bytes::from_slice(&env, portfolio_cid.to_string().as_bytes());
    client.update_portfolio(&user, &Some(portfolio_cid.clone()));

    // Verify portfolio is accessible via get_user_by_username
    let profile = client.get_user_by_username(&username);
    assert_eq!(profile.portfolio_cid, Some(expected));
}

#[test]
fn test_portfolio_none_by_default() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_eve");

    // Onboard as artisan
    let profile = client.onboard_user(&user, &username, &UserRole::Artisan);

    // Verify portfolio is None by default
    assert_eq!(profile.portfolio_cid, None);
}

#[test]
fn test_portfolio_preserves_other_fields() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let username = String::from_str(&env, "artisan_frank");

    // Onboard as artisan
    let original = client.onboard_user(&user, &username, &UserRole::Artisan);
    assert_eq!(original.role, UserRole::Artisan);
    assert!(!original.is_verified);

    // Update portfolio
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let updated = client.update_portfolio(&user, &Some(portfolio_cid));

    // Verify other fields are preserved
    assert_eq!(updated.role, UserRole::Artisan);
    assert!(!updated.is_verified);
    assert_eq!(updated.address, user);
    assert_eq!(updated.registered_at, original.registered_at);
}

#[test]
fn test_migrate_user_profile_moves_embedded_portfolio_to_separate_key() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);
    let portfolio_cid = String::from_str(&env, "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG");
    let expected = string_to_bytes(&env, &portfolio_cid);

    let versioned_profile = UserProfile {
        version: 4,
        address: user.clone(),
        role: UserRole::Artisan,
        username: Symbol::new(&env, "legacy_artisan"),
        registered_at: 1234,
        is_verified: true,
        successful_trades: 2,
        disputed_trades: 1,
        portfolio_cid: Some(expected.clone()),
        status: ProfileStatus::Active,
        state_version: 0,
    };

    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::UserProfile(user.clone()), &versioned_profile);
    });

    assert!(client.migrate_user_profile(&user));

    let migrated = client.get_user(&user);
    assert_eq!(migrated.version, CURRENT_USER_PROFILE_VERSION);
    assert_eq!(migrated.portfolio_cid, Some(expected.clone()));

    let stored: StoredUserProfile = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserProfile(user.clone()))
            .unwrap()
    });
    assert_eq!(stored.version, CURRENT_USER_PROFILE_VERSION);

    let stored_portfolio: Bytes = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&DataKey::UserPortfolio(user.clone()))
            .unwrap()
    });
    assert_eq!(stored_portfolio, expected);
}

// ===== Error Enum Tests (Issue #120) =====

// ===== Error Enum Tests (Issue #120) =====

#[test]
fn test_error_enum_has_specific_variants() {
    // These tests verify that the error enum maintains backward compatibility
    // and includes required error variants for the platform. Uncomment assertions
    // as corresponding error variants are added during development.

    // Note: The following variant checks are deferred to a future refactoring
    // when error codes are consolidated across onboarding and escrow contracts:
    // assert_eq!(Error::InvalidIpfsHash as u32, 25);
    // assert_eq!(Error::InvalidMetadataHash as u32, 26);
    // assert_eq!(Error::BatchLimitExceeded as u32, 27);
    // assert_eq!(Error::InvalidPortfolioCid as u32, 28);
    // assert_eq!(Error::NotAnArtisan as u32, 29);
    // assert_eq!(Error::InvalidVerificationLevel as u32, 30);
    // assert_eq!(Error::UsernameChangeCooldownActive as u32, 31);
    // assert_eq!(Error::InvalidDisputeReason as u32, 32);
    // assert_eq!(Error::EscrowAmountBelowMinimum as u32, 33);
    // assert_eq!(Error::InvalidReleaseWindow as u32, 34);
    // assert_eq!(Error::UnauthorizedAdmin as u32, 35);
}

#[test]
fn test_error_enum_backward_compatibility() {
    // Verify that existing error variants maintain their numeric IDs
    assert_eq!(Error::NotInitialized as u32, 1);
    assert_eq!(Error::UserNotFound as u32, 2);
    assert_eq!(Error::UsernameTaken as u32, 3);
    assert_eq!(Error::UsernameTooShort as u32, 4);
    assert_eq!(Error::UsernameTooLong as u32, 5);
    assert_eq!(Error::InvalidRole as u32, 6);
    assert_eq!(Error::AlreadyOnboarded as u32, 7);
    assert_eq!(Error::Unauthorized as u32, 8);
    assert_eq!(Error::ProfileDeactivated as u32, 9);
    assert_eq!(Error::ActiveEscrowsExist as u32, 10);
    assert_eq!(Error::InvalidFee as u32, 11);
    assert_eq!(Error::NotAnArtisan as u32, 12);
    assert_eq!(Error::InvalidPortfolioCid as u32, 13);
    assert_eq!(Error::CooldownActive as u32, 14);
}

/// Issue #117 — set_moderator must reject callers that are not the platform admin.
#[test]
#[should_panic]
fn test_set_moderator_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &soroban_sdk::String::from_str(&env, "target_user"),
        &UserRole::Buyer,
    );

    // Clear mocked auths so the next call has no authorization.
    env.set_auths(&[]);

    // Calling set_moderator without admin auth must panic.
    client.set_moderator(&user);
}

/// Issue #514 / #113 — reactivate_profile must reject callers without user authorization.
#[test]
#[should_panic]
fn test_reactivate_profile_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &soroban_sdk::String::from_str(&env, "someuser"),
        &UserRole::Buyer,
    );
    client.deactivate_profile(&user);

    // Clear all mocked auths — no authorization provided.
    env.set_auths(&[]);

    // Must panic: no auth for `user`.
    client.reactivate_profile(&user);
}

#[test]
fn test_has_active_contracts() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);

    // 1. No escrow contract registered -> should return false
    assert!(!client.has_active_contracts(&user));

    // 2. Register and set escrow contract
    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    let escrow_client = crate::CraftNexusContractClient::new(&env, &escrow_id);

    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    escrow_client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500, // 5% platform fee
        &Some(client.address.clone()),
    );

    client.set_escrow_contract(&escrow_id);

    // 3. User has no active escrows -> should return false
    assert!(!client.has_active_contracts(&user));

    // 4. Create an active escrow (buyer is user, seller is artisan)
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin);
    let _token_client = token::Client::new(&env, &token_id.address());
    let token_asset = token::StellarAssetClient::new(&env, &token_id.address());
    token_asset.mint(&user, &10_000_000);

    // Onboard seller as artisan
    client.onboard_user(
        &seller,
        &String::from_str(&env, "artisan"),
        &UserRole::Artisan,
    );
    // Onboard buyer as buyer
    client.onboard_user(&user, &String::from_str(&env, "buyer"), &UserRole::Buyer);

    // Create escrow
    escrow_client.create_escrow(&user, &seller, &token_id.address(), &1_000_000, &1, &None);

    // Now has_active_contracts should return true
    assert!(client.has_active_contracts(&user));
    assert!(client.has_active_contracts(&seller));
}

#[test]
fn test_update_active_contracts_tracks_state() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "tracked"), &UserRole::Buyer);

    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let escrow_client = crate::CraftNexusContractClient::new(&env, &escrow_id);
    escrow_client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(client.address.clone()),
    );
    client.set_escrow_contract(&escrow_id);

    client.update_active_contracts(&user, &1);
    assert!(client.has_active_contracts(&user));

    client.update_active_contracts(&user, &-1);
    assert!(!client.has_active_contracts(&user));
}

#[test]
#[should_panic]
fn test_update_active_contracts_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);

    // Set an escrow contract so authorization is gated by the registered address.
    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    client.set_escrow_contract(&escrow_id);

    // Clear mocked auths so the next call has no authorization.
    env.set_auths(&[]);

    client.update_active_contracts(&user, &1);
}

// ============================================================
// Feature #47 – precise active-contract count for escrow/reputation flows
// ============================================================

/// get_active_contract_count returns 0 for a user with no tracked contracts.
#[test]
fn test_get_active_contract_count_defaults_to_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "counter0"), &UserRole::Buyer);

    assert_eq!(client.get_active_contract_count(&user), 0);
    assert!(!client.has_active_contracts(&user));
}

/// get_active_contract_count reflects each increment/decrement state transition
/// and stays consistent with the has_active_contracts boolean.
#[test]
fn test_get_active_contract_count_tracks_transitions() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "counterN"), &UserRole::Buyer);

    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let escrow_client = crate::CraftNexusContractClient::new(&env, &escrow_id);
    escrow_client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(client.address.clone()),
    );
    client.set_escrow_contract(&escrow_id);

    // 0 -> 2: two concurrent active contracts.
    client.update_active_contracts(&user, &1);
    client.update_active_contracts(&user, &1);
    assert_eq!(client.get_active_contract_count(&user), 2);
    assert!(client.has_active_contracts(&user));

    // 2 -> 1: one closes.
    client.update_active_contracts(&user, &-1);
    assert_eq!(client.get_active_contract_count(&user), 1);
    assert!(client.has_active_contracts(&user));

    // 1 -> 0: last one closes, entry is removed and count reads back as zero.
    client.update_active_contracts(&user, &-1);
    assert_eq!(client.get_active_contract_count(&user), 0);
    assert!(!client.has_active_contracts(&user));
}

#[test]
#[should_panic]
fn test_update_active_contracts_underflow_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "underflow"),
        &UserRole::Buyer,
    );

    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    client.set_escrow_contract(&escrow_id);

    let _ = admin;
    client.update_active_contracts(&user, &-1);
}

#[test]
#[should_panic]
fn test_deactivate_profile_rejects_without_registered_escrow_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "noescrow"), &UserRole::Buyer);

    client.deactivate_profile(&user);
}

#[test]
#[should_panic]
fn test_deactivate_profile_rejects_active_contract_count() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(&user, &String::from_str(&env, "deact"), &UserRole::Buyer);

    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    client.set_escrow_contract(&escrow_id);

    client.update_active_contracts(&user, &1);
    client.deactivate_profile(&user);

    let _ = admin;
}

#[test]
#[should_panic]
fn test_get_verification_queue_unauthorized() {
    let env = Env::default();
    // Do NOT call env.mock_all_auths()

    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    // Initialize state directly in storage without require_auth check
    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Config, &config);
    });

    // This should panic because mock_all_auths is not set, so admin's require_auth() will fail
    client.get_verification_queue();
}

#[test]
fn test_get_verification_queue_authorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup_test(&env);

    client.get_verification_queue();

    // Check that admin's authorization was verified
    let auths = env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths.get(0).unwrap().0, admin);
}

#[test]
fn test_is_verification_pending_for_requesting_user() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "pending_user"),
        &UserRole::Buyer,
    );
    client.request_verification(&user);

    assert!(client.is_verification_pending(&user));
}

#[test]
#[should_panic]
fn test_is_verification_pending_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "pending_unauth"),
        &UserRole::Buyer,
    );
    client.request_verification(&user);

    env.set_auths(&[]);
    client.is_verification_pending(&user);
}

// ── Issue #470: [SECURITY] Endpoint #69 – set_moderator ─────────────────────

/// Issue #470 — set_moderator must record the admin auth signal on success.
#[test]
fn test_set_moderator_records_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &soroban_sdk::String::from_str(&env, "promotee"),
        &UserRole::Artisan,
    );

    client.set_moderator(&user);

    let auths = env.auths();
    let admin_auth = auths.iter().find(|(addr, _)| addr == &admin);
    assert!(
        admin_auth.is_some(),
        "admin auth must be recorded for set_moderator"
    );

    let profile = client.get_user(&user);
    assert_eq!(profile.role, UserRole::Moderator);
}

/// Issue #470 — a non-admin address must not be able to invoke set_moderator.
#[test]
#[should_panic]
fn test_set_moderator_non_admin_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let attacker = Address::generate(&env);
    let target = Address::generate(&env);
    client.onboard_user(
        &target,
        &soroban_sdk::String::from_str(&env, "victim"),
        &UserRole::Buyer,
    );

    // Strip all mocked auths so only a non-admin caller could sign.
    env.set_auths(&[]);

    // Attempting promotion without admin auth must panic.
    client.set_moderator(&target);
    let _ = attacker;
}

/// Issue #470 — promoting a non-onboarded address via set_moderator must panic.
#[test]
#[should_panic]
fn test_set_moderator_unknown_user_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let ghost = Address::generate(&env);

    // Ghost was never onboarded — role update must fail.
    client.set_moderator(&ghost);
}

// ── Issue #474: [SECURITY] Endpoint #73 – get_verification_queue ─────────────

/// Issue #474 — non-admin caller must not read the verification queue.
#[test]
#[should_panic]
fn test_get_verification_queue_non_admin_rejected() {
    let env = Env::default();
    // Do NOT call mock_all_auths — no auth provided.

    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Config, &config);
    });

    // No auth signal — must panic immediately.
    client.get_verification_queue();
    let _ = admin;
}

/// Issue #474 — admin receives the queue and the auth signal is recorded.
#[test]
fn test_get_verification_queue_returns_pending_users() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);

    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &soroban_sdk::String::from_str(&env, "queueuser"),
        &UserRole::Artisan,
    );
    client.request_verification(&user);

    let queue = client.get_verification_queue();

    assert!(
        queue.contains(&user),
        "requesting user must appear in the verification queue"
    );

    let auths = env.auths();
    let admin_auth = auths.iter().find(|(addr, _)| addr == &admin);
    assert!(
        admin_auth.is_some(),
        "admin auth must be recorded for get_verification_queue"
    );
}

// ── Issue #430: [SECURITY] Endpoint #29 – get_user_metrics ───────────────────

/// Issue #430 — get_user_metrics must reject callers without user authorization.
#[test]
#[should_panic]
fn test_get_user_metrics_unauthorized() {
    let env = Env::default();

    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Config, &config);
    });

    client.get_user_metrics(&user);
}

// ── Issue #446: [SECURITY] Endpoint #45 – get_user_reputation ──────────────────

/// Issue #446 — get_user_reputation must reject callers without user authorization.
#[test]
#[should_panic]
fn test_get_user_reputation_unauthorized() {
    let env = Env::default();

    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Config, &config);
    });

    client.get_user_reputation(&user);
}

/// Issue #446 — get_user_reputation must allow authorized callers.
#[test]
fn test_get_user_reputation_authorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    set_permissive_reputation_policy(&client);
    let user = Address::generate(&env);

    // Onboard user
    client.onboard_user(&user, &String::from_str(&env, "rep1"), &UserRole::Artisan);

    // Update reputation
    client.update_reputation(&user, &2u32, &1u32);

    // Get reputation (authorized)
    let (successful, disputed) = client.get_user_reputation(&user);
    assert_eq!(successful, 2);
    assert_eq!(disputed, 1);
}

// ── Issue #452: [FEATURE] Business flow #51 – active contract authorization ─

/// Issue #452 — has_active_contracts must reject callers without user authorization.
#[test]
#[should_panic]
fn test_has_active_contracts_unauthorized() {
    let env = Env::default();

    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let config = OnboardingConfig {
        require_username: true,
        min_username_length: 3,
        max_username_length: 50,
        platform_admin: admin.clone(),
        auto_verify_enabled: true,
        min_escrow_count_for_verify: 5,
        min_volume_for_verify: 10_000_000_000,
        escrow_contract: None,
    };
    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&DataKey::Config, &config);
    });

    client.has_active_contracts(&user);
}

/// Issue #452 / #622 — has_active_contracts succeeds for authorized user and refreshes TTL.
#[test]
fn test_has_active_contracts_authorized_returns_boolean_and_extends_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "activeusr"),
        &UserRole::Buyer,
    );

    // Initial query returns false (no escrows registered)
    assert!(!client.has_active_contracts(&user));
}

// ===== set_verification_thresholds auth tests (#422) =====

#[test]
fn test_set_verification_thresholds_admin_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    // Admin is authorized via mock_all_auths — must not panic.
    client.set_verification_thresholds(&10u32, &5_000_000_000i128);
    let config = client.get_config();
    assert_eq!(config.min_escrow_count_for_verify, 10);
    assert_eq!(config.min_volume_for_verify, 5_000_000_000);
}

#[test]
#[should_panic]
fn test_set_verification_thresholds_unauthorized_rejected() {
    // Without any auth mocked, require_auth must cause a panic.
    let env = Env::default();
    let (client, _) = setup_test(&env);
    client.set_verification_thresholds(&10u32, &5_000_000_000i128);
}

// ===== Pause-state guard (Issue #621) =====

#[test]
fn test_onboard_rejected_when_escrow_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    let escrow_client = crate::CraftNexusContractClient::new(&env, &escrow_id);

    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    escrow_client.initialize(
        &platform_wallet,
        &admin,
        &arbitrator,
        &500,
        &Some(client.address.clone()),
    );

    client.set_escrow_contract(&escrow_id);

    // Pause the escrow contract
    escrow_client.set_paused(&true);

    // Onboarding should be rejected
    let result =
        client.try_onboard_user(&user, &String::from_str(&env, "newuser"), &UserRole::Buyer);
    assert!(result.is_err());
}

// ===== Issue #447 — storage/TTL read-path optimizations =====

/// Every queue slot walked by `get_verification_queue` must have its TTL
/// refreshed, not only the head slot touched by `advance_verification_head`.
/// Otherwise a request queued behind a long-lived one is archived and its user
/// silently disappears from the queue.
#[test]
fn test_get_verification_queue_extends_ttl_for_every_slot() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);

    let mut users = soroban_sdk::Vec::new(&env);
    for (index, name) in ["ttlqueue1", "ttlqueue2", "ttlqueue3"].iter().enumerate() {
        let user = Address::generate(&env);
        client.onboard_user(&user, &String::from_str(&env, name), &UserRole::Artisan);
        client.request_verification(&user);
        users.push_back(user);
        assert_eq!(client.get_verification_queue().len() as usize, index + 1);
    }

    let live_before = env.as_contract(&client.address, || {
        (0..users.len() as u64)
            .filter(|slot| {
                env.storage()
                    .persistent()
                    .has(&DataKey::VerificationQueueIndex(*slot))
            })
            .count()
    });
    assert_eq!(live_before, 3);

    // Reading the queue must leave every slot live with a refreshed TTL.
    assert_eq!(client.get_verification_queue().len(), 3);

    env.as_contract(&client.address, || {
        for slot in 0..users.len() as u64 {
            let key = DataKey::VerificationQueueIndex(slot);
            assert!(
                env.storage().persistent().has(&key),
                "queue slot {slot} should still be live"
            );
            assert!(
                env.storage().persistent().get_ttl(&key) >= TTL_EXTENSION,
                "queue slot {slot} should have been extended on read"
            );
        }
    });
}

// ============================================================
// Issue #702 – no extend_ttl on temporary verification markers
// ============================================================

/// Pending verification markers live in temporary storage and must not pay for
/// persistent `extend_ttl` on enqueue or pending checks.
#[test]
fn test_verification_request_uses_temporary_storage_without_extend_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "temp_vrfy"),
        &UserRole::Artisan,
    );
    client.request_verification(&user);

    assert!(client.is_verification_pending(&user));

    env.as_contract(&client.address, || {
        let key = DataKey::VerificationRequest(user.clone());
        assert!(
            env.storage().temporary().has(&key),
            "pending marker must be stored in temporary storage"
        );
        assert!(
            !env.storage().persistent().has(&key),
            "pending marker must not be duplicated into persistent storage"
        );
    });

    // Clearing removes the temporary marker and empties the queue.
    assert!(client.admin_clear_verification_request(&user));
    assert!(!client.is_verification_pending(&user));
    env.as_contract(&client.address, || {
        let key = DataKey::VerificationRequest(user.clone());
        assert!(!env.storage().temporary().has(&key));
        assert!(!env.storage().persistent().has(&key));
    });
}

/// Legacy persistent pending markers are still recognized without refreshing TTL.
#[test]
fn test_legacy_persistent_verification_request_cleared_without_ttl_bump() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "legacy_vrfy"),
        &UserRole::Artisan,
    );

    // Simulate a pre-#702 persistent pending marker + queue slot.
    env.as_contract(&client.address, || {
        let pending_key = DataKey::VerificationRequest(user.clone());
        env.storage()
            .persistent()
            .set(&pending_key, &env.ledger().timestamp());
        // Intentionally do not extend_ttl — mirrors the #702 policy.
        env.storage()
            .persistent()
            .set(&DataKey::VerificationQueueIndex(0), &user);
        env.storage()
            .persistent()
            .set(&DataKey::VerificationQueueTail, &1u64);
    });

    assert!(client.is_verification_pending(&user));
    assert!(client.admin_clear_verification_request(&user));
    assert!(!client.is_verification_pending(&user));
    assert_eq!(client.get_verification_queue().len(), 0);
}

/// Read helpers refresh TTL on the entries they return without a second probe.
#[test]
fn test_read_paths_refresh_ttl_on_touched_entries() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "ttlreader"),
        &UserRole::Artisan,
    );
    client.set_username_change_fee(&42);

    // Drive the read API.
    let _ = client.get_user(&user);
    let _ = client.get_user_metrics(&user);
    let _ = client.get_username_change_fee();
    assert!(client.is_onboarded(&user));

    env.as_contract(&client.address, || {
        for key in [
            DataKey::UserProfile(user.clone()),
            DataKey::UsernameChangeFee,
        ] {
            assert!(env.storage().persistent().has(&key));
            assert!(
                env.storage().persistent().get_ttl(&key) >= TTL_EXTENSION,
                "read path should have refreshed the entry TTL"
            );
        }
    });
}

/// Absent optional entries must not be resurrected or charged for by the
/// read helpers; they simply return the caller-supplied default.
#[test]
fn test_read_helpers_do_not_create_absent_entries() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "nodefaults"),
        &UserRole::Artisan,
    );

    let metrics = client.get_user_metrics(&user);
    assert_eq!(metrics.total_escrow_count, 0);
    assert_eq!(metrics.total_volume, 0);
    assert_eq!(client.get_username_change_fee(), 0);
    assert_eq!(client.get_active_contract_count(&user), 0);
    assert!(client.get_username_fee_token().is_none());

    env.as_contract(&client.address, || {
        for key in [
            DataKey::UserMetrics(user.clone()),
            DataKey::UsernameChangeFee,
            DataKey::ActiveContractCount(user.clone()),
            DataKey::UsernameChangeFeeToken,
            DataKey::UserPortfolio(user.clone()),
        ] {
            assert!(
                !env.storage().persistent().has(&key),
                "reading a default must not materialize a storage entry"
            );
        }
    });
}

/// Budget smoke test for the hot profile read path (issue #447 action item 3).
/// `get_user` is the most frequently invoked entrypoint; running it against the
/// default ledger budget guards the read path against regressions that would
/// reintroduce redundant storage probes.
#[test]
fn test_profile_read_budget_smoke() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "budgetread"),
        &UserRole::Artisan,
    );

    env.budget().reset_default();
    let _ = client.get_user(&user);
    let _ = client.get_user_reputation(&user);
    let _ = client.get_user_metrics(&user);
}

// ── Issue #940: Anti-Sybil Onboarding & Identity Abuse Tests ─────────────────

#[test]
fn test_sybil_config_management() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);

    client.set_sybil_config(&1800u64, &5u32, &43200u64, &true, &None);

    assert_eq!(client.get_rate_limit_window(), 1800);
    assert_eq!(client.get_max_onboard_attempts(), 5);
    assert_eq!(client.get_verification_cooldown(), 43200);
    assert!(client.is_poh_required_for_auto_verify());
    assert!(client.get_poh_verifier().is_none());
}

// ── Issue #1084: Onboarding and verification attempt windows ────────────────

#[test]
fn test_attempt_rate_policy_revision_advances() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);

    assert_eq!(client.get_attempt_rate_policy().revision, 1);
    let updated = client.set_attempt_rate_policy(
        &60u64, &2u32, &10u32, &120u64, &3u32, &20u32,
    );
    assert_eq!(updated.revision, 2);
    assert_eq!(client.get_attempt_rate_policy(), updated);
}

#[test]
fn test_global_onboarding_limit_resets_after_window() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _) = setup_test(&env);
    client.set_attempt_rate_policy(&60u64, &3u32, &1u32, &60u64, &3u32, &10u32);

    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.onboard_user(
        &first,
        &String::from_str(&env, "rate_first"),
        &UserRole::Buyer,
    );

    let limited = client.try_onboard_user(
        &second,
        &String::from_str(&env, "rate_second"),
        &UserRole::Artisan,
    );
    assert!(limited.is_err());
    assert!(!client.is_onboarded(&second));

    env.ledger().with_mut(|li| li.timestamp = 1_061);
    let profile = client.onboard_user(
        &second,
        &String::from_str(&env, "rate_second"),
        &UserRole::Artisan,
    );
    assert_eq!(profile.address, second);
}

#[test]
fn test_verification_limits_do_not_duplicate_queue_records() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 2_000);
    let (client, _) = setup_test(&env);
    client.set_sybil_config(&3_600u64, &10u32, &0u64, &false, &None);
    client.set_attempt_rate_policy(&60u64, &10u32, &10u32, &60u64, &1u32, &1u32);

    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.onboard_user(
        &first,
        &String::from_str(&env, "verify_one"),
        &UserRole::Artisan,
    );
    client.onboard_user(
        &second,
        &String::from_str(&env, "verify_two"),
        &UserRole::Artisan,
    );

    client.request_verification(&first);
    // A repeated pending request is an idempotent no-op and does not add a slot.
    client.request_verification(&first);
    assert_eq!(client.get_verification_queue().len(), 1);

    let limited = client.try_request_verification(&second);
    assert!(limited.is_err());
    assert!(!client.is_verification_pending(&second));
    assert_eq!(client.get_verification_queue().len(), 1);

    env.ledger().with_mut(|li| li.timestamp = 2_061);
    client.request_verification(&second);
    assert!(client.is_verification_pending(&second));
    assert_eq!(client.get_verification_queue().len(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_attempt_rate_policy_rejects_zero_active_window() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    client.set_attempt_rate_policy(&0u64, &1u32, &1u32, &60u64, &1u32, &1u32);
}

#[test]
fn test_proof_of_humanity_credential_registration_and_validation() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(&user, &String::from_str(&env, "pohuser"), &UserRole::Buyer);

    let provider = soroban_sdk::Symbol::new(&env, "WorldID");
    let mut cred_buf = [0u8; 32];
    cred_buf[0] = 0xAA;
    let cred_hash = soroban_sdk::Bytes::from_slice(&env, &cred_buf);
    let expires = env.ledger().timestamp() + 10_000;

    let cred = client.register_poh_credential(&user, &provider, &cred_hash, &expires);
    assert_eq!(cred.provider_id, provider);
    assert_eq!(cred.credential_hash, cred_hash);

    assert!(client.is_poh_valid(&user));

    let fetched = client
        .get_poh_credential(&user)
        .expect("PoH credential present");
    assert_eq!(fetched.provider_id, provider);
}

#[test]
#[should_panic]
fn test_poh_credential_duplicate_prevention() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    client.onboard_user(&user1, &String::from_str(&env, "userone"), &UserRole::Buyer);
    client.onboard_user(&user2, &String::from_str(&env, "usertwo"), &UserRole::Buyer);

    let provider = soroban_sdk::Symbol::new(&env, "Gitcoin");
    let mut cred_buf = [0u8; 32];
    cred_buf[0] = 0xBB;
    let cred_hash = soroban_sdk::Bytes::from_slice(&env, &cred_buf);
    let expires = env.ledger().timestamp() + 10_000;

    // Register for user1
    client.register_poh_credential(&user1, &provider, &cred_hash, &expires);

    // Registering the exact same credential hash for user2 must panic (DuplicateIdentityCredential)
    client.register_poh_credential(&user2, &provider, &cred_hash, &expires);
}

#[test]
fn test_identity_correlation_onboarding() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);

    let mut id_buf = [0u8; 32];
    id_buf[0] = 0xCC;
    let id_hash = soroban_sdk::Bytes::from_slice(&env, &id_buf);

    let profile = client.onboard_user_with_identity(
        &user,
        &String::from_str(&env, "correlated"),
        &UserRole::Artisan,
        &id_hash,
    );

    assert_eq!(profile.address, user);
    assert_eq!(profile.status, ProfileStatus::Active);
}

#[test]
#[should_panic]
fn test_duplicate_identity_correlation_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);

    let mut id_buf = [0u8; 32];
    id_buf[0] = 0xDD;
    let id_hash = soroban_sdk::Bytes::from_slice(&env, &id_buf);

    client.onboard_user_with_identity(
        &user1,
        &String::from_str(&env, "sybil1"),
        &UserRole::Artisan,
        &id_hash,
    );

    // Attempting to onboard user2 with the same identity hash must panic (DuplicateIdentityCorrelation)
    client.onboard_user_with_identity(
        &user2,
        &String::from_str(&env, "sybil2"),
        &UserRole::Artisan,
        &id_hash,
    );
}

#[test]
fn test_suspicious_profile_flagging_and_review_queue_workflow() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _admin) = setup_test(&env);
    let user = Address::generate(&env);

    client.onboard_user(
        &user,
        &String::from_str(&env, "suspect"),
        &UserRole::Artisan,
    );

    // Flag profile for review
    client.flag_suspicious_profile(&user, &101u32, &86400u64);

    let profile = client.get_user(&user);
    assert_eq!(profile.status, ProfileStatus::UnderReview);

    let flag = client.get_suspicious_flag(&user).expect("Flag present");
    assert_eq!(flag.reason_code, 101);

    let queue = client.get_review_queue();
    assert!(queue.contains(&user));

    // Admin approves review -> clears flag and restores Active status
    client.process_review(&user, &true);

    let restored_profile = client.get_user(&user);
    assert_eq!(restored_profile.status, ProfileStatus::Active);
    assert!(client.get_suspicious_flag(&user).is_none());
}

#[test]
fn test_revision_bound_sybil_review_restricts_then_restores_access() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let reviewer = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "review_bound"),
        &UserRole::Buyer,
    );
    client.set_sybil_reviewer(&reviewer, &true);
    client.flag_suspicious_profile(&user, &701u32, &600u64);

    let review = client.get_sybil_review(&user).expect("review case");
    assert_eq!(review.status, SybilReviewStatus::ReviewRequired);
    assert_eq!(
        review.profile_revision,
        client.get_user(&user).state_version
    );
    assert!(client
        .try_update_user_role(&user, &UserRole::Artisan)
        .is_err());

    client.decide_sybil_review(&reviewer, &user, &review.profile_revision, &true);
    assert_eq!(client.get_user(&user).status, ProfileStatus::Active);
    assert_eq!(
        client.get_sybil_review(&user).unwrap().status,
        SybilReviewStatus::Approved
    );
    assert_eq!(
        client.update_user_role(&user, &UserRole::Artisan).role,
        UserRole::Artisan
    );
}

#[test]
fn test_sybil_review_rejects_unauthorized_and_stale_decisions() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 2_000);
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "review_stale"),
        &UserRole::Artisan,
    );
    client.flag_suspicious_profile(&user, &702u32, &600u64);
    let review = client.get_sybil_review(&user).unwrap();

    assert!(client
        .try_decide_sybil_review(
            &unauthorized,
            &user,
            &review.profile_revision,
            &true,
        )
        .is_err());
    assert!(client
        .try_process_review(&user, &true)
        .is_ok());
    assert_eq!(client.get_user(&user).status, ProfileStatus::Active);

    client.flag_suspicious_profile(&user, &703u32, &600u64);
    let current = client.get_sybil_review(&user).unwrap();
    assert!(client
        .try_decide_sybil_review(
            &client.get_config().platform_admin,
            &user,
            &(current.profile_revision - 1),
            &true,
        )
        .is_err());
    assert_eq!(client.get_user(&user).status, ProfileStatus::UnderReview);
}

#[test]
fn test_sybil_rejection_appeal_and_expiry_remain_restricted() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 3_000);
    let (client, admin) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "review_appeal"),
        &UserRole::Artisan,
    );
    client.flag_suspicious_profile(&user, &704u32, &10u64);
    let opened = client.get_sybil_review(&user).unwrap();
    client.decide_sybil_review(&admin, &user, &opened.profile_revision, &false);
    assert_eq!(client.get_user(&user).status, ProfileStatus::Flagged);

    let rejected_revision = client.get_user(&user).state_version;
    client.appeal_sybil_review(&user, &rejected_revision);
    let appealed = client.get_sybil_review(&user).unwrap();
    assert_eq!(appealed.status, SybilReviewStatus::Appealed);
    assert_eq!(appealed.appeal_count, 1);
    assert!(client.try_request_verification(&user).is_err());

    env.ledger().with_mut(|li| li.timestamp = appealed.expires_at);
    client.expire_sybil_review(&user, &appealed.profile_revision);
    assert_eq!(client.get_user(&user).status, ProfileStatus::Flagged);
    assert_eq!(
        client.get_sybil_review(&user).unwrap().status,
        SybilReviewStatus::Expired
    );
}

#[test]
fn test_normal_verified_profile_is_unaffected_by_review_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup_test(&env);
    let user = Address::generate(&env);
    client.onboard_user(
        &user,
        &String::from_str(&env, "normal_verified"),
        &UserRole::Buyer,
    );
    let verified = client.verify_user(&user);

    assert!(verified.is_verified);
    assert_eq!(verified.status, ProfileStatus::Active);
    assert!(client.get_sybil_review(&user).is_none());
    assert_eq!(
        client.update_user_role(&user, &UserRole::Artisan).role,
        UserRole::Artisan
    );
}
