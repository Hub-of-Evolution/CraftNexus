#![cfg(test)]

//! Shared helpers for wiring a real onboarding contract into test setups.
//!
//! Escrow operations that go through the onboarding attestation boundary
//! (`create_escrow`, `fund_escrow`, `release_funds`, `refund`, dispute
//! settlement, batch creation, ...) require a deployed, configured
//! `OnboardingContract` that can issue and validate an attestation for the
//! parties. Tests that supply a bare `Address::generate` stub now hit a
//! `MissingValue` panic instead of the intended behaviour, so every setup that
//! attaches an onboarding contract must deploy the real one and onboard its
//! parties before exercising those paths.

use crate::onboarding::{OnboardingContract, OnboardingContractClient, UserRole};
use soroban_sdk::{
    testutils::Address as _,
    Address, Env, String,
};

/// Deploy and configure an onboarding contract pointed at `escrow_contract`.
///
/// The onboarding contract is initialized with a fresh admin and its
/// `escrow_contract` is wired to the given escrow address (required for
/// `get_onboarding_attestation` to validate the `contract_instance`).
///
/// NOTE: callers must initialize the escrow contract *after* this so that
/// `onboard_user`'s `is_paused` probe against the escrow contract succeeds.
pub fn deploy_onboarding(env: &Env, escrow_contract: &Address) -> OnboardingContractClient<'static> {
    let onboarding_id = env.register_contract(None, OnboardingContract);
    let client = OnboardingContractClient::new(env, &onboarding_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    client.set_escrow_contract(escrow_contract);
    client
}

/// Onboard a single user with the given role.
pub fn onboard(env: &Env, client: &OnboardingContractClient<'static>, user: &Address, username: &str, role: UserRole) {
    client.onboard_user(user, &String::from_str(env, username), &role);
}

/// Onboard a buyer (Buyer role) and a seller (Artisan role).
pub fn onboard_buyer_and_seller(
    env: &Env,
    client: &OnboardingContractClient<'static>,
    buyer: &Address,
    seller: &Address,
) {
    onboard(env, client, buyer, "buyer_user", UserRole::Buyer);
    onboard(env, client, seller, "seller_user", UserRole::Artisan);
}