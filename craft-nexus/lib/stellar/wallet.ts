/**
 * Stellar Wallet Service
 * Handles wallet connections and account management
 */

import { 
  isConnected, 
  requestAccess,
  getAddress
} from "@stellar/freighter-api";

export interface WalletAccount {
  publicKey: string;
  isConnected: boolean;
}

/**
 * Check if Freighter extension is installed and available
 * Uses the official Freighter API method (recommended approach)
 * Don't check window object - let the API handle detection
 */
export async function isFreighterAvailable(): Promise<boolean> {
  try {
    if (typeof window === "undefined") return false;
    
    // Use the official API method - it handles detection internally
    // This is the recommended way per Freighter documentation
    const connected = await Promise.race([
      isConnected(),
      new Promise<{ isConnected: boolean }>((_, reject) =>
        setTimeout(() => reject(new Error("Timeout")), 3000)
      )
    ]);
    
    console.log("Freighter: API check result:", connected);
    return connected.isConnected;
  } catch (error) {
    // If isConnected() fails, Freighter is likely not installed
    // But we'll still allow connection attempts (requestAccess might work)
    console.warn("Freighter: isConnected() failed:", error);
    return false;
  }
}

/**
 * Check if Freighter extension is installed by checking the window object
 * This is a fallback method, but not reliable - use isFreighterAvailable() instead
 */
export function isFreighterExtensionInstalled(): boolean {
  // Note: This is not a reliable check - Freighter may not expose window.freighterApi
  // We keep this for backward compatibility but prefer API-based detection
  if (typeof window === "undefined") return false;
  
  // The @stellar/freighter-api package handles window detection internally
  // Direct window checks are unreliable, so we return false here
  // and let the API methods handle detection
  return false;
}

/**
 * Connect to Freighter wallet
 * Requests access from the user and returns their public key
 * This method will attempt connection even if initial detection fails
 */
export async function connectFreighterWallet(): Promise<WalletAccount | null> {
  try {
    console.log("Freighter: Attempting to connect...");

    // Try to check if connected first (optional check)
    let isConnectedCheck = false;
    try {
      const connectedStatus = await Promise.race([
        isConnected(),
        new Promise<{ isConnected: boolean }>((_, reject) =>
          setTimeout(() => reject(new Error("Check timeout")), 2000)
        )
      ]);
      isConnectedCheck = connectedStatus.isConnected;
      console.log("Freighter: isConnected check:", isConnectedCheck);
    } catch (checkError) {
      console.warn("Freighter: isConnected check failed, but continuing to requestAccess:", checkError);
      // Continue anyway - requestAccess might still work
    }

    // Request access from user (this is the primary method - it will work if Freighter is installed)
    // This will prompt the user if not already granted, and will throw an error if Freighter is not installed
    console.log("Freighter: Requesting access...");
    
    const access = await Promise.race([
      requestAccess(),
      new Promise<{ address?: string; error?: string }>((_, reject) =>
        setTimeout(() => reject(new Error("Request access timeout. Freighter may not be installed or is not responding.")), 15000)
      )
    ]);
    
    console.log("Freighter: Access response:", access);
    
    // Handle errors from requestAccess
    if (access.error) {
      // Check for common error messages
      if (access.error.includes("not installed") || access.error.includes("not found")) {
        throw new Error("Freighter extension not installed. Please install it from https://freighter.app and refresh this page.");
      }
      if (access.error.includes("denied") || access.error.includes("rejected")) {
        throw new Error("Access denied. Please allow Freighter to connect to this site.");
      }
      if (access.error.includes("locked") || access.error.includes("unlock")) {
        throw new Error("Please unlock your Freighter wallet and try again.");
      }
      throw new Error(access.error);
    }

    if (!access.address) {
      throw new Error("Failed to get wallet address. Please make sure your Freighter wallet is unlocked and try again.");
    }

    console.log("Freighter: Successfully connected:", access.address);

    return {
      publicKey: access.address,
      isConnected: true,
    };
  } catch (error) {
    console.error("Failed to connect Freighter wallet:", error);
    
    // Provide more helpful error messages
    if (error instanceof Error) {
      if (error.message.includes("timeout")) {
        throw new Error("Freighter is taking too long to respond. Please:\n1. Make sure Freighter is installed\n2. Refresh this page\n3. Try connecting again");
      }
      if (error.message.includes("not installed") || error.message.includes("not found")) {
        throw new Error("Freighter extension not detected. Please:\n1. Install Freighter from https://freighter.app\n2. Refresh this page\n3. Try connecting again");
      }
      throw error;
    }
    
    throw new Error("Failed to connect wallet. Please check the browser console for details.");
  }
}

/**
 * Get the current connected address without requesting access
 * Returns null if not connected or access not granted
 */
export async function getCurrentAddress(): Promise<string | null> {
  try {
    const connected = await isConnected();
    if (!connected.isConnected) {
      return null;
    }

    const addressObj = await getAddress();
    if (addressObj.error) {
      return null;
    }

    return addressObj.address || null;
  } catch (error) {
    console.error("Failed to get current address:", error);
    return null;
  }
}

/**
 * Disconnect wallet (clear local state)
 */
export function disconnectWallet(): void {
  // Note: Freighter doesn't have a disconnect method
  // This is just for local state management
  localStorage.removeItem("craftnexus_wallet");
  localStorage.removeItem("craftnexus_wallet_publicKey");
}

/**
 * Get stored wallet account from localStorage
 */
export function getStoredWallet(): WalletAccount | null {
  if (typeof window === "undefined") return null;
  
  const stored = localStorage.getItem("craftnexus_wallet_publicKey");
  if (stored) {
    return {
      publicKey: stored,
      isConnected: false, // Need to verify connection
    };
  }
  return null;
}

/**
 * Store wallet account in localStorage
 */
export function storeWallet(account: WalletAccount): void {
  if (typeof window === "undefined") return;
  localStorage.setItem("craftnexus_wallet", JSON.stringify(account));
  localStorage.setItem("craftnexus_wallet_publicKey", account.publicKey);
}

/**
 * Check if Freighter extension is installed
 * This is an alias for isFreighterAvailable for backward compatibility
 */
export async function isFreighterInstalled(): Promise<boolean> {
  return await isFreighterAvailable();
}
