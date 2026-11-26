// SPDX-License-Identifier: MPL-2.0
// Reference PWA plugin host budgets (ADR-0268 decision 4). Host configuration, not wire values: the plugin wire sets none
// (ADR-0266 rule 11). A host may lower these; it never raises them past what its own measurements support.

export const MIB = 1_048_576;
export const MAX_INSTANCES = 4;
export const MAX_INSTANCE_MEMORY_BYTES = 16 * MIB;
export const MAX_AGGREGATE_MEMORY_BYTES = 48 * MIB;

export type BudgetRefusal = "instance_limit" | "instance_memory" | "aggregate_memory";

export class BudgetError extends Error {
  readonly reason: BudgetRefusal;
  constructor(reason: BudgetRefusal) {
    super(`plugin host budget: ${reason}`);
    this.reason = reason;
  }
}

const live = new Set<number>();
let next = 0;

/**
 * Reserves one running-instance slot with the given linear-memory budget, or throws a named refusal. Nothing running is
 * ever evicted to make room. Returns a release function (idempotent) to call when the instance closes.
 */
export function reserveInstance(maxMemoryBytes: number): () => void {
  if (!Number.isSafeInteger(maxMemoryBytes) || maxMemoryBytes <= 0 || maxMemoryBytes > MAX_INSTANCE_MEMORY_BYTES) {
    throw new BudgetError("instance_memory");
  }
  if (live.size >= MAX_INSTANCES) throw new BudgetError("instance_limit");
  if (reserved + maxMemoryBytes > MAX_AGGREGATE_MEMORY_BYTES) throw new BudgetError("aggregate_memory");
  const id = next++;
  live.add(id);
  sizes.set(id, maxMemoryBytes);
  reserved += maxMemoryBytes;
  return () => {
    if (!live.delete(id)) return;
    reserved -= sizes.get(id) ?? 0;
    sizes.delete(id);
  };
}

const sizes = new Map<number, number>();
let reserved = 0;

export function budgetSnapshot(): { readonly instances: number; readonly memoryBytes: number } {
  return { instances: live.size, memoryBytes: reserved };
}
