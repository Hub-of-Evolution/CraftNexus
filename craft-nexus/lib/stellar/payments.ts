/**
 * Stellar Payment Service
 * Handles USDC payments and transactions on Stellar network
 */

import {
  Server,
  Asset,
  Operation,
  TransactionBuilder,
  Memo,
  Networks,
  BASE_FEE,
  Keypair,
  xdr,
} from "@stellar/stellar-sdk";
import {
  HORIZON_URL,
  NETWORK_PASSPHRASE,
  USDC_ISSUER,
  STELLAR_NETWORK,
  PLATFORM_COMMISSION_PERCENT,
  PLATFORM_COMMISSION_WALLET,
} from "./config";

export interface PaymentParams {
  senderSecret: string;
  recipientPublicKey: string;
  amount: string; // Amount in USDC (e.g., "10.50")
  memo?: string;
  orderId?: string;
}

export interface EscrowPaymentParams {
  buyerPublicKey: string;
  sellerPublicKey: string;
  amount: string;
  orderId: string;
}

export class StellarPaymentService {
  private server: Server;
  private network: Networks;

  constructor() {
    this.network = STELLAR_NETWORK === "PUBLIC" ? Networks.PUBLIC : Networks.TESTNET;
    this.server = new Server(HORIZON_URL);
  }

  /**
   * Send USDC payment from sender to recipient
   */
  async sendUSDC(params: PaymentParams): Promise<string> {
    try {
      const { senderSecret, recipientPublicKey, amount, memo, orderId } = params;
      
      const senderKeypair = Keypair.fromSecret(senderSecret);
      const senderAccount = await this.server.loadAccount(senderKeypair.publicKey());

      // Create USDC asset
      const usdcAsset = new Asset("USDC", USDC_ISSUER);

      // Build transaction
      const transaction = new TransactionBuilder(senderAccount, {
        fee: BASE_FEE,
        networkPassphrase: this.network,
      })
        .addOperation(
          Operation.payment({
            destination: recipientPublicKey,
            asset: usdcAsset,
            amount: amount,
          })
        )
        .addMemo(orderId ? Memo.text(`ORDER:${orderId}`) : memo ? Memo.text(memo) : Memo.none())
        .setTimeout(30)
        .build();

      transaction.sign(senderKeypair);

      // Submit transaction
      const result = await this.server.submitTransaction(transaction);
      return result.hash;
    } catch (error) {
      console.error("Payment failed:", error);
      throw new Error(`Payment failed: ${error instanceof Error ? error.message : "Unknown error"}`);
    }
  }

  /**
   * Send payment with platform commission split
   * Sends payment to seller and commission to platform wallet
   */
  async sendPaymentWithCommission(
    buyerSecret: string,
    sellerPublicKey: string,
    amount: string,
    orderId: string
  ): Promise<{ paymentHash: string; commissionHash?: string }> {
    try {
      const buyerKeypair = Keypair.fromSecret(buyerSecret);
      const buyerAccount = await this.server.loadAccount(buyerKeypair.publicKey());

      const usdcAsset = new Asset("USDC", USDC_ISSUER);
      const amountNum = parseFloat(amount);
      
      // Calculate commission (5%)
      const commissionAmount = (amountNum * PLATFORM_COMMISSION_PERCENT / 100).toFixed(7);
      const sellerAmount = (amountNum - parseFloat(commissionAmount)).toFixed(7);

      // Build transaction with multiple payments
      const operations: Operation[] = [
        Operation.payment({
          destination: sellerPublicKey,
          asset: usdcAsset,
          amount: sellerAmount,
        }),
      ];

      // Add commission payment if platform wallet is configured
      if (PLATFORM_COMMISSION_WALLET) {
        operations.push(
          Operation.payment({
            destination: PLATFORM_COMMISSION_WALLET,
            asset: usdcAsset,
            amount: commissionAmount,
          })
        );
      }

      const transaction = new TransactionBuilder(buyerAccount, {
        fee: BASE_FEE,
        networkPassphrase: this.network,
      })
        .addOperation(...operations)
        .addMemo(Memo.text(`ORDER:${orderId}`))
        .setTimeout(30)
        .build();

      transaction.sign(buyerKeypair);

      const result = await this.server.submitTransaction(transaction);
      return {
        paymentHash: result.hash,
      };
    } catch (error) {
      console.error("Payment with commission failed:", error);
      throw new Error(
        `Payment failed: ${error instanceof Error ? error.message : "Unknown error"}`
      );
    }
  }

  /**
   * Get account balance for a public key
   */
  async getUSDCBalance(publicKey: string): Promise<string> {
    try {
      const account = await this.server.loadAccount(publicKey);
      const usdcAsset = new Asset("USDC", USDC_ISSUER);

      const balance = account.balances.find(
        (b: any) =>
          b.asset_code === "USDC" &&
          b.asset_issuer === USDC_ISSUER
      );

      return balance ? balance.balance : "0";
    } catch (error) {
      console.error("Failed to get balance:", error);
      return "0";
    }
  }

  /**
   * Get account details
   */
  async getAccountDetails(publicKey: string) {
    try {
      const account = await this.server.loadAccount(publicKey);
      return {
        publicKey: account.accountId(),
        balances: account.balances,
        sequenceNumber: account.sequenceNumber(),
      };
    } catch (error) {
      console.error("Failed to get account details:", error);
      throw error;
    }
  }

  /**
   * Check if account exists on Stellar network
   */
  async accountExists(publicKey: string): Promise<boolean> {
    try {
      await this.server.loadAccount(publicKey);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Get transaction details by hash
   */
  async getTransaction(hash: string) {
    try {
      return await this.server.transactions().transaction(hash).call();
    } catch (error) {
      console.error("Failed to get transaction:", error);
      throw error;
    }
  }

  /**
   * Create escrow account for holding funds
   * Note: This is a simplified escrow. For production, use Soroban smart contracts.
   */
  async createEscrowAccount(): Promise<{ publicKey: string; secret: string }> {
    const keypair = Keypair.random();
    const publicKey = keypair.publicKey();
    const secret = keypair.secret();

    // Fund account (for testnet, use friendbot)
    if (STELLAR_NETWORK === "TESTNET") {
      try {
        const response = await fetch(
          `https://friendbot.stellar.org?addr=${publicKey}`
        );
        await response.json();
      } catch (error) {
        console.error("Failed to fund testnet account:", error);
      }
    }

    return { publicKey, secret };
  }
}

// Export singleton instance
export const paymentService = new StellarPaymentService();
