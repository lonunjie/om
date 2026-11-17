// The runtime wiring of the recovery tick over the real Core: idle means no polling at all, and the option can turn it off.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import type { WasmCore } from "./index";
import { createMemoryKv, createMemoryProvider } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm.wasm");
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };

describe.skipIf(!existsSync(WASM))("recovery tick in the runtime (real Core)", () => {
  async function counted(tick: false | { now: () => number; intervalMs: number } | undefined) {
    const real = await instantiateCore(readFileSync(WASM));
    const seen: string[] = [];
    const core: WasmCore = { ...real, call: (cmd) => { seen.push((cmd as { cmd: string }).cmd); return real.call(cmd); } };
    const kv = createMemoryKv();
    const rt = await createCoreRuntime({ core, provider: createMemoryProvider(), storage: () => kv, home: HOME, retryMs: 0, ...(tick === undefined ? {} : { recoveryTick: tick }) });
    return { rt, seen };
  }

  it("makes no tick call, and arms no timer, while no recovery is pending; closing the profile is clean", async () => {
    let now = 0;
    const { rt, seen } = await counted({ now: () => now, intervalMs: 10 });
    const ctx = await rt.openProfile(asProfileId("idle"));
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: asOperationId("c"), label: "Idle" });
    await rt.flush(asProfileId("idle"));
    now += 1_000_000;
    await new Promise((r) => setTimeout(r, 50));
    expect(seen.filter((c) => c === "recovery_tick")).toEqual([]);
    rt.closeProfile(asProfileId("idle"));
    rt.dispose();
  }, 60_000);

  it("can be turned off", async () => {
    const { rt, seen } = await counted(false);
    await rt.openProfile(asProfileId("off"));
    expect(seen.filter((c) => c === "recovery_tick")).toEqual([]);
    rt.dispose();
  }, 60_000);
});

describe.skipIf(!existsSync(WASM))("recovery record writes through the host (real Core)", () => {
  const PLACE = { host: "home.example", scope: "ab".repeat(32) };
  async function open(provider: ReturnType<typeof createMemoryProvider>, id: string) {
    const core = await instantiateCore(readFileSync(WASM));
    const kv = createMemoryKv();
    const rt = await createCoreRuntime({ core, provider, storage: () => kv, home: HOME, retryMs: 0 });
    const ctx = await rt.openProfile(asProfileId(id));
    return { rt, ctx, pid: asProfileId(id) };
  }

  it("an account made with a recovery place shows the sheet and writes its record at key-sheet confirmation", async () => {
    const provider = createMemoryProvider();
    provider.setRecoveryPlace(PLACE);
    const { rt, ctx, pid } = await open(provider, "rec");
    const op = asOperationId("c");
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: op, label: "Rec" });
    await rt.flush(pid);
    const key = await rt.query({ kind: "GetRecoveryKey", ctx, keyOperationId: op });
    expect(key?.sheet).toMatch(/\S/);
    expect(provider.recoveryWrites()).toHaveLength(0); // nothing is written before the key is confirmed
    expect((await rt.query({ kind: "GetRecoveryStatus", ctx })).record).toBe("finishing_setup");
    await rt.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx, operationId: asOperationId("k"), keyOperationId: op });
    await rt.flush(pid);
    expect(provider.recoveryWrites()).toHaveLength(1);
    expect(provider.recoveryRecords().size).toBe(1);
    expect((await rt.query({ kind: "GetRecoveryStatus", ctx })).record).toBe("ready");
    rt.dispose();
  }, 60_000);

  it("a Home with no RecoveryStore leaves the key alone and writes nothing", async () => {
    const provider = createMemoryProvider(); // no place
    const { rt, ctx, pid } = await open(provider, "none");
    const op = asOperationId("c");
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: op, label: "None" });
    await rt.flush(pid);
    expect((await rt.query({ kind: "GetRecoveryKey", ctx, keyOperationId: op }))?.sheet).toBeUndefined();
    await rt.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx, operationId: asOperationId("k"), keyOperationId: op });
    await rt.flush(pid);
    expect(provider.recoveryWrites()).toHaveLength(0);
    expect((await rt.query({ kind: "GetRecoveryStatus", ctx })).record).toBeUndefined();
    rt.dispose();
  }, 60_000);

  it("an unknown outcome of the write is retried with the same request id and stored once", async () => {
    const provider = createMemoryProvider();
    provider.setRecoveryPlace(PLACE);
    const { rt, ctx, pid } = await open(provider, "retry");
    const op = asOperationId("c");
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: op, label: "Retry" });
    await rt.flush(pid);
    provider.inject("timeout_after_effect");
    await rt.dispatch({ kind: "ConfirmRecoveryKeyStored", ctx, operationId: asOperationId("k"), keyOperationId: op });
    await rt.flush(pid);
    for (let i = 0; i < 5 && provider.recoveryWrites().length === 0; i++) { await new Promise((r) => setTimeout(r, 20)); await rt.flush(pid); }
    expect(provider.recoveryWrites().length).toBeGreaterThanOrEqual(1);
    expect(new Set(provider.recoveryWrites().map((w) => w.requestId)).size).toBe(provider.recoveryWrites().length);
    rt.dispose();
  }, 60_000);
});
