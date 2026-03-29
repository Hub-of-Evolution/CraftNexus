import { LiquidityPool } from "../structures/BridgeStructure";

export class BridgeLib {
    /** Sliding scale fee constant (approx max 8%) */
    public static readonly MAX_FEE_BPS: number = 800;
    
    /** Utilization limit before locking bridging ops (99.5%) */
    public static readonly UTILIZATION_REVERT_LIMIT: number = 9950;
    
    public static readonly BPS_DENOMINATOR: number = 10000;

    /**
     * Extracts dynamic fees from a requested transfer based on current liquidity reserves.
     * Prevents complete exhaustion by spiking fees exponentially as the pool approaches 0 available balance.
     * 
     * @param amount Requested transfer amount
     * @param pool The targeted LiquidityPool structure
     * @returns A tuple of [deductedFee, netAmountToExecute]
     */
    public static calculateDynamicFee(amount: bigint, pool: LiquidityPool): [bigint, bigint] {
        const utilizationBasisPoints = 
            Number((pool.lockedBalance * BigInt(this.BPS_DENOMINATOR)) / (pool.totalDeposits || 1n));
        
        if (utilizationBasisPoints >= this.UTILIZATION_REVERT_LIMIT) {
            throw new Error(`[BridgeLib] Insufficient Liquidity: Pool exhaustion imminent at ${utilizationBasisPoints / 100}%`);
        }

        // Base fee applies under normal conditions (e.g., 20 bps)
        let appliedBps = pool.baseFeeBps;

        // Exponential slippage fee applied if pool is over 80% utilized
        if (utilizationBasisPoints > 8000) {
            // Ramps linearly from 80% up to MAX_FEE_BPS based on how close we are to 100%
            const surge = (utilizationBasisPoints - 8000) * (this.MAX_FEE_BPS / 2000);
            appliedBps += Math.floor(surge);
            
            if (appliedBps > this.MAX_FEE_BPS) {
                appliedBps = this.MAX_FEE_BPS;
            }
        }

        const feeAmount = (amount * BigInt(appliedBps)) / BigInt(this.BPS_DENOMINATOR);
        const netAmount = amount - feeAmount;

        return [feeAmount, netAmount];
    }

    /**
     * Checks if a supplied asset meets structural validation properties and is allowed on network.
     * Hard reverts if unsupported token metadata is flagged.
     * @param tokenAddress The contract
     * @param supportedRegistry A pre-fetched map of network-authorized core token lists
     */
    public static verifyAssetIntegrity(tokenAddress: string, supportedRegistry: Set<string>): void {
        const addressMatch = supportedRegistry.has(tokenAddress);
        if (!addressMatch) {
            throw new Error(`[BridgeLib] Invalid Asset: Token ${tokenAddress} is not verified/supported by this bridge.`);
        }
    }
    
    /**
     * Evaluates output against the user's defined minimum protection limit to prevent 
     * malicious MEV or large dynamic fee spikes from causing losses natively.
     * @param netOutput Actual amount calculated to distribute
     * @param minOutput Minimum limit configured by client
     */
    public static enforceSlippageProtection(netOutput: bigint, minOutput: bigint): void {
        if (netOutput < minOutput) {
            throw new Error(`[BridgeLib] Slippage Reverted: Expected min ${minOutput.toString()}, simulated output ${netOutput.toString()}`);
        }
    }
}
