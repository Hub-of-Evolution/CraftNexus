/**
 * Escrow Creation Form
 * Form component for creating new escrow transactions
 */

"use client";

import { useState, useCallback } from "react";
import {
  EscrowService,
  CreateEscrowParams,
  getPlatformFeePercentage,
  getEscrowContractAddress,
} from "@/lib/stellar/escrow";
import { useEscrowWallet, TransactionStatusDisplay, TransactionStatus } from "@/components/wallet/EscrowWalletIntegration";
import { USDC_ISSUER, STELLAR_NETWORK } from "@/lib/stellar/config";

export interface EscrowFormData {
  buyer: string;
  seller: string;
  amount: string;
  orderId: number;
  releaseWindow?: number;
}

export interface EscrowFormProps {
  defaultSeller?: string;
  defaultOrderId?: number;
  defaultAmount?: string;
  onSuccess?: (result: { orderId: number; transactionHash: string }) => void;
  onError?: (error: string) => void;
}

export function EscrowCreationForm({
  defaultSeller = "",
  defaultOrderId,
  defaultAmount = "",
  onSuccess,
  onError,
}: EscrowFormProps) {
  const { walletState, connect, validateNetwork } = useEscrowWallet();
  const escrowService = new EscrowService();

  const [formData, setFormData] = useState<EscrowFormData>({
    buyer: walletState.publicKey || "",
    seller: defaultSeller,
    amount: defaultAmount,
    orderId: defaultOrderId || Math.floor(Math.random() * 1000000),
    releaseWindow: 604800, // 7 days default
  });

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [txStatus, setTxStatus] = useState<TransactionStatus>("idle");
  const [txHash, setTxHash] = useState<string | undefined>();
  const [error, setError] = useState<string | null>(null);
  const [showConfirm, setShowConfirm] = useState(false);

  // Calculate platform fee
  const platformFeePercentage = getPlatformFeePercentage();
  const amountNum = parseFloat(formData.amount) || 0;
  const platformFee = (amountNum * platformFeePercentage) / 100;
  const sellerReceives = amountNum - platformFee;

  // Handle form input changes
  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const { name, value } = e.target;
      setFormData((prev) => ({ ...prev, [name]: value }));
      setError(null);
    },
    []
  );

  // Validate form data
  const validateForm = useCallback((): string | null => {
    if (!walletState.isConnected) {
      return "Please connect your wallet first";
    }
    if (!formData.buyer) {
      return "Buyer address is required";
    }
    if (!formData.seller) {
      return "Seller address is required";
    }
    if (formData.buyer === formData.seller) {
      return "Buyer and seller must be different";
    }
    if (!formData.amount || amountNum <= 0) {
      return "Please enter a valid amount";
    }
    if (!formData.orderId) {
      return "Order ID is required";
    }
    return null;
  }, [walletState.isConnected, formData, amountNum]);

  // Handle form submission
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    const validationError = validateForm();
    if (validationError) {
      setError(validationError);
      return;
    }

    // Show confirmation dialog
    if (!showConfirm) {
      setShowConfirm(true);
      return;
    }

    setIsSubmitting(true);
    setError(null);
    setTxStatus("pending");

    try {
      // Validate network
      const isNetworkValid = await validateNetwork();
      if (!isNetworkValid) {
        throw new Error("Network mismatch. Please switch to the correct network.");
      }

      const params: CreateEscrowParams = {
        buyer: formData.buyer,
        seller: formData.seller,
        token: USDC_ISSUER,
        amount: formData.amount,
        orderId: formData.orderId,
        releaseWindow: formData.releaseWindow,
      };

      const result = await escrowService.createEscrow(params);

      if (result.success) {
        setTxStatus("confirmed");
        // If it's a mock result or has transaction hash
        if (result.transactionHash || result.mockMode) {
          setTxHash(result.transactionHash || `mock_${Date.now()}`);
        }
        if (onSuccess) {
          onSuccess({
            orderId: formData.orderId,
            transactionHash: result.transactionHash || "",
          });
        }
      } else {
        setTxStatus("failed");
        setError(result.error || "Failed to create escrow");
        if (onError) {
          onError(result.error || "Failed to create escrow");
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
      setIsSubmitting(false);
    }
  };

  // Connect wallet if not connected
  const handleConnect = async () => {
    try {
      const account = await connect();
      if (account) {
        setFormData((prev) => ({ ...prev, buyer: account.publicKey }));
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Failed to connect wallet";
      setError(errorMessage);
    }
  };

  return (
    <div className="bg-white rounded-lg shadow-md p-6 max-w-lg mx-auto">
      <h2 className="text-2xl font-bold text-gray-800 mb-6">Create Escrow</h2>

      {/* Wallet Connection */}
      {!walletState.isConnected ? (
        <div className="text-center py-8">
          <p className="text-gray-600 mb-4">Connect your wallet to create an escrow</p>
          <button
            onClick={handleConnect}
            className="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 transition-colors"
          >
            Connect Wallet
          </button>
        </div>
      ) : (
        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Buyer Address (from connected wallet) */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Buyer (Your Address)
            </label>
            <input
              type="text"
              name="buyer"
              value={formData.buyer}
              onChange={handleChange}
              disabled
              className="w-full px-3 py-2 border border-gray-300 rounded-lg bg-gray-50 text-gray-600 font-mono text-sm"
            />
            <p className="text-xs text-gray-500 mt-1">Connected wallet address</p>
          </div>

          {/* Seller Address */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Seller Address *
            </label>
            <input
              type="text"
              name="seller"
              value={formData.seller}
              onChange={handleChange}
              placeholder="G..."
              className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            />
          </div>

          {/* Amount */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Amount (USDC) *
            </label>
            <div className="relative">
              <input
                type="number"
                name="amount"
                value={formData.amount}
                onChange={handleChange}
                placeholder="0.00"
                min="0"
                step="0.01"
                className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
              />
              <span className="absolute right-3 top-2 text-gray-500">USDC</span>
            </div>
          </div>

          {/* Order ID */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Order ID *
            </label>
            <input
              type="number"
              name="orderId"
              value={formData.orderId}
              onChange={handleChange}
              className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            />
          </div>

          {/* Release Window */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Release Window (days)
            </label>
            <select
              name="releaseWindow"
              value={formData.releaseWindow}
              onChange={(e) =>
                setFormData((prev) => ({
                  ...prev,
                  releaseWindow: parseInt(e.target.value),
                }))
              }
              className="w-full px-3 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-blue-500"
            >
              <option value={86400}>1 day</option>
              <option value={259200}>3 days</option>
              <option value={604800}>7 days</option>
              <option value={1209600}>14 days</option>
              <option value={2592000}>30 days</option>
            </select>
          </div>

          {/* Fee Preview */}
          {amountNum > 0 && (
            <div className="bg-gray-50 rounded-lg p-4 space-y-2">
              <h3 className="font-medium text-gray-700">Transaction Preview</h3>
              <div className="flex justify-between text-sm">
                <span className="text-gray-600">Amount:</span>
                <span className="font-medium">{formData.amount} USDC</span>
              </div>
              <div className="flex justify-between text-sm">
                <span className="text-gray-600">Platform Fee ({platformFeePercentage}%):</span>
                <span className="font-medium">{platformFee.toFixed(2)} USDC</span>
              </div>
              <div className="border-t pt-2 flex justify-between">
                <span className="text-gray-600">Seller Receives:</span>
                <span className="font-bold text-green-600">{sellerReceives.toFixed(2)} USDC</span>
              </div>
            </div>
          )}

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

          {/* Submit Button */}
          <button
            type="submit"
            disabled={isSubmitting || !walletState.isConnected}
            className="w-full px-4 py-3 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isSubmitting
              ? "Creating Escrow..."
              : showConfirm
              ? "Confirm & Create Escrow"
              : "Preview Escrow"}
          </button>

          {/* Cancel Confirmation */}
          {showConfirm && (
            <button
              type="button"
              onClick={() => setShowConfirm(false)}
              className="w-full mt-2 px-4 py-2 border border-gray-300 text-gray-700 rounded-lg font-medium hover:bg-gray-50 transition-colors"
            >
              Cancel
            </button>
          )}
        </form>
      )}

      {/* Contract Info */}
      <div className="mt-6 pt-4 border-t text-xs text-gray-500">
        <p>
          Network: <span className="font-medium">{STELLAR_NETWORK}</span>
        </p>
        <p className="font-mono break-all">
          Contract: {getEscrowContractAddress() || "Not configured"}
        </p>
      </div>
    </div>
  );
}

export default EscrowCreationForm;
