# CraftNexus Contract Error Catalog

> **Version**: 1.0
> **Last Updated**: 2026-08-25
> **Source of Truth**: `craft-nexus-contract/src/lib.rs` Error Enum

## Overview

This document catalogs all public contract errors, their meanings, triggering conditions, and recommended client actions. This catalog serves as the authoritative reference for frontend developers and integrators handling contract errors.

## Error Categories

| Category | Error Range | Description |
|----------|-------------|-------------|
| Amount & Fee | 1-5, 8, 23 | Issues with amounts, fees, and validation |
| Authorization | 6-7, 18, 25, 31, 42, 49-52, 60 | Permission and state issues |
| Escrow State | 3-4, 12, 19-20 | Escrow lifecycle and status errors |
| Dispute Resolution | 10-11, 13-14, 53-55 | Dispute flow and evidence errors |
| Release & Windows | 15, 21, 23 | Timing and window errors |
| Configuration | 9, 26, 39, 44 | Platform configuration errors |
| Storage & Upgrades | 30, 32-38, 43, 45-48 | Upgrade and storage errors |
| Batch Operations | 27, 29, 57-60 | Batch and job errors |
| Staking & Tokens | 16-17, 24, 56 | Token and staking errors |
| Price Oracle & Conversion | 82-85 | Oracle guardrail violations (feeds, conversion bounds) |

---

## Complete Error List

### Amount & Fee Errors

#### Error 1: AmountBelowMinimum
**Description**: The escrow amount is below the minimum allowed threshold for the specified token.

**Triggering Conditions**:
- `amount <= 0`
- `amount < min_escrow_amount` for the token

**Suggested Client Action**:
1. Query the minimum amount: `get_min_escrow_amount(token)`
2. Increase the escrow amount accordingly
3. For batch operations, validate all amounts before submission

---

#### Error 5: InvalidFee
**Description**: Fee calculation error due to invalid parameters or arithmetic overflow.

**Triggering Conditions**:
- Fee basis points > `MAX_PLATFORM_FEE_BPS (10000)`
- Arithmetic overflow during fee calculation
- Fee allocation sum doesn't match escrow amount

**Suggested Client Action**:
1. Verify fee basis points are between 0-10000
2. Contact admin if fee configuration seems incorrect
3. Check for platform fee policy changes

---

#### Error 8: InvalidRefundAmount
**Description**: The refund amount is invalid for the current escrow state.

**Triggering Conditions**:
- `refund_amount <= 0`
- `refund_amount > escrow.amount`
- `refund_gross + seller_gross != escrow.amount` (partial refund)
- Fee allocation results in negative amounts

**Suggested Client Action**:
1. Ensure refund amount is between 1 and escrow.amount
2. For partial refunds, verify `refund + seller_remainder == escrow_amount`
3. Check that platform fee doesn't exceed seller remainder

---

#### Error 23: ReleaseWindowTooShort
**Description**: The release window is below the platform-configured minimum.

**Triggering Conditions**:
- `release_window < min_release_window`

**Suggested Client Action**:
1. Query minimum release window: `get_min_release_window()`
2. Increase the release window to meet or exceed the minimum
3. Consider using a longer window for buyer protection

---

### Authorization & State Errors

#### Error 6: Unauthorized
**Description**: The caller lacks required authorization for the operation.

**Triggering Conditions**:
- Caller is not the buyer, seller, admin, arbitrator, or moderator
- Caller is blacklisted (arbitrator blacklist)
- Caller doesn't own the batch job

**Suggested Client Action**:
1. Verify the caller address
2. Check if address has the required role
3. For arbitration: check if arbitrator is blacklisted
4. For batch jobs: ensure the owner is calling

---

#### Error 7: ContractPaused
**Description**: The contract is currently paused by the admin.

**Triggering Conditions**:
- `is_paused == true` and operation called

**Suggested Client Action**:
1. Check pause status: `is_paused()`
2. Wait until admin unpauses the contract
3. Monitor for unpause events

---

#### Error 9: PlatformNotInitialized
**Description**: Platform configuration hasn't been initialized.

**Triggering Conditions**:
- Calling functions before `initialize()` is called
- Platform config missing from storage

**Suggested Client Action**:
1. Ensure contract is properly initialized
2. Call `initialize()` with valid admin/arbitrator addresses
3. Verify initialization parameters

---

#### Error 18: InsufficientStake
**Description**: Artisan's stake is below the minimum required.

**Triggering Conditions**:
- `stake < min_stake_required`
- Active escrows exist but stake insufficient
- Unstaking would drop below minimum with active obligations

**Suggested Client Action**:
1. Check current stake: `get_stake(artisan)`
2. Check minimum required: `get_min_stake_required()`
3. Stake additional tokens: `stake_tokens(artisan, token, amount)`
4. Complete active escrows before unstaking

---

### Escrow State Errors

#### Error 3: EscrowNotFound
**Description**: No escrow exists for the provided order ID.

**Triggering Conditions**:
- Querying a non-existent `order_id`
- Order ID out of range

**Suggested Client Action**:
1. Verify the order ID is correct
2. Check escrow exists with `get_escrow(order_id)`
3. For new escrows, confirm creation succeeded

---

#### Error 4: InvalidEscrowState
**Description**: The escrow is in the wrong state for the requested operation.

**Triggering Conditions**:
- Operation on active escrow that's already released
- Dispute operations on non-disputed escrow
- Resolution on escrow not in Disputed state
- Settlement on escrow that's already settled

**Suggested Client Action**:
1. Check escrow status: `get_escrow(order_id).status`
2. Use the appropriate function for the current state
3. Wait for state transitions to complete

---

#### Error 12: EscrowAlreadyReleased
**Description**: Attempting to release an escrow that has already been released.

**Triggering Conditions**:
- Double-call to `release_funds()` or `auto_release()`

**Suggested Client Action**:
1. Check escrow status before calling
2. Verify escrow is still `Active` or `ReleasePending`
3. Skip operation if already released

---

### Dispute Resolution Errors

#### Error 10: DisputeExpired
**Description**: The dispute duration has passed without resolution.

**Triggering Conditions**:
- `current_time >= dispute_initiated_at + max_dispute_duration`

**Suggested Client Action**:
1. Use `resolve_expired_dispute()` instead of `resolve_dispute()`
2. This is the safety net for stale disputes
3. Any account can call this function

---

#### Error 11: ChallengeWindowActive
**Description**: Evidence challenge window is still active.

**Triggering Conditions**:
- `current_time < dispute_initiated_at + evidence_challenge_window`

**Suggested Client Action**:
1. Wait for the evidence challenge window to complete
2. Check remaining time: `evidence_challenge_window - elapsed`
3. Submit evidence before resolution is allowed

---

#### Error 13: EvidenceNotFound
**Description**: Evidence record not found.

**Triggering Conditions**:
- Querying non-existent evidence ID
- Evidence expired and removed

**Suggested Client Action**:
1. Verify evidence ID is correct
2. Check evidence expiry window
3. Submit new evidence if needed

---

#### Error 14: DisputeAlreadyResolved
**Description**: The dispute has already been resolved.

**Triggering Conditions**:
- Double-resolution attempt
- Escrow status is `Resolved`

**Suggested Client Action**:
1. Check escrow status
2. Withdraw funds if owed
3. No further action needed

---

#### Error 53: EvidenceExpired
**Description**: Evidence retention window has expired.

**Triggering Conditions**:
- Evidence `expires_at < current_time`
- Evidence marked as invalidated

**Suggested Client Action**:
1. Submit new evidence with fresh timestamps
2. Request extension if allowed
3. Use alternative evidence submission method

---

#### Error 54: EvidenceAlreadyUsed
**Description**: Evidence payload has already been used in a previous dispute.

**Triggering Conditions**:
- Same evidence hash detected in storage
- Evidence reuse prevention triggered

**Suggested Client Action**:
1. Submit unique evidence content
2. Include additional metadata to differentiate
3. Use fresh evidence for each dispute

---

#### Error 55: InvalidDisputeSession
**Description**: Invalid dispute session for evidence submission.

**Triggering Conditions**:
- Evidence submitted to wrong dispute
- `dispute_session_id` mismatch

**Suggested Client Action**:
1. Verify correct order ID
2. Ensure escrow is in Disputed state
3. Check dispute initiation timestamp matches

---

### Release & Window Errors

#### Error 15: ReleaseWindowNotElapsed
**Description**: The release window hasn't elapsed yet.

**Triggering Conditions**:
- `current_time - created_at < release_window` (auto-release)
- Escalation called before escalation window

**Suggested Client Action**:
1. Wait for the full release window
2. Check remaining time: `release_window - elapsed`
3. Buyer can release early if they choose

---

#### Error 21: StakeCooldownActive
**Description**: Stake cooldown period is still active.

**Triggering Conditions**:
- Unstaking before cooldown period ends
- `cooldown_end > current_time`

**Suggested Client Action**:
1. Wait for cooldown to complete
2. Check cooldown end time with deposit records
3. Schedule unstaking after cooldown

---

### Batch Operation Errors

#### Error 27: StakeQueueFull
**Description**: Stake history queue is at capacity.

**Triggering Conditions**:
- Queue size >= `MAX_STAKE_QUEUE_SIZE`
- Pruning threshold reached

**Suggested Client Action**:
1. Wait for automatic pruning
2. Unstake matured deposits first
3. Reduce frequency of stake operations

---

#### Error 29: BatchLimitExceeded
**Description**: Batch operation exceeds the maximum allowed size.

**Triggering Conditions**:
- Batch size > `MAX_BATCH_SIZE (50)`
- Rate limit exceeded (disputes)

**Suggested Client Action**:
1. Reduce batch size to ≤ 50
2. Use scheduled batch for larger operations
3. Implement pagination with `schedule_batch_escrow()`

---

#### Error 57: InvalidBatchWorkLimit
**Description**: Requested continuation size is outside scheduler bounds.

**Triggering Conditions**:
- `work_limit == 0`
- `work_limit > MAX_SCHEDULED_BATCH_WORK`

**Suggested Client Action**:
1. Use work_limit between 1-100
2. Process in chunks
3. Check progress with `get_batch_escrow_progress()`

---

#### Error 58: BatchJobCancelled
**Description**: The scheduled batch has been cancelled.

**Triggering Conditions**:
- Job status is `BatchJobStatus::Cancelled`

**Suggested Client Action**:
1. Create a new batch job
2. Check status before continuing
3. Review why cancellation occurred

---

#### Error 59: BatchJobNotFound
**Description**: The requested scheduled batch doesn't exist.

**Triggering Conditions**:
- Querying non-existent job ID
- Job ID invalid or expired

**Suggested Client Action**:
1. Verify job ID is correct
2. Schedule a new batch if needed
3. Check job creation confirmation

---

#### Error 60: BatchJobUnauthorized
**Description**: Caller is not the account that scheduled the batch.

**Triggering Conditions**:
- `caller != job.owner`

**Suggested Client Action**:
1. Use the correct owner address
2. Check batch ownership with `get_batch_escrow_progress()`
3. The owner must call continuation functions

---

### Configuration & Setup Errors

#### Error 25: InvalidAdminAddress
**Description**: Invalid admin address provided.

**Triggering Conditions**:
- Zero address provided
- Invalid address format

**Suggested Client Action**:
1. Provide a valid Soroban address
2. Check address formatting
3. Use contract-generated address

---

#### Error 26: CorruptedPlatformConfig
**Description**: Platform configuration storage is corrupted or missing required fields.

**Triggering Conditions**:
- Storage corruption
- Missing required config fields

**Suggested Client Action**:
1. Contact admin for recovery
2. Reinitialize platform
3. Run storage migration if available

---

#### Error 31: NoPendingAdmin
**Description**: No pending admin transfer to accept or cancel.

**Triggering Conditions**:
- Calling `accept_admin_transfer()` or `cancel_admin_transfer()` without pending transfer

**Suggested Client Action**:
1. Check pending admin status
2. Initiate transfer first: `initiate_admin_transfer()`
3. Only call accept/cancel when pending exists

---

#### Error 39: OnboardingContractNotSet
**Description**: Onboarding contract address hasn't been configured.

**Triggering Conditions**:
- Calling functions requiring onboarding
- Onboarding address not set in storage

**Suggested Client Action**:
1. Set onboarding contract: `set_onboarding_contract()`
2. Deploy onboarding contract first
3. Verify address is valid

---

#### Error 42: NotAnUpgradeSigner
**Description**: Caller is not an authorized upgrade signer.

**Triggering Conditions**:
- Calling `propose_upgrade_wasm()` without being in signer list

**Suggested Client Action**:
1. Check upgrade signers list
2. Contact admin to be added
3. Admin can propose directly

---

#### Error 44: InvalidTokenDecimals
**Description**: Token decimal places outside supported range.

**Triggering Conditions**:
- Token decimals < 0 or > 18

**Suggested Client Action**:
1. Use tokens with 0-18 decimals
2. Check token contract specification
3. Use whitelisted tokens

---

### Storage & Upgrade Errors

#### Error 30: DeprecatedFunction
**Description**: Deprecated function called.

**Triggering Conditions**:
- Calling deprecated function
- Legacy compatibility call

**Suggested Client Action**:
1. Use the replacement function
2. Check migration documentation
3. Update to latest contract version

---

#### Error 32: NoUpgradeProposed
**Description**: No WASM upgrade has been proposed.

**Triggering Conditions**:
- Calling `execute_upgrade()` without proposal
- Proposal already executed/cancelled

**Suggested Client Action**:
1. Propose upgrade first: `propose_upgrade_wasm()`
2. Wait for approvals and cooldown
3. Check proposal status

---

#### Error 33: UpgradeCooldownActive
**Description**: WASM upgrade cooldown period is still active.

**Triggering Conditions**:
- `current_time < upgrade_at`
- Cooldown from cancel operation

**Suggested Client Action**:
1. Wait for cooldown period to complete
2. Check `upgrade_at` timestamp
3. Execute after cooldown

---

#### Error 34: UpgradeProposalExists
**Description**: A WASM upgrade proposal already exists.

**Triggering Conditions**:
- Creating duplicate proposal

**Suggested Client Action**:
1. Use existing proposal
2. Cancel existing if needed
3. Wait for execution

---

#### Error 35: InvalidUpgradeHash
**Description**: Invalid WASM upgrade hash provided.

**Triggering Conditions**:
- Zero hash provided
- Hash doesn't match pending proposal
- Hash points to invalid WASM

**Suggested Client Action**:
1. Provide valid WASM hash
2. Hash must be from deployed WASM
3. Verify hash with deployer

---

#### Error 36: RecurringEscrowNotFound
**Description**: Recurring escrow not found.

**Triggering Conditions**:
- Querying non-existent recurring escrow ID

**Suggested Client Action**:
1. Verify ID is correct
2. Check creation confirmation
3. Use pagination to find active escrows

---

#### Error 37: CycleNotReady
**Description**: Escrow cycle not ready for release.

**Triggering Conditions**:
- `current_time < last_release_time + frequency`
- `current_cycle >= duration`
- Operation on inactive recurring escrow

**Suggested Client Action**:
1. Wait for next cycle
2. Check remaining time
3. Verify escrow is active

---

#### Error 38: RecurringEscrowIdExhausted
**Description**: Recurring escrow ID counter has reached maximum safe value.

**Triggering Conditions**:
- ID > `MAX_RECURRING_ESCROW_ID`

**Suggested Client Action**:
1. Use existing recurring escrows
2. Contact admin for extension
3. Consider using standard escrows

---

#### Error 43: AlreadyApproved
**Description**: The same signer already approved this WASM upgrade hash.

**Triggering Conditions**:
- Signer already in approval list
- Double-approval attempt

**Suggested Client Action**:
1. Check approval status
2. Wait for other signers
3. Propose new hash if needed

---

#### Error 45: UpgradeCompatibilityMissing
**Description**: No compatibility manifest has been submitted.

**Triggering Conditions**:
- Missing manifest for upgrade hash
- Manifest not found in storage

**Suggested Client Action**:
1. Submit manifest: `submit_compat_manifest()`
2. Run compatibility checks
3. Ensure all requirements met

---

#### Error 46: UpgradeCompatibilityInvalid
**Description**: The compatibility manifest is invalid.

**Triggering Conditions**:
- Version mismatch
- Zero commitments
- Invalid state commitment

**Suggested Client Action**:
1. Fix manifest values
2. Run compatibility tools
3. Verify state matches

---

#### Error 47: UpgradeMigrationIncomplete
**Description**: Migration report contains records requiring manual handling.

**Triggering Conditions**:
- `migration_complete == false`
- `manual_records > 0`

**Suggested Client Action**:
1. Complete manual migrations
2. Resolve pending records
3. Resubmit manifest

---

#### Error 48: StorageLayoutMismatch
**Description**: Persisted storage is on a legacy layout.

**Triggering Conditions**:
- Storage version mismatch
- Migration not run

**Suggested Client Action**:
1. Run migration: `migrate_storage_layout()`
2. Admin only operation
3. Check migration success

---

#### Error 49: AdminActionTerminal
**Description**: Admin action is in terminal state.

**Triggering Conditions**:
- Action already executed
- Action was cancelled

**Suggested Client Action**:
1. Check action status
2. Create new action if needed
3. No further operations possible

---

#### Error 50: AdminActionNeedsApprovals
**Description**: Admin action doesn't have enough approvals.

**Triggering Conditions**:
- Approvals < threshold

**Suggested Client Action**:
1. Get more signers to approve
2. Check required threshold
3. Wait for signatures

---

#### Error 51: AdminActionTimelockActive
**Description**: Admin action timelock is still active.

**Triggering Conditions**:
- `current_time < execution_time`

**Suggested Client Action**:
1. Wait for timelock to expire
2. Schedule execution after delay
3. Monitor timelock status

---

#### Error 52: NotAnAdminActionSigner
**Description**: Caller is not an authorized admin action signer.

**Triggering Conditions**:
- Approval attempt by unauthorized signer

**Suggested Client Action**:
1. Check authorized signers list
2. Use only authorized addresses
3. Contact admin to be added

---

### Token & Whitelist Errors

#### Error 16: TokenNotWhitelisted
**Description**: Token is not in the whitelist.

**Triggering Conditions**:
- Using token not in whitelist
- Whitelist is enabled and token missing

**Suggested Client Action**:
1. Check whitelist: `is_token_whitelisted(token)`
2. Contact admin to whitelist token
3. Use whitelisted token

---

#### Error 17: WhitelistDisabled
**Description**: Whitelist feature is disabled.

**Triggering Conditions**:
- Admin disabled whitelist
- Whitelist check attempted

**Suggested Client Action**:
1. Any token can be used
2. No action needed
3. Check platform policy

---

#### Error 24: StakeTokenMismatch
**Description**: Staked funds can only be withdrawn in the original staking token.

**Triggering Conditions**:
- Unstaking different token than staked

**Suggested Client Action**:
1. Use the original staking token
2. Check stake record for token
3. Only one token per stake

---

#### Error 56: UnsupportedToken
**Description**: Contract does not implement the supported token interface.

**Triggering Conditions**:
- Token contract doesn't support required functions
- Non-compliant token implementation

**Suggested Client Action**:
1. Use Soroban-compliant tokens
2. Check token interface
3. Use whitelisted tokens only

---

### Price Oracle & Conversion Errors

#### Error 82: PriceFeedNotFound
**Description**: No price feed is configured for the requested token.

**Triggering Conditions**:
- Oracle-backed conversion or fee quote references a token without `set_price_feed`
- Feed was removed via `remove_price_feed`

**Suggested Client Action**:
1. Check configured feeds: `get_price_feed(token)`
2. Contact admin to publish a feed
3. Retry once the feed exists

---

#### Error 83: StalePriceData
**Description**: The price feed is stale (older than `max_staleness`) or carries a future timestamp. **Retryable** — refresh the feed and retry.

**Triggering Conditions**:
- `now - feed.timestamp > max_staleness`
- `feed.timestamp > now` (malformed clock data)

**Suggested Client Action**:
1. Refresh the feed via `set_price_feed`
2. Retry the operation

---

#### Error 84: InvalidPriceData
**Description**: The price feed is malformed or an oracle configuration value is invalid.

**Triggering Conditions**:
- Zero/negative feed price
- Feed decimals outside 0–18
- `set_oracle_config` with `max_staleness == 0` or `max_deviation_bps > 10000`

**Suggested Client Action**:
1. Contact admin to fix the feed or configuration
2. No retry will succeed until the data is corrected

---

#### Error 85: ConversionOutOfBounds
**Description**: An oracle-backed conversion or fee quote fell outside the configured deviation band, or its arithmetic overflowed.

**Triggering Conditions**:
- Observed rate deviates from the oracle reference by more than `max_deviation_bps`
- Fee-quote round trip drifts beyond the band
- Intermediate arithmetic overflow (rejected rather than truncated)

**Suggested Client Action**:
1. Re-check the observed rate against `get_oracle_config().max_deviation_bps`
2. Retry with a rate closer to the oracle reference
3. Refresh feeds if the market moved

---

## Error Codes Quick Reference

| Code | Error Name | Code | Error Name |
|------|------------|------|------------|
| 1 | AmountBelowMinimum | 31 | NoPendingAdmin |
| 2 | SameBuyerSeller | 32 | NoUpgradeProposed |
| 3 | EscrowNotFound | 33 | UpgradeCooldownActive |
| 4 | InvalidEscrowState | 34 | UpgradeProposalExists |
| 5 | InvalidFee | 35 | InvalidUpgradeHash |
| 6 | Unauthorized | 36 | RecurringEscrowNotFound |
| 7 | ContractPaused | 37 | CycleNotReady |
| 8 | InvalidRefundAmount | 38 | RecurringEscrowIdExhausted |
| 9 | PlatformNotInitialized | 39 | OnboardingContractNotSet |
| 10 | DisputeExpired | 40 | InvalidMetadataHash |
| 11 | ChallengeWindowActive | 41 | InvalidIpfsHash |
| 12 | EscrowAlreadyReleased | 42 | NotAnUpgradeSigner |
| 13 | EvidenceNotFound | 43 | AlreadyApproved |
| 14 | DisputeAlreadyResolved | 44 | InvalidTokenDecimals |
| 15 | ReleaseWindowNotElapsed | 45 | UpgradeCompatibilityMissing |
| 16 | TokenNotWhitelisted | 46 | UpgradeCompatibilityInvalid |
| 17 | WhitelistDisabled | 47 | UpgradeMigrationIncomplete |
| 18 | InsufficientStake | 48 | StorageLayoutMismatch |
| 19 | ProposalNotFound | 49 | AdminActionTerminal |
| 20 | ProposalAlreadyExists | 50 | AdminActionNeedsApprovals |
| 21 | StakeCooldownActive | 51 | AdminActionTimelockActive |
| 22 | ReentryDetected | 52 | NotAnAdminActionSigner |
| 23 | ReleaseWindowTooShort | 53 | EvidenceExpired |
| 24 | StakeTokenMismatch | 54 | EvidenceAlreadyUsed |
| 25 | InvalidAdminAddress | 55 | InvalidDisputeSession |
| 26 | CorruptedPlatformConfig | 56 | UnsupportedToken |
| 27 | StakeQueueFull | 57 | InvalidBatchWorkLimit |
| 28 | AdminRecoveryFailed | 58 | BatchJobCancelled |
| 29 | BatchLimitExceeded | 59 | BatchJobNotFound |
| 30 | DeprecatedFunction | 60 | BatchJobUnauthorized |
| 80 | PaginationLimitZero | 82 | PriceFeedNotFound |
| 81 | PaginationCursorInvalid | 83 | StalePriceData |
| 86 | EscrowAlreadyExists | 84 | InvalidPriceData |
| — | — | 85 | ConversionOutOfBounds |

> **Note**: `EscrowAlreadyExists` was renumbered from `80` to `86` to resolve a
> duplicate discriminant with `PaginationLimitZero` (Issue #1045).

---

## Frontend Integration Guide

### Error Handling Pattern

```typescript
try {
  await contract.create_escrow(params);
} catch (error) {
  const errorCode = parseContractError(error);
  switch(errorCode) {
    case 1: // AmountBelowMinimum
      showError(`Minimum amount required: ${await getMinAmount(token)}`);
      break;
    case 6: // Unauthorized
      showError('You don\'t have permission for this operation');
      break;
    case 7: // ContractPaused
      showError('Contract is paused. Please try again later.');
      break;
    // ... handle other errors
  }
}
```

### Error Code Parsing

```typescript
function parseContractError(error: any): number {
  // Soroban error format may vary
  const errorString = error.toString();
  const match = errorString.match(/Error: (\d+)/);
  return match ? parseInt(match[1]) : -1;
}
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-08-25 | Initial catalog creation from error enum |
| 1.1 | 2026-08-27 | Added price-oracle errors 82–85 (Issue #1044); documented `EscrowAlreadyExists` renumber 80→86 (Issue #1045) |

---

*This catalog is automatically generated from the contract source code and should be updated whenever errors are added or modified.*
