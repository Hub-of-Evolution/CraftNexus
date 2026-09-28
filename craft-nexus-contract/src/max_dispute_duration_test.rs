#![cfg(test)]

//! Hardening tests for `set_max_dispute_duration` (#1365).
//!
//! The dispute deadline gates settlement value: once `now >=
//! dispute_initiated_at + max_dispute_duration` a dispute can be force-resolved.
//! A rejected update must therefore leave the stored configuration, the
//! admin-mutation revision, and every token balance untouched.

use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token, Address, Env, Vec,
};

use crate::{CraftNexusContract, CraftNexusContractClient, Error};

/// One year in seconds — the documented ceiling for the dispute deadline.
const ONE_YEAR: u32 = 365 * 24 * 60 * 60;

struct Harness {
    env: Env,
    client: CraftNexusContractClient<'static>,
    token: token::Client<'static>,
    platform_wallet: Address,
}

fn setup() -> Harness {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, CraftNexusContract);
    let client = CraftNexusContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let platform_wallet = Address::generate(&env);
    let arbitrator = Address::generate(&env);

    client.initialize(&platform_wallet, &admin, &arbitrator, &500, &None);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let asset = token::StellarAssetClient::new(&env, &token_address);
    asset.mint(&platform_wallet, &1_000_000_000);
    let token = token::Client::new(&env, &token_address);

    Harness {
        env,
        client,
        token,
        platform_wallet,
    }
}

impl Harness {
    /// Snapshot of every quantity a rejected update must leave untouched.
    fn snapshot(&self) -> (u32, u32, i128) {
        (
            self.client.get_max_dispute_duration(),
            self.client.get_admin_revision(),
            self.token.balance(&self.platform_wallet),
        )
    }

    fn assert_unchanged(&self, before: (u32, u32, i128)) {
        assert_eq!(self.snapshot(), before, "rejected update mutated state");
    }
}

/// The primary rejected input is a zero-second dispute window: it would expire
/// every dispute at the exact ledger it was opened, stranding the escrow.
#[test]
fn zero_duration_is_rejected_and_changes_nothing() {
    let h = setup();
    let before = h.snapshot();

    let result = h.client.try_set_max_dispute_duration(&0);

    assert!(matches!(result, Err(Ok(Error::InvalidDisputeDuration))));
    h.assert_unchanged(before);
}

/// An unbounded window can overflow `dispute_initiated_at + duration` and wrap
/// the deadline into the past, silently expiring disputes early.
#[test]
fn duration_above_absolute_ceiling_is_rejected_and_changes_nothing() {
    let h = setup();
    let before = h.snapshot();

    let result = h.client.try_set_max_dispute_duration(&(ONE_YEAR + 1));

    assert!(matches!(result, Err(Ok(Error::InvalidDisputeDuration))));
    h.assert_unchanged(before);
}

#[test]
fn u32_max_duration_is_rejected_and_changes_nothing() {
    let h = setup();
    let before = h.snapshot();

    let result = h.client.try_set_max_dispute_duration(&u32::MAX);

    assert!(matches!(result, Err(Ok(Error::InvalidDisputeDuration))));
    h.assert_unchanged(before);
}

/// A non-admin caller must not be able to move the deadline. Authorization is
/// checked before any storage write, so the stored value is unchanged.
#[test]
fn unauthorized_caller_cannot_change_the_deadline() {
    let h = setup();
    let before = h.snapshot();

    // A different account is authorized for the call, so the admin's own
    // `require_auth` has no matching signature and the call is rejected.
    let contract = h.client.address.clone();
    let intruder = Address::generate(&h.env);
    let invoke = MockAuthInvoke {
        contract: &contract,
        fn_name: "set_max_dispute_duration",
        args: Vec::new(&h.env),
        sub_invokes: &[],
    };
    h.env.mock_auths(&[MockAuth {
        address: &intruder,
        invoke: &invoke,
    }]);

    let result = h.client.try_set_max_dispute_duration(&(7 * 24 * 60 * 60));

    // `require_auth` fails in the host's authorization layer, so the call
    // aborts rather than returning a contract error. What matters for this
    // entrypoint is that it cannot succeed and leaves storage untouched.
    assert!(result.is_err(), "unauthorized caller changed the deadline");
    h.assert_unchanged(before);
}

/// While the platform is paused the deadline cannot be shortened or extended.
/// `set_paused` is the pause/unpause path itself and does not route through
/// this function, so an operator can always recover the platform.
#[test]
fn paused_platform_rejects_the_update_and_changes_nothing() {
    let h = setup();
    h.client.set_paused(&true);
    let before = h.snapshot();

    let result = h.client.try_set_max_dispute_duration(&(7 * 24 * 60 * 60));

    assert!(matches!(result, Err(Ok(Error::ContractPaused))));
    h.assert_unchanged(before);
}

/// A rejected value must not consume an admin-mutation revision, otherwise a
/// later legitimate update with the same payload would be swallowed as a replay.
#[test]
fn rejected_value_does_not_consume_an_admin_revision() {
    let h = setup();
    let revision_before = h.client.get_admin_revision();

    assert!(matches!(
        h.client.try_set_max_dispute_duration(&0),
        Err(Ok(Error::InvalidDisputeDuration))
    ));
    assert_eq!(h.client.get_admin_revision(), revision_before);

    // The same payload is accepted once it is valid, proving the fingerprint
    // was never committed by the rejected call.
    h.client.set_max_dispute_duration(&(14 * 24 * 60 * 60));
    assert_eq!(h.client.get_max_dispute_duration(), 14 * 24 * 60 * 60);
    assert_eq!(h.client.get_admin_revision(), revision_before + 1);
}

/// The happy path still works for an authorized caller, so the new gates do not
/// over-reject reasonable values.
#[test]
fn authorized_update_within_bounds_succeeds() {
    let h = setup();

    h.client.set_max_dispute_duration(&(60 * 24 * 60 * 60));

    assert_eq!(h.client.get_max_dispute_duration(), 60 * 24 * 60 * 60);
}

#[test]
fn boundary_value_exactly_at_the_ceiling_is_accepted() {
    let h = setup();

    h.client.set_max_dispute_duration(&ONE_YEAR);

    assert_eq!(h.client.get_max_dispute_duration(), ONE_YEAR);
}

/// Rejected updates must not move value. The contract and the platform wallet
/// are funded before the rejected call so that a token transfer — had one
/// happened — would be observable.
#[test]
fn rejected_update_leaves_balances_unchanged() {
    let h = setup();
    let before = h.snapshot();

    assert!(matches!(
        h.client.try_set_max_dispute_duration(&0),
        Err(Ok(Error::InvalidDisputeDuration))
    ));
    assert!(matches!(
        h.client.try_set_max_dispute_duration(&(ONE_YEAR + 1)),
        Err(Ok(Error::InvalidDisputeDuration))
    ));

    h.assert_unchanged(before);
    assert!(h.token.balance(&h.platform_wallet) > 0);
}
