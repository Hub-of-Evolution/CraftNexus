# Implementation Plan: Platform Wallet Validation

## Overview

This feature adds validation for the platform wallet address to ensure safe fee collection. The validation service performs three key checks: zero-amount ping, account existence check, and contract transfer check. This document breaks down the implementation into discrete, actionable steps.

## Tasks

- [ ] 1. Set up project structure and core interfaces
  - Create directory structure for validation service
  - Define core TypeScript interfaces and types
  - Set up testing framework and configuration
  - _Requirements: 1, 2, 3, 4, 5, 6, 7, 8_

- [ ] 2. Implement data structures
  - [ ] 2.1 Create validation result structure
    - Implement `ValidationResult`, `ValidationError` interfaces
    - Implement `PingResult` interface
    - Implement `AccountCheckResult` interface
    - Implement `ContractCheckResult` interface
    - Implement `WalletConfig` interface
    - Implement `ValidationLogEntry` interface
    - _Requirements: 1, 2, 3, 4, 5, 6, 7, 8_

  - [ ]\* 2.2 Write unit tests for data structures
    - Test interface instantiation and validation
    - Test edge cases for each data structure
    - _Requirements: 1, 2, 3, 4, 5, 6, 7, 8_

- [ ] 3. Implement validation service
  - [ ] 3.1 Implement zero-amount ping function
    - Create `zeroAmountPing` function in validation service
    - Build zero-value transaction to test address
    - Handle transaction success/failure cases
    - Return `PingResult` with appropriate status
    - _Requirements: 3.1, 3.2, 3.3_

  - [ ]\* 3.2 Write property test for zero-amount ping
    - **Property 5: Zero-amount ping verifies fund receipt capability**
    - **Validates: Requirements 3.1, 3.2, 3.3**
    - Test that successful ping indicates fund receipt capability
    - Test that failed ping indicates inability to receive funds

  - [ ] 3.3 Implement account existence check function
    - Create `checkAccountExists` function in validation service
    - Query Stellar Horizon API for account existence
    - Distinguish between existing and uninitialized accounts
    - Return `AccountCheckResult` with existence status
    - _Requirements: 4.1, 4.2, 4.3_

  - [ ]\* 3.4 Write unit tests for account existence check
    - Test existing account detection
    - Test uninitialized account detection
    - Test error handling for network failures
    - _Requirements: 4.1, 4.2, 4.3_

  - [ ] 3.5 Implement contract transfer check function
    - Create `checkContractTransfers` function in validation service
    - Query Soroban RPC for contract metadata
    - Check for receive function that accepts payments
    - Return `ContractCheckResult` with transfer capability
    - _Requirements: 5.1, 5.2, 5.3_

  - [ ]\* 3.6 Write unit tests for contract transfer check
    - Test contract with receive function
    - Test contract without receive function
    - Test contract that rejects transfers
    - _Requirements: 5.1, 5.2, 5.3_

  - [ ] 3.7 Implement validation orchestrator
    - Create `validateWallet` function in validation service
    - Coordinate sequential validation checks
    - Aggregate results and errors from all checks
    - Implement timeout handling (default 5 seconds)
    - Return comprehensive `ValidationResult`
    - _Requirements: 1, 2, 3, 4, 5, 6, 8.1, 8.3_

  - [ ]\* 3.8 Write property test for validation orchestrator
    - **Property 1: Validation determines address validity correctly**
    - **Validates: Requirements 1.1, 1.2, 2.1, 2.2, 3.1, 3.2, 3.3, 4.1, 4.2, 4.3, 5.1, 5.2, 5.3**
    - Test valid addresses return valid status
    - Test invalid addresses return invalid status with errors

  - [ ]\* 3.9 Write property test for timeout handling
    - **Property 4: Validation completes within timeout**
    - **Validates: Requirements 8.1, 8.3**
    - Test validation completes within specified timeout
    - Test timeout error handling

- [ ] 4. Implement configuration manager integration
  - [ ] 4.1 Add validation to initialize function
    - Update `initializeWallet` to call validation service
    - Validate new address before storing configuration
    - Return error if validation fails
    - Store validated address on success
    - _Requirements: 1.1, 1.2, 1.3_

  - [ ] 4.2 Add validation to update function
    - Update `updateWallet` to call validation service
    - Validate new address before updating configuration
    - Return error and preserve existing config on failure
    - Update address on validation success
    - _Requirements: 2.1, 2.2, 2.3_

  - [ ] 4.3 Add validation status field to config
    - Add `validationStatus` field to wallet configuration
    - Add `lastValidatedAt` timestamp field
    - Add `lastError` field for error details
    - Update configuration storage schema
    - _Requirements: 1, 2, 6_

  - [ ]\* 4.4 Write unit tests for configuration manager
    - Test successful initialization with valid address
    - Test rejection of invalid address during initialization
    - Test successful update with valid address
    - Test rejection of invalid address during update
    - Test config preservation on failure
    - _Requirements: 1, 2, 6.3_

- [ ] 5. Implement escrow operation integration
  - [ ] 5.1 Add validation check to escrow processing
    - Update payment service to check wallet validity
    - Verify platform wallet is configured before commission transfer
    - Add validation status check to escrow transaction processing
    - _Requirements: 7.1_

  - [ ] 5.2 Add warning logging for invalid config
    - Implement warning log when wallet is invalid
    - Log warning before attempting commission transfer
    - Continue escrow processing despite invalid wallet
    - _Requirements: 7.2_

  - [ ] 5.3 Implement commission transfer with validation
    - Update commission calculation logic
    - Transfer commission to validated platform wallet address
    - Handle transfer failures gracefully
    - _Requirements: 7.3_

  - [ ]\* 5.4 Write integration tests for escrow integration
    - Test escrow processing with valid wallet
    - Test escrow processing with invalid wallet (warning logged)
    - Test commission transfer to validated wallet
    - _Requirements: 7.1, 7.2, 7.3_

- [ ] 6. Implement logging and error handling
  - [ ] 6.1 Implement validation logging
    - Create `ValidationLogEntry` storage
    - Log all validation attempts with timestamp
    - Include wallet address, result, errors, duration
    - Support audit trail queries
    - _Requirements: 6.2_

  - [ ] 6.2 Implement error handling
    - Create `ValidationErrorCode` enum
    - Implement descriptive error messages
    - Handle format validation errors
    - Handle ping failures with specific codes
    - Handle account not found errors
    - Handle contract rejection errors
    - Handle timeout errors
    - Handle network errors
    - _Requirements: 6.1, 8.3_

  - [ ] 6.3 Implement error response format
    - Create standardized error response structure
    - Include error code, message, details, timestamp
    - Support error aggregation for multiple failures
    - _Requirements: 6.1_

- [ ] 7. Implement configuration storage
  - [ ] 7.1 Create platform wallet configuration storage
    - Implement `PlatformWalletConfig` storage
    - Add `createdAt` and `updatedAt` timestamps
    - Implement persistent storage mechanism
    - Support configuration retrieval
    - _Requirements: 1.3, 2.3_

  - [ ] 7.2 Implement configuration versioning
    - Add version field to configuration
    - Support configuration migration
    - Track configuration changes over time
    - _Requirements: 1, 2_

- [ ] 8. Integration and wiring
  - [ ] 8.1 Wire validation service to configuration manager
    - Inject validation service into configuration manager
    - Connect validation calls in initialize and update
    - Handle validation results appropriately
    - _Requirements: 1, 2, 3, 4, 5, 6, 7, 8_

  - [ ] 8.2 Wire configuration manager to escrow operations
    - Inject configuration manager into payment service
    - Connect wallet validity checks in escrow processing
    - Connect commission transfer logic
    - _Requirements: 7.1, 7.2, 7.3_

  - [ ]\* 8.3 Write integration tests for full validation flow
    - Test complete validation sequence end-to-end
    - Test initialization with validation
    - Test update with validation
    - Test escrow processing with validation
    - _Requirements: 1, 2, 3, 4, 5, 6, 7, 8_

  - [ ]\* 8.4 Write property test for configuration preservation
    - **Property 2: Validation preserves existing configuration on failure**
    - **Validates: Requirements 1.3, 2.2, 6.3**
    - Test that existing config is preserved on validation failure
    - Test that new config is stored on validation success

  - [ ]\* 8.5 Write property test for audit logging
    - **Property 3: Validation logs all attempts for audit trail**
    - **Validates: Requirement 6.2**
    - Test that all validation attempts are logged
    - Test log entry contains required fields

- [ ] 9. Checkpoint - Ensure all tests pass
  - Run all unit tests and fix any failures
  - Run all integration tests and fix any failures
  - Ensure property-based tests pass
  - Ensure all acceptance criteria are covered
  - Ask the user if questions arise.

- [ ] 10. Documentation
  - [ ] 10.1 Add API documentation for validation service
    - Document `validateWallet` function
    - Document `zeroAmountPing` function
    - Document `checkAccountExists` function
    - Document `checkContractTransfers` function
    - Include usage examples
    - _Requirements: 1, 2, 3, 4, 5, 6, 7, 8_

  - [ ] 10.2 Add configuration guide for platform wallet setup
    - Document initialization process
    - Document update process
    - Document validation requirements
    - Document error handling
    - Include troubleshooting guide
    - _Requirements: 1, 2, 6_

- [ ] 11. Final checkpoint - Ensure all tests pass
  - Run full test suite
  - Verify all 24 acceptance criteria are covered
  - Verify all correctness properties are tested
  - Ensure no regressions in existing functionality
  - Ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties
- Unit tests validate specific examples and edge cases
- All implementation will use TypeScript based on the design document
- The validation service will integrate with Stellar Horizon API and Soroban RPC API
- Default timeout for validation is 5 seconds as specified in requirements
