# Expired-Dispute Refund Policy

This is the single policy document for force-closing a dispute after
`max_dispute_duration` (issue **#1055**).

## State machine

```text
Disputed
  │  now >= dispute_initiated_at + max_dispute_duration
  │  caller: anyone
  ▼
resolve_expired_dispute()
  │
  ▼
Resolved   (SettlementPath::ExpiredDispute)
```

| Field | Rule |
|---|---|
| Predecessor | `EscrowStatus::Disputed` with `dispute_initiated_at` set |
| Eligible caller | Permissionless (any account) |
| Deadline | Accepted only when `now >= dispute_initiated_at + max_dispute_duration` (the exact deadline second is included) |
| Successor | `EscrowStatus::Resolved` |
| Other paths after deadline | `resolve_dispute`, `resolve_dispute_partial`, and `accept_partial_refund` return `ArbitratorDeadlineExceeded` |

## Fee treatment

Configured by `ExpiredDisputeFeePolicy` (admin, revision-gated):

| Policy | Buyer | Platform | Seller |
|---|---|---|---|
| `RefundFullNoPlatformFee` (default) | full amount | 0 | 0 |
| `RefundMinusPlatformFee` | amount − fee | fee | 0 |
| `DeductFeeFromSeller` | full amount | 0 | opportunity cost only (see residual risk) |
| `SplitFee` | amount − fee/2 | fee/2 | 0 |

Every policy conserves the escrow pot:

```text
platform_fee + seller_amount + buyer_amount == escrow.amount
```

`DeductFeeFromSeller` cannot debit the seller's wallet without taking from the
locked pot, which would either underpay the buyer or break conservation. The
implementation therefore refunds the buyer in full and records the seller's
loss as opportunity cost of the stalled arbitration.

## Mutual exclusion

A settlement receipt is written on the expired path. A second call — expiry
or otherwise — returns `SettlementAlreadyFinalized`. Before the deadline,
`resolve_expired_dispute` returns `DisputeExpired` so the arbitrator path
remains the only live exit.
