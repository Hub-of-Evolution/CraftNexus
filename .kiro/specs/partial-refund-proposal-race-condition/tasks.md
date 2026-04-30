# Implementation Plan

- [ ] 1. Write bug condition exploration test
  - **Property 1: Bug Condition** - Partial Refund Proposal Race Condition
  - **CRITICAL**: This test MUST FAIL on unfixed code - failure confirms the race condition exists
  - **DO NOT attempt to fix the test or the code when it fails**
  - **NOTE**: This test encodes the expected behavior - it will validate the fix when it passes after implementation
  - **GOAL**: Surface counterexamples that demonstrate the race condition exists
  - **Scoped PBT Approach**: For deterministic bugs, scope the property to concrete failing case(s) to ensure reproducibility
  - Test implementation details from Bug Condition in design
  - The test assertions should match the Expected Behavior Properties from design
  - Run test on UNFIXED code
  - **EXPECTED OUTCOME**: Test FAILS (this is correct - it proves the race condition exists)
  - Document counterexamples found to understand root cause
  - Mark task complete when test is written, run, and failure is documented
  - _Requirements: 1.1, 1.2, 1.3_

- [ ] 2. Write preservation property tests (BEFORE implementing fix)
  - **Property 2: Preservation** - Existing Negotiation Flows
  - **IMPORTANT**: Follow observation-first methodology
  - Observe behavior on UNFIXED code for single proposals and acceptance
  - Write property-based tests capturing observed behavior patterns from Preservation Requirements
  - Property-based testing generates many test cases for stronger guarantees
  - Run tests on UNFIXED code
  - **EXPECTED OUTCOME**: Tests PASS (this confirms baseline behavior to preserve)
  - Mark task complete when tests are written, run, and passing on unfixed code
  - _Requirements: 3.1, 3.2, 3.3, 3.4_

- [ ] 3. Fix for partial refund proposal race condition
  - [ ] 3.1 Update smart contract storage structure
    - Add `proposal_id: string` field to Proposal struct
    - Add `proposer_address: string` field to Proposal struct
    - Add `timestamp: number` field to Proposal struct
    - Add `active: boolean` field to Proposal struct
    - Update Escrow struct to store separate buyer_proposal and seller_proposal fields
    - _Bug_Condition: isBugCondition(input) where both buyer and seller submit proposals in same ledger block_
    - _Expected_Behavior: expectedBehavior(result) from design - unique proposal IDs and locking_
    - _Preservation: Preservation Requirements from design - existing negotiation flows unchanged_
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 3.1, 3.2, 3.3, 3.4_

  - [ ] 3.2 Implement propose_partial_refund logic
    - Generate unique proposal ID using hash of (proposer_address + timestamp + nonce)
    - Check for existing active proposals before allowing new proposals
    - Require explicit cancellation or proposal ID match
    - Return error if proposer attempts to overwrite without cancellation
    - _Bug_Condition: isBugCondition(input) where existingProposalExists(input.order_id) and NOT isSameProposalId(input, existingProposal)_
    - _Expected_Behavior: expectedBehavior(result) from design - unique proposal tracking and locking_
    - _Preservation: Preservation Requirements from design - unauthorized proposals still rejected_
    - _Requirements: 2.1, 2.2, 3.4_

  - [ ] 3.3 Implement cancel_proposal function
    - Validate proposer authorization (only proposer can cancel their own proposal)
    - Update proposal status to cancelled (active = false)
    - Return success confirmation
    - Return error if proposal not found or already cancelled
    - _Bug_Condition: isBugCondition(input) where no mechanism exists for proposers to withdraw proposals_
    - _Expected_Behavior: expectedBehavior(result) from design - proposers can cancel their proposals_
    - _Preservation: Preservation Requirements from design - existing negotiation flows unchanged_
    - _Requirements: 2.3, 3.1, 3.2, 3.3_

  - [ ] 3.4 Implement tie-breaking mechanism
    - Implement deterministic selection using lexicographic order of proposer addresses
    - Proposals ordered by (timestamp, proposer_address) tuple
    - Lower timestamp wins; if same timestamp, lower proposer address (lexicographic) wins
    - _Bug_Condition: isBugCondition(input) where no deterministic tie-breaking exists for same-block proposals_
    - _Expected_Behavior: expectedBehavior(result) from design - deterministic ordering using lexicographic comparison_
    - _Preservation: Preservation Requirements from design - existing negotiation flows unchanged_
    - _Requirements: 2.4, 3.1, 3.2, 3.3_

  - [ ] 3.5 Update acceptance logic
    - Update proposal acceptance to handle cancelled proposals
    - Reject acceptance of cancelled proposals (active = false)
    - Use tie-breaking mechanism when both proposals are active
    - Validate proposer authorization (only counterparty can accept)
    - _Bug_Condition: isBugCondition(input) where proposals can be silently overwritten without notification_
    - _Expected_Behavior: expectedBehavior(result) from design - proposals tracked with unique IDs and cannot be overwritten_
    - _Preservation: Preservation Requirements from design - fund distribution and escrow status transitions unchanged_
    - _Requirements: 2.1, 2.2, 3.1, 3.2, 3.3, 3.4_

  - [ ] 3.6 Write unit tests for proposal ID generation
    - Test proposal ID is unique for different (proposer, timestamp, nonce) combinations
    - Test proposal ID generation uses hash function correctly
    - Test proposal ID is deterministic for same inputs
    - _Requirements: 2.1_

  - [ ] 3.7 Write property-based tests for race condition prevention
    - Generate random escrow states and verify proposals are tracked with unique IDs
    - Generate random proposal submission sequences and verify no overwrites occur
    - Generate random proposer addresses and verify tie-breaking is deterministic
    - _Requirements: 2.1, 2.2, 2.4_

  - [ ] 3.8 Write integration tests for complete negotiation flow
    - Test full negotiation flow with multiple proposals
    - Test cancellation and new proposal submission sequence
    - Test same-block proposal race condition resolution
    - Test escrow status transitions with active proposals
    - Test unauthorized proposal rejection
    - _Requirements: 2.1, 2.2, 2.3, 2.4, 3.1, 3.2, 3.3, 3.4_

  - [ ] 3.9 Verify bug condition exploration test now passes
    - **Property 1: Expected Behavior** - Partial Refund Proposal Race Condition
    - **IMPORTANT**: Re-run the SAME test from task 1 - do NOT write a new test
    - The test from task 1 encodes the expected behavior
    - When this test passes, it confirms the expected behavior is satisfied
    - Run bug condition exploration test from step 1
    - **EXPECTED OUTCOME**: Test PASSES (confirms bug is fixed)
    - _Requirements: 2.1, 2.2, 2.3, 2.4_

  - [ ] 3.10 Verify preservation tests still pass
    - **Property 2: Preservation** - Existing Negotiation Flows
    - **IMPORTANT**: Re-run the SAME tests from task 2 - do NOT write new tests
    - Run preservation property tests from step 2
    - **EXPECTED OUTCOME**: Tests PASS (confirms no regressions)
    - Confirm all tests still pass after fix (no regressions)
    - _Requirements: 3.1, 3.2, 3.3, 3.4_

- [ ] 4. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.
