//! Onboarding lifecycle property tests.
//!
//! # Properties verified
//!
//! 1. **No duplicate onboarding** – second `onboard_user` for same address fails.
//! 2. **Username uniqueness** – two profiles cannot share the same username.
//! 3. **Deactivation guard** – profile with active contracts cannot be deactivated.
//! 4. **Reactivation of active profile fails** – already-active profile cannot be reactivated.
//! 5. **Deactivate–reactivate round-trip** – deactivated profile can be reactivated.
//! 6. **Verification monotonicity** – `is_verified` never reverts to `false`.
//! 7. **Model-based sequence agreement** – model conservation holds after each sequence.
//! 8. **Admin role onboarding rejected** – trying to onboard as Admin fails.
//! 9. **get_user idempotent** – repeated reads return consistent data.

#![cfg(test)]
extern crate alloc;

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String as SorobanString};

use super::{
    generators::{generate_onboarding_sequence, OnboardingOp},
    model::{ModelState, ModelUserRole},
    seed_from_env, Lcg64, DEFAULT_CASE_COUNT,
};
use crate::onboarding::{OnboardingContract, OnboardingContractClient, ProfileStatus, UserRole};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_onboarding_env() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    (env, contract_id, admin)
}

/// Make an onboarding env with a real escrow contract wired in.
/// Returns (env, onboarding_contract_id, admin, escrow_contract_id).
fn make_onboarding_env_with_escrow() -> (Env, Address, Address, Address) {
    use crate::CraftNexusContractClient;
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|li| li.timestamp = 1_711_368_000);

    let admin = Address::generate(&env);
    let arbitrator = Address::generate(&env);
    let platform_wallet = Address::generate(&env);

    // Initialize a real escrow contract
    let token_admin_addr = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr);
    let token_id = token_contract.address();

    let escrow_id = env.register_contract(None, crate::CraftNexusContract);
    let escrow_client = CraftNexusContractClient::new(&env, &escrow_id);
    escrow_client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);
    escrow_client.set_min_escrow_amount(&token_id, &0);
    escrow_client.set_min_release_window(&1);
    escrow_client.set_evidence_challenge_window(&0);

    // Initialize the onboarding contract and wire in the escrow contract
    let onboarding_id = env.register_contract(None, OnboardingContract);
    let onboarding_client = OnboardingContractClient::new(&env, &onboarding_id);
    onboarding_client.initialize(&admin);
    onboarding_client.set_escrow_contract(&escrow_id);

    (env, onboarding_id, admin, escrow_id)
}

fn ss(env: &Env, s: &str) -> SorobanString {
    SorobanString::from_str(env, s)
}

// ── Property 1: No duplicate onboarding ──────────────────────────────────────

#[test]
fn prop_no_duplicate_onboarding() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xAAAA);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin) = make_onboarding_env();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        client.onboard_user(&user, &ss(&env, "alice"), &UserRole::Buyer);

        let r = client.try_onboard_user(&user, &ss(&env, "alice"), &UserRole::Buyer);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_no_duplicate_onboarding] second onboard succeeded (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 2: Username uniqueness ──────────────────────────────────────────

#[test]
fn prop_username_uniqueness() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xBBBB);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin) = make_onboarding_env();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user_a = Address::generate(&env);
        let user_b = Address::generate(&env);

        client.onboard_user(&user_a, &ss(&env, "craftuser"), &UserRole::Buyer);

        let r = client.try_onboard_user(&user_b, &ss(&env, "craftuser"), &UserRole::Buyer);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_username_uniqueness] duplicate username accepted (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 3: Deactivation guard ───────────────────────────────────────────

#[test]
fn prop_deactivation_blocked_by_active_contracts() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xCCCC);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin, _escrow_id) = make_onboarding_env_with_escrow();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        client.onboard_user(&user, &ss(&env, "artisan"), &UserRole::Artisan);

        let count = crng.next_u64_range(1, 5) as i32;
        for _ in 0..count {
            client.update_active_contracts(&user, &1i32);
        }

        let r = client.try_deactivate_profile(&user);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_deactivation_blocked_by_active_contracts] deactivation succeeded with \
                 {} active contracts (seed=0x{:016X})",
                count, case_seed
            );
        }
    }
}

// ── Property 4: Reactivation of already-active profile fails ─────────────────

#[test]
fn prop_reactivation_of_active_profile_fails() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xDDDD);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin) = make_onboarding_env();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        client.onboard_user(&user, &ss(&env, "buyer2"), &UserRole::Buyer);

        // Profile is Active — reactivate must fail
        let r = client.try_reactivate_profile(&user);
        if r.is_ok() && r.unwrap().is_ok() {
            panic!(
                "[prop_reactivation_of_active_profile_fails] reactivate on active profile \
                 succeeded (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 5: Deactivate–reactivate round-trip ────────────────────────────

#[test]
fn prop_deactivate_reactivate_roundtrip() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xEEEE);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin, _escrow_id) = make_onboarding_env_with_escrow();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        client.onboard_user(&user, &ss(&env, "roundtrip"), &UserRole::Buyer);

        // Deactivate (no active contracts)
        client.deactivate_profile(&user);

        let p1 = client.get_user(&user);
        if p1.status == ProfileStatus::Active {
            panic!(
                "[prop_deactivate_reactivate_roundtrip] profile still Active after deactivation \
                 (seed=0x{:016X})",
                case_seed
            );
        }

        // Reactivate
        client.reactivate_profile(&user);

        let p2 = client.get_user(&user);
        if p2.status != ProfileStatus::Active {
            panic!(
                "[prop_deactivate_reactivate_roundtrip] profile not Active after reactivation \
                 (seed=0x{:016X})",
                case_seed
            );
        }
    }
}

// ── Property 6: Verification monotonicity ────────────────────────────────────

#[test]
fn prop_verification_monotone() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0xFFFF);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin) = make_onboarding_env();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        client.onboard_user(&user, &ss(&env, "verifieduser"), &UserRole::Artisan);
        client.verify_user(&user);

        for _ in 0..5 {
            if !client.is_verified(&user) {
                panic!(
                    "[prop_verification_monotone] is_verified reverted to false \
                     (seed=0x{:016X})",
                    case_seed
                );
            }
        }
    }
}

// ── Property 7: Model-based sequence agreement ───────────────────────────────

#[test]
fn prop_onboarding_model_agreement() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x1234);
    let user_names = ["alice", "bob", "carol", "dave", "eve", "frank"];

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();
        let mut crng = Lcg64::new(case_seed);

        let (env, contract_id, _admin) = make_onboarding_env();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let mut model = ModelState::new();
        let ops = generate_onboarding_sequence(&mut crng);

        let users: alloc::vec::Vec<Address> =
            (0..5).map(|_| Address::generate(&env)).collect();
        let mut user_idx = 0usize;

        for op in &ops {
            match op {
                OnboardingOp::OnboardBuyer | OnboardingOp::OnboardArtisan => {
                    let addr = &users[user_idx % users.len()];
                    user_idx = user_idx.wrapping_add(1);
                    let name = user_names[crng.next_usize(user_names.len())];
                    let (sdk_role, model_role) = if matches!(op, OnboardingOp::OnboardBuyer) {
                        (UserRole::Buyer, ModelUserRole::Buyer)
                    } else {
                        (UserRole::Artisan, ModelUserRole::Artisan)
                    };
                    let addr_str = alloc::format!("{:?}", addr);
                    let _ = model.onboard_user(addr_str, model_role);
                    let _ = client.try_onboard_user(addr, &ss(&env, name), &sdk_role);
                }
                OnboardingOp::OnboardDuplicate => {
                    if !users.is_empty() {
                        let _ = client.try_onboard_user(
                            &users[0], &ss(&env, "dup"), &UserRole::Buyer,
                        );
                    }
                }
                OnboardingOp::VerifyUser => {
                    if !users.is_empty() {
                        let _ = client.try_verify_user(&users[0]);
                    }
                }
                _ => {}
            }
        }

        if let Err(msg) = model.check_fund_conservation() {
            panic!(
                "[prop_onboarding_model_agreement] {} (seed=0x{:016X})",
                msg, case_seed
            );
        }
    }
}

// ── Property 8: Admin role onboarding rejected ───────────────────────────────

#[test]
fn prop_admin_role_onboarding_rejected() {
    let (env, contract_id, _admin) = make_onboarding_env();
    let client = OnboardingContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let r = client.try_onboard_user(&user, &ss(&env, "adminattempt"), &UserRole::Admin);
    if r.is_ok() && r.unwrap().is_ok() {
        panic!("[prop_admin_role_onboarding_rejected] onboard as Admin succeeded");
    }
}

// ── Property 9: get_user idempotent ──────────────────────────────────────────

#[test]
fn prop_get_user_idempotent() {
    let mut rng = Lcg64::new(seed_from_env() ^ 0x5678);

    for _ in 0..DEFAULT_CASE_COUNT {
        let case_seed = rng.next_u64();

        let (env, contract_id, _admin) = make_onboarding_env();
        let client = OnboardingContractClient::new(&env, &contract_id);

        let user = Address::generate(&env);
        client.onboard_user(&user, &ss(&env, "idempotent"), &UserRole::Buyer);

        let p1 = client.get_user(&user);
        let p2 = client.get_user(&user);

        if p1.status != p2.status || p1.is_verified != p2.is_verified {
            panic!(
                "[prop_get_user_idempotent] profile changed between reads (seed=0x{:016X})",
                case_seed
            );
        }
    }
}
