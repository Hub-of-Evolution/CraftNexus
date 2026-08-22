/**
 * Deterministic Fee Policy Engine (Frontend)
 *
 * Mirrors the on-chain Rust fee computation exactly using integer arithmetic.
 * Every settlement path produces the same allocation as the Soroban contract,
 * ensuring frontend previews and on-chain outcomes never diverge.
 */

export const FEE_POLICY_VERSION = 1;

export type SettlementKind =
  | { kind: "ReleaseFunds" }
  | { kind: "FullRefundNoFee" }
  | { kind: "ExpiredDisputeDeductFromSeller" }
  | { kind: "ExpiredDisputeDeductFromBuyer" }
  | { kind: "ExpiredDisputeSplitFee" }
  | { kind: "PartialRefund"; refundGross: number; sellerGross: number };

export interface FeeAllocation {
  platformFee: number;
  sellerAmount: number;
  buyerAmount: number;
}

export function calculateFee(amount: number, feeBps: number): number {
  if (amount < 0) {
    throw new Error("InvalidFee: amount must be non-negative");
  }
  return Math.floor((amount * feeBps) / 10_000);
}

export function computeFeeAllocation(
  escrowAmount: number,
  feeBps: number,
  kind: SettlementKind,
): FeeAllocation {
  if (escrowAmount < 0) {
    throw new Error("InvalidFee: escrowAmount must be non-negative");
  }

  let allocation: FeeAllocation;

  switch (kind.kind) {
    case "ReleaseFunds": {
      const platformFee = calculateFee(escrowAmount, feeBps);
      const sellerAmount = escrowAmount - platformFee;
      allocation = { platformFee, sellerAmount, buyerAmount: 0 };
      break;
    }
    case "FullRefundNoFee":
      allocation = { platformFee: 0, sellerAmount: 0, buyerAmount: escrowAmount };
      break;
    case "ExpiredDisputeDeductFromSeller":
      allocation = { platformFee: 0, sellerAmount: 0, buyerAmount: escrowAmount };
      break;
    case "ExpiredDisputeDeductFromBuyer": {
      const platformFee = calculateFee(escrowAmount, feeBps);
      const buyerAmount = escrowAmount - platformFee;
      allocation = { platformFee, sellerAmount: 0, buyerAmount };
      break;
    }
    case "ExpiredDisputeSplitFee": {
      const fullFee = calculateFee(escrowAmount, feeBps);
      const platformFee = Math.floor(fullFee / 2);
      const buyerAmount = escrowAmount - platformFee;
      allocation = { platformFee, sellerAmount: 0, buyerAmount };
      break;
    }
    case "PartialRefund": {
      const safeRefundGross = Math.max(0, kind.refundGross);
      const safeSellerGross = Math.max(0, kind.sellerGross);

      const refundFee = calculateFee(safeRefundGross, feeBps);
      const sellerFee = calculateFee(safeSellerGross, feeBps);

      const platformFee = refundFee + sellerFee;
      const buyerAmount = safeRefundGross - refundFee;
      const sellerAmount = safeSellerGross - sellerFee;

      allocation = { platformFee, sellerAmount, buyerAmount };
      break;
    }
    default:
      throw new Error(`Unknown settlement kind: ${(kind as { kind: string }).kind}`);
  }

  validateAllocation(allocation, escrowAmount);
  return allocation;
}

export function validateAllocation(
  allocation: FeeAllocation,
  escrowAmount: number,
): asserts allocation is FeeAllocation {
  const sum = allocation.platformFee + allocation.sellerAmount + allocation.buyerAmount;
  if (sum !== escrowAmount) {
    throw new Error(
      `InvalidFee: allocation ${JSON.stringify(allocation)} does not balance to escrow amount ${escrowAmount}`,
    );
  }
}

export function getEffectiveFeeBps(artisanCustomBps: number | null, platformDefaultBps: number): number {
  return artisanCustomBps !== null ? artisanCustomBps : platformDefaultBps;
}

export function previewReleaseFunds(amount: number, feeBps: number): FeeAllocation {
  return computeFeeAllocation(amount, feeBps, { kind: "ReleaseFunds" });
}

export function previewFullRefund(amount: number): FeeAllocation {
  return computeFeeAllocation(amount, 0, { kind: "FullRefundNoFee" });
}

export function previewExpiredDisputeAllocation(
  amount: number,
  feeBps: number,
  policy: "RefundFullNoPlatformFee" | "RefundMinusPlatformFee" | "DeductFeeFromSeller" | "SplitFee",
): FeeAllocation {
  switch (policy) {
    case "RefundFullNoPlatformFee":
    case "DeductFeeFromSeller":
      return computeFeeAllocation(amount, feeBps, { kind: "ExpiredDisputeDeductFromSeller" });
    case "RefundMinusPlatformFee":
      return computeFeeAllocation(amount, feeBps, { kind: "ExpiredDisputeDeductFromBuyer" });
    case "SplitFee":
      return computeFeeAllocation(amount, feeBps, { kind: "ExpiredDisputeSplitFee" });
    default:
      throw new Error(`Unknown expired dispute policy: ${policy}`);
  }
}

export function previewPartialRefund(
  escrowAmount: number,
  refundGross: number,
  sellerGross: number,
  feeBps: number,
): FeeAllocation {
  return computeFeeAllocation(escrowAmount, feeBps, { kind: "PartialRefund", refundGross, sellerGross });
}
