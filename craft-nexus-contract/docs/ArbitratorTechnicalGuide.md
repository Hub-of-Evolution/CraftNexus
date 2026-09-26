# Arbitrator Technical Guide

This guide provides technical instructions for Arbitrators on how to interact with the CraftNexus Escrow contract to resolve disputes.

## Overview

Arbitrators are responsible for resolving disputes between buyers and artisans. When a dispute is initiated, the funds in the escrow are locked in a `Disputed` state. The Arbitrator must review the evidence provided and decide whether to release the funds to the seller or refund them to the buyer.

## Dispute Data

When an escrow is disputed, the following data is available to the Arbitrator:

- **Dispute Reason**: A string provided by the party that initiated the dispute, explaining the issue.
- **IPFS Hash**: A Content Identifier (CID) pointing to off-chain metadata (e.g., order details, photos, communication logs).
- **Metadata Hash**: A SHA-256 hash of the off-chain metadata for verification.

### Viewing Dispute Details

You can view the full details of an escrow using the `get_escrow` function.

**CLI Command:**
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <YOUR_ACCOUNT> \
  --network <NETWORK> \
  -- \
  get_escrow \
  --order_id <ORDER_ID>
```

Replace `<CONTRACT_ID>` with the deployed contract address, `<YOUR_ACCOUNT>` with your Stellar identity name or secret, `<NETWORK>` with `testnet` or `mainnet`, and `<ORDER_ID>` with the specific order identifier.

## Resolution Enum

The `resolve_dispute` function requires a `resolution` parameter, which is an enumeration:

| Value | Name | Impact |
|-------|------|--------|
| `0` | `ReleaseToSeller` | Funds are released to the Artisan, minus the platform fee. |
| `1` | `RefundToBuyer` | Full original amount is returned to the Buyer. No platform fee is charged. |

> [!NOTE]
> Platform fees are only collected when funds are released to the seller. Refunds are returned in full to the buyer to ensure they are not penalized for failed transactions.

## Evidence Challenge Period

Before an Arbitrator can finalize a dispute, both parties get a guaranteed window to submit evidence and counter-evidence — this prevents a one-sided or premature decision.

- `submit_evidence(order_id, submitter, evidence_hash)` — either the buyer or seller may submit a content-addressed evidence reference (e.g. an IPFS CID) while the order is `Disputed`. Returns the assigned `evidence_id`.
- `submit_counter_evidence(order_id, submitter, evidence_hash, parent_evidence_id)` — either party may rebut a specific prior submission by referencing its `evidence_id`. Fails with `Error::InvalidDisputeAction` if `parent_evidence_id` does not exist for that order.
- `get_evidence(order_id)` — returns the full evidence/counter-evidence log for the dispute (bounded to the most recent `MAX_EVIDENCE_PER_DISPUTE` entries).

`resolve_dispute` will reject the call with `Error::ChallengeWindowActive` until `evidence_challenge_window` seconds (default: 2 days) have elapsed since the dispute was opened, regardless of how much evidence has been submitted. Arbitrators should treat a rejected `resolve_dispute` call as "review window still open," not as an error to retry blindly — wait for the window to elapse, or check `get_escrow(order_id).dispute_initiated_at` plus the configured window.

## Resolving a Dispute

Once a decision is made, the Arbitrator calls the `resolve_dispute` function.

> [!IMPORTANT]
> **Arbitrator time-lock**: `resolve_dispute` will be rejected with
> `Error::ArbitratorDeadlineExceeded` (46) once `max_dispute_duration` seconds have elapsed
> since the dispute was opened. After the deadline the escrow must be settled via
> `resolve_expired_dispute` instead. This prevents a stale or compromised arbitrator from
> issuing a resolution after the platform's expiry policy has already taken effect.
> Arbitrators must act before the deadline — the default window is 30 days from
> `dispute_initiated_at`.

**Step-by-Step Resolution:**

1. **Review Evidence**: Fetch the escrow details using `get_escrow` and examine the `dispute_reason` and `ipfs_hash`, plus the full history via `get_evidence`.
2. **Wait for the challenge window**: Confirm `evidence_challenge_window` seconds have passed since `dispute_initiated_at` — `resolve_dispute` fails with `Error::ChallengeWindowActive` otherwise.
3. **Authorize and Invoke**: Run the `resolve_dispute` command with the chosen resolution.

**CLI Command Example (Release to Seller):**
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ARBITRATOR_ACCOUNT> \
  --network testnet \
  -- \
  resolve_dispute \
  --order_id 42 \
  --resolution 0
```

**CLI Command Example (Refund to Buyer):**
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ARBITRATOR_ACCOUNT> \
  --network testnet \
  -- \
  resolve_dispute \
  --order_id 42 \
  --resolution 1
```

## Dispute Resolution Deadline

Disputes are not open indefinitely. Each dispute has a maximum duration configured by the platform, with a default of 30 days. If the arbitrator does not resolve the dispute before that window expires, the contract can automatically settle the dispute using the configured expired-dispute policy.

### Deadline behavior

- The dispute deadline is measured from the timestamp recorded when the dispute was initiated.
- The contract compares that timestamp against the current ledger time using the platform's `max_dispute_duration` setting.
- If the deadline has not yet passed, calling `resolve_expired_dispute` returns `Error::DisputeExpired`.
- Once the deadline has elapsed, `resolve_expired_dispute` can be invoked to finalize the escrow according to the configured fee policy.

### CLI example

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ADMIN_ACCOUNT> \
  --network testnet \
  -- \
  resolve_expired_dispute \
  --order_id 42
```

### What if the arbitrator does not respond?

If an arbitrator stalls past the deadline, the platform admin can still trigger the expired-dispute resolution path to prevent the escrow from remaining in a disputed state indefinitely.

## Escalation Checkpoints and Timeouts

A pending dispute climbs an ordered ladder of escalation checkpoints, all measured from `dispute_initiated_at`. Each checkpoint widens the set of accounts allowed to escalate; it never narrows it. The ladder ends at the dispute's final deadline, which is always `max_dispute_duration`, so the ladder and the settlement paths can never disagree about when a dispute is over.

| Tier | Reached at | Who may call `escalate_dispute` |
|---|---|---|
| `Assigned` | dispute opened | nobody — the assigned arbitrator is inside its service window |
| `PartyFlagged` | `party_checkpoint` (default 3 days) | buyer, seller |
| `ModeratorReview` | `moderator_checkpoint` (default 7 days) | buyer, seller, moderator, arbitrator, admin |
| `AdminReview` | `admin_checkpoint` (default 14 days) | buyer, seller, moderator, arbitrator, admin |
| `TimedOut` | `max_dispute_duration` (default 30 days) | anyone — permissionless safety net |

Checkpoints follow the crate-wide inclusive-end convention: a tier unlocks *at* its deadline, not one second later.

### Escalating

`escalate_dispute(order_id, caller)` advances the dispute to the highest tier the ledger clock has reached. It is a signalling mechanism only: it never moves funds and never changes the escrow status.

- Escalating does **not** change who can call `resolve_dispute` — it is an auditable checkpoint (`dispute_escalated`, carrying the tier, the escalator, and the timestamp) that off-chain monitors and priority queues can use to flag disputes approaching their deadline.
- One escalation per checkpoint: escalating again before a *new* checkpoint has elapsed fails with `Error::InvalidDisputeAction`.
- Calling before the first checkpoint fails with `Error::EscalationWindowActive`.
- Calling by an account that is not eligible for the tier being reached fails with `Error::Unauthorized`.
- Calling on a dispute that has already been settled fails with `Error::SettlementAlreadyFinalized`.

### Inspecting the ladder

- `get_dispute_escalation_status(order_id)` — every checkpoint timestamp, the final deadline, the tier implied by the clock, the tier recorded on-chain, whether the dispute is finalized, and the settlement a timeout would produce.
- `get_dispute_final_deadline(order_id)` — the timestamp after which arbitration is closed off.
- `can_escalate_dispute(order_id, caller)` — the permission matrix above as a queryable predicate.
- `get_dispute_escalation_state(order_id)` — the recorded ladder state (`tier`, `previous_tier`, `escalated_by`, `escalated_at`, `escalation_count`).
- `get_dispute_escalation(order_id)` — the original single-shot record from #941, now pointing at the *first* escalation of the dispute.

### Configuring the checkpoints

`set_escalation_checkpoints(party, moderator, admin)` (admin only) reschedules the ladder. Offsets are in seconds and must be strictly increasing and strictly below `max_dispute_duration`, otherwise the call fails with `Error::InvalidEscalationPolicy`. The tier-1 offset and the legacy `dispute_escalation_window` are kept in sync so there is a single source of truth.

If `max_dispute_duration` is later shortened below a configured checkpoint, the schedule is clamped backwards from the final deadline at read time — no tier is ever scheduled past the point of no return.

### Deterministic timeout outcomes

Once the final deadline passes, arbitration is closed off (`resolve_dispute` fails with `Error::ArbitratorDeadlineExceeded`) and the dispute can only be settled by `resolve_expired_dispute`, whose outcome is fully determined by the operator-configured `expired_dispute_fee_policy`:

| `expired_dispute_fee_policy` | `TimeoutOutcome` | Buyer receives |
|---|---|---|
| `RefundFullNoPlatformFee` | `RefundBuyerFull` | full `amount` |
| `DeductFeeFromSeller` | `RefundBuyerFull` | full `amount` |
| `RefundMinusPlatformFee` | `RefundBuyerMinusFee` | `amount - fee` |
| `SplitFee` | `RefundBuyerSplitFee` | `amount - fee/2` |

`get_timeout_outcome()` previews this before any dispute times out, so the consequence of letting a dispute expire is knowable in advance and cannot be influenced by whoever happens to call `resolve_expired_dispute`.

Settlement is written behind a settlement receipt, so a timed-out dispute settles exactly once: a second `resolve_expired_dispute`, or a late `resolve_dispute`, fails with `Error::SettlementAlreadyFinalized`. Successful settlement emits `dispute_timed_out` alongside the usual `Resolved` escrow event.

## Technical Edge Cases

### Transaction Atomicity
All resolution actions (releasing funds, collecting fees, updating state) are performed within a single Stellar transaction. This ensures that:
- Either all actions succeed, or none do (the transaction reverts).
- There is no risk of funds being "lost" or stuck in an inconsistent state.

### Refund Failures
If a refund to a buyer fails (e.g., due to account constraints on the buyer's side, though rare for standard assets), the entire `resolve_dispute` transaction will fail and revert. The escrow will remain in the `Disputed` state, allowing the Arbitrator to retry or investigate the cause.

### State Constraints
The `resolve_dispute` function can only be called on escrows that are currently in the `Disputed` status. Attempting to resolve an `Active`, `Released`, or already `Resolved` escrow will result in an `Error::NotInDispute` (8) or `Error::InvalidEscrowState` (3).
