#![cfg(test)]

use core::mem::offset_of;

use crate::onboarding::{
    AttemptRateLimitedEvent, AutoVerifiedEvent, IdentityCorrelatedEvent, OnboardCallFailedEvent,
    PohCredentialRegisteredEvent, ProfileFlaggedEvent, ReviewCompletedEvent,
    SybilPatternDetectedEvent, SybilReviewDecisionEvent, UserOnboardedEvent,
};
use crate::{
    ArtisanFeeTierUpdatedEvent, ConfigUpdatedEvent, EscrowEvent, EscrowResolvedEvent,
    FeeTokenConfigsMigratedEvent, MetadataVerifiedEvent, PlatformPausedEvent,
    PlatformUnpausedEvent, RecurringEscrowEvent, ReputationUpdateEvent, TokensStakedEvent,
    TokensUnstakedEvent, UpgradeApprovalEvent, UpgradeProposalEvent,
    LIFECYCLE_EVENT_SCHEMA_VERSION,
};

/// Verifies each expected field exists on the struct. Canonical declaration
/// order is documented in `test_snapshots/*_event.json`.
macro_rules! check_fields {
    ($t:ty, [$($field:ident),+ $(,)?]) => {{
        let _ = ( $( offset_of!($t, $field) , )+ );
    }};
}

#[test]
fn lifecycle_event_schema_version_is_pinned() {
    assert_eq!(LIFECYCLE_EVENT_SCHEMA_VERSION, 1);
}

#[test]
fn snapshot_escrow_event() {
    check_fields!(
        EscrowEvent,
        [
            schema_version,
            escrow_id,
            action,
            buyer,
            seller,
            amount,
            token,
            timestamp
        ]
    );
}

#[test]
fn snapshot_escrow_resolved_event() {
    check_fields!(
        EscrowResolvedEvent,
        [
            schema_version,
            escrow_id,
            buyer,
            seller,
            arbitrator,
            amount,
            token,
            timestamp
        ]
    );
}

#[test]
fn snapshot_reputation_update_event() {
    check_fields!(
        ReputationUpdateEvent,
        [
            schema_version,
            address,
            successful_delta,
            disputed_delta,
            metrics_sales_delta,
            metrics_amount,
            token,
            timestamp
        ]
    );
}

#[test]
fn snapshot_config_updated_event() {
    check_fields!(
        ConfigUpdatedEvent,
        [schema_version, field_name, old_value, new_value, revision]
    );
}

#[test]
fn snapshot_artisan_fee_tier_updated_event() {
    check_fields!(
        ArtisanFeeTierUpdatedEvent,
        [schema_version, artisan, fee_bps]
    );
}

#[test]
fn snapshot_tokens_staked_event() {
    check_fields!(TokensStakedEvent, [schema_version, artisan, token, amount]);
}

#[test]
fn snapshot_tokens_unstaked_event() {
    check_fields!(
        TokensUnstakedEvent,
        [schema_version, artisan, token, amount]
    );
}

#[test]
fn snapshot_metadata_verified_event() {
    check_fields!(
        MetadataVerifiedEvent,
        [schema_version, order_id, verifier, timestamp]
    );
}

#[test]
fn snapshot_platform_paused_event() {
    check_fields!(
        PlatformPausedEvent,
        [schema_version, initiator, timestamp, revision]
    );
}

#[test]
fn snapshot_platform_unpaused_event() {
    check_fields!(
        PlatformUnpausedEvent,
        [schema_version, initiator, timestamp, revision]
    );
}

#[test]
fn snapshot_recurring_escrow_event() {
    check_fields!(
        RecurringEscrowEvent,
        [
            schema_version,
            id,
            action,
            buyer,
            artisan,
            amount,
            timestamp
        ]
    );
}

#[test]
fn snapshot_upgrade_proposal_event() {
    check_fields!(
        UpgradeProposalEvent,
        [
            schema_version,
            action,
            wasm_hash,
            admin,
            timestamp,
            upgrade_at
        ]
    );
}

#[test]
fn snapshot_upgrade_approval_event() {
    check_fields!(
        UpgradeApprovalEvent,
        [nonce, signer, wasm_hash, timestamp, approval_count]
    );
}

#[test]
fn snapshot_user_onboarded_event() {
    check_fields!(UserOnboardedEvent, [schema_version, user, username, role]);
}

#[test]
fn snapshot_onboard_call_failed_event() {
    check_fields!(
        OnboardCallFailedEvent,
        [schema_version, user, reason, timestamp]
    );
}

#[test]
fn snapshot_auto_verified_event() {
    check_fields!(
        AutoVerifiedEvent,
        [schema_version, user, escrow_count, volume]
    );
}

#[test]
fn snapshot_attempt_rate_limited_event() {
    check_fields!(
        AttemptRateLimitedEvent,
        [
            schema_version,
            user,
            operation,
            scope,
            policy_revision,
            retry_after
        ]
    );
}

#[test]
fn snapshot_sybil_pattern_detected_event() {
    check_fields!(
        SybilPatternDetectedEvent,
        [schema_version, user, reason, timestamp]
    );
}

#[test]
fn snapshot_poh_credential_registered_event() {
    check_fields!(
        PohCredentialRegisteredEvent,
        [schema_version, user, provider_id, credential_hash]
    );
}

#[test]
fn snapshot_identity_correlated_event() {
    check_fields!(
        IdentityCorrelatedEvent,
        [schema_version, user, identity_hash]
    );
}

#[test]
fn snapshot_profile_flagged_event() {
    check_fields!(
        ProfileFlaggedEvent,
        [schema_version, user, reason_code, timestamp]
    );
}

#[test]
fn snapshot_review_completed_event() {
    check_fields!(
        ReviewCompletedEvent,
        [schema_version, user, action, timestamp]
    );
}

#[test]
fn snapshot_sybil_review_decision_event() {
    check_fields!(
        SybilReviewDecisionEvent,
        [
            schema_version,
            user,
            reviewer,
            profile_revision,
            outcome,
            timestamp
        ]
    );
}

#[test]
fn snapshot_fee_token_configs_migrated_event() {
    check_fields!(
        FeeTokenConfigsMigratedEvent,
        [
            schema_version,
            scanned_tokens,
            migrated_configs,
            skipped_existing
        ]
    );
}
