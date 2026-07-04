/**
 * Transaction Signer
 * Handles transaction signing, simulation, and submission to Stellar network
 */

"use client";

import { useState, useCallback, useMemo } from "react";
import { TransactionBuilder, BASE_FEE, SorobanRpc, Transaction, Operation, Horizon } from "@stellar/stellar-sdk";
import {
  SOROBAN_RPC_URL,
  STELLAR_NETWORK,
  NETWORK_PASSPHRASE,
  HORIZON_URL,
} from "@/lib/stellar/config";
import { getCurrentAddress } from "@/lib/stellar/wallet";
import type { TransactionStatus } from "./EscrowWalletIntegration";

export interface SignAndSubmitResult {
  success: boolean;
  transactionHash?: string;
  error?: string;
  result?: unknown;
}

export interface SimulatedTransaction {
  minFee: number;
  minResourceFee: number;
  result: unknown;
}

/**
 * Hook for signing and submitting transactions
 */
export function useTransactionSigner() {
  const [status, setStatus] = useState<TransactionStatus>("idle");
  const [txHash, setTxHash] = useState<string | undefined>();
  const [error, setError] = useState<string | undefined>();
  const [isSubmitting, setIsSubmitting] = useState(false);

  const rpc = useMemo(() => new SorobanRpc.Server(SOROBAN_RPC_URL), []);
  const horizonUrl = HORIZON_URL;

  /**
   * Simulate a transaction before submission
   * This helps catch errors early and estimate fees
   */
  const simulateTransaction = useCallback(
    async (transaction: Transaction): Promise<SimulatedTransaction | null> => {
      try {
        const simulation = await rpc.simulateTransaction(transaction);

        if ("error" in simulation) {
          console.error("Simulation error:", simulation.error);
          return null;
        }

        // Extract simulation results
        const successResponse = simulation as unknown as { minFee?: number; minResourceFee?: number; result?: unknown };
        const minFee = successResponse.minFee || BASE_FEE;
        const minResourceFee = successResponse.minResourceFee || BASE_FEE;

        return {
          minFee: parseInt(String(minFee), 10),
          minResourceFee: parseInt(String(minResourceFee), 10),
          result: successResponse.result,
        };
      } catch (err) {
        console.error("Failed to simulate transaction:", err);
        return null;
      }
    },
    [rpc]
  );

  /**
   * Build and sign a Soroban contract transaction
   */
  const buildContractTransaction = useCallback(
    async (
      contractId: string,
      method: string,
      args: (string | number | bigint)[]
    ): Promise<Transaction | null> => {
      try {
        const publicKey = await getCurrentAddress();
        if (!publicKey) {
          throw new Error("Wallet not connected");
        }

        const horizonServer = new Horizon.Server(horizonUrl);
        const sourceAccount = await horizonServer.loadAccount(publicKey);

        // Build transaction for contract call
        const fee = BASE_FEE.toString();

        // Create a simple payment operation as placeholder
        // In production, this would be a proper Soroban contract invoke
        const transaction = new TransactionBuilder(sourceAccount, {
          fee,
          networkPassphrase: NETWORK_PASSPHRASE,
        })
          .setTimeout(180)
          .addOperation(Operation.invokeContractFunction({
            contract: contractId,
            function: method,
            args: args as unknown as never,
          }))
          .build();

        return transaction;
      } catch (err) {
        console.error("Failed to build contract transaction:", err);
        return null;
      }
    },
    [horizonUrl]
  );

  /**
   * Sign and submit a transaction using Freighter wallet
   */
  const signAndSubmit = useCallback(
    async (
      contractId: string,
      method: string,
      args: (string | number | bigint)[],
      options?: {
        timeout?: number;
        onStatusChange?: (status: TransactionStatus) => void;
      }
    ): Promise<SignAndSubmitResult> => {
      setIsSubmitting(true);
      setStatus("pending");
      setError(undefined);
      setTxHash(undefined);

      options?.onStatusChange?.("pending");

      try {
        // Build the transaction
        const transaction = await buildContractTransaction(contractId, method, args);

        if (!transaction) {
          throw new Error("Failed to build transaction");
        }

        // Serialize for wallet signing
        const transactionXdr = transaction.toXDR();

        // Sign using Freighter API
        const signedXdr = await signWithFreighter(transactionXdr, NETWORK_PASSPHRASE);

        if (!signedXdr) {
          throw new Error("Transaction signing failed or was rejected");
        }

        // Parse the signed XDR
        const signedTransaction = new Transaction(
          signedXdr,
          NETWORK_PASSPHRASE
        );

        // Submit to Horizon
        const horizonServer = new Horizon.Server(horizonUrl);
        const result = await horizonServer.submitTransaction(signedTransaction);

        const hash = result.hash;
        setTxHash(hash);
        setStatus("confirmed");
        options?.onStatusChange?.("confirmed");

        return {
          success: true,
          transactionHash: hash,
          result,
        };
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : "Unknown error occurred";
        console.error("Transaction failed:", errorMessage);
        setError(errorMessage);
        setStatus("failed");
        options?.onStatusChange?.("failed");

        return {
          success: false,
          error: errorMessage,
        };
      } finally {
        setIsSubmitting(false);
      }
    },
    [buildContractTransaction, horizonUrl]
  );

  /**
   * Reset the transaction state
   */
  const reset = useCallback(() => {
    setStatus("idle");
    setTxHash(undefined);
    setError(undefined);
    setIsSubmitting(false);
  }, []);

  return {
    status,
    txHash,
    error,
    isSubmitting,
    simulateTransaction,
    buildContractTransaction,
    signAndSubmit,
    reset,
  };
}

/**
 * Sign transaction using Freighter wallet
 */
async function signWithFreighter(
  transactionXdr: string,
  networkPassphrase: string
): Promise<string | null> {
  try {
    // Dynamic import to avoid SSR issues
    const FreighterApi = await import("@stellar/freighter-api");

    const signedTx = await FreighterApi.signTransaction(transactionXdr, {
      networkPassphrase,
    });

    return signedTx;
  } catch (error) {
    console.error("Freighter signTransaction failed:", error);
    return null;
  }
}

/**
 * Transaction Signer Component
 * UI component for signing and submitting escrow transactions
 */
interface TransactionSignerProps {
  contractId: string;
  method: string;
  args: (string | number | bigint)[];
  onSuccess?: (result: SignAndSubmitResult) => void;
  onError?: (error: string) => void;
  disabled?: boolean;
  buttonText?: string;
  loadingText?: string;
  className?: string;
}

export function TransactionSigner({
  contractId,
  method,
  args,
  onSuccess,
  onError,
  disabled = false,
  buttonText = "Sign & Submit",
  loadingText = "Processing...",
  className = "",
}: TransactionSignerProps) {
  const { signAndSubmit, status, txHash, error: txError, isSubmitting } = useTransactionSigner();

  const handleSign = async () => {
    try {
      const result = await signAndSubmit(contractId, method, args);

      if (result.success && onSuccess) {
        onSuccess(result);
      } else if (!result.success && onError && result.error) {
        onError(result.error);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Transaction failed";
      if (onError) {
        onError(errorMessage);
      }
    }
  };

  return (
    <div className={`space-y-3 ${className}`}>
      <button
        onClick={handleSign}
        disabled={disabled || isSubmitting}
        className="w-full px-4 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      >
        {isSubmitting ? loadingText : buttonText}
      </button>

      {status === "pending" && (
        <div className="p-3 rounded-lg bg-yellow-50 border border-yellow-200 text-yellow-700 text-sm">
          Transaction submitted. Waiting for confirmation...
        </div>
      )}

      {status === "confirmed" && txHash && (
        <div className="p-3 rounded-lg bg-green-50 border border-green-200 text-green-700 text-sm">
          <p className="font-medium">Transaction Confirmed</p>
          <a
            href={`https://stellar.explorer.expert/explorer/${STELLAR_NETWORK.toLowerCase()}/tx/${txHash}`}
            target="_blank"
            rel="noopener noreferrer"
            className="text-xs hover:underline font-mono break-all"
          >
            {txHash.slice(0, 12)}...{txHash.slice(-8)}
          </a>
        </div>
      )}

      {status === "failed" && txError && (
        <div className="p-3 rounded-lg bg-red-50 border border-red-200 text-red-700 text-sm">
          {txError}
        </div>
      )}
    </div>
  );
}

export default useTransactionSigner;
