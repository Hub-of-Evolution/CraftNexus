/**
 * Escrow Actions Component
 * Provides action buttons for escrow operations (release, refund, auto-release)
 */

"use client";

import { useState, useCallback, useMemo } from "react";
import {
  EscrowService,
  Escrow,
  EscrowStatusEnum,
} from "@/lib/stellar/escrow";
import { useEscrowWallet, TransactionStatusDisplay, TransactionStatus } from "@/components/wallet/EscrowWalletIntegration";
import { PLATFORM_COMMISSION_WALLET } from "@/lib/stellar/config";

export interface EscrowActionsProps {
  escrow: Escrow;
  orderId: number;
  currentUserAddress: string;
  onSuccess?: (action: string, result: { transactionHash?: string }) => void;
  onError?: (error: string) => void;
  className?: string;
}

export function EscrowActions({
  escrow,
  orderId,
  currentUserAddress,
  onSuccess,
  onError,
  className = "",
}: EscrowActionsProps) {
  const { walletState, validateNetwork } = useEscrowWallet();
  const escrowService = useMemo(() => new EscrowService(), []);

  const [isProcessing, setIsProcessing] = useState(false);
  const [txStatus, setTxStatus] = useState<TransactionStatus>("idle");
  const [txHash, setTxHash] = useState<string | undefined>();
  const [error, setError] = useState<string | null>(null);
  const [actionType, setActionType] = useState<string | null>(null);

  // Check if current user is the buyer
  const isBuyer = currentUserAddress.toLowerCase() === escrow.buyer.toLowerCase();
  
  // Check if current user is the seller
  const isSeller = currentUserAddress.toLowerCase() === escrow.seller.toLowerCase();

  // Check if user can perform actions
  const canPerformAction = isBuyer || isSeller;

  // Check if escrow is in pending state
  const isPending = escrow.status === EscrowStatusEnum.Pending;

  // Handle release funds action
  const handleRelease = useCallback(async () => {
    if (!walletState.isConnected) {
      setError("Please connect your wallet first");
      return;
    }

    setIsProcessing(true);
    setError(null);
    setActionType("release");
    setTxStatus("pending");

    try {
      // Validate network
      const isNetworkValid = await validateNetwork();
      if (!isNetworkValid) {
        throw new Error("Network mismatch. Please switch to the correct network.");
      }

      const result = await escrowService.releaseFunds(orderId);

      if (result.success) {
        setTxStatus("confirmed");
        setTxHash(result.transactionHash);
        if (onSuccess) {
          onSuccess("release", { transactionHash: result.transactionHash });
        }
      } else {
        setTxStatus("failed");
        setError(result.error || "Failed to release funds");
        if (onError) {
          onError(result.error || "Failed to release funds");
        }
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Unknown error occurred";
      setTxStatus("failed");
      setError(errorMessage);
      if (onError) {
        onError(errorMessage);
      }
    } finally {
      setIsProcessing(false);
    }
  }, [walletState.isConnected, validateNetwork, escrowService, orderId, onSuccess, onError]);

  // Handle refund action
  const handleRefund = useCallback(async () => {
    if (!walletState.isConnected) {
      setError("Please connect your wallet first");
      return;
    }

    setIsProcessing(true);
    setError(null);
    setActionType("refund");
    setTxStatus("pending");

    try {
      // Validate network
      const isNetworkValid = await validateNetwork();
      if (!isNetworkValid) {
        throw new Error("Network mismatch. Please switch to the correct network.");
      }

      const authorizedAddress = PLATFORM_COMMISSION_WALLET || currentUserAddress;
      const result = await escrowService.refund(orderId, authorizedAddress);

      if (result.success) {
        setTxStatus("confirmed");
        setTxHash(result.transactionHash);
        if (onSuccess) {
          onSuccess("refund", { transactionHash: result.transactionHash });
        }
      } else {
        setTxStatus("failed");
        setError(result.error || "Failed to refund");
        if (onError) {
          onError(result.error || "Failed to refund");
        }
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Unknown error occurred";
      setTxStatus("failed");
      setError(errorMessage);
      if (onError) {
        onError(errorMessage);
      }
    } finally {
      setIsProcessing(false);
    }
  }, [walletState.isConnected, validateNetwork, escrowService, orderId, currentUserAddress, onSuccess, onError]);

  // Handle auto-release action
  const handleAutoRelease = useCallback(async () => {
    if (!walletState.isConnected) {
      setError("Please connect your wallet first");
      return;
    }

    setIsProcessing(true);
    setError(null);
    setActionType("auto-release");
    setTxStatus("pending");

    try {
      // Validate network
      const isNetworkValid = await validateNetwork();
      if (!isNetworkValid) {
        throw new Error("Network mismatch. Please switch to the correct network.");
      }

      // Check if auto-release is available
      const canAutoRelease = await escrowService.canAutoRelease(orderId);
      if (!canAutoRelease) {
        throw new Error("Auto-release is not available yet. The release window may not have passed.");
      }

      const result = await escrowService.autoRelease(orderId);

      if (result.success) {
        setTxStatus("confirmed");
        setTxHash(result.transactionHash);
        if (onSuccess) {
          onSuccess("auto-release", { transactionHash: result.transactionHash });
        }
      } else {
        setTxStatus("failed");
        setError(result.error || "Failed to auto-release funds");
        if (onError) {
          onError(result.error || "Failed to auto-release funds");
        }
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Unknown error occurred";
      setTxStatus("failed");
      setError(errorMessage);
      if (onError) {
        onError(errorMessage);
      }
    } finally {
      setIsProcessing(false);
    }
  }, [walletState.isConnected, validateNetwork, escrowService, orderId, onSuccess, onError]);

  // Not connected state
  if (!walletState.isConnected) {
    return (
      <div className={`bg-gray-50 rounded-lg p-4 text-center ${className}`}>
        <p className="text-gray-600 text-sm">Connect your wallet to perform actions</p>
      </div>
    );
  }

  // Not authorized state
  if (!canPerformAction) {
    return (
      <div className={`bg-gray-50 rounded-lg p-4 text-center ${className}`}>
        <p className="text-gray-600 text-sm">You are not authorized to perform actions on this escrow</p>
      </div>
    );
  }

  // Already completed state
  if (!isPending) {
    return (
      <div className={`bg-gray-50 rounded-lg p-4 text-center ${className}`}>
        <p className="text-gray-600 text-sm">
          This escrow has been {statusLabels[escrow.status as EscrowStatusEnum].toLowerCase()}
        </p>
      </div>
    );
  }

  return (
    <div className={`space-y-4 ${className}`}>
      {/* Action Buttons */}
      <div className="grid grid-cols-2 gap-3">
        {/* Release Button - Only visible to buyer */}
        {isBuyer && (
          <button
            onClick={handleRelease}
            disabled={isProcessing}
            className="px-4 py-2 bg-green-600 text-white rounded-lg font-medium hover:bg-green-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm"
          >
            {isProcessing && actionType === "release" ? "Processing..." : "Release Funds"}
          </button>
        )}

        {/* Refund Button - Visible to buyer */}
        {isBuyer && (
          <button
            onClick={handleRefund}
            disabled={isProcessing}
            className="px-4 py-2 bg-red-600 text-white rounded-lg font-medium hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm"
          >
            {isProcessing && actionType === "refund" ? "Processing..." : "Request Refund"}
          </button>
        )}

        {/* Auto-Release Button - Only visible to seller */}
        {isSeller && (
          <button
            onClick={handleAutoRelease}
            disabled={isProcessing}
            className="col-span-2 px-4 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-sm"
          >
            {isProcessing && actionType === "auto-release" ? "Processing..." : "Auto-Release Funds"}
          </button>
        )}
      </div>

      {/* Error Display */}
      {error && (
        <div className="p-3 bg-red-50 border border-red-200 rounded-lg text-red-700 text-sm">
          {error}
        </div>
      )}

      {/* Transaction Status */}
      {txStatus !== "idle" && (
        <TransactionStatusDisplay status={txStatus} txHash={txHash} error={error || undefined} />
      )}

      {/* Help Text */}
      <div className="text-xs text-gray-500 space-y-1">
        <p>
          <strong>Release:</strong> Confirm the order is complete and release funds to the seller.
        </p>
        <p>
          <strong>Refund:</strong> Cancel the order and refund the funds to the buyer.
        </p>
        <p>
          <strong>Auto-Release:</strong> Automatically release funds after the release window has passed.
        </p>
      </div>
    </div>
  );
}

// Status labels mapping
const statusLabels: Record<number, string> = {
  0: "Pending",
  1: "Released",
  2: "Refunded",
  3: "Disputed",
};

export default EscrowActions;
