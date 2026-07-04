# Deprecated storage and bounded growth

This note covers three storage-hygiene tracks landed together. Each refers
back to a specific issue and the exact `DataKey` it touches, so a future
maintainer can decide whether the legacy compatibility shim is still
worth carrying.

## `DataKey::StakeCooldownEnd(Address)` — Issue #235

* Status: **completed and removed**.
* What it stored: a single `u64` cooldown timestamp per artisan.
* Why it was removed: older off-chain readers were updated to read `DataKey::ArtisanStakeQueue` instead. The legacy mirror writes in `stake_tokens` and `unstake_tokens` have been eliminated to save storage costs.

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
