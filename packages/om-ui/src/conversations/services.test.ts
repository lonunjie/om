import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { CoreClient, OutcomeClass, ProfileExecutionContext } from "@om/platform-api";
import { settledOutcome } from "./services";
import type { ConversationServices } from "./services";

const OP = asOperationId("op-1");
const ctx = { profileId: asProfileId("p") } as unknown as ProfileExecutionContext;

/** A client whose only job is `observe`: `emit` pushes outcomes to the listener, `unsubscribed` records cleanup. */
function fake() {
  const state = { listener: null as null | ((o: OutcomeClass) => void), unsubscribed: 0 };
  const client = {
    observe: (_c: unknown, _o: unknown, l: (o: OutcomeClass) => void) => {
      state.listener = l;
      return () => { state.unsubscribed += 1; };
    },
  } as unknown as CoreClient;
  const services = { client, ctx, registry: {} } as unknown as ConversationServices;
  return { state, services, emit: (o: OutcomeClass) => state.listener?.(o) };
}

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

describe("settledOutcome (audit M5)", () => {
  it("resolves with the first outcome that is not waiting on the network and stops observing", async () => {
    const f = fake();
    const p = settledOutcome(f.services, OP);
    f.emit({ kind: "PendingNetwork" });
    f.emit({ kind: "Committed" });
    await expect(p).resolves.toEqual({ kind: "Committed" });
    expect(f.state.unsubscribed).toBe(1);
  });

  it("without a provider it settles on an honest pending state instead of waiting forever, and unsubscribes", async () => {
    const f = fake();
    const p = settledOutcome(f.services, OP, { timeoutMs: 5_000 });
    f.emit({ kind: "PendingNetwork" });
    await vi.advanceTimersByTimeAsync(5_000);
    await expect(p).resolves.toEqual({ kind: "PendingNetwork" });
    expect(f.state.unsubscribed).toBe(1);
  });

  it("settles on a pending state by default after a bounded wait", async () => {
    const f = fake();
    const p = settledOutcome(f.services, OP);
    await vi.advanceTimersByTimeAsync(60_000);
    await expect(p).resolves.toEqual({ kind: "PendingNetwork" });
  });

  it("an abort (for example an unmounted step) ends the wait and unsubscribes", async () => {
    const f = fake();
    const ac = new AbortController();
    const p = settledOutcome(f.services, OP, { signal: ac.signal });
    f.emit({ kind: "PendingReconciliation" });
    ac.abort();
    await expect(p).resolves.toEqual({ kind: "PendingReconciliation" });
    expect(f.state.unsubscribed).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("a late terminal outcome after the timeout is ignored", async () => {
    const f = fake();
    const p = settledOutcome(f.services, OP, { timeoutMs: 1_000 });
    await vi.advanceTimersByTimeAsync(1_000);
    f.emit({ kind: "Committed" });
    await expect(p).resolves.toEqual({ kind: "PendingNetwork" });
  });
});
