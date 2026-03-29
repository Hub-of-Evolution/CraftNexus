import { BridgeReceipt, SupportedChain } from "../structures/BridgeStructure";

/**
 * Interface for the CrossChainBridge
 * Abstracting bridging operations to allow implementations to span
 * EVM chains (Ethereum, Polygon) and Stellar networks.
 */
export interface ICrossChainBridge {
    /**
     * Wrap an energy asset on the source chain to be minted on the destination chain.
     * @param tokenAddress Original token contract address
     * @param amount Amount to bridge
     * @param destinationChain Target blockchain network
     * @param destinationAddress Address receiving the minted asset on target chain
     * @param minOutput Slippage protection - the minimum amount of wrapped asset acceptable after fees
     * @returns Emitted BridgeReceipt indicating sequence processing
     */
    wrapAsset(
        tokenAddress: string,
        amount: string,
        destinationChain: SupportedChain,
        destinationAddress: string,
        minOutput: string
    ): Promise<BridgeReceipt>;

    /**
     * Unwrap a wrapped asset, burning it on the current chain and unlocking natively on its home chain.
     * @param wrappedTokenAddress The localized wrapped token address
     * @param amount Amount to burn
     * @param destinationChain Target blockchain network (usually the home chain of the asset)
     * @param destinationAddress Address to receive the native unlocked asset
     * @param minOutput Slippage protection - minimum unlocked amount acceptable
     * @returns Emitted BridgeReceipt
     */
    unwrapAsset(
        wrappedTokenAddress: string,
        amount: string,
        destinationChain: SupportedChain,
        destinationAddress: string,
        minOutput: string
    ): Promise<BridgeReceipt>;

    /**
     * Provide liquidity to the native bridging pools.
     * @param tokenAddress The contract address
     * @param amount Amount to deposit
     */
    depositLiquidity(tokenAddress: string, amount: string): Promise<void>;

    /**
     * Withdraw previously provided liquidity.
     * @param tokenAddress The contract address
     * @param amount Amount to withdraw
     */
    withdrawLiquidity(tokenAddress: string, amount: string): Promise<void>;

    /**
     * Inbound relayer function. Executes an approved cross-chain receipt payload
     * after the MultiChainValidator successfully verifies signatures.
     * @param receipt The validated BridgeReceipt
     * @param signatures The gathered 51%+ validator signatures
     */
    executeInbound(receipt: BridgeReceipt, signatures: string[]): Promise<boolean>;

    /**
     * Admin function: Pause operations across the bridge (Emergency handle).
     */
    pauseBridge(): Promise<void>;

    /**
     * Admin function: Safely resume ops.
     */
    resumeBridge(): Promise<void>;
}
