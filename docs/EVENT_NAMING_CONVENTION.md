# Contract Event Naming Convention

> First-step scaffolding for #1020. This document pins the convention
> every emitted event must follow across escrow, staking, onboarding and
> admin flows. The actual rename of existing identifiers to match this
> convention — and the accompanying snapshot test updates — is deliberately
> scoped out of this PR so each rename can land with a focused snapshot
> diff and indexer migration note.

## The rule

Every event is published as:

```rust
env.events().publish(
    (<domain>, <action>, <optional id/filter>),
    <payload>,
);
```

### Topic

A tuple whose first element identifies the **domain**, the second the
**action**, and any following elements are **indexable filters** (ids,
addresses) an off-chain indexer can use without decoding the payload.

| Position | Required | Format | Example |
|---|---|---|---|
| 0 — domain | yes | `snake_case` noun, singular | `escrow`, `stake`, `dispute`, `upgrade`, `admin`, `batch` |
| 1 — action | yes | `snake_case` verb in **past tense** for state transitions; present tense for lifecycle checkpoints | `created`, `funded`, `released`, `cancelled`, `scheduled` |
| 2+ — filter | when it exists | one id/address per slot — never a struct | `escrow_id`, `artisan` |

### Payload

A single `#[contracttype]` struct (or a primitive when there is exactly
one scalar to publish). Never a tuple that mirrors the topic — if a
value is interesting enough to filter on, put it in the topic; if it is
part of the record, put it in the payload struct.

### Symbol choice

- **Short identifiers (≤ 9 chars, 0-9 A-Z \_, case-sensitive)** must use
  `symbol_short!(…)` and be declared as module-level `const` so the
  compiler catches typos and the identifier appears exactly once in the
  WASM:
  ```rust
  const ESCROW_FUNDED: Symbol = symbol_short!("E_FUNDED");
  ```
- **Longer identifiers** use `Symbol::new(&env, "escrow_funded")`. These
  must also be declared in one place — typically next to the module
  header — so renames are a single-line change.

Never call `Symbol::new` inline at every publish site: the identifier
then drifts (see current `"escrow_released"` emitted from three
different sites) and renames become a 20-line diff.

## Naming table

| Domain | Action / abbreviation | Full identifier |
|---|---|---|
| escrow | created / E\_CREAT  | `escrow_created` |
| escrow | funded / E\_FUNDED | `escrow_funded` |
| escrow | released / E\_RELEAS | `escrow_released` |
| escrow | cancelled / E\_CANCEL | `escrow_cancelled` |
| escrow | refunded / E\_REFUND | `escrow_refunded` |
| escrow | cycle\_released / E\_CYCREL | `escrow_cycle_released` |
| recurring | locked / R\_LOCK | `recurring_escrow_locked` |
| recurring | released / R\_REL | `recurring_release` |
| recurring | cancel\_refund / R\_CANC | `recurring_cancel_refund` |
| dispute | opened / D\_OPEN | `dispute_opened` |
| dispute | assignment\_changed / D\_ASGN | `dispute_assignment_changed` |
| dispute | expired\_fee / D\_EXPFEE | `dispute_expired_fee` |
| stake | liquidated / S\_LIQ | `stake_liquidated` |
| stake | liquidation\_cured / S\_CURED | `stake_liquidation_cured` |
| batch | scheduled / B\_SCHED | `batch_scheduled` |
| batch | progress / B\_PROG | `batch_progress` |
| batch | cancelled / B\_CANC | `batch_cancelled` |
| admin | recovery\_initiated / A\_RECOV | `admin_recovery_initiated` |
| admin | sweep\_unallocated / A\_SWEEP | `admin_sweep_unallocated` |
| fee | config\_migrated / F\_MIGR | `fee_cfg_migrated` |
| upgrade | proposed / UPG\_PROP | `upgrade_proposed` |
| upgrade | cancelled / UPG\_CANC | `upgrade_cancelled` |
| upgrade | executed / UPG\_EXEC | `upgrade_executed` |

**Why both columns?** The short form is for the on-wire `Symbol` that
ships inside the WASM (cheap, must be ≤ 9 chars). The full identifier is
the one indexers and docs reference — the short form is derived from it,
not the other way round. When a long form has to change, the short form
must change too, in the same PR.

## Current deviations (to be fixed under #1020)

These sites violate the convention today. They are listed so the rename
PRs that follow this doc have a concrete checklist; nothing in **this**
PR renames them (that avoids breaking every snapshot test in one go).

- `craft-nexus-contract/src/contract.rs:51` emits
  `escrow_cancelled` via `Symbol::new(&env, "escrow_cancelled")`
  inline — should reference a single module-level `Symbol` constant.
- `craft-nexus-contract/src/contract.rs:76` emits `cycle_released` —
  the domain prefix `escrow_` is missing. Target: `escrow_cycle_released`.
- `craft-nexus-contract/src/lib.rs` has `Symbol::new(&env, "…")` at 25+
  publish sites; each of the identifiers listed in the table above
  should be declared once at the top of the module and reused.
- `scheduled`, `progress`, `cancelled` are emitted as **action-only**
  topics with the domain in position 0 (`"batch_scheduler"`). Switch to
  `batch_scheduled` / `batch_progress` / `batch_cancelled` so every
  event's position-0 topic is a stable, filterable domain.

## Indexer migration notes

When a rename lands:

1. Open a dedicated PR per domain so the snapshot diff is reviewable.
2. Call out the old and new symbol values in the PR body so operators
   can patch their indexer filter lists in lockstep with the deploy.
3. Do **not** change payload shapes in the same PR — rename and reshape
   should never collide.
4. If the upgrade is on-chain, emit the rename in the same ledger as the
   `upgrade_executed` event so indexers only have one switch-over point.

## How to check

A quick grep of the two files below is enough to catch new deviations
in review:

```bash
grep -nE 'events\(\)\.publish' craft-nexus-contract/src/**/*.rs \
  | grep -E 'Symbol::new|symbol_short!'
```

Every publish site should reference one of the module-level `const`
symbols. Inline `Symbol::new(&env, "…")` at a publish site is a smell
and should be flagged in review.
