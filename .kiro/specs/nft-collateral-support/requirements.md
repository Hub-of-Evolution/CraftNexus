# Requirements Document

## Introduction

This feature adds support for NFT (Non-Fungible Tokens) and semi-fungible tokens as collateral in the escrow system. Currently, the contract only supports standard fungible tokens (like USDC) for staking and escrow operations. Artisans may want to stake NFTs, specialized receipt tokens, or wrapped assets as collateral, requiring the system to handle multiple token standards.

## Glossary

- **System**: The Craft Nexus escrow platform that manages collateral and staking operations
- **NFT**: Non-Fungible Token - a unique digital asset that cannot be interchanged
- **Semi-Fungible Token**: A token that has both fungible and non-fungible aspects (e.g., multiple units of unique items)
- **Token Standard**: The technical specification that defines how a token behaves (e.g., Fungible Token, Non-Fungible Token)
- **Collateral**: Assets pledged by a party to secure a transaction or obligation
- **Staking**: The process of locking assets as collateral to participate in a system
- **Token Contract**: The smart contract that implements a specific token standard
- **Token ID**: A unique identifier for a specific NFT or semi-fungible token unit
- **Fungible Token**: A token where all units are identical and interchangeable (e.g., USDC)

## Requirements

### Requirement 1: Support NFT Collateral

**User Story:** As an artisan, I want to stake NFTs as collateral, so that I can use my unique digital assets to secure transactions.

#### Acceptance Criteria

1. WHEN an NFT is provided as collateral, THE System SHALL accept it as valid collateral
2. WHEN an NFT collateral is pledged, THE System SHALL store the Token Contract address and Token ID
3. WHILE an NFT is pledged as collateral, THE System SHALL prevent its transfer or sale
4. IF an NFT collateral is invalid or the token contract is not supported, THEN THE System SHALL return an error with a descriptive message
5. WHERE an NFT is used as collateral, THE System SHALL validate that the user owns the NFT before accepting it

### Requirement 2: Support Semi-Fungible Collateral

**User Story:** As an artisan, I want to stake semi-fungible tokens as collateral, so that I can use assets that have both unique and fungible aspects.

#### Acceptance Criteria

1. WHEN a semi-fungible token is provided as collateral, THE System SHALL accept it as valid collateral
2. WHEN semi-fungible collateral is pledged, THE System SHALL store the Token Contract address, Token ID, and quantity
3. WHILE semi-fungible collateral is pledged, THE System SHALL prevent transfer of the pledged units
4. IF semi-fungible collateral has an invalid quantity (zero or negative), THEN THE System SHALL return an error
5. WHERE semi-fungible tokens are used as collateral, THE System SHALL validate ownership of the specific token units

### Requirement 3: Token Standard Detection

**User Story:** As the system, I want to detect the token standard of a given contract, so that I can apply the appropriate validation and handling logic.

#### Acceptance Criteria

1. WHEN a token contract address is provided, THE System SHALL query the contract to determine its token standard
2. WHEN querying a token contract, THE System SHALL check for NFT metadata interfaces (e.g., `nft_metadata`, `nft_core`)
3. WHEN querying a token contract, THE System SHALL check for semi-fungible interfaces (e.g., `sfungible_token`)
4. IF the token standard cannot be determined, THEN THE System SHALL return an error indicating the token standard is unknown
5. WHERE a token contract supports multiple standards, THE System SHALL use the most specific standard available

### Requirement 4: Collateral Validation

**User Story:** As the system, I want to validate collateral before accepting it, so that only valid assets are used in escrow operations.

#### Acceptance Criteria

1. WHEN collateral is submitted, THE System SHALL validate that the user owns the collateral asset
2. WHEN NFT collateral is submitted, THE System SHALL verify the Token ID exists in the contract
3. WHEN semi-fungible collateral is submitted, THE System SHALL verify the requested quantity is available
4. IF collateral validation fails, THEN THE System SHALL return a descriptive error message
5. WHERE collateral validation is performed, THE System SHALL cache the validation result for the duration of the escrow creation

### Requirement 5: Backward Compatibility with Fungible Tokens

**User Story:** As an existing user, I want the system to continue supporting standard fungible tokens, so that my current workflows are not disrupted.

#### Acceptance Criteria

1. WHEN a fungible token (e.g., USDC) is used as collateral, THE System SHALL process it using the existing logic
2. WHEN a fungible token is used as collateral, THE System SHALL NOT apply NFT-specific validation
3. WHERE a token contract is a standard fungible token, THE System SHALL treat it as fungible regardless of other interfaces
4. IF a token contract supports both fungible and NFT interfaces, THEN THE System SHALL default to fungible handling unless explicitly specified otherwise
5. WHERE backward compatibility is required, THE System SHALL maintain the same error messages and behavior as before

### Requirement 6: Token Contract Interface Support

**User Story:** As the system, I want to support common token contract interfaces, so that I can work with a wide range of token standards.

#### Acceptance Criteria

1. WHEN a token contract implements the Fungible Token interface, THE System SHALL support it as a fungible token
2. WHEN a token contract implements the Non-Fungible Token interface, THE System SHALL support it as an NFT
3. WHEN a token contract implements the Semi-Fungible Token interface, THE System SHALL support it as a semi-fungible token
4. WHERE a token contract implements multiple interfaces, THE System SHALL prioritize based on specificity (NFT > semi-fungible > fungible)
5. IF a token contract implements an unsupported interface, THEN THE System SHALL return an error indicating the interface is not supported

### Requirement 7: Error Handling for Unsupported Tokens

**User Story:** As an artisan, I want clear error messages when using unsupported tokens, so that I can understand why my collateral was rejected.

#### Acceptance Criteria

1. IF an unsupported token standard is used as collateral, THEN THE System SHALL return an error with a descriptive message
2. IF a token contract is not deployed or does not exist, THEN THE System SHALL return an error indicating the contract is invalid
3. IF a user attempts to use a token they do not own, THEN THE System SHALL return an error indicating ownership verification failed
4. WHERE an error occurs during collateral validation, THE System SHALL log the error for debugging purposes
5. WHEN an error occurs, THE System SHALL provide actionable guidance for the user to resolve the issue

### Requirement 8: Collateral Release and Refund

**User Story:** As an artisan, I want my NFT or semi-fungible collateral to be released when the escrow is completed, so that I retain ownership of my assets.

#### Acceptance Criteria

1. WHEN an escrow with NFT collateral is released, THE System SHALL return the NFT to the original owner
2. WHEN an escrow with semi-fungible collateral is released, THE System SHALL return the pledged units to the original owner
3. WHEN an escrow with NFT collateral is refunded, THE System SHALL return the NFT to the pledgor
4. WHEN an escrow with semi-fungible collateral is refunded, THE System SHALL return the pledged units to the pledgor
5. IF collateral release fails, THEN THE System SHALL log the error and attempt recovery

### Requirement 9: Metadata Handling for NFTs

**User Story:** As the system, I want to store NFT metadata for reference, so that I can provide additional context about the collateral.

#### Acceptance Criteria

1. WHEN an NFT is used as collateral, THE System SHALL retrieve and store the NFT metadata URI
2. WHEN NFT metadata is retrieved, THE System SHALL cache it for the duration of the escrow
3. WHERE NFT metadata is available, THE System SHALL include it in escrow status queries
4. IF NFT metadata cannot be retrieved, THE System SHALL continue with the escrow using available information
5. WHEN NFT metadata is requested, THE System SHALL provide it in a standardized format

### Requirement 10: Gas and Transaction Efficiency

**User Story:** As the system, I want to minimize transaction costs for NFT collateral operations, so that users are not burdened with excessive fees.

#### Acceptance Criteria

1. WHEN multiple NFTs are used as collateral in a single escrow, THE System SHALL batch operations where possible
2. WHEN NFT metadata is cached, THE System SHALL reuse the cached data instead of re-fetching
3. WHERE a token standard has been validated, THE System SHALL skip redundant validation checks
4. IF a transaction fails due to insufficient gas, THEN THE System SHALL provide a clear error message with gas requirements
5. WHEN estimating transaction costs, THE System SHALL account for the additional complexity of NFT operations
