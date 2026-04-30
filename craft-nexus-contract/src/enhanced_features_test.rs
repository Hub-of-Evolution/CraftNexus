extern crate alloc;

use super::*;
use crate::onboarding::{OnboardingContract, OnboardingContractClient, ProfileStatus, UserRole};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

fn setup_enhanced_test(
    env: &Env,
) -> (
    EscrowContractClient<'static>,
    OnboardingContractClient<'static>,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'static>,
    Address,
    Address,
) {
    env.mock_all_auths();

    // Register Onboarding Contract
    let onboarding_id = env.register_contract(None, OnboardingContract);
    let onboarding_client = OnboardingContractClient::new(env, &onboarding_id);

    // Register Escrow Contract
    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(env, &escrow_id);

    let admin = Address::generate(env);
    let platform_wallet = Address::generate(env);
    let arbitrator = Address::generate(env);

    // Initialize Onboarding
    onboarding_client.initialize(&admin);
    onboarding_client.set_escrow_contract(&escrow_id);

    // Initialize Escrow with onboarding contract address
    escrow_client.initialize(&platform_wallet, &admin, &arbitrator, &500, &Some(onboarding_id.clone()));

    let buyer = Address::generate(env);
    let artisan = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract.address());

    // Onboard users
    onboarding_client.onboard_user(&buyer, &String::from_str(env, "buyer"), &UserRole::Buyer);
    onboarding_client.onboard_user(
        &artisan,
        &String::from_str(env, "artisan"),
        &UserRole::Artisan,
    );

    (
        escrow_client,
        onboarding_client,
        buyer,
        artisan,
        token_contract.address(),
        token_admin_client,
        platform_wallet,
        admin,
    )
}

#[test]
fn test_recurring_escrow_lifecycle() {
    let env = Env::default();
    let (escrow, _, buyer, artisan, token_id, token_admin, platform_wallet, _) =
        setup_enhanced_test(&env);

    let total_amount: i128 = 1000;
    token_admin.mint(&buyer, &total_amount);

    // 1. Create Recurring Escrow (2 cycles, frequency 3600)
    let rec_escrow =
        escrow.create_recurring_escrow(&buyer, &artisan, &token_id, &total_amount, &3600, &2);
    assert_eq!(rec_escrow.total_amount, total_amount);
    assert!(rec_escrow.is_active);
    assert!(escrow.has_active_escrows(&buyer));
    assert!(escrow.has_active_escrows(&artisan));

    // 2. Release 1st Cycle (after frequency)
    env.ledger().with_mut(|li| li.timestamp = 3601);
    escrow.release_next_cycle(&rec_escrow.id);

    let token_client = token::Client::new(&env, &token_id);
    // Cycle 1: 500. Fee 5% = 25. Artisan gets 475.
    assert_eq!(token_client.balance(&artisan), 475);
    assert_eq!(token_client.balance(&platform_wallet), 25);

    let updated = escrow.get_recurring_escrow(&rec_escrow.id);
    assert_eq!(updated.current_cycle, 1);
    assert_eq!(updated.released_amount, 500);
    assert!(updated.is_active);

    // 3. Release 2nd Cycle
    env.ledger().with_mut(|li| li.timestamp = 7202);
    escrow.release_next_cycle(&rec_escrow.id);

    assert_eq!(token_client.balance(&artisan), 950);
    assert_eq!(token_client.balance(&platform_wallet), 50);

    let final_escrow = escrow.get_recurring_escrow(&rec_escrow.id);
    assert!(!final_escrow.is_active);
    assert!(!escrow.has_active_escrows(&buyer));
    assert!(!escrow.has_active_escrows(&artisan));
}

#[test]
fn test_cancel_recurring_escrow() {
    let env = Env::default();
    let (escrow, _, buyer, artisan, token_id, token_admin, _, _) = setup_enhanced_test(&env);

    let total_amount: i128 = 1000;
    token_admin.mint(&buyer, &total_amount);

    let rec_escrow =
        escrow.create_recurring_escrow(&buyer, &artisan, &token_id, &total_amount, &3600, &2);

    // Cancel immediately
    escrow.cancel_recurring_escrow(&rec_escrow.id);

    let token_client = token::Client::new(&env, &token_id);
    assert_eq!(token_client.balance(&buyer), 1000);
    assert_eq!(token_client.balance(&escrow.address), 0);

    assert!(!escrow.has_active_escrows(&buyer));
}

#[test]
fn test_profile_deactivation_success() {
    let env = Env::default();
    let (escrow, onboarding, buyer, _, _, _, _, _) = setup_enhanced_test(&env);

    // No active escrows, should succeed
    onboarding.deactivate_profile(&buyer);
    let profile = onboarding.get_user(&buyer);
    assert_eq!(profile.status, ProfileStatus::Deactivated);
    assert!(!onboarding.is_username_taken(&String::from_str(&env, "buyer")));
}

#[test]
#[should_panic]
fn test_profile_deactivation_fails_with_active_traditional_escrow() {
    let env = Env::default();
    let (escrow, onboarding, buyer, artisan, token_id, token_admin, _, _) =
        setup_enhanced_test(&env);

    token_admin.mint(&buyer, &1000);
    escrow.create_escrow(&buyer, &artisan, &token_id, &500, &1, &None);

    onboarding.deactivate_profile(&buyer);
}

#[test]
#[should_panic]
fn test_profile_deactivation_fails_with_active_recurring_escrow() {
    let env = Env::default();
    let (escrow, onboarding, buyer, artisan, token_id, token_admin, _, _) =
        setup_enhanced_test(&env);

    token_admin.mint(&buyer, &1000);
    escrow.create_recurring_escrow(&buyer, &artisan, &token_id, &1000, &3600, &2);

    onboarding.deactivate_profile(&buyer);
}

#[test]
#[should_panic]
fn test_admin_deactivation_fails() {
    let env = Env::default();
    let (_, onboarding, _, _, _, _, _, admin) = setup_enhanced_test(&env);

    onboarding.deactivate_profile(&admin);
}
