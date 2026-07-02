# Versioned State Migration

## Scope

This contract uses explicit schema versions for persisted records that have
changed shape over time:

- `Escrow`
- onboarding `UserProfile`

The current onboarding profile schema version is `5`.

## Onboarding Profile Layout

`UserProfile` is now split into:

- `DataKey::UserProfile(address)` storing a flat `StoredUserProfile`
- `DataKey::UserPortfolio(address)` storing raw CID `Bytes` when present

Public onboarding methods still return a composed `UserProfile` that includes
`portfolio_cid`, but the optional CID payload no longer increases rent for the
main profile entry.

## Migration Strategy

Onboarding profiles are upgraded lazily on read and may also be upgraded
proactively through `migrate_user_profile(user)`:

1. read the raw persistent value from `DataKey::UserProfile`
2. inspect the underlying map for `version` and `portfolio_cid`
3. decode legacy versionless records as `LegacyUserProfile`
4. decode versioned records with embedded `portfolio_cid` as the old public shape
5. rewrite the profile into the flat `StoredUserProfile` schema
6. move any embedded portfolio CID into `DataKey::UserPortfolio(address)`

This keeps old state readable without a one-shot migration transaction and
ensures newly touched records are normalized automatically.

## Admin Helper

`OnboardingContract::migrate_user_profile(user)` is:

- admin-only
- bounded to one user per call
- idempotent
- returns `true` only when storage was rewritten during that invocation

Lazy migration remains the fallback for normal read/write flows, so the helper
is optional operationally.

## Backward Compatibility Notes

- legacy versionless onboarding profiles are upgraded to version `5`
- older versioned onboarding profiles that still embed `portfolio_cid` are
  rewritten to the flat schema and keep the same business fields
- portfolio CIDs remain available through `get_user`, `get_user_by_username`,
  and `update_portfolio`

## Persisted-State Audit

The storage-rent audit for issue `#641` was limited to persisted state structs:

- onboarding `UserProfile` was flattened by moving `portfolio_cid` to a
  dedicated key
- `UserMetrics` already follows the recommended flat-scalar layout
- deprecated legacy `Vec`-based keys remain read-only migration inputs until
  their existing lazy migration paths consume them

Request/response-only types and transient function parameters were intentionally
left unchanged.

## Test Coverage

Migration behavior is covered by host tests that:

- inject legacy versionless onboarding profiles directly
- inject versioned onboarding profiles that still embed `portfolio_cid`
- call `get_user` and `migrate_user_profile`
- assert the returned public shape still includes `portfolio_cid`
- assert the persisted main profile is flat and the CID is stored separately
