#![cfg(test)]

use core::mem::offset_of;

use crate::onboarding::UserOnboardedEvent;
use crate::{
    ArtisanFeeTierUpdatedEvent, ConfigUpdatedEvent, EscrowEvent, EscrowResolvedEvent,
    MetadataVerifiedEvent, PlatformPausedEvent, PlatformUnpausedEvent, RecurringEscrowEvent,
    ReputationUpdateEvent, TokensStakedEvent, TokensUnstakedEvent, UpgradeProposalEvent,
    LIFECYCLE_EVENT_SCHEMA_VERSION,
};

/// Verifies each expected field exists in deterministic declaration order. If a
/// field is renamed, removed, or reordered, this fails at compile/test time.
macro_rules! check_fields {
    ($t:ty, [$($field:ident),+ $(,)?]) => {{
        let offsets = [$( offset_of!($t, $field) , )+];
        for pair in offsets.windows(2) {
            assert!(pair[0] < pair[1], "event fields must keep canonical order");
        }
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
        [schema_version, field_name, old_value, new_value]
    );
}

#[test]
fn snapshot_artisan_fee_tier_updated_event() {
    check_fields!(ArtisanFeeTierUpdatedEvent, [schema_version, artisan, fee_bps]);
}

#[test]
fn snapshot_tokens_staked_event() {
    check_fields!(TokensStakedEvent, [schema_version, artisan, token, amount]);
}

#[test]
fn snapshot_tokens_unstaked_event() {
    check_fields!(TokensUnstakedEvent, [schema_version, artisan, token, amount]);
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
    check_fields!(PlatformPausedEvent, [schema_version, initiator, timestamp]);
}

#[test]
fn snapshot_platform_unpaused_event() {
    check_fields!(PlatformUnpausedEvent, [schema_version, initiator, timestamp]);
}

#[test]
fn snapshot_recurring_escrow_event() {
    check_fields!(
        RecurringEscrowEvent,
        [schema_version, id, action, buyer, artisan, amount, timestamp]
    );
}

#[test]
fn snapshot_upgrade_proposal_event() {
    check_fields!(
        UpgradeProposalEvent,
        [schema_version, action, wasm_hash, admin, timestamp, upgrade_at]
    );
}

#[test]
fn snapshot_user_onboarded_event() {
    check_fields!(UserOnboardedEvent, [schema_version, user, username, role]);
}
