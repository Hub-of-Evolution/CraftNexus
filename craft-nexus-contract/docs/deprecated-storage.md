# Deprecated storage and bounded growth

This note covers four storage-hygiene tracks landed together. Each refers
back to a specific issue and the exact `DataKey` it touches, so a future
maintainer can decide whether the legacy compatibility shim is still
worth carrying.

## `DataKey::ReferralRewardBps` — Issue #234

* Status: **deprecated, retained for ABI compatibility only**.
* What it stored: a `u32` basis-points figure that an admin could set via
  `set_referral_reward_bps`.
* Why it was kept: referral payout logic was scoped but never shipped.
  The slot was preserved in case clients had already serialized the call.

### Active behaviour

* `set_referral_reward_bps` now panics with
  `Error::DeprecatedFunction` after admin auth. No new value can be
  written to the slot; existing entries from older deployments are
  inert.
* `get_referral_reward_bps` always returns `0`. It does not read the
  legacy slot, so callers cannot accidentally rely on stale state.
* No payout, fee, or reward path in the contract reads
  `DataKey::ReferralRewardBps`. Grep is the source of truth — if a new
  PR adds a read, it must clear the deprecation in this doc first.

### Migration path

If a future feature wants to revive referrals it must introduce a fresh
`DataKey` variant. Re-using `ReferralRewardBps` is forbidden because we
cannot distinguish "value left over from a 2024 deployment" from
"intentional new value".

## `DataKey::StakeCooldownEnd(Address)` — Issue #235

* Status: **deprecated, written for legacy clients but never read by
  active logic**.
* What it stored: a single `u64` cooldown timestamp per artisan.
* Why it was kept: older off-chain readers polled this key directly to
  show "stake unlocks at …" without understanding the queue layout.

### Active behaviour

* Active staking uses [`DataKey::ArtisanStakeQueue`]. `unstake_tokens`
  never reads the single timestamp; matured deposits are decided per
  queue entry.
* `stake_tokens` and `unstake_tokens` continue to mirror the maximum
  `cooldown_end` from the queue into this key so legacy readers still
  see a conservative value. Both call sites are now annotated with
  "DEPRECATED storage write" comments referencing this issue.
* When a queue empties, `unstake_tokens` removes the deprecated key
  alongside the queue and the stake record.
* Operators can call `purge_stake_cooldown_end(artisan)` (admin-only)
  to clear a stale entry without disturbing the queue. The function
  returns `true` when an entry was removed and `false` otherwise.

### Migration path

When the off-chain readers that depend on this single timestamp are
retired, drop both the mirror writes in `stake_tokens`/`unstake_tokens`
and the `DataKey::StakeCooldownEnd` variant in the same release. Until
then, the key must remain a *write-only* mirror.

## `DataKey::NextRecurringEscrowId` — Issue #233

* Status: **active, but bounded**.
* What it stores: the next `u64` ID for a recurring escrow.

### Active behaviour

* `MAX_RECURRING_ESCROW_ID = u64::MAX - 1` (defined in `lib.rs`) is the
  hard ceiling. `u64::MAX` is reserved as a sentinel.
* `create_recurring_escrow` rejects allocation past the cap with
  `Error::RecurringEscrowIdExhausted`. The increment uses
  `checked_add`, so the contract panics loudly rather than wrapping
  into an existing ID.
* The cap is far above any realistic deployment lifetime (one new
  recurring escrow per ledger for ~3 trillion years), so the bound is
  defensive — not a near-term operational concern. Its purpose is to
  remove the silent-collision failure mode entirely.

### Migration path

If recurring escrow churn ever needs ID recycling (e.g. a contract
fork that prunes long-cancelled escrows), introduce a separate
allocator with explicit free-list semantics; do not lower the cap on
this counter without a coordinated migration.

## `DataKey::BuyerEscrowCount` / `DataKey::SellerEscrowCount` — Issue #244

* Status: **active, indexed pattern documented**.
* What they store: a single `u32` per buyer/seller giving the total
  number of escrows that party has been involved in.

### Scaling characteristics

* Per-account: **O(1) storage**, **O(1) updates**. One ledger entry
  per buyer/seller, irrespective of how many escrows they have.
* Per-platform: footprint scales with the number of distinct
  participants, not with the number of escrows. There is no 64KB
  per-key limit to worry about because every escrow ID lives in its
  own `BuyerEscrowIndexed`/`SellerEscrowIndexed` entry.
* Pagination: `get_escrows_by_buyer` / `get_escrows_by_seller` read
  one indexed entry per item per page; cost is bounded by the page
  size, not the total count.
* TTL: like every other persistent entry, counts obey the standard
  TTL extension (`TTL_EXTENSION`). Inactive accounts archive
  naturally.

### Why not a sparser alternative?

A bitmap or sharded counter would compress storage if the platform had
millions of accounts that each held only a handful of escrows, but it
would penalise the common case (frequent reads/writes by active
buyers/sellers) with extra masking and indirection. The current
single-entry `u32` is the cheapest design that still preserves O(1)
updates and supports the indexed pagination pattern. We will revisit
if telemetry shows unique-account growth dominating storage cost.

## `DataKey::AllEscrowIds` — Issue #515 / #226 / #634

* Status: **deprecated, lazily migrated on read/write**.
* What it stored: a monolithic `Vec<u32>` containing every escrow order
  ID on the platform.
* Why it was replaced: every new escrow rewrote the full vector, making
  global enumeration O(n) per write and increasing persistent rent with
  a single ever-growing entry.

### Active behaviour

* New writes use `DataKey::EscrowCount` plus
  `DataKey::GlobalEscrowIdIndexed(index)`. Each escrow ID is appended to
  its own persistent slot, so creation remains O(1) even as the total
  number of escrows grows.
* The relevant migration surface for the global registry is the
  internal `migrate_legacy_all_escrow_ids` helper. `migrate_user_escrows`
  only handles per-user buyer/seller legacy vectors and is not involved
  in the global `AllEscrowIds` transition.
* Lazy migration runs before every global append and before both public
  global read paths: `get_escrow_count` and
  `get_all_escrow_ids_iterative`.
* Migration backfills any missing `GlobalEscrowIdIndexed(i)` entries,
  raises `EscrowCount` to the legacy vector length when needed, and then
  removes `AllEscrowIds`.

### Operator verification

For a production or testnet deployment with a large backlog, verify the
migration with the same bounded read surface clients use in practice:

* Call `get_escrow_count()` and record the returned total.
* Page through `get_all_escrow_ids_iterative(page, limit)` until it
  returns empty, then confirm the number of IDs fetched matches the
  reported count.
* Spot-check storage over RPC:
  `DataKey::EscrowCount` should equal the on-chain total, and sampled
  `DataKey::GlobalEscrowIdIndexed(i)` entries should decode to the same
  order IDs returned by paginated reads.
* After the first successful lazy migration pass, `DataKey::AllEscrowIds`
  should no longer be present.

### Migration path

Keep the deprecated key only as a compatibility shim for older
deployments that still carry the monolithic vector. Once host tests and
testnet verification confirm the lazy migrator is complete for the live
dataset, plan removal of `DataKey::AllEscrowIds` in the next contract
version rather than in the same release that validates migration
correctness.
