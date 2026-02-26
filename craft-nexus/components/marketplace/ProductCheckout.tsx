/**
 * Product Checkout Component
 * Integrated escrow checkout for marketplace purchases
 */

"use client";

import { useState, useCallback, useMemo } from "react";
import Image from "next/image";
import {
  EscrowCreationForm,
} from "@/components/escrow/EscrowCreationForm";
import {
  EscrowStatus,
} from "@/components/escrow/EscrowStatus";
import {
  EscrowActions,
} from "@/components/escrow/EscrowActions";
import {
  TransactionHistory,
  useTransactionHistory,
  EscrowTransaction,
} from "@/components/escrow/TransactionHistory";
import { useEscrowWallet } from "@/components/wallet/EscrowWalletIntegration";
import {
  EscrowService,
  Escrow,
  EscrowStatusEnum,
  getPlatformFeePercentage,
} from "@/lib/stellar/escrow";
import { STELLAR_NETWORK, PLATFORM_COMMISSION_PERCENT } from "@/lib/stellar/config";

export interface Product {
  id: string;
  name: string;
  description: string;
  price: number;
  sellerAddress: string;
  sellerName?: string;
  imageUrl?: string;
}

export interface ProductCheckoutProps {
  product: Product;
  onSuccess?: (result: {
    orderId: number;
    transactionHash: string;
    escrow: Escrow;
  }) => void;
  onError?: (error: string) => void;
  className?: string;
}

export function ProductCheckout({
  product,
  onSuccess,
  onError,
  className = "",
}: ProductCheckoutProps) {
  const { walletState } = useEscrowWallet();
  const escrowService = useMemo(() => new EscrowService(), []);
  const { transactions, addTransaction } = useTransactionHistory();

  const [checkoutStep, setCheckoutStep] = useState<
    "details" | "escrow" | "status" | "complete"
  >("details");
  const [currentEscrow, setCurrentEscrow] = useState<Escrow | null>(null);
  const [orderId, setOrderId] = useState<number | null>(null);

  // Calculate fees
  const platformFeePercentage = getPlatformFeePercentage();
  const platformFee = (product.price * platformFeePercentage) / 100;
  const sellerReceives = product.price - platformFee;

  // Handle successful escrow creation
  const handleEscrowSuccess = useCallback(
    async (result: { orderId: number; transactionHash: string }) => {
      // Add transaction to history
      addTransaction({
        orderId: result.orderId,
        type: "create",
        status: result.transactionHash ? "confirmed" : "pending",
        transactionHash: result.transactionHash,
        amount: product.price.toString(),
      });

      setOrderId(result.orderId);

      // Fetch the created escrow
      const escrow = await escrowService.getEscrow(result.orderId);
      setCurrentEscrow(escrow);
      setCheckoutStep("status");

      if (onSuccess) {
        onSuccess({
          orderId: result.orderId,
          transactionHash: result.transactionHash,
          escrow: escrow!,
        });
      }
    },
    [addTransaction, escrowService, product.price, onSuccess]
  );

  // Handle escrow action success (release, refund, auto-release)
  const handleActionSuccess = useCallback(
    async (
      action: string,
      result: { transactionHash?: string }
    ) => {
      // Add transaction to history
      if (orderId) {
        const txType = action as EscrowTransaction["type"];
        addTransaction({
          orderId,
          type: txType,
          status: result.transactionHash ? "confirmed" : "pending",
          transactionHash: result.transactionHash,
        });

        // Refresh escrow status
        const escrow = await escrowService.getEscrow(orderId);
        setCurrentEscrow(escrow);

        // If released or refunded, mark as complete
        if (escrow && (escrow.status === EscrowStatusEnum.Released || escrow.status === EscrowStatusEnum.Refunded)) {
          setCheckoutStep("complete");
        }
      }
    },
    [addTransaction, escrowService, orderId]
  );

  // Handle escrow action error
  const handleActionError = useCallback(
    (error: string) => {
      if (onError) {
        onError(error);
      }
    },
    [onError]
  );

  return (
    <div className={`bg-white rounded-lg shadow-md ${className}`}>
      {/* Step Indicator */}
      <div className="border-b border-gray-200 p-4">
        <div className="flex items-center justify-between">
          {["Details", "Escrow", "Status", "Complete"].map((step, index) => {
            const stepKey = ["details", "escrow", "status", "complete"][index];
            const isActive = checkoutStep === stepKey;
            const isComplete = ["details", "escrow", "status", "complete"].indexOf(checkoutStep) > index;

            return (
              <div key={step} className="flex items-center">
                <div
                  className={`w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium ${
                    isComplete
                      ? "bg-green-500 text-white"
                      : isActive
                      ? "bg-blue-500 text-white"
                      : "bg-gray-200 text-gray-500"
                  }`}
                >
                  {isComplete ? "✓" : index + 1}
                </div>
                <span
                  className={`ml-2 text-sm ${
                    isActive ? "text-blue-600 font-medium" : "text-gray-500"
                  }`}
                >
                  {step}
                </span>
                {index < 3 && <div className="w-8 h-0.5 bg-gray-200 mx-2" />}
              </div>
            );
          })}
        </div>
      </div>

      {/* Product Details */}
      {checkoutStep === "details" && (
        <div className="p-6">
          <div className="flex gap-4 mb-6">
            {product.imageUrl && (
              <div className="w-24 h-24 relative flex-shrink-0">
                <Image
                  src={product.imageUrl}
                  alt={product.name}
                  fill
                  className="object-cover rounded-lg"
                />
              </div>
            )}
            <div>
              <h2 className="text-xl font-bold text-gray-800">
                {product.name}
              </h2>
              <p className="text-gray-600 text-sm mt-1">
                {product.description}
              </p>
              {product.sellerName && (
                <p className="text-sm text-gray-500 mt-1">
                  Seller: {product.sellerName}
                </p>
              )}
            </div>
          </div>

          {/* Price Breakdown */}
          <div className="bg-gray-50 rounded-lg p-4 mb-6">
            <h3 className="font-medium text-gray-700 mb-3">Price Breakdown</h3>
            <div className="space-y-2 text-sm">
              <div className="flex justify-between">
                <span className="text-gray-600">Product Price</span>
                <span className="font-medium">${product.price.toFixed(2)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-600">
                  Platform Fee ({platformFeePercentage}%)
                </span>
                <span className="font-medium">${platformFee.toFixed(2)}</span>
              </div>
              <div className="border-t pt-2 flex justify-between">
                <span className="text-gray-600">Total</span>
                <span className="font-bold text-lg">
                  ${product.price.toFixed(2)}
                </span>
              </div>
              <div className="flex justify-between text-green-600">
                <span>Seller Receives</span>
                <span className="font-medium">${sellerReceives.toFixed(2)}</span>
              </div>
            </div>
          </div>

          {/* Escrow Info */}
          <div className="bg-blue-50 rounded-lg p-4 mb-6">
            <h3 className="font-medium text-blue-800 mb-2">
              Secure Escrow Payment
            </h3>
            <p className="text-sm text-blue-700">
              Your payment will be held in escrow until you confirm the order
              is complete. This protects both buyers and sellers.
            </p>
          </div>

          {/* Proceed Button */}
          <button
            onClick={() => setCheckoutStep("escrow")}
            className="w-full px-4 py-3 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 transition-colors"
          >
            Proceed to Escrow
          </button>
        </div>
      )}

      {/* Escrow Creation */}
      {checkoutStep === "escrow" && (
        <div className="p-6">
          <EscrowCreationForm
            defaultSeller={product.sellerAddress}
            defaultOrderId={Math.floor(Math.random() * 1000000)}
            defaultAmount={product.price.toString()}
            onSuccess={handleEscrowSuccess}
            onError={onError}
          />

          <button
            onClick={() => setCheckoutStep("details")}
            className="w-full mt-4 text-gray-600 hover:text-gray-800 text-sm"
          >
            ← Back to Details
          </button>
        </div>
      )}

      {/* Escrow Status */}
      {checkoutStep === "status" && orderId && currentEscrow && (
        <div className="p-6">
          <EscrowStatus
            orderId={orderId}
            onStatusChange={(status) => {
              if (status === EscrowStatusEnum.Released || status === EscrowStatusEnum.Refunded) {
                setCheckoutStep("complete");
              }
            }}
          />

          <div className="mt-6">
            <h3 className="font-medium text-gray-700 mb-3">Actions</h3>
            <EscrowActions
              escrow={currentEscrow}
              orderId={orderId}
              currentUserAddress={walletState.publicKey || ""}
              onSuccess={handleActionSuccess}
              onError={handleActionError}
            />
          </div>
        </div>
      )}

      {/* Complete */}
      {checkoutStep === "complete" && (
        <div className="p-6 text-center">
          <div className="w-16 h-16 bg-green-100 rounded-full flex items-center justify-center mx-auto mb-4">
            <span className="text-3xl text-green-600">✓</span>
          </div>
          <h2 className="text-xl font-bold text-gray-800 mb-2">
            Order Complete!
          </h2>
          <p className="text-gray-600 mb-6">
            {currentEscrow?.status === EscrowStatusEnum.Released
              ? "Funds have been released to the seller."
              : "The order has been refunded."}
          </p>

          <button
            onClick={() => {
              setCheckoutStep("details");
              setCurrentEscrow(null);
              setOrderId(null);
            }}
            className="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700 transition-colors"
          >
            Start New Order
          </button>
        </div>
      )}

      {/* Transaction History (always visible except on details) */}
      {checkoutStep !== "details" && transactions.length > 0 && (
        <div className="border-t border-gray-200 p-4">
          <TransactionHistory transactions={transactions} />
        </div>
      )}

      {/* Network Info */}
      <div className="border-t border-gray-200 p-4 text-xs text-gray-500">
        <p>
          Network: <span className="font-medium">{STELLAR_NETWORK}</span>
        </p>
        <p>
          Platform Fee: <span className="font-medium">{PLATFORM_COMMISSION_PERCENT}%</span>
        </p>
      </div>
    </div>
  );
}

export default ProductCheckout;
