# Contract Event Naming Convention

This document defines the single, predictable naming convention for every event
topic emitted by the CraftNexus contracts (escrow, staking, onboarding, admin,
upgrade, and storage-lifecycle flows). It is the source of truth referenced by
the README "Event Reference" table and by the snapshot tests in
`src/event_snapshot_test.rs`.

## The convention

1. **Topic identifiers are `snake_case`.** Every event topic symbol is a
   lowercase `snake_case` string, e.g. `escrow`, `escrow_resolved`,
   `admin_changed`, `user_onboarded`. No `PascalCase`, no `camelCase`, no
   mixed-case identifiers.
2. **Topics are nouns or noun phrases describing the fact that happened**, not
   the function that emitted them. Prefer `escrow_resolved` over
   `resolve_escrow`.
3. **Domain prefixes group related events.** Admin/configuration events use the
   `admin_` prefix (`admin_changed`, `admin_config_updated`,
   `admin_fee_tier_updated`, `admin_platform_paused`, `admin_platform_unpaused`,
   `admin_config_recovered`). Staking events use `stake_` / `tokens_` prefixes.
4. **Payload meaning never changes when a topic is renamed.** Renames are
   topic-only; the payload struct, its field order, and its
   `schema_version` are untouched.
5. **Topic symbols stay within the Soroban `Symbol` length limit** (9
   characters for `symbol_short!`, 32 for `Symbol::new`). Longer identifiers use
   `Symbol::new`.

## Canonical naming table

| Flow | Canonical topic | Payload | Emitted by |
|------|-----------------|---------|------------|
| Escrow | `escrow` | `EscrowEvent` | `emit_escrow_created`, `create_escrow`, `batch_create` |
| Escrow | `escrow_resolved` | `EscrowResolvedEvent` | `emit_escrow_resolved_event`, `resolve_escrow` |
| Escrow | `escrow_cancelled` | `i128` refund amount | `cancel_escrow` |
| Escrow | `recurring_escrow` | `RecurringEscrowEvent` | recurring escrow APIs |
| Escrow | `cycle_released` | `i128` cycle amount | `release_next_cycle` |
| Escrow | `metadata_verified` | `MetadataVerifiedEvent` | `emit_metadata_verified`, `verify_metadata` |
| Staking | `stake_reputation_update` | `ReputationUpdateEvent` | `emit_reputation_update` |
| Staking | `tokens_staked` | `TokensStakedEvent` | staking functions |
| Staking | `tokens_unstaked` | `TokensUnstakedEvent` | unstaking functions |
| Staking | `stake_operation` | `(artisan, new_stake)` | stake history helpers |
| Staking | `stake_history_warning` | `String` | stake history maintenance |
| Admin | `admin_changed` | `(previous_admin, new_admin)` | `emit_admin_changed`, `update_admin` |
| Admin | `admin_config_updated` | `ConfigUpdatedEvent` | `emit_config_updated`, config setters |
| Admin | `admin_fee_tier_updated` | `ArtisanFeeTierUpdatedEvent` | `emit_artisan_fee_tier_updated` |
| Admin | `admin_platform_paused` | `PlatformPausedEvent` | `emit_platform_paused` |
| Admin | `admin_platform_unpaused` | `PlatformUnpausedEvent` | `emit_platform_unpaused` |
| Admin | `admin_config_recovered` | `String` | config recovery path |
| Upgrade | `wasm_upgrade` | `UpgradeProposalEvent` | `emit_upgrade_event` |
| Upgrade | `fee_cfg_migrated` | `FeeTokenConfigsMigratedEvent` | migration utilities |
| Onboarding | `user_onboarded` | `UserOnboardedEvent` | `onboard_user` |
| Onboarding | `onboard_call_failed` | `OnboardCallFailedEvent` | `emit_onboard_failed_and_panic` |
| Onboarding | `role_updated` | `(user, old_role, new_role)` | `update_user_role` |
| Onboarding | `profile_deactivated` | `(user, role)` | `deactivate_profile` |
| Onboarding | `profile_reactivated` | `(user, role)` | `reactivate_profile` |
| Onboarding | `user_verified` | `Address` | `verify_user`, `auto_verify_user`, `process_verification_request` |
| Onboarding | `username_changed` | `Address` | `change_username` |
| Onboarding | `username_changed_revoked` | audit symbol | username-change flow |
| Onboarding | `portfolio_updated` | `Address` | `update_portfolio` |
| Onboarding | `auto_verified` | `AutoVerifiedEvent` | `try_auto_verify` |
| Onboarding | `attempt_rate_limited` | `AttemptRateLimitedEvent` | rate-limit guard |
| Onboarding | `sybil_pattern_detected` | `SybilPatternDetectedEvent` | sybil detection |
| Onboarding | `identity_correlated` | `IdentityCorrelatedEvent` | identity correlation check |
| Onboarding | `poh_credential_registered` | `PohCredentialRegisteredEvent` | PoH registration |
| Onboarding | `profile_flagged` | `ProfileFlaggedEvent` | admin flagging |
| Onboarding | `review_completed` | `ReviewCompletedEvent` | review processing |
| Onboarding | `sybil_review_decision` | `SybilReviewDecisionEvent` | sybil review decision |

## Indexer migration notes

The onboarding flow previously emitted `PascalCase` topics. Those topics have
been renamed to the canonical `snake_case` form. **Payloads are unchanged** —
only the topic symbol differs, so indexers only need to update their topic
filters.

| Old topic (deprecated) | New canonical topic |
|------------------------|---------------------|
| `UserOnboarded` | `user_onboarded` |
| `OnboardCallFailed` | `onboard_call_failed` |
| `RoleUpdated` | `role_updated` |
| `ProfileDeactivated` | `profile_deactivated` |
| `ProfileReactivated` | `profile_reactivated` |
| `UserVerified` | `user_verified` |
| `UsernameChanged` | `username_changed` |
| `UsernameChangedRevoked` | `username_changed_revoked` |
| `PortfolioUpdated` | `portfolio_updated` |
| `AutoVerifiedEvent` | `auto_verified` |
| `AttemptRateLimited` | `attempt_rate_limited` |
| `SybilPatternDetected` | `sybil_pattern_detected` |
| `IdentityCorrelated` | `identity_correlated` |
| `PohCredentialRegistered` | `poh_credential_registered` |
| `ProfileFlagged` | `profile_flagged` |
| `ReviewCompleted` | `review_completed` |
| `SybilReviewDecision` | `sybil_review_decision` |

### Migration guidance

- **Dual-subscribe during rollout.** Indexers should subscribe to both the old
  and new topics for one release window, then drop the old topics once no
  historical ledgers in their retention window emit them.
- **No payload migration is required.** Because only the topic symbol changed,
  existing decoders continue to work unchanged.
- **Replay safety.** When replaying historical ledgers, map the old topic names
  to the new canonical names before persisting so downstream consumers see a
  single, consistent topic vocabulary.
