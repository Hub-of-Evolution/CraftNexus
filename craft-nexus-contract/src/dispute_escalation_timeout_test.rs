#![cfg(test)]
#![cfg(not(target_family = "wasm"))]
//! Dispute escalation timeouts (#1080).
//!
//! Covers the three acceptance criteria:
//!
//! 1. every pending dispute has a final deadline;
//! 2. escalation permissions are explicit and auditable;
//! 3. a timed-out dispute cannot be resolved twice.

extern crate alloc;

use super::*;
use crate::onboarding::{OnboardingContract, OnboardingContractClient, UserRole as OnboardingRole};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String, Symbol,
};

const DAY: u64 = 24 * 60 * 60;
const START: u64 = 1_700_000_000;
const AMOUNT: i128 = 10_000_000;

struct Harness {
    env: Env,
    escrow: CraftNexusContractClient<'static>,
    onboarding: OnboardingContractClient<'static>,
    buyer: Address,
    seller: Address,
    admin: Address,
    arbitrator: Address,
    moderator: Address,
    token: Address,
    token_admin: token::StellarAssetClient<'static>,
}

impl Harness {
    /// Wire up a real onboarding contract next to the escrow contract so the
    /// onboarding attestation checks on the escalation path run for real.
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.budget().reset_unlimited();
        env.ledger().with_mut(|li| li.timestamp = START);

        let onboarding_id = env.register_contract(None, OnboardingContract);
        let onboarding = OnboardingContractClient::new(&env, &onboarding_id);
        let escrow_id = env.register_contract(None, CraftNexusContract);
        let escrow = CraftNexusContractClient::new(&env, &escrow_id);

        let admin = Address::generate(&env);
        let platform_wallet = Address::generate(&env);
        let arbitrator = Address::generate(&env);
        let moderator = Address::generate(&env);

        onboarding.initialize(&admin);
        onboarding.set_escrow_contract(&escrow_id);

        escrow.initialize(
            &platform_wallet,
            &admin,
            &arbitrator,
            &500,
            &Some(onboarding_id),
        );
        escrow.set_min_release_window(&1);
        // Arbitration must be reachable the moment a dispute opens so the tests
        // can isolate the escalation clock from the evidence challenge clock.
        escrow.set_evidence_challenge_window(&0);
        escrow.set_moderator(&moderator);

        let buyer = Address::generate(&env);
        let seller = Address::generate(&env);
        onboarding.onboard_user(
            &buyer,
            &String::from_str(&env, "buyer_user"),
            &OnboardingRole::Buyer,
        );
        onboarding.onboard_user(
            &seller,
            &String::from_str(&env, "seller_user"),
            &OnboardingRole::Artisan,
        );

        let token_issuer = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(token_issuer);
        let token_admin = token::StellarAssetClient::new(&env, &token_contract.address());

        Self {
            env,
            escrow,
            onboarding,
            buyer,
            seller,
            admin,
            arbitrator,
            moderator,
            token: token_contract.address(),
            token_admin,
        }
    }

    fn dispute(&self, order_id: u32) {
        self.token_admin.mint(&self.buyer, &(AMOUNT * 2));
        self.escrow.create_escrow(
            &self.buyer,
            &self.seller,
            &self.token,
            &AMOUNT,
            &order_id,
            &Some(3600u32),
        );
        self.escrow
            .dispute_escrow(&order_id, &Symbol::new(&self.env, "stalled"), &self.buyer);
    }

    fn warp_to(&self, offset_from_dispute: u64) {
        self.env
            .ledger()
            .with_mut(|li| li.timestamp = START + offset_from_dispute);
    }

    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(who)
    }
}

/// Assert the error of a contract function that signals failure by panicking.
fn assert_contract_error<T>(
    result: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: Error,
) {
    let expected_err = soroban_sdk::Error::from_contract_error(expected as u32);
    assert!(
        matches!(result, Err(Ok(err)) if err == expected_err),
        "expected contract error {:?}",
        expected
    );
}

/// Assert the error of a contract function declared as returning `Result<_, Error>`.
///
/// The generated `try_*` for those carries the crate error type directly rather
/// than an opaque `soroban_sdk::Error`, and is generic over the return-value
/// conversion error, which varies with the return type.
fn assert_typed_error<T, C>(
    result: Result<Result<T, C>, Result<Error, soroban_sdk::InvokeError>>,
    expected: Error,
) {
    assert!(
        matches!(result, Err(Ok(err)) if err == expected),
        "expected contract error {:?}",
        expected
    );
}

// ── Criterion 1: pending disputes have a final deadline ───────────────────────

#[test]
fn pending_dispute_exposes_a_final_deadline() {
    let h = Harness::new();
    h.dispute(1);

    let deadline = h.escrow.get_dispute_final_deadline(&1);
    assert_eq!(
        deadline,
        START + DEFAULT_MAX_DISPUTE_DURATION as u64,
        "the final deadline is max_dispute_duration after the dispute opened"
    );

    let status = h.escrow.get_dispute_escalation_status(&1);
    assert_eq!(status.schedule.initiated_at, START);
    assert_eq!(status.schedule.final_deadline, deadline);
    assert_eq!(
        status.schedule.party_deadline,
        START + DEFAULT_DISPUTE_ESCALATION_WINDOW as u64
    );
    assert_eq!(
        status.schedule.moderator_deadline,
        START + DEFAULT_MODERATOR_ESCALATION_CHECKPOINT as u64
    );
    assert_eq!(
        status.schedule.admin_deadline,
        START + DEFAULT_ADMIN_ESCALATION_CHECKPOINT as u64
    );
    assert!(!status.is_timed_out);
    assert!(!status.is_finalized);
}

#[test]
fn checkpoints_are_clamped_when_the_final_deadline_is_shortened() {
    let h = Harness::new();
    h.dispute(1);

    // Two days is shorter than every default checkpoint offset.
    h.escrow.set_max_dispute_duration(&(2 * DAY as u32));

    let status = h.escrow.get_dispute_escalation_status(&1);
    let final_deadline = START + 2 * DAY;
    assert_eq!(status.schedule.final_deadline, final_deadline);
    // Clamping works backwards from the final deadline, so no checkpoint can be
    // scheduled past the point at which the dispute is already over.
    assert_eq!(status.schedule.admin_deadline, final_deadline);
    assert_eq!(status.schedule.moderator_deadline, final_deadline);
    assert_eq!(status.schedule.party_deadline, final_deadline);

    h.warp_to(2 * DAY - 1);
    assert_eq!(
        h.escrow.get_dispute_escalation_status(&1).current_tier,
        EscalationTier::Assigned
    );

    // A collapsed schedule resolves to the highest reached tier, never to an
    // intermediate one that can no longer be acted on.
    h.warp_to(2 * DAY);
    let status = h.escrow.get_dispute_escalation_status(&1);
    assert_eq!(status.current_tier, EscalationTier::TimedOut);
    assert!(status.is_timed_out);
}

#[test]
fn tier_boundaries_are_inclusive_ended() {
    let h = Harness::new();
    h.dispute(1);

    let cases = [
        (0, EscalationTier::Assigned),
        (3 * DAY - 1, EscalationTier::Assigned),
        (3 * DAY, EscalationTier::PartyFlagged),
        (7 * DAY - 1, EscalationTier::PartyFlagged),
        (7 * DAY, EscalationTier::ModeratorReview),
        (14 * DAY - 1, EscalationTier::ModeratorReview),
        (14 * DAY, EscalationTier::AdminReview),
        (30 * DAY - 1, EscalationTier::AdminReview),
        (30 * DAY, EscalationTier::TimedOut),
        (60 * DAY, EscalationTier::TimedOut),
    ];

    for (offset, expected) in cases {
        h.warp_to(offset);
        assert_eq!(
            h.escrow.get_dispute_escalation_status(&1).current_tier,
            expected,
            "unexpected tier at dispute + {offset}s"
        );
    }
}

// ── Criterion 2: escalation permissions are explicit and auditable ────────────

#[test]
fn escalation_is_locked_while_the_arbitrator_window_is_open() {
    let h = Harness::new();
    h.dispute(1);

    assert!(!h.escrow.can_escalate_dispute(&1, &h.buyer));
    assert_contract_error(
        h.escrow.try_escalate_dispute(&1, &h.buyer),
        Error::EscalationWindowActive,
    );
    assert!(h.escrow.get_dispute_escalation_state(&1).is_none());
}

#[test]
fn ladder_advances_one_tier_per_checkpoint_and_records_who_escalated() {
    let h = Harness::new();
    h.dispute(1);

    h.warp_to(3 * DAY);
    h.escrow.escalate_dispute(&1, &h.buyer);
    let state = h.escrow.get_dispute_escalation_state(&1).unwrap();
    assert_eq!(state.tier, EscalationTier::PartyFlagged);
    assert_eq!(state.previous_tier, EscalationTier::Assigned);
    assert_eq!(state.escalated_by, h.buyer);
    assert_eq!(state.escalated_at, START + 3 * DAY);
    assert_eq!(state.escalation_count, 1);

    h.warp_to(7 * DAY);
    h.escrow.escalate_dispute(&1, &h.moderator);
    let state = h.escrow.get_dispute_escalation_state(&1).unwrap();
    assert_eq!(state.tier, EscalationTier::ModeratorReview);
    assert_eq!(state.previous_tier, EscalationTier::PartyFlagged);
    assert_eq!(state.escalated_by, h.moderator);
    assert_eq!(state.escalation_count, 2);

    h.warp_to(14 * DAY);
    h.escrow.escalate_dispute(&1, &h.admin);
    let state = h.escrow.get_dispute_escalation_state(&1).unwrap();
    assert_eq!(state.tier, EscalationTier::AdminReview);
    assert_eq!(state.escalated_by, h.admin);
    assert_eq!(state.escalation_count, 3);

    // The timeout tier is permissionless: a bot with no relationship to the
    // escrow can flag it.
    h.warp_to(30 * DAY);
    let bot = Address::generate(&h.env);
    h.escrow.escalate_dispute(&1, &bot);
    let state = h.escrow.get_dispute_escalation_state(&1).unwrap();
    assert_eq!(state.tier, EscalationTier::TimedOut);
    assert_eq!(state.escalated_by, bot);
    assert_eq!(state.escalation_count, 4);

    // The #941 single-shot record still points at the first escalation.
    let legacy = h.escrow.get_dispute_escalation(&1).unwrap();
    assert_eq!(legacy.escalated_by, h.buyer);
    assert_eq!(legacy.escalated_at, START + 3 * DAY);
}

#[test]
fn escalating_twice_within_one_checkpoint_is_rejected() {
    let h = Harness::new();
    h.dispute(1);

    h.warp_to(3 * DAY);
    h.escrow.escalate_dispute(&1, &h.buyer);

    // Same checkpoint, different eligible party: still no new tier to reach.
    assert!(!h.escrow.can_escalate_dispute(&1, &h.seller));
    assert_contract_error(
        h.escrow.try_escalate_dispute(&1, &h.seller),
        Error::InvalidDisputeAction,
    );
    assert_eq!(
        h.escrow
            .get_dispute_escalation_state(&1)
            .unwrap()
            .escalation_count,
        1
    );
}

#[test]
fn permission_matrix_widens_but_never_narrows() {
    let h = Harness::new();
    h.dispute(1);
    let stranger = Address::generate(&h.env);

    // Tier 1 is reserved for the parties to the escrow.
    h.warp_to(3 * DAY);
    assert!(h.escrow.can_escalate_dispute(&1, &h.buyer));
    assert!(h.escrow.can_escalate_dispute(&1, &h.seller));
    assert!(!h.escrow.can_escalate_dispute(&1, &h.moderator));
    assert!(!h.escrow.can_escalate_dispute(&1, &h.admin));
    assert!(!h.escrow.can_escalate_dispute(&1, &stranger));
    assert_contract_error(
        h.escrow.try_escalate_dispute(&1, &h.moderator),
        Error::Unauthorized,
    );

    // Tier 2 adds the privileged resolvers without dropping the parties.
    h.warp_to(7 * DAY);
    assert!(h.escrow.can_escalate_dispute(&1, &h.buyer));
    assert!(h.escrow.can_escalate_dispute(&1, &h.moderator));
    assert!(h.escrow.can_escalate_dispute(&1, &h.arbitrator));
    assert!(h.escrow.can_escalate_dispute(&1, &h.admin));
    assert!(!h.escrow.can_escalate_dispute(&1, &stranger));
    assert_contract_error(
        h.escrow.try_escalate_dispute(&1, &stranger),
        Error::Unauthorized,
    );

    // Past the final deadline escalation is permissionless.
    h.warp_to(30 * DAY);
    assert!(h.escrow.can_escalate_dispute(&1, &stranger));
    h.escrow.escalate_dispute(&1, &stranger);
}

#[test]
fn escalation_requires_a_pending_dispute() {
    let h = Harness::new();
    h.token_admin.mint(&h.buyer, &(AMOUNT * 2));
    h.escrow
        .create_escrow(&h.buyer, &h.seller, &h.token, &AMOUNT, &1, &Some(3600u32));

    h.warp_to(3 * DAY);
    assert!(!h.escrow.can_escalate_dispute(&1, &h.buyer));
    assert_contract_error(
        h.escrow.try_escalate_dispute(&1, &h.buyer),
        Error::NotInDispute,
    );
    // An escrow that does not exist at all is simply not escalatable.
    assert!(!h.escrow.can_escalate_dispute(&999, &h.buyer));
}

#[test]
fn a_settled_dispute_cannot_be_escalated() {
    let h = Harness::new();
    h.dispute(1);

    h.warp_to(3 * DAY);
    h.escrow
        .resolve_dispute(&1, &Resolution::RefundToBuyer, &h.arbitrator);

    assert!(!h.escrow.can_escalate_dispute(&1, &h.buyer));
    assert_contract_error(
        h.escrow.try_escalate_dispute(&1, &h.buyer),
        Error::SettlementAlreadyFinalized,
    );
}

// ── Checkpoint configuration ─────────────────────────────────────────────────

#[test]
fn admin_can_reschedule_the_checkpoints() {
    let h = Harness::new();
    h.escrow
        .set_escalation_checkpoints(&(DAY as u32), &(2 * DAY as u32), &(4 * DAY as u32));

    let checkpoints = h.escrow.get_escalation_checkpoints();
    assert_eq!(checkpoints.party_checkpoint, DAY as u32);
    assert_eq!(checkpoints.moderator_checkpoint, 2 * DAY as u32);
    assert_eq!(checkpoints.admin_checkpoint, 4 * DAY as u32);
    // The tier-1 checkpoint and the legacy escalation window stay in sync.
    assert_eq!(
        h.escrow.get_platform_config().dispute_escalation_window,
        DAY as u32
    );

    h.dispute(1);
    h.warp_to(DAY);
    h.escrow.escalate_dispute(&1, &h.buyer);
    assert_eq!(
        h.escrow.get_dispute_escalation_state(&1).unwrap().tier,
        EscalationTier::PartyFlagged
    );
    h.warp_to(4 * DAY);
    assert_eq!(
        h.escrow.get_dispute_escalation_status(&1).current_tier,
        EscalationTier::AdminReview
    );
}

#[test]
fn non_monotonic_checkpoints_are_rejected() {
    let h = Harness::new();
    let day = DAY as u32;

    // party >= moderator
    assert_contract_error(
        h.escrow
            .try_set_escalation_checkpoints(&(2 * day), &(2 * day), &(4 * day)),
        Error::InvalidEscalationPolicy,
    );
    // moderator >= admin
    assert_contract_error(
        h.escrow
            .try_set_escalation_checkpoints(&day, &(5 * day), &(4 * day)),
        Error::InvalidEscalationPolicy,
    );
    // admin checkpoint must sit strictly before the final deadline
    assert_contract_error(
        h.escrow
            .try_set_escalation_checkpoints(&day, &(2 * day), &DEFAULT_MAX_DISPUTE_DURATION),
        Error::InvalidEscalationPolicy,
    );
    // a zero-length first checkpoint would make escalation instant
    assert_contract_error(
        h.escrow
            .try_set_escalation_checkpoints(&0, &(2 * day), &(4 * day)),
        Error::InvalidEscalationPolicy,
    );

    // None of the rejected calls may have mutated the schedule.
    let checkpoints = h.escrow.get_escalation_checkpoints();
    assert_eq!(
        checkpoints.party_checkpoint,
        DEFAULT_DISPUTE_ESCALATION_WINDOW
    );
    assert_eq!(
        checkpoints.admin_checkpoint,
        DEFAULT_ADMIN_ESCALATION_CHECKPOINT
    );
}

#[test]
fn setting_the_legacy_window_keeps_later_checkpoints_ordered() {
    let h = Harness::new();
    // Push tier 1 past the default tier-2 and tier-3 offsets.
    h.escrow.set_dispute_escalation_window(&(20 * DAY as u32));

    let checkpoints = h.escrow.get_escalation_checkpoints();
    assert_eq!(checkpoints.party_checkpoint, 20 * DAY as u32);
    assert_eq!(checkpoints.moderator_checkpoint, 20 * DAY as u32);
    assert_eq!(checkpoints.admin_checkpoint, 20 * DAY as u32);

    h.dispute(1);
    h.warp_to(20 * DAY - 1);
    assert_eq!(
        h.escrow.get_dispute_escalation_status(&1).current_tier,
        EscalationTier::Assigned
    );
    h.warp_to(20 * DAY);
    assert_eq!(
        h.escrow.get_dispute_escalation_status(&1).current_tier,
        EscalationTier::AdminReview
    );
}

// ── Criterion 3: a timed-out dispute cannot be resolved twice ─────────────────

#[test]
fn arbitration_is_closed_off_once_the_final_deadline_passes() {
    let h = Harness::new();
    h.dispute(1);

    h.warp_to(DEFAULT_MAX_DISPUTE_DURATION as u64);
    assert_contract_error(
        h.escrow
            .try_resolve_dispute(&1, &Resolution::ReleaseToSeller, &h.arbitrator),
        Error::ArbitratorDeadlineExceeded,
    );
    assert_eq!(h.escrow.get_escrow(&1).status, EscrowStatus::Disputed);
}

#[test]
fn timeout_settlement_is_deterministic_and_runs_exactly_once() {
    let h = Harness::new();
    h.dispute(1);
    let buyer_before = h.balance(&h.buyer);

    // The outcome is knowable before the timeout happens.
    assert_eq!(
        h.escrow.get_timeout_outcome(),
        TimeoutOutcome::RefundBuyerFull
    );
    assert_eq!(
        h.escrow.get_dispute_escalation_status(&1).timeout_outcome,
        TimeoutOutcome::RefundBuyerFull
    );

    h.warp_to(DEFAULT_MAX_DISPUTE_DURATION as u64);
    h.escrow.resolve_expired_dispute(&1);

    assert_eq!(h.escrow.get_escrow(&1).status, EscrowStatus::Resolved);
    assert_eq!(
        h.balance(&h.buyer) - buyer_before,
        AMOUNT,
        "the default policy refunds the buyer in full"
    );

    // Second attempt on the same dispute is rejected by the settlement receipt.
    assert_typed_error(
        h.escrow.try_resolve_expired_dispute(&1),
        Error::SettlementAlreadyFinalized,
    );
    // And so is a late arbitrated resolution.
    assert_contract_error(
        h.escrow
            .try_resolve_dispute(&1, &Resolution::ReleaseToSeller, &h.arbitrator),
        Error::SettlementAlreadyFinalized,
    );
    assert_eq!(h.balance(&h.buyer) - buyer_before, AMOUNT);
}

#[test]
fn timeout_settlement_is_rejected_before_the_final_deadline() {
    let h = Harness::new();
    h.dispute(1);

    h.warp_to(DEFAULT_MAX_DISPUTE_DURATION as u64 - 1);
    assert_typed_error(
        h.escrow.try_resolve_expired_dispute(&1),
        Error::DisputeExpired,
    );
    assert_eq!(h.escrow.get_escrow(&1).status, EscrowStatus::Disputed);
}

#[test]
fn timeout_outcome_tracks_the_configured_fee_policy() {
    let h = Harness::new();

    let cases = [
        (
            ExpiredDisputeFeePolicy::RefundFullNoPlatformFee,
            TimeoutOutcome::RefundBuyerFull,
        ),
        (
            ExpiredDisputeFeePolicy::DeductFeeFromSeller,
            TimeoutOutcome::RefundBuyerFull,
        ),
        (
            ExpiredDisputeFeePolicy::RefundMinusPlatformFee,
            TimeoutOutcome::RefundBuyerMinusFee,
        ),
        (
            ExpiredDisputeFeePolicy::SplitFee,
            TimeoutOutcome::RefundBuyerSplitFee,
        ),
    ];

    for (policy, expected) in cases {
        h.escrow.update_expired_dispute_policy(&policy);
        assert_eq!(h.escrow.get_timeout_outcome(), expected);
    }
}

#[test]
fn timeout_under_fee_deducting_policy_matches_the_previewed_outcome() {
    let h = Harness::new();
    h.escrow
        .update_expired_dispute_policy(&ExpiredDisputeFeePolicy::RefundMinusPlatformFee);
    h.dispute(1);
    let buyer_before = h.balance(&h.buyer);

    assert_eq!(
        h.escrow.get_timeout_outcome(),
        TimeoutOutcome::RefundBuyerMinusFee
    );

    h.warp_to(DEFAULT_MAX_DISPUTE_DURATION as u64);
    h.escrow.resolve_expired_dispute(&1);

    // 5% platform fee is withheld from the refund.
    let fee = AMOUNT * 500 / 10_000;
    assert_eq!(h.balance(&h.buyer) - buyer_before, AMOUNT - fee);
}

#[test]
fn escalation_status_reports_finalization() {
    let h = Harness::new();
    h.dispute(1);

    h.warp_to(DEFAULT_MAX_DISPUTE_DURATION as u64);
    let before = h.escrow.get_dispute_escalation_status(&1);
    assert!(before.is_timed_out);
    assert!(!before.is_finalized);

    h.escrow.resolve_expired_dispute(&1);

    // Once settled the escrow leaves the Disputed state entirely, so the
    // status view no longer applies to it.
    assert_typed_error(
        h.escrow.try_get_dispute_escalation_status(&1),
        Error::InvalidEscrowState,
    );
    assert_typed_error(
        h.escrow.try_get_dispute_final_deadline(&1),
        Error::InvalidEscrowState,
    );
}

#[test]
fn privileged_escalators_do_not_need_an_onboarding_profile() {
    let h = Harness::new();
    h.dispute(1);

    // Only the parties are asked for an onboarding attestation. The moderator
    // and the unrelated bot below have no onboarding profile at all, so if the
    // attestation check were applied to them these escalations would panic.
    assert!(!h.onboarding.is_onboarded(&h.moderator));

    h.warp_to(7 * DAY);
    h.escrow.escalate_dispute(&1, &h.moderator);
    assert_eq!(
        h.escrow.get_dispute_escalation_state(&1).unwrap().tier,
        EscalationTier::ModeratorReview
    );

    h.warp_to(14 * DAY);
    h.escrow.escalate_dispute(&1, &h.admin);
    assert_eq!(
        h.escrow.get_dispute_escalation_state(&1).unwrap().tier,
        EscalationTier::AdminReview
    );

    h.warp_to(30 * DAY);
    let bot = Address::generate(&h.env);
    assert!(!h.onboarding.is_onboarded(&bot));
    h.escrow.escalate_dispute(&1, &bot);
    assert_eq!(
        h.escrow.get_dispute_escalation_state(&1).unwrap().tier,
        EscalationTier::TimedOut
    );
}

#[test]
fn party_escalation_still_presents_an_onboarding_attestation() {
    let h = Harness::new();
    h.dispute(1);

    // The parties are onboarded, so the attestation the escalation path demands
    // resolves and tier 1 is reachable by either of them. (The negative case
    // cannot be staged here: a profile with an open dispute cannot be
    // deactivated, since `deactivate_profile` refuses while escrows are active.)
    h.warp_to(3 * DAY);
    assert!(h.escrow.can_escalate_dispute(&1, &h.buyer));
    assert!(h.escrow.can_escalate_dispute(&1, &h.seller));
    h.escrow.escalate_dispute(&1, &h.seller);
    assert_eq!(
        h.escrow
            .get_dispute_escalation_state(&1)
            .unwrap()
            .escalated_by,
        h.seller
    );
}
