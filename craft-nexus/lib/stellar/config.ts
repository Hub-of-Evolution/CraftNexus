/**
 * Stellar Network Configuration
 * Configure for TESTNET or PUBLIC network
 */

export const STELLAR_NETWORK = process.env.NEXT_PUBLIC_STELLAR_NETWORK || "TESTNET";

// USDC Issuer on Stellar
// Testnet: Circle Testnet USDC
// Mainnet: Circle USDC
export const USDC_ISSUER = 
  STELLAR_NETWORK === "PUBLIC"
    ? "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN" // Mainnet USDC
    : "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"; // Testnet USDC (same for now)

// Horizon Server URLs
export const HORIZON_URL =
  STELLAR_NETWORK === "PUBLIC"
    ? "https://horizon.stellar.org"
    : "https://horizon-testnet.stellar.org";

// Soroban RPC URLs for Smart Contracts
export const SOROBAN_RPC_URL =
  STELLAR_NETWORK === "PUBLIC"
    ? "https://soroban-rpc.mainnet.stellar.org"
    : "https://soroban-testnet.stellar.org";

export const NETWORK_PASSPHRASE =
  STELLAR_NETWORK === "PUBLIC"
    ? "Public Global Stellar Network ; September 2015"
    : "Test SDF Network ; September 2015";

// Platform commission wallet (should be set in env)
export const PLATFORM_COMMISSION_WALLET = 
  process.env.NEXT_PUBLIC_PLATFORM_WALLET || "";

export const PLATFORM_COMMISSION_PERCENT = 5; // 5% commission
