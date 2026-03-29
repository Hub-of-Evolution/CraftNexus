/**
 * Enumeration of supported blockchain networks mapped
 * via generic or standard Chain IDs.
 */
export enum SupportedChain {
    Ethereum = 1,
    Polygon = 137,
    Stellar = 0, // Not EVM, ID abstracted
}

/**
 * Receipt structure defining a cross-chain message envelope.
 * Used for both outbound (emitted) and inbound (executed) bridging.
 */
export interface BridgeReceipt {
    nonce: number;
    sourceChain: SupportedChain;
    destChain: SupportedChain;
    
    // Core addresses mapped into normalized formats (e.g. hexstrings)
    sender: string;
    destinationAddress: string;
    
    tokenContractAddress: string;
    
    // Abstract big integers mapped to string representation for large precisions
    amount: string; 
    
    /** Timestamp the outbound wrap occurred on source chain */
    timestamp: number;
}

/**
 * Internal accounting state for a single token's liquidity on a specific chain.
 */
export interface LiquidityPool {
    tokenAddress: string;
    totalDeposits: bigint;
    
    /** Amount locked protecting transit assets currently active */
    lockedBalance: bigint;

    /** Available reserve for inbound unlocking */
    availableBalance: bigint;

    /** Fee parameter base metric in basis points (configurable) */
    baseFeeBps: number;
}

/**
 * Bridged Event standard layout emitted for external indexing/relayers.
 */
export interface BridgeEvent {
    type: 'LOCKED' | 'MINTED' | 'BURNED' | 'RELEASED';
    receipt: BridgeReceipt;
    transactionId: string; // The tx hash where this originally occurred
}
