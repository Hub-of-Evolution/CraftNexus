import { BridgeReceipt, SupportedChain } from "../structures/BridgeStructure";

export class MultiChainValidator {
    private processedNonces: Map<SupportedChain, Set<number>>;
    private allowedSigners: Set<string>;
    
    // Configurable signature threshold required for quorum (typically 66% + 1)
    private readonly SIGNATURE_THRESHOLD: number = 3; 

    constructor(initialSigners: string[]) {
        this.processedNonces = new Map();
        
        // Initialize sequence trackers per chain
        this.processedNonces.set(SupportedChain.Ethereum, new Set());
        this.processedNonces.set(SupportedChain.Polygon, new Set());
        this.processedNonces.set(SupportedChain.Stellar, new Set());

        this.allowedSigners = new Set(initialSigners);
    }

    /**
     * Replay protection checkpoint. Validates whether a given cross-chain nonce
     * has already been successfully executed on the target destination.
     * @param nonce Extracted receipt sequence number
     * @param dest Target chain
     */
    public assertNonceIsUnique(nonce: number, dest: SupportedChain): void {
        const destTracker = this.processedNonces.get(dest);
        if (!destTracker) {
            throw new Error(`[MultiChainValidator] Unsupported Destination Chain: ${dest}`);
        }

        if (destTracker.has(nonce)) {
            throw new Error(`[MultiChainValidator] Double-spend Replay Rejected: Nonce ${nonce} already executed on ${dest}`);
        }
    }

    /**
     * Mark a nonce executed (called only after inbound transfer processes natively).
     */
    public markNonceExecuted(nonce: number, dest: SupportedChain): void {
        this.processedNonces.get(dest)?.add(nonce);
    }

    /**
     * Validates cryptographic signatures from independent Oracle components.
     * Requires at least `SIGNATURE_THRESHOLD` valid signatures to securely authorize.
     * @param receipt The unpacked raw payload struct representing the transfer
     * @param signatures A list of ECDSA/ED25519 payload signatures
     * @returns Boolean indicating Quorum achieved
     */
    public async verifyMultiChainQuorum(receipt: BridgeReceipt, signatures: string[]): Promise<boolean> {
        if (signatures.length < this.SIGNATURE_THRESHOLD) {
            throw new Error(`[MultiChainValidator] Quorum Failure: Expected ${this.SIGNATURE_THRESHOLD} signatures, received ${signatures.length}`);
        }

        // Generate the raw message hash the validator expects
        const messageHash = this.deriveMessageHash(receipt);
        const verifiedAddresses = new Set<string>();

        for (const sig of signatures) {
            try {
                // Mocking cross-chain elliptic curve recoveries
                const recoveredSigner = this.mockRecoverSigner(messageHash, sig);
                
                if (this.allowedSigners.has(recoveredSigner)) {
                    verifiedAddresses.add(recoveredSigner);
                }
            } catch (e) {
               console.warn(`[MultiChainValidator] Invalid signature encountered: ${sig}`);
            }
        }

        if (verifiedAddresses.size < this.SIGNATURE_THRESHOLD) {
            throw new Error(`[MultiChainValidator] Security Breach Rejected: Only ${verifiedAddresses.size} unique valid signatures recognized. Expected ${this.SIGNATURE_THRESHOLD}`);
        }

        return true;
    }

    /**
     * Unique cryptographic representation of the entire payload state ensuring modifications
     * force a hash mismatch natively.
     */
    private deriveMessageHash(receipt: BridgeReceipt): string {
        return `${receipt.nonce}:${receipt.sourceChain}:${receipt.destChain}:${receipt.sender}:${receipt.destinationAddress}:${receipt.tokenContractAddress}:${receipt.amount}`;
    }

    /**
     * Mock function mapping for structural design demonstrating ECDSA recovery logic.
     * In solidity this is `ecrecover()`, on stellar `verify()`.
     */
    private mockRecoverSigner(hash: string, signature: string): string {
        // Assume signature string structure embeds public key for this TS abstraction
        return signature.split("-")[0];
    }
}
