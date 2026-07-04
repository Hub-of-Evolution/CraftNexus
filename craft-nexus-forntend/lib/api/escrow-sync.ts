/**
 * Backend Sync Service
 * Handles syncing escrow state with the backend API
 */

"use client";

import { Escrow, EscrowStatusEnum } from "@/lib/stellar/escrow";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "/api";

export interface BackendEscrowOrder {
  id: number;
  productId: string;
  buyerAddress: string;
  sellerAddress: string;
  amount: string;
  status: "pending" | "funded" | "released" | "refunded" | "disputed";
  escrowAddress?: string;
  transactionHash?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateEscrowOrderRequest {
  productId: string;
  orderId: number;
  buyerAddress: string;
  sellerAddress: string;
  amount: string;
  transactionHash?: string;
}

export interface UpdateEscrowOrderRequest {
  status?: BackendEscrowOrder["status"];
  transactionHash?: string;
  escrowAddress?: string;
}

/**
 * Map escrow status to backend status
 */
function mapEscrowStatusToBackend(status: EscrowStatusEnum): BackendEscrowOrder["status"] {
  switch (status) {
    case EscrowStatusEnum.Pending:
      return "pending";
    case EscrowStatusEnum.Released:
      return "released";
    case EscrowStatusEnum.Refunded:
      return "refunded";
    case EscrowStatusEnum.Disputed:
      return "disputed";
    default:
      return "pending";
  }
}

/**
 * Map backend status to escrow status
 */
function mapBackendStatusToEscrow(status: BackendEscrowOrder["status"]): EscrowStatusEnum {
  switch (status) {
    case "funded":
      return EscrowStatusEnum.Pending;
    case "released":
      return EscrowStatusEnum.Released;
    case "refunded":
      return EscrowStatusEnum.Refunded;
    case "disputed":
      return EscrowStatusEnum.Disputed;
    default:
      return EscrowStatusEnum.Pending;
  }
}

/**
 * Create a new escrow order in the backend
 */
export async function createEscrowOrder(
  data: CreateEscrowOrderRequest
): Promise<BackendEscrowOrder> {
  const response = await fetch(`${API_BASE_URL}/orders/escrow`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(data),
  });

  if (!response.ok) {
    throw new Error(`Failed to create escrow order: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Get an escrow order by order ID
 */
export async function getEscrowOrder(orderId: number): Promise<BackendEscrowOrder | null> {
  try {
    const response = await fetch(`${API_BASE_URL}/orders/escrow/${orderId}`);

    if (response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(`Failed to get escrow order: ${response.statusText}`);
    }

    return response.json();
  } catch (error) {
    console.error("Error fetching escrow order:", error);
    return null;
  }
}

/**
 * Update an escrow order in the backend
 */
export async function updateEscrowOrder(
  orderId: number,
  data: UpdateEscrowOrderRequest
): Promise<BackendEscrowOrder> {
  const response = await fetch(`${API_BASE_URL}/orders/escrow/${orderId}`, {
    method: "PATCH",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(data),
  });

  if (!response.ok) {
    throw new Error(`Failed to update escrow order: ${response.statusText}`);
  }

  return response.json();
}

/**
 * Sync escrow status from blockchain to backend
 */
export async function syncEscrowToBackend(
  orderId: number,
  escrow: Escrow,
  transactionHash?: string
): Promise<void> {
  const backendStatus = mapEscrowStatusToBackend(escrow.status);

  try {
    await updateEscrowOrder(orderId, {
      status: backendStatus,
      transactionHash,
    });
    console.log(`Synced escrow ${orderId} status to backend: ${backendStatus}`);
  } catch (error) {
    console.error("Failed to sync escrow to backend:", error);
    // Don't throw - sync failures shouldn't block the user
  }
}

/**
 * Fetch escrow status from backend
 */
export async function fetchEscrowFromBackend(
  orderId: number
): Promise<{ status: EscrowStatusEnum; lastSyncedAt: Date } | null> {
  const order = await getEscrowOrder(orderId);

  if (!order) {
    return null;
  }

  return {
    status: mapBackendStatusToEscrow(order.status),
    lastSyncedAt: new Date(order.updatedAt),
  };
}

/**
 * Webhook handler for escrow confirmations
 * This would be called by the backend when it receives a webhook from the blockchain
 */
export async function handleEscrowWebhook(
  payload: {
    orderId: number;
    status: BackendEscrowOrder["status"];
    transactionHash?: string;
  }
): Promise<{ success: boolean; message: string }> {
  try {
    const { orderId, status } = payload;

    // Update the local state based on webhook data
    // In a real implementation, this would update a local cache or store
    console.log(`Webhook received for order ${orderId}: ${status}`);

    return {
      success: true,
      message: `Order ${orderId} updated to ${status}`,
    };
  } catch (error) {
    console.error("Webhook processing failed:", error);
    return {
      success: false,
      message: error instanceof Error ? error.message : "Unknown error",
    };
  }
}

/**
 * Check if backend is available
 */
export async function checkBackendHealth(): Promise<boolean> {
  try {
    const response = await fetch(`${API_BASE_URL}/health`, {
      method: "GET",
      headers: {
        "Content-Type": "application/json",
      },
    });
    return response.ok;
  } catch {
    return false;
  }
}

const escrowSyncModule = {
  createEscrowOrder,
  getEscrowOrder,
  updateEscrowOrder,
  syncEscrowToBackend,
  fetchEscrowFromBackend,
  handleEscrowWebhook,
  checkBackendHealth,
};

export default escrowSyncModule;
