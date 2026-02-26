/**
 * Escrow Status Component
 * Displays the current status of an escrow transaction
 */

"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import {
  EscrowService,
  Escrow,
  EscrowStatusEnum,
} from "@/lib/stellar/escrow";
import { STELLAR_NETWORK } from "@/lib/stellar/config";

export interface EscrowStatusProps {
  orderId: number;
  refreshInterval?: number; // milliseconds
  onStatusChange?: (status: EscrowStatusEnum) => void;
  className?: string;
}

const statusLabels: Record<EscrowStatusEnum, string> = {
  [EscrowStatusEnum.Pending]: "Pending",
  [EscrowStatusEnum.Released]: "Released",
  [EscrowStatusEnum.Refunded]: "Refunded",
  [EscrowStatusEnum.Disputed]: "Disputed",
};

const statusColors: Record<EscrowStatusEnum, string> = {
  [EscrowStatusEnum.Pending]: "bg-yellow-100 text-yellow-800 border-yellow-200",
  [EscrowStatusEnum.Released]: "bg-green-100 text-green-800 border-green-200",
  [EscrowStatusEnum.Refunded]: "bg-red-100 text-red-800 border-red-200",
  [EscrowStatusEnum.Disputed]: "bg-purple-100 text-purple-800 border-purple-200",
};

export function EscrowStatus({
  orderId,
  refreshInterval = 30000, // Default 30 seconds
  onStatusChange,
  className = "",
}: EscrowStatusProps) {
  const escrowService = useMemo(() => new EscrowService(), []);
  const [escrow, setEscrow] = useState<Escrow | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  const fetchEscrowStatus = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await escrowService.getEscrow(orderId);
      setEscrow(result);
      setLastUpdated(new Date());

      if (result && onStatusChange) {
        onStatusChange(result.status);
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Failed to fetch escrow status";
      setError(errorMessage);
    } finally {
      setIsLoading(false);
    }
  }, [orderId, escrowService, onStatusChange]);

  // Initial fetch
  useEffect(() => {
    fetchEscrowStatus();
  }, [fetchEscrowStatus]);

  // Auto-refresh
  useEffect(() => {
    if (!escrow || escrow.status === EscrowStatusEnum.Released || escrow.status === EscrowStatusEnum.Refunded) {
      // Stop refreshing for completed escrows
      return;
    }

    const interval = setInterval(fetchEscrowStatus, refreshInterval);
    return () => clearInterval(interval);
  }, [escrow, refreshInterval, fetchEscrowStatus]);

  // Format timestamp
  const formatTimestamp = (timestamp: number): string => {
    if (!timestamp) return "N/A";
    return new Date(timestamp * 1000).toLocaleString();
  };

  // Calculate time remaining for auto-release
  const getTimeRemaining = (): string | null => {
    if (!escrow || escrow.status !== EscrowStatusEnum.Pending) return null;

    const now = Math.floor(Date.now() / 1000);
    const releaseTime = escrow.createdAt + escrow.releaseWindow;
    const remaining = releaseTime - now;

    if (remaining <= 0) return "Auto-release available";

    const days = Math.floor(remaining / 86400);
    const hours = Math.floor((remaining % 86400) / 3600);
    const minutes = Math.floor((remaining % 3600) / 60);

    if (days > 0) return `${days}d ${hours}h remaining`;
    if (hours > 0) return `${hours}h ${minutes}m remaining`;
    return `${minutes}m remaining`;
  };

  // Loading state
  if (isLoading && !escrow) {
    return (
      <div className={`animate-pulse ${className}`}>
        <div className="bg-gray-100 rounded-lg p-4 h-32"></div>
      </div>
    );
  }

  // Error state
  if (error && !escrow) {
    return (
      <div className={`bg-red-50 border border-red-200 rounded-lg p-4 ${className}`}>
        <p className="text-red-700 text-sm">{error}</p>
        <button
          onClick={fetchEscrowStatus}
          className="mt-2 text-sm text-red-600 hover:text-red-800 underline"
        >
          Try again
        </button>
      </div>
    );
  }

  // Not found state
  if (!escrow) {
    return (
      <div className={`bg-gray-50 border border-gray-200 rounded-lg p-4 ${className}`}>
        <p className="text-gray-600 text-sm">Escrow not found for order #{orderId}</p>
      </div>
    );
  }

  const statusEnum = escrow.status as EscrowStatusEnum;
  const timeRemaining = getTimeRemaining();

  return (
    <div className={`bg-white border border-gray-200 rounded-lg p-4 ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold text-gray-800">Escrow Status</h3>
        <span
          className={`px-3 py-1 rounded-full text-sm font-medium border ${statusColors[statusEnum]}`}
        >
          {statusLabels[statusEnum]}
        </span>
      </div>

      {/* Details Grid */}
      <div className="grid grid-cols-2 gap-4 text-sm">
        {/* Buyer */}
        <div>
          <p className="text-gray-500">Buyer</p>
          <p className="font-mono text-gray-800 break-all">{escrow.buyer}</p>
        </div>

        {/* Seller */}
        <div>
          <p className="text-gray-500">Seller</p>
          <p className="font-mono text-gray-800 break-all">{escrow.seller}</p>
        </div>

        {/* Amount */}
        <div>
          <p className="text-gray-500">Amount</p>
          <p className="font-semibold text-gray-800">{escrow.amount} USDC</p>
        </div>

        {/* Order ID */}
        <div>
          <p className="text-gray-500">Order ID</p>
          <p className="font-mono text-gray-800">#{orderId}</p>
        </div>

        {/* Created At */}
        <div>
          <p className="text-gray-500">Created</p>
          <p className="text-gray-800">{formatTimestamp(escrow.createdAt)}</p>
        </div>

        {/* Release Window */}
        <div>
          <p className="text-gray-500">Release Window</p>
          <p className="text-gray-800">{Math.floor(escrow.releaseWindow / 86400)} days</p>
        </div>
      </div>

      {/* Time Remaining (for pending escrows) */}
      {timeRemaining && (
        <div className="mt-4 pt-4 border-t border-gray-100">
          <div className="flex items-center justify-between">
            <span className="text-sm text-gray-600">Auto-release Timer</span>
            <span
              className={`text-sm font-medium ${
                timeRemaining === "Auto-release available" ? "text-green-600" : "text-orange-600"
              }`}
            >
              {timeRemaining}
            </span>
          </div>
        </div>
      )}

      {/* Last Updated */}
      <div className="mt-4 pt-3 border-t border-gray-100">
        <p className="text-xs text-gray-400">
          Last updated: {lastUpdated?.toLocaleTimeString() || "Loading..."}
        </p>
      </div>

      {/* Explorer Link */}
      <div className="mt-2">
        <a
          href={`https://stellar.explorer.expert/explorer/${STELLAR_NETWORK.toLowerCase()}/contract/${process.env.NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS}`}
          target="_blank"
          rel="noopener noreferrer"
          className="text-xs text-blue-600 hover:underline"
        >
          View on Stellar Explorer →
        </a>
      </div>
    </div>
  );
}

export default EscrowStatus;
