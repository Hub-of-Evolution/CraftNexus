# Versioned State Migration

## Scope

This contract stores explicit schema versions on:

- `Escrow`
- `UserProfile`

The current `Escrow` schema version is `3` (`CURRENT_ESCROW_VERSION`).

## Escrow Schema History

The `Escrow` record has evolved through three on-chain shapes. Older records
are migrated lazily on read:

- **Legacy (pre-versioning, `LegacyEscrow`)** - no `version` field, no
  `batch_id`, and `dispute_reason` stored as a `String`.
- **v2 (`EscrowWithoutBatch`)** - adds an explicit `version` field but still
  has no `batch_id`.
- **v3 (`Escrow`, current)** - adds `batch_id: Option<u64>`, normalizes
  `dispute_reason` to a lightweight `Symbol`, and adds the `funded` flag.

## Migration Strategy

Existing on-chain records created before a given change do not contain the
newer fields. To preserve compatibility, reads decode the raw persistent value
and select the correct struct shape:

1. read the raw persistent value as a `Val`
2. inspect the underlying map for the `version` key
3. if `version` is present, check for the `batch_id` key to distinguish the
   current `Escrow` shape from the `EscrowWithoutBatch` shape
4. if `version` is absent, decode as the pre-versioning `LegacyEscrow`
5. upgrade the decoded record to the current schema and rewrite it immediately

This upgrade happens lazily on read through:

- `EscrowContract::get_escrow` (and its internal helpers
  `get_stored_escrow` / `try_get_escrow_readonly`)
- `upgrade_escrow`, which stamps the current version and persists the result
- `escrow_from_without_batch`, which lifts a v2 record to the current shape
- `OnboardingContract::get_user` for `UserProfile`

## Why Lazy Upgrade

- avoids a one-shot migration transaction
- keeps old state readable without downtime
- amortizes migration cost across normal usage
- ensures newly touched records are normalized automatically

## Backward Compatibility Notes

- `LegacyEscrow` data is upgraded to `version = CURRENT_ESCROW_VERSION` with
  `batch_id = None` and `funded = true`
- `EscrowWithoutBatch` (v2) data is upgraded with `batch_id = None`
- records with `version < 3` have `funded` set to `true` during upgrade
- business fields are preserved during upgrade
- migrated entries are written back to persistent storage immediately

## Test Coverage

Migration behavior is covered by host tests that:

- inject a legacy `Escrow` (`LegacyEscrow`) directly into persistent storage
  (`test_get_escrow_migrates_legacy_state`)
- inject a legacy `UserProfile` directly
  (`test_get_user_migrates_legacy_profile`)
- read the records back through public contract methods
- assert the returned and persisted values are upgraded to the current schema
  version with the new fields defaulted correctly