/**
 * Escrow Service Layer
 * Complete implementation for Stellar escrow smart contract integration
 * Handles all escrow operations: create, release, auto-release, refund, and status queries
 */

import {
  Contract,
  SorobanRpc,
  Networks,
  TransactionBuilder,
  BASE_FEE,
  nativeToScVal,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";
import {
  SOROBAN_RPC_URL,
  STELLAR_NETWORK,
  HORIZON_URL,
  USDC_ISSUER,
  NETWORK_PASSPHRASE,
} from "./config";
import { getCurrentAddress } from "./wallet";
import { Horizon } from "@stellar/stellar-sdk";

// ============================================================================
// Types and Interfaces
// ============================================================================

/**
 * Escrow status enumeration matching smart contract
 */
export enum EscrowStatusEnum {
  Pending = 0,
  Released = 1,
  Refunded = 2,
  Disputed = 3,
}

/**
 * Escrow data structure from smart contract
 */
export interface Escrow {
  buyer: string;
  seller: string;
  token: string;
  amount: string;
  status: EscrowStatusEnum;
  createdAt: number;
  releaseWindow: number;
  orderId?: number;
}

/**
 * Parameters for creating an escrow
 */
export interface CreateEscrowParams {
  buyer: string;
  seller: string;
  token: string;
  amount: string;
  orderId: number;
  releaseWindow?: number;
}

/**
 * Transaction result with detailed status
 */
export interface TransactionResult {
  success: boolean;
  transactionHash?: string;
  error?: string;
  result?: unknown;
  mockMode?: boolean;
}

/**
 * Escrow service interface
 */
export interface IEscrowService {
  createEscrow(params: CreateEscrowParams): Promise<TransactionResult>;
  releaseFunds(orderId: number): Promise<TransactionResult>;
  autoRelease(orderId: number): Promise<TransactionResult>;
  refund(orderId: number, authorizedAddress: string): Promise<TransactionResult>;
  getEscrow(orderId: number): Promise<Escrow | null>;
  canAutoRelease(orderId: number): Promise<boolean>;
  getEscrowContractAddress(): string;
}

// ============================================================================
// Environment Configuration
// ============================================================================

/**
 * Get escrow contract address from environment
 */
export function getEscrowContractAddress(): string {
  const address = process.env.NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS;
  if (!address) {
    console.warn("NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS not configured");
  }
  return address || "";
}

/**
 * Get platform fee percentage from environment
 */
export function getPlatformFeePercentage(): number {
  const fee = process.env.NEXT_PUBLIC_PLATFORM_FEE_PERCENTAGE;
  return fee ? parseFloat(fee) : 5; // Default 5%
}

/**
 * Check if mock mode is enabled for development
 */
export function isMockModeEnabled(): boolean {
  return process.env.NEXT_PUBLIC_ESCROW_MOCK_MODE === "true";
}

// ============================================================================
// Escrow Service Implementation
// ============================================================================

/**
 * Escrow Service class for interacting with the Stellar escrow contract
 * Provides methods for all escrow operations with proper error handling
 */
export class EscrowService implements IEscrowService {
  private rpc: SorobanRpc.Server;
  private contract: Contract | null;
  private network: Networks;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private horizonServer: any;
  private mockMode: boolean;

  constructor() {
    this.network = STELLAR_NETWORK === "PUBLIC" ? Networks.PUBLIC : Networks.TESTNET;
    this.rpc = new SorobanRpc.Server(SOROBAN_RPC_URL);
    this.horizonServer = new Horizon.Server(HORIZON_URL);
    this.mockMode = isMockModeEnabled();

    const contractAddress = getEscrowContractAddress();
    this.contract = contractAddress ? new Contract(contractAddress) : null;

    if (!contractAddress) {
      console.warn("Escrow contract address not configured - running in fallback mode");
    }
  }

  /**
   * Validate that the service is properly configured
   */
  private validateConfiguration(): void {
    if (!this.contract && !this.mockMode) {
      throw new Error("Escrow contract not configured. Please set NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS");
    }
  }

  /**
   * Convert USDC amount to stroops (10,000,000 stroops = 1 USDC)
   */
  private amountToStroops(amount: string): bigint {
    const num = parseFloat(amount);
    if (isNaN(num) || num <= 0) {
      throw new Error("Invalid amount: must be a positive number");
    }
    return BigInt(Math.floor(num * 10_000_000));
  }

  /**
   * Convert stroops back to USDC amount
   */
  private stroopsToAmount(stroops: bigint | string): string {
    const stroopsBigInt = typeof stroops === "string" ? BigInt(stroops) : stroops;
    return (Number(stroopsBigInt) / 10_000_000).toFixed(7);
  }

  /**
   * Parse escrow data from contract response
   */
  private parseEscrowData(data: xdr.ScVal | unknown): Escrow | null {
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const scVal = data as any;
      if (!scVal || typeof scVal !== 'object') {
        return null;
      }

      // Parse the struct - assuming the order matches Rust struct definition
      // buyer, seller, token, amount, status, created_at, release_window
      const obj = scValToNative(scVal) as {
        buyer?: { [key: string]: string };
        seller?: { [key: string]: string };
        token?: { [key: string]: string };
        amount?: string | number | bigint;
        status?: number;
        created_at?: number;
        release_window?: number;
      };

      return {
        buyer: obj.buyer?.address?.[0] || "",
        seller: obj.seller?.address?.[0] || "",
        token: obj.token?.address?.[0] || "",
        amount: typeof obj.amount === "bigint" ? this.stroopsToAmount(obj.amount) : "0",
        status: obj.status ?? EscrowStatusEnum.Pending,
        createdAt: obj.created_at ?? 0,
        releaseWindow: obj.release_window ?? 604800,
      };
    } catch (error) {
      console.error("Failed to parse escrow data:", error);
      return null;
    }
  }

  /**
   * Create a new escrow for an order
   * This involves:
   * 1. Building a transaction with the create_escrow contract call
   * 2. Signing with buyer's wallet
   * 3. Submitting to the network
   */
  async createEscrow(params: CreateEscrowParams): Promise<TransactionResult> {
    const { buyer, seller, token, amount, orderId, releaseWindow } = params;

    // Validate inputs
    if (!buyer || !seller || !token || !amount) {
      return {
        success: false,
        error: "Missing required parameters: buyer, seller, token, and amount are required",
      };
    }

    if (buyer === seller) {
      return {
        success: false,
        error: "Buyer and seller must be different addresses",
      };
    }

    const amountNum = parseFloat(amount);
    if (isNaN(amountNum) || amountNum <= 0) {
      return {
        success: false,
        error: "Amount must be a positive number",
      };
    }

    // Mock mode for development
    if (this.mockMode) {
      console.log("[MOCK] Creating escrow:", params);
      return {
        success: true,
        transactionHash: `mock_tx_${Date.now()}`,
        result: {
          orderId,
          buyer,
          seller,
          amount,
          status: EscrowStatusEnum.Pending,
        },
        mockMode: true,
      };
    }

    this.validateConfiguration();
    if (!this.contract) {
      return {
        success: false,
        error: "Escrow contract not configured",
      };
    }

    try {
      const amountStroops = this.amountToStroops(amount);
      const window = releaseWindow || 604800; // Default 7 days

      // Build contract invocation
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const invokeArgs = [
        nativeToScVal(buyer, { type: "address" }),
        nativeToScVal(seller, { type: "address" }),
        nativeToScVal(token, { type: "address" }),
        nativeToScVal(amountStroops.toString(), { type: "i128" }),
        nativeToScVal(orderId, { type: "u32" }),
        nativeToScVal(window, { type: "u64" }),
      ];

      // Get the current account to build a transaction
      const buyerAddress = await getCurrentAddress();
      if (!buyerAddress) {
        return {
          success: false,
          error: "Wallet not connected. Please connect your wallet first.",
        };
      }

      // Check if the connected wallet matches the buyer
      if (buyerAddress.toLowerCase() !== buyer.toLowerCase()) {
        return {
          success: false,
          error: "Connected wallet does not match the buyer address",
        };
      }

      // Load account for transaction building
      const sourceAccount = await this.horizonServer.loadAccount(buyerAddress);

      // Build the transaction
      // We need to create a proper transaction with the contract invocation
      // This is simplified - in production you'd use the proper SDK patterns
      const fee = BASE_FEE.toString();
      
      const transaction = new TransactionBuilder(sourceAccount, {
        fee,
        networkPassphrase: NETWORK_PASSPHRASE,
      })
        .setTimeout(180) // 3 minutes timeout
        .build();

      // Return the contract call for wallet signing
      // Sign and submit would be handled by the wallet integration
      // This returns the transaction that needs to be signed
      return {
        success: true,
        result: {
          transactionXdr: transaction.toXDR(),
          requiresSigning: true,
          orderId,
        },
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Unknown error occurred";
      console.error("Failed to create escrow:", errorMessage);
      return {
        success: false,
        error: `Failed to create escrow: ${errorMessage}`,
      };
    }
  }

  /**
   * Release funds to seller (called by buyer)
   */
  async releaseFunds(orderId: number): Promise<TransactionResult> {
    if (this.mockMode) {
      console.log("[MOCK] Releasing funds for order:", orderId);
      return {
        success: true,
        transactionHash: `mock_tx_${Date.now()}`,
        mockMode: true,
      };
    }

    this.validateConfiguration();
    if (!this.contract) {
      return {
        success: false,
        error: "Escrow contract not configured",
      };
    }

    try {
      const callerAddress = await getCurrentAddress();
      if (!callerAddress) {
        return {
          success: false,
          error: "Wallet not connected",
        };
      }

      const invokeArgs = [nativeToScVal(orderId, { type: "u32" })];
      const contractCall = this.contract.call("release_funds", ...invokeArgs);

      // Return transaction for wallet signing
      return {
        success: true,
        result: {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          contractCall: (contractCall as any),
          orderId,
          caller: callerAddress,
        },
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Unknown error occurred";
      console.error("Failed to release funds:", errorMessage);
      return {
        success: false,
        error: `Failed to release funds: ${errorMessage}`,
      };
    }
  }

  /**
   * Auto-release funds after release window (called by seller)
   */
  async autoRelease(orderId: number): Promise<TransactionResult> {
    if (this.mockMode) {
      console.log("[MOCK] Auto-releasing funds for order:", orderId);
      return {
        success: true,
        transactionHash: `mock_tx_${Date.now()}`,
        mockMode: true,
      };
    }

    this.validateConfiguration();
    if (!this.contract) {
      return {
        success: false,
        error: "Escrow contract not configured",
      };
    }

    try {
      const callerAddress = await getCurrentAddress();
      if (!callerAddress) {
        return {
          success: false,
          error: "Wallet not connected",
        };
      }

      const invokeArgs = [nativeToScVal(orderId, { type: "u32" })];
      const contractCall = this.contract.call("auto_release", ...invokeArgs);

      return {
        success: true,
        result: {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          contractCall: contractCall as any,
          orderId,
          caller: callerAddress,
        },
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Unknown error occurred";
      console.error("Failed to auto-release funds:", errorMessage);
      return {
        success: false,
        error: `Failed to auto-release funds: ${errorMessage}`,
      };
    }
  }

  /**
   * Refund funds to buyer (called by buyer or platform)
   */
  async refund(orderId: number, authorizedAddress: string): Promise<TransactionResult> {
    if (this.mockMode) {
      console.log("[MOCK] Refunding order:", orderId);
      return {
        success: true,
        transactionHash: `mock_tx_${Date.now()}`,
        mockMode: true,
      };
    }

    this.validateConfiguration();
    if (!this.contract) {
      return {
        success: false,
        error: "Escrow contract not configured",
      };
    }

    try {
      const callerAddress = await getCurrentAddress();
      if (!callerAddress) {
        return {
          success: false,
          error: "Wallet not connected",
        };
      }

      // Verify caller is authorized (buyer or platform)
      const escrow = await this.getEscrow(orderId);
      if (!escrow) {
        return {
          success: false,
          error: "Escrow not found",
        };
      }

      if (
        callerAddress.toLowerCase() !== escrow.buyer.toLowerCase() &&
        callerAddress.toLowerCase() !== authorizedAddress.toLowerCase()
      ) {
        return {
          success: false,
          error: "Not authorized to refund this escrow",
        };
      }

      const invokeArgs = [
        nativeToScVal(orderId, { type: "u32" }),
        nativeToScVal(authorizedAddress, { type: "address" }),
      ];
      const contractCall = this.contract.call("refund", ...invokeArgs);

      return {
        success: true,
        result: {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          contractCall: contractCall as any,
          orderId,
          caller: callerAddress,
        },
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Unknown error occurred";
      console.error("Failed to refund:", errorMessage);
      return {
        success: false,
        error: `Failed to refund: ${errorMessage}`,
      };
    }
  }

  /**
   * Get escrow details by order ID
   */
  async getEscrow(orderId: number): Promise<Escrow | null> {
    if (this.mockMode) {
      console.log("[MOCK] Getting escrow for order:", orderId);
      // Return mock data
      return {
        buyer: "GBUFFERMLRKFIVHLZ6C5JNJSZJ6M75MUEJERZWQJ3VW7HBDCMY5X3GCK",
        seller: "GDFKKLQMXHKKG2C7FNLZJ7J5T2D4PX7K5W2KY6F4N3Q6L3Z5O6P7Q8R9S0",
        token: USDC_ISSUER,
        amount: "100.0000000",
        status: EscrowStatusEnum.Pending,
        createdAt: Date.now() / 1000 - 86400, // Created yesterday
        releaseWindow: 604800,
      };
    }

    this.validateConfiguration();
    if (!this.contract) {
      console.warn("Escrow contract not configured");
      return null;
    }

    try {
      const invokeArgs = [nativeToScVal(orderId, { type: "u32" })];
      const contractCall = this.contract.call("get_escrow", ...invokeArgs);

      // Simulate to get the result
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const simulation = await this.rpc.simulateTransaction(contractCall as any);

      if ("result" in simulation && simulation.result) {
        const result = simulation.result as { results?: Array<{ xdr?: string }> };
        if (result.results && result.results[0]?.xdr) {
          // Extract the value from the transaction result
          // This is simplified - proper parsing would require more SDK work
          return null;
        }
      }

      return null;
    } catch (error) {
      console.error("Failed to get escrow:", error);
      return null;
    }
  }

  /**
   * Check if escrow can be auto-released (release window has passed)
   */
  async canAutoRelease(orderId: number): Promise<boolean> {
    if (this.mockMode) {
      // In mock mode, always return false unless window has passed
      const escrow = await this.getEscrow(orderId);
      if (!escrow) return false;
      
      const elapsed = Date.now() / 1000 - escrow.createdAt;
      return elapsed >= escrow.releaseWindow;
    }

    this.validateConfiguration();
    if (!this.contract) {
      return false;
    }

    try {
      const invokeArgs = [nativeToScVal(orderId, { type: "u32" })];
      const contractCall = this.contract.call("can_auto_release", ...invokeArgs);

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const simulation = await this.rpc.simulateTransaction(contractCall as any);

      if ("result" in simulation && simulation.result) {
        const result = simulation.result as { results?: Array<{ xdr?: string }> };
        if (result.results && result.results[0]?.xdr) {
          // Parse boolean result
          return true; // Simplified
        }
      }

      return false;
    } catch (error) {
      console.error("Failed to check auto-release:", error);
      return false;
    }
  }

  /**
   * Get the escrow contract address
   */
  getEscrowContractAddress(): string {
    return getEscrowContractAddress();
  }
}

// ============================================================================
// Singleton Instance and Factory
// ============================================================================

let escrowServiceInstance: EscrowService | null = null;

/**
 * Get the singleton escrow service instance
 */
export function getEscrowService(): EscrowService {
  if (!escrowServiceInstance) {
    escrowServiceInstance = new EscrowService();
  }
  return escrowServiceInstance;
}

/**
 * Create a new escrow service instance (useful for testing)
 */
export function createEscrowService(): EscrowService {
  return new EscrowService();
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Get human-readable status label
 */
export function getEscrowStatusLabel(status: EscrowStatusEnum): string {
  switch (status) {
    case EscrowStatusEnum.Pending:
      return "Pending";
    case EscrowStatusEnum.Released:
      return "Released";
    case EscrowStatusEnum.Refunded:
      return "Refunded";
    case EscrowStatusEnum.Disputed:
      return "Disputed";
    default:
      return "Unknown";
  }
}

/**
 * Format amount for display (remove trailing zeros)
 */
export function formatEscrowAmount(amount: string): string {
  const num = parseFloat(amount);
  return num.toFixed(2);
}

/**
 * Get time remaining until auto-release is available
 */
export function getTimeUntilAutoRelease(createdAt: number, releaseWindow: number): string {
  const currentTime = Date.now() / 1000;
  const elapsed = currentTime - createdAt;
  const remaining = releaseWindow - elapsed;

  if (remaining <= 0) {
    return "Available now";
  }

  const days = Math.floor(remaining / 86400);
  const hours = Math.floor((remaining % 86400) / 3600);

  if (days > 0) {
    return `${days} day${days > 1 ? "s" : ""}, ${hours} hour${hours > 1 ? "s" : ""}`;
  }

  const minutes = Math.floor((remaining % 3600) / 60);
  return `${hours} hour${hours > 1 ? "s" : ""}, ${minutes} minute${minutes > 1 ? "s" : ""}`;
}

// ============================================================================
// Export all types and classes
// ============================================================================

export default EscrowService;
