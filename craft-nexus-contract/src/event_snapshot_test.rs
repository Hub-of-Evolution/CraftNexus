#![cfg(test)]

use core::mem::offset_of;

use crate::{
    ArtisanFeeTierUpdatedEvent, ConfigUpdatedEvent, EscrowEvent, EscrowResolvedEvent,
    MetadataVerifiedEvent, PlatformPausedEvent, PlatformUnpausedEvent, RecurringEscrowEvent,
    ReputationUpdateEvent, TokensStakedEvent, TokensUnstakedEvent, UpgradeProposalEvent,
};
use crate::onboarding::UserOnboardedEvent;

/// Verifies each expected field exists on the struct. If a field is renamed or
/// removed, the corresponding `offset_of!` line fails to compile.
macro_rules! check_fields {
    ($t:ty, [$($field:ident),+ $(,)?]) => {{
        let _ = ( $( offset_of!($t, $field) , )+ );
    }};
}

#[test]
fn snapshot_escrow_event() {
    check_fields!(EscrowEvent, [escrow_id, action, buyer, seller, amount, token, timestamp]);
}

#[test]
fn snapshot_escrow_resolved_event() {
    check_fields!(EscrowResolvedEvent, [escrow_id, buyer, seller, arbitrator, amount, token, timestamp]);
}

#[test]
fn snapshot_reputation_update_event() {
    check_fields!(ReputationUpdateEvent, [address, successful_delta, disputed_delta, metrics_sales_delta, metrics_amount, token, timestamp]);
}

#[test]
fn snapshot_config_updated_event() {
    check_fields!(ConfigUpdatedEvent, [field_name, old_value, new_value]);
}

#[test]
fn snapshot_artisan_fee_tier_updated_event() {
    check_fields!(ArtisanFeeTierUpdatedEvent, [artisan, fee_bps]);
}

#[test]
fn snapshot_tokens_staked_event() {
    check_fields!(TokensStakedEvent, [artisan, token, amount]);
}

#[test]
fn snapshot_tokens_unstaked_event() {
    check_fields!(TokensUnstakedEvent, [artisan, token, amount]);
}

#[test]
fn snapshot_metadata_verified_event() {
    check_fields!(MetadataVerifiedEvent, [order_id, verifier, timestamp]);
}

#[test]
fn snapshot_platform_paused_event() {
    check_fields!(PlatformPausedEvent, [initiator, timestamp]);
}

#[test]
fn snapshot_platform_unpaused_event() {
    check_fields!(PlatformUnpausedEvent, [initiator, timestamp]);
}

#[test]
fn snapshot_recurring_escrow_event() {
    check_fields!(RecurringEscrowEvent, [id, action, buyer, artisan, amount, timestamp]);
}

#[test]
fn snapshot_upgrade_proposal_event() {
    check_fields!(UpgradeProposalEvent, [action, wasm_hash, admin, timestamp, upgrade_at]);
}

#[test]
fn snapshot_user_onboarded_event() {
    check_fields!(UserOnboardedEvent, [user, username, role]);
}
