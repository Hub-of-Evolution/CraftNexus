# Versioned State Migration

## Scope

This contract now stores explicit schema versions on:

- `Escrow`
- `UserProfile`

The current schema version is `2`.

## Migration Strategy

Existing on-chain records created before this change do not contain a `version` field. To preserve compatibility:

1. read the raw persistent value
2. inspect the underlying map for the `version` key
3. decode as the current struct when `version` exists
4. decode as the legacy struct when `version` is missing
5. rewrite the entry immediately in the current schema

This upgrade happens lazily on read through:

- `EscrowContract::get_escrow`
- `OnboardingContract::get_user`
- any internal helpers that load those records

## Why Lazy Upgrade

- avoids a one-shot migration transaction
- keeps old state readable without downtime
- amortizes migration cost across normal usage
- ensures newly touched records are normalized automatically

## Backward Compatibility Notes

- v1 `Escrow` data is upgraded with `version = 2`
- v1 `UserProfile` data is upgraded with `version = 2`
- business fields are preserved during upgrade
- migrated entries are written back to persistent storage immediately

## Test Coverage

Migration behavior is covered by host tests that:

- inject legacy `UserProfile` storage directly
- inject legacy `Escrow` storage directly
- read the records through public contract methods
- assert the returned and persisted values are upgraded to version `2`
