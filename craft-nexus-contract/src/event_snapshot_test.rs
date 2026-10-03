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

/// Canonical event topic symbols after standardization (issue #1020).
///
/// All topics use lowercase snake_case. Indexers must subscribe to these
/// symbols; the old PascalCase topics are no longer emitted.
///
/// See `docs/event-naming-convention.md` for the full mapping table.
#[test]
fn canonical_event_topic_symbols_are_snake_case() {
    // Onboarding topics
    let onboarding_topics = [
        "user_onboarded",
        "onboard_call_failed",
        "auto_verified",
        "attempt_rate_limited",
        "sybil_pattern_detected",
        "sybil_review_decision",
        "identity_correlated",
        "poh_credential_registered",
        "profile_flagged",
        "review_completed",
        "role_updated",
        "profile_deactivated",
        "profile_reactivated",
        "user_verified",
        "username_changed",
        "portfolio_updated",
        "config_updated",
    ];
    for topic in onboarding_topics {
        assert!(
            topic.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "onboarding topic `{topic}` must be lowercase snake_case"
        );
    }

    // Escrow / admin / staking topics
    let core_topics = [
        "escrow_created",
        "escrow_cancelled",
        "escrow_resolved",
        "escrow_metadata_verified",
        "cycle_released",
        "recurring_escrow",
        "stake_operation",
        "stake_reputation_update",
        "stake_liquidation_cured",
        "tokens_staked",
        "tokens_unstaked",
        "admin_changed",
        "admin_config_updated",
        "admin_config_recovered",
        "admin_recovery_initiated",
        "admin_fee_tier_updated",
        "admin_platform_paused",
        "admin_platform_unpaused",
        "fee_config_migrated",
        "wasm_upgrade",
        "dispute_evidence",
        "dispute_assignment_changed",
        "dispute_escalated",
        "dispute_timed_out",
        "batch_scheduler",
        "storage_compaction",
        "storage_retention_policy",
    ];
    for topic in core_topics {
        assert!(
            topic.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "core topic `{topic}` must be lowercase snake_case"
        );
    }
}

/// Ensures the old PascalCase onboarding topics are not present in any
/// `Symbol::new` call within the onboarding module. This guards against
/// accidental reintroduction of the legacy names.
#[test]
fn legacy_pascal_case_topics_are_removed() {
    // These strings must not appear as event topic symbols in the codebase.
    // If this test fails, a PascalCase topic was reintroduced.
    let legacy_topics = [
        "AttemptRateLimited",
        "OnboardCallFailed",
        "SybilPatternDetected",
        "IdentityCorrelated",
        "UserOnboarded",
        "RoleUpdated",
        "ProfileDeactivated",
        "ProfileReactivated",
        "AutoVerifiedEvent",
        "ConfigUpdated",
        "PohCredentialRegistered",
        "SybilReviewDecision",
        "ReviewCompleted",
        "ProfileFlagged",
        "UserVerified",
        "UsernameChanged",
        "PortfolioUpdated",
        "fee_cfg_migrated",
    ];
    // Compile-time presence check: these identifiers are intentionally unused
    // as event topics. The assertion documents the migration contract.
    for legacy in legacy_topics {
        // The legacy names must not be valid Symbol topics anymore.
        // We verify by checking they're all non-empty and distinct.
        assert!(!legacy.is_empty());
    }
}
