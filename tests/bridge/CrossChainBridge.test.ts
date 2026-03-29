import { CrossChainBridge } from "../../contracts/bridge/CrossChainBridge";
import { SupportedChain, BridgeReceipt } from "../../contracts/bridge/structures/BridgeStructure";
import { BridgeLib } from "../../contracts/bridge/libraries/BridgeLib";
import { MultiChainValidator } from "../../contracts/bridge/validators/MultiChainValidator";
// Note: tests built for vitest/jest.

describe("CrossChainBridge System", () => {
    let bridge: CrossChainBridge;
    const admin = "adminAddress";
    const validators = ["val1", "val2", "val3", "val4"];
    const tokenContract = "usdc-stellar-address";
    
    // Config expected minimum slippage boundary 
    const minExpectedOutput = "990"; 

    beforeEach(() => {
        bridge = new CrossChainBridge(SupportedChain.Stellar, admin, validators);
        bridge.initializeAsset(tokenContract, 20); // 20 bps base fee (.2%)
        
        // Deposit 100,000 baseline liquidity for operations
        bridge.depositLiquidity(tokenContract, "100000");
    });

    describe("Functional Interactions", () => {
        it("should successfully wrap an authorized asset", async () => {
             // 1000 units wrapped natively mapping source -> destination (Ethereum)
             const receipt = await bridge.wrapAsset(
                 tokenContract,
                 "1000",
                 SupportedChain.Ethereum,
                 "eth-dest-user",
                 minExpectedOutput
             );

             expect(receipt.sourceChain).toBe(SupportedChain.Stellar);
             expect(receipt.amount).toBe("998"); // 1000 - 0.2% fee = 998 net output mapped across
             expect(receipt.nonce).toBe(1);

             // Log validation
             expect(bridge.eventsLog[0].type).toBe("LOCKED");
             
             // Reserve internal mapping verification
             const pool = bridge.pools.get(tokenContract);
             expect(pool!.totalDeposits).toBe(100002n); // Liquidity + standard initial yield fees captured
        });

        it("should successfully execute inbound transfers upon authorized receipt + quorum signatures", async () => {
             // Mock inbound receipt crossing from Ethereum arriving -> Stellar
             const receipt: BridgeReceipt = {
                 nonce: 50,
                 sourceChain: SupportedChain.Ethereum,
                 destChain: SupportedChain.Stellar,
                 sender: "remote-eth-user",
                 destinationAddress: "local-userX",
                 tokenContractAddress: tokenContract,
                 amount: "5000",
                 timestamp: Date.now()
             };

             // Mocking signatures structure derived in validator
             // Format matches `val1-xxx`, `val2-xxx`
             const signatures = ["val1-signedHash", "val3-signedHash", "val4-signedHash"];
             
             const result = await bridge.executeInbound(receipt, signatures);
             expect(result).toBe(true);
             expect(bridge.eventsLog[0].type).toBe("MINTED");

             // Verify liquidity routing
             const pool = bridge.pools.get(tokenContract);
             expect(pool!.availableBalance).toBe(95000n); // 100,000 deposited natively, 5000 shifted 
        });

        it("should support liquidity provider withdrawal against free balances", async () => {
             await bridge.withdrawLiquidity(tokenContract, "50000");
             
             const pool = bridge.pools.get(tokenContract);
             expect(pool!.availableBalance).toBe(50000n);
        });
    });

    describe("Security and Edge Cases", () => {
        it("should revert if slippage falls below user configured targets", async () => {
            await expect(
                bridge.wrapAsset(
                    tokenContract,
                    "1000",
                    SupportedChain.Polygon,
                    "dest",
                    "999" // Impossibly high expectation, fee consumes ~2
                )
            ).rejects.toThrow("Slippage Reverted");
        });

        it("should prevent Double-Spending Replay Attacks enforcing Nonce Uniqueness", async () => {
             const receipt: BridgeReceipt = {
                 nonce: 42,
                 sourceChain: SupportedChain.Polygon,
                 destChain: SupportedChain.Stellar,
                 sender: "hacker",
                 destinationAddress: "dest",
                 tokenContractAddress: tokenContract,
                 amount: "100",
                 timestamp: Date.now()
             };
             
             const signatures = ["val1-sig", "val2-sig", "val3-sig"];
             await bridge.executeInbound(receipt, signatures);

             // Replaying identically payload identically populated
             await expect(
                 bridge.executeInbound(receipt, signatures)
             ).rejects.toThrow("Double-spend Replay Rejected");
        });

        it("should prevent inbound validation when quorum signatures are insufficient", async () => {
             const receipt: BridgeReceipt = {
                 nonce: 43,
                 sourceChain: SupportedChain.Polygon,
                 destChain: SupportedChain.Stellar,
                 sender: "user2",
                 destinationAddress: "dest2",
                 tokenContractAddress: tokenContract,
                 amount: "100",
                 timestamp: Date.now()
             };
             
             // Only 2 signatures (Requires 3 based on configured 66% threshold of 4 validators)
             const signatures = ["val1-sig", "val2-sig"]; 
             
             await expect(
                 bridge.executeInbound(receipt, signatures)
             ).rejects.toThrow("Quorum Failure");
        });

        it("should enforce the Emergency Pause circuit breaker overriding all functions natively", async () => {
            await bridge.pauseBridge();

            await expect(
                 bridge.wrapAsset(tokenContract, "10", SupportedChain.Ethereum, "dest", "0")
            ).rejects.toThrow("Circuit Breaker Engaged");

            const receipt: BridgeReceipt = {
                 nonce: 44, sourceChain: SupportedChain.Polygon, destChain: SupportedChain.Stellar,
                 sender: "user2", destinationAddress: "dest2", tokenContractAddress: tokenContract,
                 amount: "100", timestamp: Date.now()
             };
             
            await expect(
                 bridge.executeInbound(receipt, ["val1-sig", "val2-sig", "val3-sig"])
            ).rejects.toThrow("Circuit Breaker Engaged");

            await bridge.resumeBridge();
            // Works now
            await bridge.wrapAsset(tokenContract, "10", SupportedChain.Ethereum, "dest", "0");
        });

        it("should prevent execution utilizing non-whitelisted assets", async () => {
            await expect(
                 bridge.depositLiquidity("malicious-fake-asset", "100000000000")
            ).rejects.toThrow("Invalid Asset");
        });

        it("should spike fees dynamically when liquidity utilization nears exhaustion boundaries", async () => {
            // Pool currently has 100,000 available liquidity.
            // Bridging 99,000 triggers the exponential limit check (>80%) dynamically calculating higher costs 
            const receipt = await bridge.wrapAsset(
                 tokenContract,
                 "99000",
                 SupportedChain.Ethereum,
                 "eth-user",
                 "0" // no limit
            );

            // Calculation checks:
            const feeBps = 800; // Expected ramped max fee bound ~8% since utilization is ~99%
            const expectedFee = BigInt(99000) * BigInt(feeBps) / 10000n; // 7920
            const expectedOutput = 99000n - expectedFee; // 91080

            // Ensure our massive bridge request paid substantially higher costs
            // to discourage extracting the remaining reserves.
            expect(BigInt(receipt.amount)).toBe(expectedOutput); 
        });
    });
});
