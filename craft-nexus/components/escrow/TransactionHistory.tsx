/**
 * Transaction History Component
 * Displays escrow-related transaction history
 */

"use client";

import { useState, useCallback } from "react";
import { STELLAR_NETWORK } from "@/lib/stellar/config";

export interface EscrowTransaction {
  id: string;
  orderId: number;
  type: "create" | "release" | "refund" | "auto_release";
  status: "pending" | "confirmed" | "failed";
  transactionHash?: string;
  timestamp: number;
  amount?: string;
  error?: string;
}

export interface TransactionHistoryProps {
  transactions: EscrowTransaction[];
  onRefresh?: () => void;
  className?: string;
}

export function TransactionHistory({
  transactions,
  onRefresh,
  className = "",
}: TransactionHistoryProps) {
  const [isLoading, setIsLoading] = useState(false);

  // Sort transactions by timestamp (newest first)
  const sortedTransactions = [...transactions].sort(
    (a, b) => b.timestamp - a.timestamp
  );

  const handleRefresh = useCallback(async () => {
    if (onRefresh) {
      setIsLoading(true);
      await onRefresh();
      setIsLoading(false);
    }
  }, [onRefresh]);

  // Format timestamp
  const formatTimestamp = (timestamp: number): string => {
    return new Date(timestamp).toLocaleString();
  };

  // Get transaction type label
  const getTypeLabel = (type: EscrowTransaction["type"]): string => {
    const labels = {
      create: "Create Escrow",
      release: "Release Funds",
      refund: "Refund",
      auto_release: "Auto-Release",
    };
    return labels[type];
  };

  // Get transaction type color
  const getTypeColor = (type: EscrowTransaction["type"]): string => {
    const colors = {
      create: "bg-blue-100 text-blue-800",
      release: "bg-green-100 text-green-800",
      refund: "bg-red-100 text-red-800",
      auto_release: "bg-purple-100 text-purple-800",
    };
    return colors[type];
  };

  // Get status color
  const getStatusColor = (status: EscrowTransaction["status"]): string => {
    const colors = {
      pending: "bg-yellow-100 text-yellow-800",
      confirmed: "bg-green-100 text-green-800",
      failed: "bg-red-100 text-red-800",
    };
    return colors[status];
  };

  // Get explorer URL
  const getExplorerUrl = (txHash: string): string => {
    return `https://stellar.explorer.expert/explorer/${STELLAR_NETWORK.toLowerCase()}/tx/${txHash}`;
  };

  // Empty state
  if (transactions.length === 0) {
    return (
      <div className={`bg-white rounded-lg p-6 text-center ${className}`}>
        <p className="text-gray-500">No transactions yet</p>
        <p className="text-sm text-gray-400 mt-1">
          Transaction history will appear here once you create an escrow
        </p>
      </div>
    );
  }

  return (
    <div className={`bg-white rounded-lg shadow-sm ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b border-gray-100">
        <h3 className="text-lg font-semibold text-gray-800">
          Transaction History
        </h3>
        {onRefresh && (
          <button
            onClick={handleRefresh}
            disabled={isLoading}
            className="text-sm text-blue-600 hover:text-blue-800 disabled:opacity-50"
          >
            {isLoading ? "Refreshing..." : "Refresh"}
          </button>
        )}
      </div>

      {/* Transaction List */}
      <div className="divide-y divide-gray-100">
        {sortedTransactions.map((tx) => (
          <div key={tx.id} className="p-4 hover:bg-gray-50 transition-colors">
            <div className="flex items-start justify-between">
              <div className="flex-1">
                {/* Type and Status */}
                <div className="flex items-center gap-2 mb-2">
                  <span
                    className={`px-2 py-0.5 rounded text-xs font-medium ${getTypeColor(
                      tx.type
                    )}`}
                  >
                    {getTypeLabel(tx.type)}
                  </span>
                  <span
                    className={`px-2 py-0.5 rounded text-xs font-medium ${getStatusColor(
                      tx.status
                    )}`}
                  >
                    {tx.status.charAt(0).toUpperCase() + tx.status.slice(1)}
                  </span>
                </div>

                {/* Order ID */}
                <p className="text-sm text-gray-600">
                  Order #{tx.orderId}
                </p>

                {/* Amount */}
                {tx.amount && (
                  <p className="text-sm font-medium text-gray-800 mt-1">
                    {tx.amount} USDC
                  </p>
                )}

                {/* Timestamp */}
                <p className="text-xs text-gray-400 mt-1">
                  {formatTimestamp(tx.timestamp)}
                </p>

                {/* Error Message */}
                {tx.error && (
                  <p className="text-xs text-red-500 mt-1">{tx.error}</p>
                )}
              </div>

              {/* Transaction Hash Link */}
              {tx.transactionHash && (
                <a
                  href={getExplorerUrl(tx.transactionHash)}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-xs text-blue-600 hover:underline font-mono ml-4"
                >
                  {tx.transactionHash.slice(0, 8)}...
                  {tx.transactionHash.slice(-6)}
                </a>
              )}
            </div>
          </div>
        ))}
      </div>

      {/* Footer */}
      <div className="p-4 border-t border-gray-100 text-xs text-gray-500">
        <p>
          View more on{" "}
          <a
            href={`https://stellar.explorer.expert/explorer/${STELLAR_NETWORK.toLowerCase()}`}
            target="_blank"
            rel="noopener noreferrer"
            className="text-blue-600 hover:underline"
          >
            Stellar Explorer
          </a>
        </p>
      </div>
    </div>
  );
}

/**
 * Hook for managing transaction history state
 */
export function useTransactionHistory(initialTransactions: EscrowTransaction[] = []) {
  const [transactions, setTransactions] = useState<EscrowTransaction[]>(initialTransactions);

  // Add a new transaction
  const addTransaction = useCallback((tx: Omit<EscrowTransaction, "id" | "timestamp">) => {
    const newTx: EscrowTransaction = {
      ...tx,
      id: `tx_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      timestamp: Date.now(),
    };
    setTransactions((prev) => [newTx, ...prev]);
    return newTx;
  }, []);

  // Update transaction status
  const updateTransactionStatus = useCallback(
    (id: string, status: EscrowTransaction["status"], error?: string) => {
      setTransactions((prev) =>
        prev.map((tx) =>
          tx.id === id ? { ...tx, status, error } : tx
        )
      );
    },
    []
  );

  // Clear all transactions
  const clearTransactions = useCallback(() => {
    setTransactions([]);
  }, []);

  return {
    transactions,
    addTransaction,
    updateTransactionStatus,
    clearTransactions,
  };
}

export default TransactionHistory;
