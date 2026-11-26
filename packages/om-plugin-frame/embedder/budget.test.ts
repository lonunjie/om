// SPDX-License-Identifier: MPL-2.0
import assert from "node:assert/strict";
import { test } from "node:test";
import { BudgetError, MAX_AGGREGATE_MEMORY_BYTES, MAX_INSTANCES, MAX_INSTANCE_MEMORY_BYTES, MIB, budgetSnapshot, reserveInstance } from "./budget.ts";

const refusal = (f: () => unknown): string | undefined => {
  try { f(); } catch (e) { return e instanceof BudgetError ? e.reason : "other"; }
  return undefined;
};

test("per-instance memory must be positive and within the cap", () => {
  assert.equal(refusal(() => reserveInstance(0)), "instance_memory");
  assert.equal(refusal(() => reserveInstance(MAX_INSTANCE_MEMORY_BYTES + 1)), "instance_memory");
  assert.equal(refusal(() => reserveInstance(1.5)), "instance_memory");
  assert.deepEqual(budgetSnapshot(), { instances: 0, memoryBytes: 0 });
});

test("at most four instances, and none is evicted to make room", () => {
  const releases = Array.from({ length: MAX_INSTANCES }, () => reserveInstance(4 * MIB));
  assert.equal(budgetSnapshot().instances, MAX_INSTANCES);
  assert.equal(refusal(() => reserveInstance(MIB)), "instance_limit");
  assert.equal(budgetSnapshot().instances, MAX_INSTANCES);
  releases[0]!();
  releases[0]!(); // idempotent
  assert.equal(budgetSnapshot().instances, MAX_INSTANCES - 1);
  const again = reserveInstance(MIB);
  again();
  for (const r of releases) r();
  assert.deepEqual(budgetSnapshot(), { instances: 0, memoryBytes: 0 });
});

test("aggregate memory is capped across instances", () => {
  const a = reserveInstance(MAX_INSTANCE_MEMORY_BYTES);
  const b = reserveInstance(MAX_INSTANCE_MEMORY_BYTES);
  const c = reserveInstance(MAX_INSTANCE_MEMORY_BYTES);
  assert.equal(budgetSnapshot().memoryBytes, MAX_AGGREGATE_MEMORY_BYTES);
  assert.equal(refusal(() => reserveInstance(MIB)), "aggregate_memory");
  a(); b(); c();
  assert.deepEqual(budgetSnapshot(), { instances: 0, memoryBytes: 0 });
});
