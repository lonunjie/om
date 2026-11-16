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
