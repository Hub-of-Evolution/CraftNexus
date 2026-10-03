# State Migration

## Versioned Event Topic Migration (Issue #1020)

### Summary

Event topic symbols were standardized from mixed PascalCase/snake_case to uniform lowercase snake_case. This is a **topic-only change** — payload structures, field names, and field types are unchanged.

### Contract Upgrade Steps

1. Deploy the updated contract WASM.
2. No on-chain state migration is required — only event topic symbols changed.
3. Indexers must update their topic filters (see below).

### Indexer Topic Filter Updates

| Old Topic (PascalCase) | New Topic (snake_case) |
|------------------------|------------------------|
| `AttemptRateLimited` | `attempt_rate_limited` |
| `AutoVerifiedEvent` | `auto_verified` |
| `ConfigUpdated` | `config_updated` |
| `IdentityCorrelated` | `identity_correlated` |
| `OnboardCallFailed` | `onboard_call_failed` |
| `PohCredentialRegistered` | `poh_credential_registered` |
| `PortfolioUpdated` | `portfolio_updated` |
| `ProfileDeactivated` | `profile_deactivated` |
| `ProfileFlagged` | `profile_flagged` |
| `ProfileReactivated` | `profile_reactivated` |
| `ReviewCompleted` | `review_completed` |
| `RoleUpdated` | `role_updated` |
| `SybilPatternDetected` | `sybil_pattern_detected` |
| `SybilReviewDecision` | `sybil_review_decision` |
| `UserOnboarded` | `user_onboarded` |
| `UserVerified` | `user_verified` |
| `UsernameChanged` | `username_changed` |
| `fee_cfg_migrated` | `fee_config_migrated` |

### Dual-Acceptance Period

During the transition window, indexers should accept both old and new topic names to avoid missing events emitted before/after the contract upgrade. Once the upgrade is confirmed on all environments, remove the legacy filters.

### Reference

See `docs/event-naming-convention.md` for the complete canonical event topic table and naming rules.
