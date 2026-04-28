# Implementation Plan: NFT and Semi-Fungible Collateral Support

## Overview

This feature extends the Craft Nexus escrow system to support NFT and semi-fungible tokens as collateral. The implementation follows a modular architecture with dedicated components for token standard detection, collateral validation, and token-specific management. All tasks build incrementally from core types to integration.

## Tasks

- [ ] 1. Set up project structure and core types
  - Create `collateral/` directory structure
  - Define core TypeScript interfaces and types
  - Implement token standard enumeration
  - _Requirements: 1, 2, 3, 5, 6_

- [ ] 2. Implement token standard detection
  - [ ] 2.1 Create token detector interface and implementation
    - Implement `detectStandard()` method
    - Implement `hasInterface()` method
    - Implement `getAvailableInterfaces()` method
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 3.5_
  - [ ]\* 2.2 Write property test for token standard detection
    - **Property 1: Token Standard Detection Accuracy**
    - **Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.5, 6.4**
  - [ ] 2.3 Implement interface checking logic
    - Check for NFT interfaces (nft_metadata, nft_core)
    - Check for semi-fungible interfaces (sfungible_token)
    - Check for fungible token interfaces (fungible_token)
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_

- [ ] 3. Implement NFT collateral management
  - [ ] 3.1 Create NFT manager interface
    - Implement `validateOwnership()` method
    - Implement `lock()` method
    - Implement `unlock()` method
    - _Requirements: 1.1, 1.5, 8.1, 8.3_
  - [ ] 3.2 Implement NFT ownership validation
    - Query token contract for owner
    - Verify user owns the specific NFT
    - Return descriptive errors
    - _Requirements: 4.1, 4.2, 4.4, 4.5_
  - [ ] 3.3 Implement NFT lock/unlock operations
    - Lock NFT for escrow duration
    - Unlock NFT after escrow release/refund
    - Handle transaction errors
    - _Requirements: 1.3, 8.1, 8.3_
  - [ ] 3.4 Implement NFT metadata handling
    - Retrieve NFT metadata URI
    - Cache metadata during escrow
    - Include metadata in status queries
    - _Requirements: 9.1, 9.2, 9.3, 9.4, 9.5_
  - [ ]\* 3.5 Write property test for NFT ownership validation
    - **Property 2: NFT Ownership Validation**
    - **Validates: Requirements 1.1, 1.5**
  - [ ]\* 3.6 Write property test for NFT storage integrity
    - **Property 3: NFT Collateral Storage Integrity**
    - **Validates: Requirements 1.2**
  - [ ]\* 3.7 Write property test for NFT transfer prevention
    - **Property 4: NFT Transfer Prevention During Escrow**
    - **Validates: Requirements 1.3, 8.1, 8.3**

- [ ] 4. Implement semi-fungible collateral management
  - [ ] 4.1 Create semi-fungible manager interface
    - Implement `validateOwnership()` method
    - Implement `lock()` method
    - Implement `unlock()` method
    - _Requirements: 2.1, 2.5, 8.2, 8.4_
  - [ ] 4.2 Implement semi-fungible quantity validation
    - Verify quantity is non-zero
    - Verify quantity is available
    - Return descriptive errors
    - _Requirements: 2.4, 4.3, 4.4, 4.5_
  - [ ] 4.3 Implement semi-fungible balance tracking
    - Lock specific token units
    - Track pledged quantity
    - Prevent transfer of locked units
    - _Requirements: 2.2, 2.3, 6.3, 8.2, 8.4_
  - [ ]\* 4.4 Write property test for semi-fungible quantity validation
    - **Property 5: Semi-Fungible Quantity Validation**
    - **Validates: Requirements 2.4**
  - [ ]\* 4.5 Write property test for semi-fungible balance tracking
    - **Property 6: Semi-Fungible Balance Tracking**
    - **Validates: Requirements 2.2, 2.3**

- [ ] 5. Implement unified collateral validator
  - [ ] 5.1 Create collateral validator service
    - Implement unified validation interface
    - Route to token-specific validators
    - Cache validation results
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5_
  - [ ] 5.2 Implement error handling
    - Token detection errors
    - Validation errors
    - Operation errors
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5_
  - [ ]\* 5.3 Write unit tests for collateral validator
    - Test all validation scenarios
    - Test error handling
    - Test caching behavior
    - _Requirements: 4.1-4.5, 7.1-7.5_

- [ ] 6. Implement unified collateral service
  - [ ] 6.1 Create unified collateral service
    - Implement single entry point for all collateral types
    - Route based on token standard
    - Handle backward compatibility
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5_
  - [ ] 6.2 Implement backward compatibility layer
    - Maintain existing fungible token logic
    - Ensure same error messages
    - No NFT-specific validation for fungible tokens
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5_
  - [ ]\* 6.3 Write property test for backward compatibility
    - **Property 7: Backward Compatibility with Fungible Tokens**
    - **Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5**

- [ ] 7. Update escrow contract for new collateral types
  - [ ] 7.1 Update collateral storage structure
    - Add NFT collateral variant
    - Add semi-fungible collateral variant
    - Update enum definitions
    - _Requirements: 1.2, 2.2, 8.1, 8.2, 8.3, 8.4_
  - [ ] 7.2 Update release/refund logic
    - Handle NFT release
    - Handle semi-fungible release
    - Handle fungible release (existing)
    - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5_
  - [ ] 7.3 Update escrow status queries
    - Include collateral type in responses
    - Include NFT metadata when available
    - Include semi-fungible quantity
    - _Requirements: 9.3, 10.3_
  - [ ]\* 7.4 Write integration tests for escrow contract updates
    - Test NFT collateral escrow flow
    - Test semi-fungible collateral escrow flow
    - Test refund flows
    - _Requirements: 8.1-8.5_

- [ ] 8. Implement gas optimization
  - [ ] 8.1 Implement metadata caching
    - Cache NFT metadata during escrow
    - Reuse cached metadata
    - Invalidate on escrow completion
    - _Requirements: 10.2, 10.3_
  - [ ] 8.2 Implement batch operations
    - Batch NFT lock operations
    - Batch NFT unlock operations
    - Minimize transaction count
    - _Requirements: 10.1, 10.3, 10.5_
  - [ ] 8.3 Implement validation caching
    - Cache validation results
    - Skip redundant checks
    - Include timestamp for freshness
    - _Requirements: 10.3, 10.4_
  - [ ]\* 8.4 Write property test for gas efficiency
    - **Property 10: Gas Efficiency Through Batching**
    - **Validates: Requirements 10.1, 10.2, 10.3**

- [ ] 9. Integration tests
  - [ ] 9.1 Write end-to-end NFT collateral flow test
    - Create NFT collateral
    - Validate ownership
    - Lock NFT
    - Create escrow
    - Release escrow
    - Unlock NFT
    - Verify NFT returned
    - _Requirements: 1.1-1.5, 8.1, 8.3, 9.1-9.5_
  - [ ] 9.2 Write end-to-end semi-fungible collateral flow test
    - Create semi-fungible collateral
    - Validate quantity
    - Lock semi-fungible
    - Create escrow
    - Release escrow
    - Unlock semi-fungible
    - Verify units returned
    - _Requirements: 2.1-2.5, 8.2, 8.4_
  - [ ] 9.3 Write multi-token escrow test
    - Create escrow with multiple NFTs
    - Verify all NFTs locked
    - Release escrow
    - Verify all NFTs returned
    - _Requirements: 10.1_
  - [ ] 9.4 Write error handling integration test
    - Test invalid NFT
    - Test non-owned collateral
    - Test zero quantity
    - Test unsupported token
    - _Requirements: 7.1-7.5_

- [ ] 10. Checkpoint - Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [ ] 11. Final checkpoint - Verify requirements coverage
  - Verify all 10 requirements are covered by implementation
  - Verify all acceptance criteria are tested
  - Verify error messages are descriptive and actionable
  - Ensure gas efficiency optimizations are working
  - Ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties
- Unit tests validate specific examples and edge cases
- Integration tests validate end-to-end flows
- TypeScript is used as the implementation language based on the design document
