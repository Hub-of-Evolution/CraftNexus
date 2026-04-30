# Bugfix Requirements Document

## Introduction

This bugfix addresses a race condition in the `propose_partial_refund` function that allows creating settlement proposals for disputed escrows. When both buyer and seller submit proposals in the same ledger block or rapidly consecutively, they might overwrite each other's proposals without knowing, leading to unpredictable negotiation flows and potential loss of proposal state.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN both buyer and seller submit partial refund proposals in the same ledger block THEN the system allows overwriting without notification, causing race conditions in proposal acceptance

1.2 WHEN a party submits a proposal and the counterparty submits another proposal before the first is accepted OR cancelled THEN the system overwrites the first proposal without explicit cancellation from the proposing party

1.3 WHEN a proposal exists and no mechanism is available for the proposing party to withdraw it THEN the proposal remains active indefinitely, blocking new proposals

### Expected Behavior (Correct)

2.1 WHEN a party submits a partial refund proposal THEN the system SHALL assign a unique proposal ID and store the proposal with metadata including the proposer and timestamp

2.2 WHEN a proposal exists and the counterparty attempts to submit a new proposal THEN the system SHALL require explicit cancellation of the existing proposal before accepting the new one

2.3 WHEN the proposing party wishes to withdraw their proposal BEFORE it is accepted THEN the system SHALL allow explicit cancellation with a dedicated function

2.4 WHEN both proposals are submitted in the same ledger block THEN the system SHALL use a deterministic tie-breaking mechanism (e.g., lexicographic order of proposer addresses) to ensure predictable ordering

### Unchanged Behavior (Regression Prevention)

3.1 WHEN a proposal is accepted by the counterparty THEN the system SHALL CONTINUE TO distribute funds according to the proposal amount and platform fee

3.2 WHEN a proposal is not accepted within the escrow's lifetime THEN the system SHALL CONTINUE TO allow the proposal to be accepted until the escrow is resolved

3.3 WHEN only one party submits a proposal THEN the system SHALL CONTINUE TO allow the counterparty to accept it for negotiation

3.4 WHEN a proposal is submitted by an unauthorized party (neither buyer nor seller) THEN the system SHALL CONTINUE TO reject the proposal with an unauthorized error
