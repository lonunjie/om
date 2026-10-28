// The harness and the real Core are interchangeable behind CoreClient: the same call sequence yields the same outcome
// classes and the same view shapes. (Seeded fixtures and fake data stay harness-only.)
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asConversationId, asOperationId, asProfileId, isTerminalOutcome } from "@om/platform-api";
import type { CoreClient, OutcomeClass, ProfileExecutionContext } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createCoreRuntime, instantiateCore } from "./index";
import { createMemoryKv, createMemoryProvider } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm.wasm");
const op = asOperationId;
const PID = asProfileId("me");

interface Subject { readonly client: CoreClient; readonly ctx: ProfileExecutionContext; readonly settle: () => Promise<void> }

function harness(): Promise<Subject> {
  const h = createHarness({ env: {}, clock: () => 1000 });
  return Promise.resolve({ client: h, ctx: h.seedProfile({ profileId: PID, label: "pending" }), settle: () => Promise.resolve() });
}
async function wasm(): Promise<Subject> {
  const rt = await createCoreRuntime({ core: await instantiateCore(readFileSync(WASM)), provider: createMemoryProvider(), storage: () => createMemoryKv(), home: { tier: "T1", observers: [], migrationAvailable: false, clientOriginChoice: [] }, autoDrain: false });
  return { client: rt, ctx: await rt.openProfile(PID), settle: () => rt.flush(PID) };
}

const subjects: [string, () => Promise<Subject>][] = [["harness", harness], ...(existsSync(WASM) ? [["om-wasm", wasm] as [string, () => Promise<Subject>]] : [])];
const shape = (v: unknown): unknown => (Array.isArray(v) ? v.slice(0, 1).map(shape) : v !== null && typeof v === "object" ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, shape((v as Record<string, unknown>)[k])])) : typeof v);

async function terminal(s: Subject, id: string): Promise<OutcomeClass> {
  await s.settle();
  let last: OutcomeClass | undefined;
  s.client.observe(s.ctx, op(id), (o) => { last = o; })();
  if (!last || !isTerminalOutcome(last)) throw new Error(`${id} not terminal: ${JSON.stringify(last)}`);
  return last;
}

describe.each(subjects)("CoreClient contract: %s", (_name, make) => {
  it("onboarding intents and queries have the platform-api shapes and outcome classes", async () => {
    const s = await make();
    const { client, ctx } = s;
    await client.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: op("c1"), label: "Me" });
    expect(await terminal(s, "c1")).toEqual({ kind: "Committed" });
    const key = await client.query({ kind: "GetRecoveryKey", ctx, keyOperationId: op("c1") });
    expect(key).not.toBeNull();
    expect(Object.keys(key ?? {}).sort()).toEqual(["groups", "keyOperationId"]);
    await client.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx, operationId: op("c2"), keyOperationId: op("c1") });
    expect(await terminal(s, "c2")).toEqual({ kind: "Committed" });
    expect(await client.query({ kind: "GetRecoveryKey", ctx, keyOperationId: op("c1") })).toBeNull();
    expect(await client.query({ kind: "GetProfileList" })).toEqual([{ id: PID, label: "Me" }]);
    const devices = await client.query({ kind: "GetDevices", ctx });
    expect(Object.keys(devices.devices[0] ?? {}).sort()).toEqual(["actions", "cautions", "enrolledAt", "id", "isCurrent", "label", "lastSeen", "role", "state"]);
    expect(Object.keys(devices).sort()).toEqual(["controlState", "devices", "linkRequests", "notices"]);
    expect(shape(await client.query({ kind: "GetHomeSummary", ctx }))).toEqual({ tier: "string", observers: expect.anything() as unknown, migrationAvailable: "boolean", clientOriginChoice: expect.anything() as unknown });
    expect(Object.keys(await client.query({ kind: "GetRecoveryStatus", ctx })).sort()).toEqual(["activationDelayMs", "custodians", "policy", "recoveryKeySaved", "requests", "scopes"]);
  });

  it("errors, idempotency and context checks behave the same", async () => {
    const s = await make();
    const { client, ctx } = s;
    await client.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: op("c1"), label: "Me" });
    await client.dispatch({ kind: "SendMessage", ctx, operationId: op("m1"), conversationId: asConversationId("nope"), draft: { text: "x" } });
    expect(await terminal(s, "m1")).toEqual({ kind: "Rejected", reason: "not_found" });
    const a = await client.dispatch({ kind: "ClearAdaptiveNavigation", ctx, operationId: op("n1") });
    const b = await client.dispatch({ kind: "ClearAdaptiveNavigation", ctx, operationId: op("n1") });
    expect(b).toEqual(a);
    await expect(client.dispatch({ kind: "CreateInvite", ctx, operationId: op("n1") })).rejects.toThrow();
    const forged = Object.freeze({ profileId: PID, generation: "forged" as ProfileExecutionContext["generation"] });
    await expect(client.query({ kind: "GetDevices", ctx: forged })).rejects.toThrow();
    const ops = await client.pendingOperations(ctx);
    expect(ops.map((o) => o.operationId)).toEqual(expect.arrayContaining(["c1", "m1", "n1"]));
  });
});
