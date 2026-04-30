# Design Document: NFT and Semi-Fungible Collateral Support

## Overview

This document describes the design for adding NFT (Non-Fungible Token) and semi-fungible token support to the Craft Nexus escrow system. Currently, the system only supports standard fungible tokens (like USDC) for staking and escrow operations. This feature extends the system to handle multiple token standards while maintaining full backward compatibility.

### Key Objectives

1. **Token Standard Detection**: Automatically detect whether a token contract implements Fungible Token, NFT, or Semi-Fungible Token interfaces
2. **Collateral Validation**: Validate ownership and availability of NFTs and semi-fungible tokens before accepting as collateral
3. **Collateral Management**: Properly lock/unlock NFTs and track semi-fungible token balances during escrow
4. **Backward Compatibility**: Maintain existing fungible token support without disruption
5. **Metadata Handling**: Retrieve and store NFT metadata for reference
6. **Gas Efficiency**: Minimize transaction costs through batching and caching

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Collateral Service                            │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐   │
│  │  Token Detector  │  │  NFT Manager     │  │  Semi-Fungible   │   │
│  │                  │  │                  │  │  Manager         │   │
│  │ - Interface      │  │ - Ownership      │  │ - Balance        │   │
│  │   Detection      │  │   Validation     │  │   Validation     │   │
│  │ - Standard       │  │ - Lock/Unlock    │  │ - Lock/Unlock    │   │
│  │   Identification │  │ - Metadata       │  │ - Balance Track  │   │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘   │
│                              │                                        │
│                              ▼                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    Collateral Validator                         │  │
│  │  - Validates ownership and availability                         │  │
│  │  - Caches validation results                                    │  │
│  │  - Returns descriptive errors                                   │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                        │
│                              ▼                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    Escrow Integration                           │  │
│  │  - Unified collateral interface                                 │  │
│  │  - Backward compatible with fungible tokens                     │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

## Architecture

### Component Structure

The collateral support feature is organized into the following components:

```
collateral/
├── types.ts              # Shared types and interfaces
├── detector.ts           # Token standard detection logic
├── validator.ts          # Collateral validation
├── nft/                  # NFT-specific logic
│   ├── manager.ts        # NFT lock/unlock operations
│   └── metadata.ts       # NFT metadata handling
├── semi-fungible/        # Semi-fungible token logic
│   └── manager.ts        # Semi-fungible balance management
└── service.ts            # Unified collateral service
```

### Token Standard Detection Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Token Standard Detection                          │
├─────────────────────────────────────────────────────────────────────┤
│ 1. Input: Token Contract Address                                     │
│ 2. Query Contract for Available Interfaces                           │
│ 3. Check for NFT Interfaces (highest priority)                       │
│ 4. Check for Semi-Fungible Interfaces (medium priority)              │
│ 5. Check for Fungible Token Interface (lowest priority)              │
│ 6. Return TokenStandard with detected standard                       │
└─────────────────────────────────────────────────────────────────────┘
```

### Data Flow

```
┌──────────────┐     ┌──────────────────┐     ┌──────────────────┐
│   User       │────>│  Collateral      │────>│  Escrow          │
│  Provides    │     │  Service         │     │  Service         │
│  Collateral  │     │                  │     │                  │
└──────────────┘     └──────────────────┘     └──────────────────┘
                          │                            │
                          ▼                            ▼
                 ┌──────────────────┐          ┌──────────────────┐
                 │  Token Detector  │          │  Collateral      │
                 │                  │          │  Validator       │
                 └──────────────────┘          └──────────────────┘
                          │                            │
                          ▼                            ▼
                 ┌──────────────────┐          ┌──────────────────┐
                 │  Token Standard  │          │  Validation      │
                 │  Identified      │          │  Results         │
                 └──────────────────┘          └───────��──────────┘
```

## Components and Interfaces

### Core Types

```typescript
// Token standard enumeration
export enum TokenStandard {
  Fungible = "fungible_token",
  NonFungible = "non_fungible_token",
  SemiFungible = "semi_fungible_token",
  Unknown = "unknown",
}

// Token contract interface identifiers
export const TOKEN_INTERFACES = {
  Fungible: ["fungible_token", "fungible_token_metadata"],
  NonFungible: ["nft_metadata", "nft_core", "non_fungible_token"],
  SemiFungible: ["sfungible_token", "semi_fungible_token"],
};

// Collateral type enumeration
export enum CollateralType {
  Fungible = "fungible",
  NFT = "nft",
  SemiFungible = "semi-fungible",
}

// Base collateral interface
export interface Collateral {
  type: CollateralType;
  contractAddress: string;
  owner: string;
  validated: boolean;
  validationTimestamp: number;
}

// NFT collateral
export interface NFTCollateral extends Collateral {
  type: CollateralType.NFT;
  tokenId: string;
  metadataUri?: string;
  metadata?: NFTMetadata;
}

// Semi-fungible collateral
export interface SemiFungibleCollateral extends Collateral {
  type: CollateralType.SemiFungible;
  tokenId: string;
  quantity: bigint;
}

// Fungible collateral (existing)
export interface FungibleCollateral extends Collateral {
  type: CollateralType.Fungible;
  amount: bigint;
}

// Unified collateral union type
export type AnyCollateral =
  | FungibleCollateral
  | NFTCollateral
  | SemiFungibleCollateral;
```

### Token Detector Interface

```typescript
export interface TokenDetector {
  /**
   * Detect the token standard for a given contract address
   * @param contractAddress - The token contract address
   * @returns TokenStandard with detected standard and interface info
   */
  detectStandard(contractAddress: string): Promise<TokenStandardInfo>;

  /**
   * Check if a contract implements a specific interface
   * @param contractAddress - The token contract address
   * @param interfaceName - The interface to check
   * @returns True if the interface is implemented
   */
  hasInterface(
    contractAddress: string,
    interfaceName: string,
  ): Promise<boolean>;

  /**
   * Get all available interfaces for a contract
   * @param contractAddress - The token contract address
   * @returns Array of interface names implemented by the contract
   */
  getAvailableInterfaces(contractAddress: string): Promise<string[]>;
}

export interface TokenStandardInfo {
  standard: TokenStandard;
  interfaces: string[];
  priority: number; // Higher = more specific
}
```

### Collateral Manager Interface

```typescript
export interface CollateralManager<T extends AnyCollateral> {
  /**
   * Validate that the user owns the collateral
   * @param collateral - The collateral to validate
   * @returns True if ownership is verified
   */
  validateOwnership(collateral: T): Promise<boolean>;

  /**
   * Lock the collateral for escrow
   * @param collateral - The collateral to lock
   * @param escrowId - The escrow ID
   * @returns Transaction hash or result
   */
  lock(collateral: T, escrowId: number): Promise<TransactionResult>;

  /**
   * Unlock/release the collateral after escrow
   * @param collateral - The collateral to unlock
   * @param escrowId - The escrow ID
   * @returns Transaction hash or result
   */
  unlock(collateral: T, escrowId: number): Promise<TransactionResult>;

  /**
   * Get the current balance/ownership status
   * @param collateral - The collateral to check
   * @returns Current balance or ownership status
   */
  getStatus(collateral: T): Promise<CollateralStatus>;
}

export interface TransactionResult {
  success: boolean;
  transactionHash?: string;
  error?: string;
}

export interface CollateralStatus {
  owner: string;
  locked: boolean;
  escrowId?: number;
  timestamp: number;
}
```

## Data Models

### Collateral Storage Structure

The escrow contract's collateral storage needs to be extended to support multiple token standards:

```rust
// Updated Collateral struct for the escrow contract
#[derive(Clone, Debug, Encodable, Decodable, HasSpec, Serialize, Deserialize)]
#[spec(type = "Collateral")]
pub enum Collateral {
    Fungible(FungibleCollateral),
    NonFungible(NonFungibleCollateral),
    SemiFungible(SemiFungibleCollateral),
}

// Fungible collateral (existing)
#[derive(Clone, Debug, Encodable, Decodable, HasSpec, Serialize, Deserialize)]
#[spec(type = "FungibleCollateral")]
pub struct FungibleCollateral {
    pub token_contract: Address,
    pub amount: i128, // Stroops for Stellar tokens
}

// NFT collateral (new)
#[derive(Clone, Debug, Encodable, Decodable, HasSpec, Serialize, Deserialize)]
#[spec(type = "NonFungibleCollateral")]
pub struct NonFungibleCollateral {
    pub token_contract: Address,
    pub token_id: BytesN<32>, // Token ID hash
    pub metadata_uri: Option<String>,
    pub metadata: Option<BytesN<32>>, // Hash of metadata for verification
}

// Semi-fungible collateral (new)
#[derive(Clone, Debug, Encodable, Decodable, HasSpec, Serialize, Deserialize)]
#[spec(type = "SemiFungibleCollateral")]
pub struct SemiFungibleCollateral {
    pub token_contract: Address,
    pub token_id: BytesN<32>, // Token ID
    pub quantity: i128,
}
```

### Escrow Storage Extension

```rust
// Updated Escrow struct
#[derive(Clone, Debug, Encodable, Decodable, HasSpec, Serialize, Deserialize)]
#[spec(type = "Escrow")]
pub struct Escrow {
    pub buyer: Address,
    pub seller: Address,
    pub collateral: Collateral,
    pub status: EscrowStatus,
    pub created_at: i64,
    pub release_window: u64,
    pub order_id: u32,
}

// Escrow status (existing)
#[derive(Clone, Debug, Encodable, Decodable, HasSpec, Serialize, Deserialize)]
#[spec(type = "EscrowStatus")]
pub enum EscrowStatus {
    Pending,
    Released,
    Refunded,
    Disputed,
}
```

### Token Metadata Structure

```typescript
// NFT metadata structure (following ERC-721/ERC-1155 conventions)
export interface NFTMetadata {
  name: string;
  description: string;
  image?: string;
  animation_url?: string;
  attributes?: Attribute[];
  external_url?: string;
  [key: string]: unknown;
}

export interface Attribute {
  trait_type: string;
  value: string | number;
  display_type?: string;
}

// Token standard interface definitions
export interface TokenStandardInterfaces {
  // Fungible Token (SIP-010)
  fungible_token?: {
    name: string;
    symbol: string;
    decimals: number;
    total_supply: bigint;
  };

  // Non-Fungible Token (SIP-012)
  nft_metadata?: {
    name: string;
    symbol: string;
    token_id_required?: boolean;
    metadata_required?: boolean;
  };

  // Semi-Fungible Token
  semi_fungible_token?: {
    name: string;
    symbol: string;
    total_supply: bigint;
  };
}
```

## Correctness Properties

_A property is a characteristic or behavior that should hold true across all valid executions of a system-essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees._

### Property 1: Token Standard Detection Accuracy

_For any_ valid token contract address, the token detector SHALL correctly identify the token standard based on implemented interfaces, with NFT interfaces taking priority over semi-fungible, and semi-fungible taking priority over fungible.

**Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.5, 6.4**

### Property 2: NFT Ownership Validation

_For any_ NFT collateral, the system SHALL only accept it as valid collateral if the user owns the specific NFT identified by the token contract address and token ID.

**Validates: Requirements 1.1, 1.5**

### Property 3: NFT Collateral Storage Integrity

_For any_ NFT collateral that is pledged, the system SHALL correctly store the token contract address and token ID, and these values SHALL be recoverable when querying the escrow.

**Validates: Requirements 1.2**

### Property 4: NFT Transfer Prevention During Escrow

_For any_ NFT pledged as collateral, the system SHALL prevent its transfer or sale while the escrow is in pending status, and the NFT SHALL be returned to the original owner upon escrow release or refund.

**Validates: Requirements 1.3, 8.1, 8.3**

### Property 5: Semi-Fungible Quantity Validation

_For any_ semi-fungible collateral, the system SHALL validate that the requested quantity is available and non-zero before accepting it as collateral.

**Validates: Requirements 2.4**

### Property 6: Semi-Fungible Balance Tracking

_For any_ semi-fungible collateral, the system SHALL correctly track the pledged quantity and prevent transfer of those specific units while the escrow is active.

**Validates: Requirements 2.2, 2.3**

### Property 7: Backward Compatibility with Fungible Tokens

_For any_ fungible token contract, the system SHALL process it using the existing fungible token logic without applying NFT-specific validation, and error messages SHALL match the original behavior.

**Validates: Requirements 5.1, 5.2, 5.5**

### Property 8: Interface Priority Logic

_For any_ token contract that implements multiple token standards, the system SHALL prioritize based on specificity (NFT > semi-fungible > fungible) unless explicitly specified otherwise.

**Validates: Requirements 3.5, 5.4, 6.4**

### Property 9: Metadata Retrieval and Caching

_For any_ NFT used as collateral, if metadata is available, the system SHALL retrieve and cache the metadata URI for the duration of the escrow, and the cached metadata SHALL be included in escrow status queries.

**Validates: Requirements 9.1, 9.2, 9.3**

### Property 10: Gas Efficiency Through Batching

_For any_ escrow involving multiple NFTs, the system SHALL batch operations where possible to minimize transaction costs, and cached data SHALL be reused instead of re-fetching.

**Validates: Requirements 10.1, 10.2, 10.3**

## Error Handling

### Error Categories

```typescript
// Token detection errors
export enum TokenDetectionError {
  ContractNotFound = "CONTRACT_NOT_FOUND",
  InterfaceNotFound = "INTERFACE_NOT_FOUND",
  MultipleStandards = "MULTIPLE_STANDARDS",
  UnknownStandard = "UNKNOWN_STANDARD",
}

// Collateral validation errors
export enum CollateralValidationError {
  OwnershipFailed = "OWNERSHIP_FAILED",
  TokenIdNotFound = "TOKEN_ID_NOT_FOUND",
  InsufficientBalance = "INSUFFICIENT_BALANCE",
  InvalidQuantity = "INVALID_QUANTITY",
}

// Escrow operation errors
export enum EscrowOperationError {
  InvalidCollateral = "INVALID_COLLATERAL",
  AlreadyLocked = "ALREADY_LOCKED",
  NotOwner = "NOT_OWNER",
  ReleaseWindowNotPassed = "RELEASE_WINDOW_NOT_PASSED",
}

// Unified error type
export interface CollateralError {
  code: TokenDetectionError | CollateralValidationError | EscrowOperationError;
  message: string;
  details?: Record<string, unknown>;
  actionable?: boolean; // Whether the user can take action to resolve
}
```

### Error Handling Strategy

1. **Token Detection Errors**:
   - Contract not found: Return descriptive error with contract address
   - Interface not found: List expected interfaces
   - Unknown standard: Provide guidance on supported standards

2. **Validation Errors**:
   - Ownership failed: Explain how to verify ownership
   - Token ID not found: Provide valid token ID format
   - Insufficient balance: Show available balance vs required

3. **Operation Errors**:
   - Invalid collateral: Specify what makes it invalid
   - Already locked: Provide escrow ID and status
   - Not owner: Show current owner address

### Error Message Guidelines

All error messages MUST:

- Be descriptive and actionable
- Include the specific field that failed validation
- Provide guidance on how to resolve the issue
- Log the error for debugging with sensitive data redacted

## Testing Strategy

### Unit Tests

#### Token Standard Detection Tests

```typescript
describe("TokenDetector", () => {
  describe("detectStandard", () => {
    it("should detect NFT standard for NFT contract", async () => {
      // Test with NFT contract
    });

    it("should detect semi-fungible standard for semi-fungible contract", async () => {
      // Test with semi-fungible contract
    });

    it("should detect fungible standard for fungible contract", async () => {
      // Test with fungible contract
    });

    it("should return unknown for unsupported contract", async () => {
      // Test with unsupported contract
    });

    it("should prioritize NFT over other standards", async () => {
      // Test with contract implementing multiple standards
    });
  });

  describe("hasInterface", () => {
    it("should return true for implemented interface", async () => {
      // Test interface detection
    });

    it("should return false for non-implemented interface", async () => {
      // Test non-existent interface
    });
  });
});
```

#### Collateral Validation Tests

```typescript
describe("CollateralValidator", () => {
  describe("validateOwnership", () => {
    it("should validate NFT ownership correctly", async () => {
      // Test NFT ownership validation
    });

    it("should validate semi-fungible ownership correctly", async () => {
      // Test semi-fungible ownership validation
    });

    it("should reject non-owned collateral", async () => {
      // Test with non-owned collateral
    });
  });

  describe("validateQuantity", () => {
    it("should accept valid quantity", async () => {
      // Test with valid quantity
    });

    it("should reject zero quantity", async () => {
      // Test with zero quantity
    });

    it("should reject negative quantity", async () => {
      // Test with negative quantity
    });
  });
});
```

#### NFT Manager Tests

```typescript
describe("NFTManager", () => {
  describe("lock", () => {
    it("should lock NFT for escrow", async () => {
      // Test NFT locking
    });

    it("should return error for already locked NFT", async () => {
      // Test with already locked NFT
    });
  });

  describe("unlock", () => {
    it("should unlock NFT after escrow", async () => {
      // Test NFT unlocking
    });

    it("should return NFT to original owner", async () => {
      // Test NFT return
    });
  });
});
```

### Integration Tests

#### End-to-End NFT Collateral Flow

```typescript
describe("NFT Collateral Integration", () => {
  it("should complete full NFT collateral escrow flow", async () => {
    // 1. Create NFT collateral
    // 2. Validate ownership
    // 3. Lock NFT
    // 4. Create escrow
    // 5. Release escrow
    // 6. Unlock NFT
    // 7. Verify NFT returned to owner
  });

  it("should handle NFT collateral refund", async () => {
    // 1. Create NFT collateral escrow
    // 2. Refund escrow
    // 3. Verify NFT returned to owner
  });

  it("should handle multiple NFTs in single escrow", async () => {
    // 1. Create escrow with multiple NFTs
    // 2. Verify all NFTs locked
    // 3. Release escrow
    // 4. Verify all NFTs returned
  });
});
```

#### Semi-Fungible Collateral Flow

```typescript
describe("Semi-Fungible Collateral Integration", () => {
  it("should complete full semi-fungible collateral escrow flow", async () => {
    // 1. Create semi-fungible collateral
    // 2. Validate quantity
    // 3. Lock semi-fungible
    // 4. Create escrow
    // 5. Release escrow
    // 6. Verify units returned
  });

  it("should handle partial quantity locking", async () => {
    // 1. Create escrow with partial quantity
    // 2. Verify only specified units locked
    // 3. Release escrow
    // 4. Verify units returned
  });
});
```

### Property-Based Tests

#### Token Standard Detection Properties

```typescript
// Property 1: Token Standard Detection Accuracy
// Feature: nft-collateral-support, Property 1: Token Standard Detection Accuracy

property("Token standard detection is accurate", async () => {
  // Generate random token contract addresses
  // For each contract, verify correct standard detection
  // Verify priority logic for multiple standards
});
```

#### NFT Collateral Properties

```typescript
// Property 2: NFT Ownership Validation
// Feature: nft-collateral-support, Property 2: NFT Ownership Validation

property("NFT ownership validation is accurate", async () => {
  // Generate random NFTs
  // For owned NFTs, validation should pass
  // For non-owned NFTs, validation should fail
});

// Property 3: NFT Collateral Storage Integrity
// Feature: nft-collateral-support, Property 3: NFT Collateral Storage Integrity

property("NFT collateral storage is consistent", async () => {
  // Generate random NFT collateral
  // Pledge and verify stored values match
  // Query escrow and verify retrieval
});

// Property 4: NFT Transfer Prevention During Escrow
// Feature: nft-collateral-support, Property 4: NFT Transfer Prevention During Escrow

property("NFT transfer is prevented during escrow", async () => {
  // Generate random NFTs
  // Pledge and verify transfer is blocked
  // Release and verify transfer is allowed
});
```

#### Semi-Fungible Properties

```typescript
// Property 5: Semi-Fungible Quantity Validation
// Feature: nft-collateral-support, Property 5: Semi-Fungible Quantity Validation

property("Semi-fungible quantity validation is accurate", async () => {
  // Generate random quantities
  // Valid quantities should pass
  // Zero/negative quantities should fail
});

// Property 6: Semi-Fungible Balance Tracking
// Feature: nft-collateral-support, Property 6: Semi-Fungible Balance Tracking

property("Semi-fungible balance tracking is accurate", async () => {
  // Generate random semi-fungible tokens
  // Pledge and verify balance is locked
  // Release and verify balance is restored
});
```

#### Backward Compatibility Properties

```typescript
// Property 7: Backward Compatibility with Fungible Tokens
// Feature: nft-collateral-support, Property 7: Backward Compatibility with Fungible Tokens

property("Fungible tokens use existing logic", async () => {
  // Generate random fungible tokens
  // Verify fungible token logic is used
  // Verify NFT validation is not applied
});
```

#### Metadata Properties

```typescript
// Property 9: Metadata Retrieval and Caching
// Feature: nft-collateral-support, Property 9: Metadata Retrieval and Caching

property("NFT metadata is retrieved and cached", async () => {
  // Generate random NFTs with metadata
  // Verify metadata is retrieved
  // Verify metadata is cached
  // Verify cached metadata is used
});
```

### Test Configuration

```typescript
// Property-based test configuration
const PBT_CONFIG = {
  numRuns: 100, // Minimum 100 iterations per property
  timeout: 30000, // 30 seconds per test
  verbose: false,
};

// Tag format for property tests
// Feature: nft-collateral-support, Property {number}: {property_text}
```

### Test Coverage Matrix

| Requirement                     | Unit Test | Integration Test | Property Test |
| ------------------------------- | --------- | ---------------- | ------------- |
| 1.1 NFT acceptance              | ✓         | ✓                | ✓             |
| 1.2 NFT storage                 | ✓         | ✓                | ✓             |
| 1.3 Transfer prevention         | ✓         | ✓                | ✓             |
| 1.4 Error handling              | ✓         | ✓                | ✓             |
| 1.5 Ownership validation        | ✓         | ✓                | ✓             |
| 2.1 Semi-fungible acceptance    | ✓         | ✓                | ✓             |
| 2.2 Semi-fungible storage       | ✓         | ✓                | ✓             |
| 2.3 Transfer prevention         | ✓         | ✓                | ✓             |
| 2.4 Quantity validation         | ✓         | ✓                | ✓             |
| 2.5 Ownership validation        | ✓         | ✓                | ✓             |
| 3.1 Standard detection          | ✓         | ✓                | ✓             |
| 3.2 NFT interface check         | ✓         | ✓                | ✓             |
| 3.3 Semi-fungible check         | ✓         | ✓                | ✓             |
| 3.4 Unknown standard error      | ✓         | ✓                | ✓             |
| 3.5 Multiple standards priority | ✓         | ✓                | ✓             |
| 4.1-4.5 Validation              | ✓         | ✓                | ✓             |
| 5.1-5.5 Backward compatibility  | ✓         | ✓                | ✓             |
| 6.1-6.5 Interface support       | ✓         | ✓                | ✓             |
| 7.1-7.5 Error handling          | ✓         | ✓                | ✓             |
| 8.1-8.5 Release/refund          | ✓         | ✓                | ✓             |
| 9.1-9.5 Metadata handling       | ✓         | ✓                | ✓             |
| 10.1-10.5 Gas efficiency        | ✓         | ✓                | ✓             |

## Implementation Notes

### Key Design Decisions

1. **Token Standard Priority**: NFT > Semi-Fungible > Fungible
   - Rationale: More specific standards should take precedence
   - Implementation: Priority scores in detection logic

2. **Validation Caching**: Cache validation results for escrow duration
   - Rationale: Avoid redundant contract calls
   - Implementation: In-memory cache with timestamp

3. **Metadata Caching**: Cache NFT metadata during escrow
   - Rationale: Reduce external API calls
   - Implementation: Cache with escrow ID as key

4. **Batch Operations**: Batch NFT operations when possible
   - Rationale: Reduce transaction costs
   - Implementation: Group operations by escrow

5. **Backward Compatibility**: Fungible tokens use existing logic
   - Rationale: No disruption to existing users
   - Implementation: Separate code paths with unified interface

### Performance Considerations

1. **Contract Calls**: Minimize RPC calls through caching
2. **Batching**: Group operations to reduce transaction count
3. **Parallel Processing**: Validate multiple NFTs in parallel
4. **Lazy Loading**: Load metadata only when requested

### Security Considerations

1. **Ownership Verification**: Always verify ownership before accepting collateral
2. **Input Validation**: Validate all user inputs
3. **Error Handling**: Never leak sensitive information in errors
4. **Access Control**: Ensure only authorized parties can release collateral

## Next Steps

1. Implement token standard detection logic
2. Implement NFT manager with lock/unlock operations
3. Implement semi-fungible manager with balance tracking
4. Implement unified collateral validator
5. Update escrow contract to support new collateral types
6. Write unit tests for each component
7. Write integration tests for end-to-end flows
8. Write property-based tests for correctness properties
9. Update documentation and user guides
