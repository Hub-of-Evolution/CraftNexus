/**
 * Environment Variables Validation
 * Validates required environment variables for escrow functionality
 */

export interface EnvValidationResult {
  isValid: boolean;
  errors: string[];
  warnings: string[];
}

export interface EscrowEnvConfig {
  escrowContractAddress: string;
  stellarNetwork: "TESTNET" | "PUBLIC";
  platformFeePercentage: number;
  isMockMode: boolean;
  apiUrl: string;
}

/**
 * Validate all required environment variables for escrow
 */
export function validateEscrowEnv(): EnvValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  // Check NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS
  const contractAddress = process.env.NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS;
  if (!contractAddress) {
    errors.push("NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS is not set");
  } else if (!isValidStellarAddress(contractAddress)) {
    errors.push("NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS is not a valid Stellar address");
  }

  // Check NEXT_PUBLIC_STELLAR_NETWORK
  const network = process.env.NEXT_PUBLIC_STELLAR_NETWORK;
  if (!network) {
    warnings.push("NEXT_PUBLIC_STELLAR_NETWORK not set, defaulting to TESTNET");
  } else if (network !== "TESTNET" && network !== "PUBLIC") {
    errors.push("NEXT_PUBLIC_STELLAR_NETWORK must be either 'TESTNET' or 'PUBLIC'");
  }

  // Check NEXT_PUBLIC_PLATFORM_FEE_PERCENTAGE
  const feePercentage = process.env.NEXT_PUBLIC_PLATFORM_FEE_PERCENTAGE;
  if (feePercentage) {
    const fee = parseFloat(feePercentage);
    if (isNaN(fee) || fee < 0 || fee > 100) {
      errors.push("NEXT_PUBLIC_PLATFORM_FEE_PERCENTAGE must be a number between 0 and 100");
    }
  }

  // Check NEXT_PUBLIC_API_URL
  const apiUrl = process.env.NEXT_PUBLIC_API_URL;
  if (apiUrl && !isValidUrl(apiUrl)) {
    warnings.push("NEXT_PUBLIC_API_URL is not a valid URL");
  }

  return {
    isValid: errors.length === 0,
    errors,
    warnings,
  };
}

/**
 * Get escrow environment configuration
 */
export function getEscrowEnvConfig(): EscrowEnvConfig {
  const contractAddress = process.env.NEXT_PUBLIC_ESCROW_CONTRACT_ADDRESS || "";
  const network = process.env.NEXT_PUBLIC_STELLAR_NETWORK || "TESTNET";
  const feePercentage = process.env.NEXT_PUBLIC_PLATFORM_FEE_PERCENTAGE || "5";
  const mockMode = process.env.NEXT_PUBLIC_ESCROW_MOCK_MODE === "true";
  const apiUrl = process.env.NEXT_PUBLIC_API_URL || "/api";

  return {
    escrowContractAddress: contractAddress,
    stellarNetwork: network as "TESTNET" | "PUBLIC",
    platformFeePercentage: parseFloat(feePercentage),
    isMockMode: mockMode,
    apiUrl,
  };
}

/**
 * Validate Stellar address format
 */
function isValidStellarAddress(address: string): boolean {
  // Stellar addresses start with G and are 56 characters long
  const stellarAddressRegex = /^G[A-Z0-9]{55}$/;
  return stellarAddressRegex.test(address);
}

/**
 * Validate URL format
 */
function isValidUrl(url: string): boolean {
  try {
    new URL(url);
    return true;
  } catch {
    return false;
  }
}

/**
 * Log validation results to console
 */
export function logEnvValidation(): void {
  const result = validateEscrowEnv();

  if (result.errors.length > 0) {
    console.error("❌ Escrow Environment Validation Failed:");
    result.errors.forEach((error) => console.error(`  - ${error}`));
  }

  if (result.warnings.length > 0) {
    console.warn("⚠️ Escrow Environment Warnings:");
    result.warnings.forEach((warning) => console.warn(`  - ${warning}`));
  }

  if (result.isValid && result.warnings.length === 0) {
    console.log("✅ Escrow Environment Validation Passed");
  }
}

const envValidationModule = {
  validateEscrowEnv,
  getEscrowEnvConfig,
  logEnvValidation,
};

export default envValidationModule;
