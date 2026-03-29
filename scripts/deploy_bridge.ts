import { CrossChainBridge } from "../contracts/bridge/CrossChainBridge";
import { SupportedChain } from "../contracts/bridge/structures/BridgeStructure";

/**
 * Local deployment script demonstrating standard bootstrap instantiation parameters
 * scaling into Production (Stellar / EVM RPC networks)
 */
async function main() {
    console.log("[Node] Initializing CrossChainBridge Off-chain Configuration");

    // Environmental mapping dependencies loaded natively
    const ADMIN_WALLET = process.env.ADMIN_PUBKEY || "G_ADMIN_DEFAULT";
    const TARGET_CHAIN = Number(process.env.CHAIN_ID) || SupportedChain.Stellar;

    // Secure node oracle dependencies mapping authorized keys allowed to execute payload signatures
    const AUTHORIZED_VALIDATORS = [
        "V1_NODE_PUB",
        "V2_NODE_PUB", 
        "V3_NODE_PUB",
        "V4_NODE_PUB"
    ];

    console.log(`[Config] Operating Chain: ${TARGET_CHAIN}`);
    console.log(`[Config] Bound Administrative Key: ${ADMIN_WALLET}`);
    console.log(`[Config] Establishing Validator Set: ${AUTHORIZED_VALIDATORS.length} Oracles Loaded`);

    const bridgeController = new CrossChainBridge(TARGET_CHAIN, ADMIN_WALLET, AUTHORIZED_VALIDATORS);

    // Initial Whitelist configuration
    const USDC_STELLAR = "CCW67TSZV34CE5H...";
    const USDC_POLYGON = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174";
    
    // Establishing pool pairs assigning roughly .2% base conversion fees dynamically
    bridgeController.initializeAsset(USDC_STELLAR, 20);
    console.log(`[Registry] Native Energy Token Pool Created: ${USDC_STELLAR}`);

    if (TARGET_CHAIN === SupportedChain.Polygon) {
        bridgeController.initializeAsset(USDC_POLYGON, 20);
        console.log(`[Registry] Polygon USDC Wrapper Mapped: ${USDC_POLYGON}`);
    }

    // Deploying system dependencies asynchronously completing genesis
    // simulate delay or on-chain tx bindings
    await new Promise((res) => setTimeout(res, 2000));

    // Expose controller routing instance internally
    console.log("\n[Status] Bridge System Genesis Boot complete. Relayer listeners live.");
}

main().catch((error) => {
    console.error(`[Error] Fatal Node Error executing Bridge Deployment: ${error}`);
    process.exit(1);
});
