# Partial Refund Proposal Race Condition Bugfix Design

## Overview

This bugfix addresses a race condition in the `propose_partial_refund` function that allows creating settlement proposals for disputed escrows. When both buyer and seller submit proposals in the same ledger block or rapidly consecutively, they might overwrite each other's proposals without knowing, leading to unpredictable negotiation flows and potential loss of proposal state.

The fix introduces:

- Unique proposal IDs to track individual proposals
- Proposal locks to prevent overwrites without explicit cancellation
- A dedicated `cancel_proposal` function for proposers to withdraw their proposals
- Deterministic tie-breaking for same-block proposals using lexicographic order of proposer addresses

## Glossary

- **Bug_Condition (C)**: The condition that triggers the race condition - when both buyer and seller submit proposals in the same ledger block without tracking, causing overwrites
- **Property (P)**: The desired behavior when proposals are submitted - each proposal gets a unique ID, proposals cannot be overwritten without explicit cancellation, and tie-breaking is deterministic
- **Preservation**: Existing negotiation flows that must remain unchanged - proposal acceptance, fund distribution, and escrow status transitions
- **proposal_id**: A unique identifier generated for each proposal (hash of proposer + timestamp + nonce)
- **proposal_lock**: A mechanism that prevents new proposals from overwriting existing ones without explicit cancellation
- **active_proposal**: A proposal that has been submitted but not yet accepted, cancelled, or expired
- **proposer_address**: The Stellar address of the party who submitted the proposal
- **proposal_timestamp**: The ledger timestamp when the proposal was submitted

## Bug Details

### Bug Condition

The race condition manifests when both buyer and seller submit partial refund proposals in the same ledger block or rapidly consecutively. The system allows overwriting without notification, causing:

1. Proposals from one party being silently overwritten by the other party's proposal
2. No mechanism for proposers to withdraw their proposals before acceptance
3. No deterministic tie-breaking for same-block proposals

**Formal Specification:**

```
FUNCTION isBugCondition(input)
  INPUT: input of type EscrowProposalSubmission
  OUTPUT: boolean

  RETURN input.proposer IN [escrow.buyer, escrow.seller]
         AND existingProposalExists(input.order_id)
         AND NOT isSameProposalId(input, existingProposal)
         AND NOT isCancellation(input)
END FUNCTION
```

### Examples

- **Example 1 - Overwrite Race**: Buyer submits proposal at ledger 1000, seller submits proposal at ledger 1000 (same block). Seller's proposal overwrites buyer's without notification. Expected: Both proposals should be tracked with unique IDs, seller should be required to cancel buyer's proposal first.

- **Example 2 - No Withdrawal**: Buyer submits proposal, seller submits counter-proposal before buyer can accept. Buyer has no way to withdraw their original proposal. Expected: Buyer should be able to call `cancel_proposal` to withdraw their proposal.

- **Example 3 - No Tie-Breaking**: Two proposals submitted in the same block by different parties. System has no deterministic way to order them. Expected: Proposals should be ordered lexicographically by proposer address.

- **Edge Case - Same Proposal**: Buyer submits proposal, then submits identical proposal again. Expected: This should be allowed (idempotent) or rejected with clear error.

## Expected Behavior

### Preservation Requirements

**Unchanged Behaviors:**

- When a proposal is accepted by the counterparty, funds SHALL be distributed according to the proposal amount and platform fee
- When a proposal is not accepted within the escrow's lifetime, the proposal SHALL remain active until the escrow is resolved
- When only one party submits a proposal, the counterparty SHALL be able to accept it for negotiation
- When a proposal is submitted by an unauthorized party (neither buyer nor seller), the proposal SHALL be rejected with an unauthorized error

**Scope:**
All existing negotiation flows and escrow operations should continue to work exactly as before. This fix only adds tracking and locking mechanisms without changing the core negotiation logic.

## Hypothesized Root Cause

Based on the bug description, the most likely issues are:

1. **Missing Proposal Tracking**: The current implementation likely stores only one proposal per escrow without unique identifiers
   - No proposal_id field in storage
   - No proposer_address tracking
   - No timestamp for tie-breaking

2. **No Locking Mechanism**: The system allows proposals to overwrite each other without validation
   - No check for existing active proposals before accepting new ones
   - No requirement for explicit cancellation before overwriting

3. **No Cancellation Function**: There is no way for proposers to withdraw their proposals
   - Missing `cancel_proposal` function
   - Proposals remain active indefinitely if not accepted

4. **No Deterministic Ordering**: Same-block proposals have no tie-breaking mechanism
   - No lexicographic ordering by proposer address
   - No timestamp-based ordering for proposals in different ledgers

## Correctness Properties

Property 1: Bug Condition - Unique Proposal Tracking and Locking

_For any_ escrow where both buyer and seller submit proposals, the fixed implementation SHALL assign unique proposal IDs to each proposal, store them with proposer metadata, and prevent overwrites without explicit cancellation.

**Validates: Requirements 2.1, 2.2**

Property 2: Preservation - Existing Negotiation Flows

_For any_ input that does NOT involve proposal race conditions (single proposals, proposal acceptance, escrow status transitions), the fixed implementation SHALL produce exactly the same behavior as the original implementation, preserving all existing negotiation flows.

**Validates: Requirements 3.1, 3.2, 3.3, 3.4**

## Fix Implementation

### Changes Required

**File**: `craft-nexus/lib/stellar/escrow.ts` and smart contract storage

#### 1. Data Structure Changes

**Add proposal ID field to proposal storage:**

- Add `proposal_id: string` - unique identifier for each proposal
- Add `proposer_address: string` - Stellar address of the proposing party
- Add `timestamp: number` - ledger timestamp when proposal was submitted
- Add `active: boolean` - status field to track if proposal is still active

**Storage Structure (Rust-style pseudocode):**

```rust
struct Proposal {
    proposal_id: String,           // Hash of proposer + timestamp + nonce
    proposer_address: Address,     // Who submitted the proposal
    amount: i128,                  // Proposal amount in stroops
    timestamp: u64,                // Ledger timestamp
    active: bool,                  // Is this proposal still active?
}

struct Escrow {
    buyer: Address,
    seller: Address,
    token: Address,
    amount: i128,
    status: EscrowStatus,
    created_at: u64,
    release_window: u64,
    // New fields for proposal tracking
    buyer_proposal: Option<Proposal>,
    seller_proposal: Option<Proposal>,
}
```

#### 2. Logic Changes to `propose_partial_refund`

**Add proposal ID generation:**

- Generate unique proposal ID using hash of (proposer_address + timestamp + nonce)
- Use ledger timestamp for deterministic ordering
- Add random nonce to prevent hash collisions for same-timestamp proposals

**Add existing proposal check:**

- Before accepting a new proposal, check if proposer already has an active proposal
- If proposer has active proposal, require explicit cancellation first
- Return error if attempting to overwrite without cancellation

**Pseudocode:**

```rust
FUNCTION propose_partial_refund(order_id: u32, proposer: Address, amount: i128) -> Result<(), String>
  // Load escrow
  escrow := load_escrow(order_id)

  // Validate proposer is buyer or seller
  IF proposer != escrow.buyer AND proposer != escrow.seller THEN
    RETURN Error("Unauthorized: only buyer or seller can propose")
  END IF

  // Check if proposer already has an active proposal
  IF proposer == escrow.buyer AND escrow.buyer_proposal.is_some() THEN
    existing := escrow.buyer_proposal.unwrap()
    IF existing.active THEN
      RETURN Error("Existing proposal active. Call cancel_proposal first.")
    END IF
  END IF

  IF proposer == escrow.seller AND escrow.seller_proposal.is_some() THEN
    existing := escrow.seller_proposal.unwrap()
    IF existing.active THEN
      RETURN Error("Existing proposal active. Call cancel_proposal first.")
    END IF
  END IF

  // Generate unique proposal ID
  timestamp := get_ledger_timestamp()
  nonce := generate_random_nonce()
  proposal_id := hash(proposer + timestamp + nonce)

  // Create new proposal
  new_proposal := Proposal {
    proposal_id: proposal_id,
    proposer_address: proposer,
    amount: amount,
    timestamp: timestamp,
    active: true,
  }

  // Store proposal
  IF proposer == escrow.buyer THEN
    escrow.buyer_proposal := Some(new_proposal)
  ELSE
    escrow.seller_proposal := Some(new_proposal)
  END IF

  save_escrow(escrow)
  RETURN Ok(())
END FUNCTION
```

#### 3. New Function: `cancel_proposal`

**Allow proposing party to withdraw their proposal:**

- Validate proposer authorization (only proposer can cancel)
- Update proposal status to cancelled (active = false)
- Return success confirmation

**Pseudocode:**

```rust
FUNCTION cancel_proposal(order_id: u32, proposer: Address) -> Result<(), String>
  // Load escrow
  escrow := load_escrow(order_id)

  // Find and cancel proposer's active proposal
  IF proposer == escrow.buyer AND escrow.buyer_proposal.is_some() THEN
    proposal := escrow.buyer_proposal.unwrap()
    IF proposal.proposer_address != proposer THEN
      RETURN Error("Unauthorized: only proposer can cancel")
    END IF
    IF NOT proposal.active THEN
      RETURN Error("Proposal already cancelled or accepted")
    END IF
    proposal.active := false
    escrow.buyer_proposal := Some(proposal)
    save_escrow(escrow)
    RETURN Ok(())
  END IF

  IF proposer == escrow.seller AND escrow.seller_proposal.is_some() THEN
    proposal := escrow.seller_proposal.unwrap()
    IF proposal.proposer_address != proposer THEN
      RETURN Error("Unauthorized: only proposer can cancel")
    END IF
    IF NOT proposal.active THEN
      RETURN Error("Proposal already cancelled or accepted")
    END IF
    proposal.active := false
    escrow.seller_proposal := Some(proposal)
    save_escrow(escrow)
    RETURN Ok(())
  END IF

  RETURN Error("No active proposal found")
END FUNCTION
```

#### 4. Tie-Breaking Mechanism

**For same-block proposals, use lexicographic order of proposer addresses:**

- Proposals are ordered by (timestamp, proposer_address) tuple
- Lower timestamp wins
- If same timestamp, lower proposer address (lexicographic) wins

**Pseudocode:**

```rust
FUNCTION compare_proposals(p1: Proposal, p2: Proposal) -> Ordering
  // First compare by timestamp
  IF p1.timestamp < p2.timestamp THEN
    RETURN Less
  END IF
  IF p1.timestamp > p2.timestamp THEN
    RETURN Greater
  END IF

  // Same timestamp - use lexicographic order of proposer addresses
  RETURN compare(p1.proposer_address, p2.proposer_address)
END FUNCTION

FUNCTION get_winning_proposal(buyer_proposal: Option<Proposal>, seller_proposal: Option<Proposal>) -> Option<Proposal>
  IF buyer_proposal.is_none() THEN
    RETURN seller_proposal
  END IF

  IF seller_proposal.is_none() THEN
    RETURN buyer_proposal
  END IF

  // Both exist - compare and return winner
  IF compare_proposals(buyer_proposal.unwrap(), seller_proposal.unwrap()) == Less THEN
    RETURN buyer_proposal
  ELSE
    RETURN seller_proposal
  END IF
END FUNCTION
```

#### 5. Smart Contract Changes

**Update proposal storage structure:**

- Add `proposal_id`, `proposer_address`, `timestamp`, `active` fields to Proposal struct
- Add separate fields for buyer_proposal and seller_proposal

**Add cancellation storage field:**

- No additional storage needed - use `active: bool` field

**Update proposal acceptance logic:**

- Before accepting a proposal, check if it's still active
- If proposal is cancelled (active = false), reject acceptance
- Use tie-breaking mechanism when both proposals are active

**Pseudocode for acceptance:**

```rust
FUNCTION accept_proposal(order_id: u32, acceptor: Address) -> Result<(), String>
  escrow := load_escrow(order_id)

  // Determine which proposal to accept
  // If acceptor is buyer, accept seller's proposal
  // If acceptor is seller, accept buyer's proposal

  IF acceptor == escrow.buyer THEN
    proposal := escrow.seller_proposal
  ELSE
    proposal := escrow.buyer_proposal
  END IF

  // Check proposal exists and is active
  IF proposal.is_none() THEN
    RETURN Error("No proposal found")
  END IF

  IF NOT proposal.unwrap().active THEN
    RETURN Error("Proposal is no longer active (cancelled or accepted)")
  END IF

  // Check acceptor is the counterparty
  expected_proposer := IF acceptor == escrow.buyer THEN escrow.seller ELSE escrow.buyer
  IF proposal.unwrap().proposer_address != expected_proposer THEN
    RETURN Error("Proposal not from counterparty")
  END IF

  // Accept proposal and distribute funds
  distribute_funds(escrow, proposal.unwrap().amount)

  // Mark proposal as accepted (not just cancelled)
  proposal.unwrap().active := false
  save_escrow(escrow)

  RETURN Ok(())
END FUNCTION
```

## Testing Strategy

### Validation Approach

The testing strategy follows a two-phase approach: first, surface counterexamples that demonstrate the race condition on unfixed code, then verify the fix works correctly and preserves existing behavior.

### Exploratory Bug Condition Checking

**Goal**: Surface counterexamples that demonstrate the race condition BEFORE implementing the fix. Confirm or refute the root cause analysis. If we refute, we will need to re-hypothesize.

**Test Plan**: Write tests that simulate concurrent proposal submissions and assert that:

1. Proposals are tracked with unique IDs
2. Overwrites are prevented without explicit cancellation
3. Tie-breaking is deterministic for same-block proposals

**Test Cases**:

1. **Overwrite Race Test**: Simulate buyer and seller submitting proposals in same block (will fail on unfixed code)
2. **No Withdrawal Test**: Verify proposer cannot withdraw proposal without cancellation function (will fail on unfixed code)
3. **No Tie-Breaking Test**: Verify same-block proposals have deterministic ordering (will fail on unfixed code)
4. **Cancellation Test**: Verify proposer can cancel their own proposal (will fail on unfixed code)

**Expected Counterexamples**:

- Proposals are overwritten without notification
- No mechanism to withdraw proposals
- No deterministic ordering for same-block proposals
- Proposers cannot cancel their proposals

### Fix Checking

**Goal**: Verify that for all inputs where the bug condition holds, the fixed function produces the expected behavior.

**Pseudocode:**

```
FOR ALL (order_id, proposer, amount) WHERE isBugCondition(submission) DO
  result := propose_partial_refund_fixed(order_id, proposer, amount)
  ASSERT proposal_id_is_unique(result.proposal_id)
  ASSERT proposal_is_locked(result.proposal_id)
  ASSERT proposer_can_cancel(result.proposal_id)
END FOR
```

### Preservation Checking

**Goal**: Verify that for all inputs where the bug condition does NOT hold, the fixed function produces the same result as the original function.

**Pseudocode:**

```
FOR ALL (order_id, proposer, amount) WHERE NOT isBugCondition(submission) DO
  ASSERT original_propose(order_id, proposer, amount) = fixed_propose(order_id, proposer, amount)
  ASSERT original_accept(order_id, acceptor) = fixed_accept(order_id, acceptor)
END FOR
```

**Testing Approach**: Property-based testing is recommended for preservation checking because:

- It generates many test cases automatically across the input domain
- It catches edge cases that manual unit tests might miss
- It provides strong guarantees that behavior is unchanged for all non-buggy inputs

**Test Plan**: Observe behavior on UNFIXED code first for single proposals and acceptance, then write property-based tests capturing that behavior.

**Test Cases**:

1. **Single Proposal Preservation**: Verify single proposal submission works as before
2. **Proposal Acceptance Preservation**: Verify proposal acceptance works as before
3. **Fund Distribution Preservation**: Verify fund distribution works as before
4. **Escrow Status Preservation**: Verify escrow status transitions work as before

### Unit Tests

- Test proposal ID generation is unique
- Test existing proposal check prevents overwrites
- Test cancellation function validates proposer authorization
- Test tie-breaking mechanism uses lexicographic order
- Test acceptance logic checks proposal is still active

### Property-Based Tests

- Generate random escrow states and verify proposals are tracked with unique IDs
- Generate random proposal submission sequences and verify no overwrites occur
- Generate random proposer addresses and verify tie-breaking is deterministic
- Generate random negotiation flows and verify preservation of acceptance logic

### Integration Tests

- Test full negotiation flow with multiple proposals
- Test cancellation and new proposal submission sequence
- Test same-block proposal race condition resolution
- Test escrow status transitions with active proposals
- Test unauthorized proposal rejection
