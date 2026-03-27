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

## Resolving a Dispute

Once a decision is made, the Arbitrator calls the `resolve_dispute` function.

**Step-by-Step Resolution:**

1. **Review Evidence**: Fetch the escrow details using `get_escrow` and examine the `dispute_reason` and `ipfs_hash`.
2. **Authorize and Invoke**: Run the `resolve_dispute` command with the chosen resolution.

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

## Technical Edge Cases

### Transaction Atomicity
All resolution actions (releasing funds, collecting fees, updating state) are performed within a single Stellar transaction. This ensures that:
- Either all actions succeed, or none do (the transaction reverts).
- There is no risk of funds being "lost" or stuck in an inconsistent state.

### Refund Failures
If a refund to a buyer fails (e.g., due to account constraints on the buyer's side, though rare for standard assets), the entire `resolve_dispute` transaction will fail and revert. The escrow will remain in the `Disputed` state, allowing the Arbitrator to retry or investigate the cause.

### State Constraints
The `resolve_dispute` function can only be called on escrows that are currently in the `Disputed` status. Attempting to resolve an `Active`, `Released`, or already `Resolved` escrow will result in an `Error::NotInDispute` (8) or `Error::InvalidEscrowState` (3).
