// D88: the host clock is read again by Core for every command; nothing in the platform adapter accumulates time from timer ticks, so
// a frozen then resumed page cannot hand Core a stale or summed time. Real Core (skipped only if the wasm artifact is absent).
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary } from "@om/platform-api";
import { createCoreRuntime, instantiateCore } from "./index";
import { createMemoryKv, createMemoryProvider } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm.wasm");
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };

describe.skipIf(!existsSync(WASM))("the host clock is re-read for every command (real Core)", () => {
  it("a command issued after a frozen interval sees the host clock as it is then, not the last timer tick", async () => {
    let host = 1_800_000_000_000;
    const seen: number[] = [];
    const core = await instantiateCore(readFileSync(WASM), { nowMs: () => { seen.push(host); return host; } });
    const rt = await createCoreRuntime({ core, provider: createMemoryProvider(), storage: () => createMemoryKv(), home: HOME, autoDrain: false, retryMs: 0, recoveryTick: false });
    const ctx = await rt.openProfile(asProfileId("p"));
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: asOperationId("c"), label: "P" });
    await rt.flush(asProfileId("p"));
    const first = seen.length;
    expect(first).toBeGreaterThan(0);
    // The tab is frozen: no timer fires, no tick arrives, and the host clock moves a long way on.
    host += 6 * 3_600_000;
    await rt.dispatch({ kind: "CreateInvite", ctx, operationId: asOperationId("i") });
    await rt.flush(asProfileId("p"));
    expect(seen.length).toBeGreaterThan(first);
    expect(seen.slice(first).every((t) => t === host)).toBe(true); // every read after the freeze is the live host clock
    rt.dispose();
  }, 60_000);
});
