/**
 * Append-only fund-movement audit records.
 *
 * The client store is intentionally immutable: records are frozen, keyed by a
 * generated id, and sequence numbers are allocated monotonically per account.
 * A successful network transaction must be recorded only after submission.
 */

export type FundMovementKind =
  | "transfer"
  | "refund"
  | "release"
  | "stake"
  | "recovery";

export interface FundMovementAuditRecord {
  readonly id: string;
  readonly sequence: number;
  readonly kind: FundMovementKind;
  readonly actor: string;
  readonly account: string;
  readonly asset: string;
  readonly amount: string;
  readonly reason: string;
  readonly transactionHash: string;
  readonly occurredAt: string;
}

const recordsByAccount = new Map<string, FundMovementAuditRecord[]>();
const nextSequenceByAccount = new Map<string, number>();
const recordIds = new Set<string>();

function accountKey(account: string): string {
  return account.trim().toLowerCase();
}

function cloneRecord(record: FundMovementAuditRecord): FundMovementAuditRecord {
  return Object.freeze({ ...record });
}

/** Record one successfully completed movement. Records cannot be edited or reused. */
export function recordFundMovement(input: Omit<FundMovementAuditRecord, "id" | "sequence" | "occurredAt">): FundMovementAuditRecord {
  const key = accountKey(input.account);
  if (!key || !input.actor || !input.asset || !input.amount || !input.reason || !input.transactionHash) {
    throw new Error("Audit record requires account, actor, asset, amount, reason, and transaction hash");
  }

  const sequence = nextSequenceByAccount.get(key) ?? 1;
  const record = cloneRecord({
    ...input,
    id: `${key}:${sequence}:${input.transactionHash}`,
    sequence,
    occurredAt: new Date().toISOString(),
  });

  const existing = recordsByAccount.get(key) ?? [];
  if (recordIds.has(record.id) || existing.some((item) => item.id === record.id)) {
    throw new Error("Audit record already exists");
  }
  recordIds.add(record.id);
  recordsByAccount.set(key, [...existing, record]);
  nextSequenceByAccount.set(key, sequence + 1);
  return record;
}

/** Return a defensive copy ordered by the stable per-account sequence. */
export function getFundMovementHistory(account: string): FundMovementAuditRecord[] {
  return [...(recordsByAccount.get(accountKey(account)) ?? [])].sort((a, b) => a.sequence - b.sequence);
}

/** Test/support utility; production callers should never need to clear audit data. */
export function clearFundMovementAuditForTests(): void {
  recordsByAccount.clear();
  nextSequenceByAccount.clear();
  recordIds.clear();
}
