# Contract Event Naming Convention

## Standard

All contract events use **lowercase snake_case** for the event topic symbol. This applies uniformly across escrow, staking, onboarding, admin, dispute, and storage modules.

### Format

```
<domain>_<action>[_<qualifier>]
```

Examples:
- `escrow_created`
- `escrow_cancelled`
- `user_onboarded`
- `stake_operation`
- `admin_platform_paused`

### Rules

1. **Lowercase snake_case only** — no PascalCase, no camelCase, no abbreviations that obscure meaning.
2. **Domain prefix** — every event starts with its functional domain (`escrow`, `stake`, `admin`, `user`, `dispute`, `storage`, `recurring_escrow`, `fee_config`, etc.).
3. **Past tense for state changes** — `_created`, `_cancelled`, `_resolved`, `_updated`, `_paused`, `_unstaked`.
4. **Present tense for ongoing actions** — `_operation`, `_scheduled`, `_progress`.
5. **No `Event` suffix** in the topic symbol — the suffix belongs only to Rust struct names (e.g., struct `UserOnboardedEvent`, topic `user_onboarded`).
6. **Expand abbreviations** — `fee_config_migrated` not `fee_cfg_migrated`.

## Canonical Event Topic Table

### Escrow

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `escrow_created` | `EscrowEvent` | `create_escrow` |
| `escrow_funded` | `EscrowEvent` | `fund_escrow` |
| `escrow_cancelled` | `(escrow_id)` + refund | `cancel_escrow` |
| `escrow_resolved` | `EscrowResolvedEvent` | `resolve_escrow` |
| `escrow_metadata_verified` | `MetadataVerifiedEvent` | `verify_metadata` |
| `cycle_released` | `(escrow_id)` + amount | `release_cycle` |

### Recurring Escrow

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `recurring_escrow` | `RecurringEscrowEvent` | `create_recurring_escrow`, `release_recurring_cycle`, `cancel_recurring_escrow` |

### Staking

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `stake_operation` | `(artisan, new_stake)` | `stake_tokens`, `unstake_tokens` |
| `stake_reputation_update` | `ReputationUpdateEvent` | reputation updates |
| `stake_liquidation_cured` | `(artisan, timestamp)` | `cure_liquidation` |
| `stake_history_warning` | `"queue_full"` + message | stake history overflow |
| `tokens_staked` | `TokensStakedEvent` | `stake_tokens` |
| `tokens_unstaked` | `TokensUnstakedEvent` | `unstake_tokens` |

### Admin

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `admin_changed` | `(change_type, prev, new)` | `set_admin` |
| `admin_config_updated` | `ConfigUpdatedEvent` | config mutations |
| `admin_config_recovered` | `true` + message | fallback admin recovery |
| `admin_recovery_initiated` | `true` + message | `initiate_admin_recovery` |
| `admin_fee_tier_updated` | `ArtisanFeeTierUpdatedEvent` | `set_fee_tier` |
| `admin_platform_paused` | `PlatformPausedEvent` | `pause_platform` |
| `admin_platform_unpaused` | `PlatformUnpausedEvent` | `unpause_platform` |

### Onboarding

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `user_onboarded` | `UserOnboardedEvent` | `onboard_user` |
| `onboard_call_failed` | `OnboardCallFailedEvent` | cross-contract onboarding failures |
| `role_updated` | `(user, old_role, new_role)` | `change_role` |
| `profile_deactivated` | `(user, role)` | `deactivate_profile` |
| `profile_reactivated` | `(user, role)` | `reactivate_profile` |
| `profile_flagged` | `ProfileFlaggedEvent` | `flag_profile` |
| `user_verified` | `(user)` | `approve_verification`, `auto_verify_user` |
| `username_changed` | `(user)` | `change_username` |
| `portfolio_updated` | `(user)` | portfolio mutations |
| `auto_verified` | `AutoVerifiedEvent` | `auto_verify_user` |
| `attempt_rate_limited` | `AttemptRateLimitedEvent` | rate limiter |
| `sybil_pattern_detected` | `SybilPatternDetectedEvent` | sybil detection |
| `sybil_review_decision` | `SybilReviewDecisionEvent` | `review_sybil_profile` |
| `identity_correlated` | `IdentityCorrelatedEvent` | identity correlation |
| `poh_credential_registered` | `PohCredentialRegisteredEvent` | `register_poh_credential` |
| `review_completed` | `ReviewCompletedEvent` | `complete_review` |
| `config_updated` | `(field, admin)` | sybil config mutations |

### Dispute / Arbitration

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `dispute_evidence` | `(submitted, order_id, ...)` | `submit_evidence` |
| `dispute_assignment_changed` | `DisputeAssignmentChangedEvent` | arbitrator reassignment |
| `dispute_escalated` | `(tier, by, at)` | `escalate_dispute` |
| `dispute_timed_out` | `(outcome, deadline, settled)` | timeout settlement |

### Batch Scheduler

| Topic | Sub-topic | Payload |
|-------|-----------|---------|
| `batch_scheduler` | `scheduled` | `(job_id, param_count)` |
| `batch_scheduler` | `progress` | `(job_id, index, total, revision, status)` |
| `batch_scheduler` | `cancelled` | `job_id` |

### Upgrade

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `wasm_upgrade` | `UpgradeProposalEvent` / `UpgradeApprovalEvent` | propose/approve upgrade |

### Fee Config

| Topic | Payload struct | Emitted by |
|-------|---------------|------------|
| `fee_config_migrated` | `FeeTokenConfigsMigratedEvent` | `migrate_fee_token_configs` |

### Storage Lifecycle

| Topic | Sub-topic | Payload |
|-------|-----------|---------|
| `storage_compaction` | `run` | `(run_count, entries_removed, ttl_extended)` |
| `storage_retention_policy` | `applied` | `(fund, stake, emergency, upgrade retentions)` |

## Schema Versioning

All lifecycle event structs carry a `schema_version: u32` field. The current value is pinned by `LIFECYCLE_EVENT_SCHEMA_VERSION` (currently `1`). Indexers must branch on this field before decoding payloads.

## Indexer Migration Notes

### Breaking Changes (Event Topic Renames)

The following event topics were renamed from PascalCase to snake_case. **Payload structures are unchanged** — only the topic symbol differs.

| Old Topic | New Topic | Module |
|-----------|-----------|--------|
| `AttemptRateLimited` | `attempt_rate_limited` | onboarding |
| `AutoVerifiedEvent` | `auto_verified` | onboarding |
| `ConfigUpdated` | `config_updated` | onboarding |
| `IdentityCorrelated` | `identity_correlated` | onboarding |
| `OnboardCallFailed` | `onboard_call_failed` | onboarding |
| `PohCredentialRegistered` | `poh_credential_registered` | onboarding |
| `PortfolioUpdated` | `portfolio_updated` | onboarding |
| `ProfileDeactivated` | `profile_deactivated` | onboarding |
| `ProfileFlagged` | `profile_flagged` | onboarding |
| `ProfileReactivated` | `profile_reactivated` | onboarding |
| `ReviewCompleted` | `review_completed` | onboarding |
| `RoleUpdated` | `role_updated` | onboarding |
| `SybilPatternDetected` | `sybil_pattern_detected` | onboarding |
| `SybilReviewDecision` | `sybil_review_decision` | onboarding |
| `UserOnboarded` | `user_onboarded` | onboarding |
| `UserVerified` | `user_verified` | onboarding |
| `UsernameChanged` | `username_changed` | onboarding |
| `fee_cfg_migrated` | `fee_config_migrated` | escrow/admin |

### Migration Steps for Indexers

1. **Update topic filters** — replace old PascalCase topics with snake_case equivalents in your subscription/filter configuration.
2. **Dual-write period (recommended)** — during transition, accept both old and new topic names to avoid data loss while the contract is upgraded.
3. **No payload changes** — decoding logic for event payloads remains identical; only the topic string changes.
4. **Snapshot tests** — `craft-nexus-contract/test_snapshots/*.json` documents the canonical struct field order for each event type.

### Example: Before/After

```typescript
// Before (old)
subscription.subscribe({
  topics: ["UserOnboarded", "UserVerified", "RoleUpdated"]
});

// After (new)
subscription.subscribe({
  topics: ["user_onboarded", "user_verified", "role_updated"]
});
```
