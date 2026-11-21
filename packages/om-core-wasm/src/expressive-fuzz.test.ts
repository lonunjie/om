// SPDX-License-Identifier: MPL-2.0
// Browser-path fuzz of the real expressive Core (om_wasm_expressive.wasm) through the TypeScript runtime (V-012, ADR-0251).
// Mutated pack files, ContentPackRelease and control-state bodies (from the checked-in seed corpus) and hostile expressive
// command fields cross the JSON/hex boundary. Checks: no wasm trap (a trap is a panic in Core and kills the instance), every
// refusal is a typed value or a typed CoreCallError, no input runs past the per-input limit, the instance still answers
// afterwards and resident memory stays bounded. Deterministic (seeded); smoke by default, OM_BROWSER_FUZZ_ITERATIONS scales it.
// Needs `pnpm --filter @om/core-wasm build:wasm:expressive`; skipped, loudly, only if the artifact is absent.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { gunzipSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { asOperationId, asProfileId } from "@om/platform-api";
import type { HomeSummary, ProfileExecutionContext } from "@om/platform-api";
import { CoreCallError, createCoreRuntime, fromHex, instantiateCore, toHex } from "./index";
import { createMemoryKv } from "./test-support";

const WASM = join(import.meta.dirname, "../dist/om_wasm_expressive.wasm");
const SEEDS = join(import.meta.dirname, "../../../verification/prototypes/expressive-fuzz/seeds.json.gz");
const present = existsSync(WASM);
if (!present) console.warn("om_wasm_expressive.wasm not built: run `pnpm --filter @om/core-wasm build:wasm:expressive`; skipping the expressive Core fuzz");

const ITERATIONS = Number(process.env["OM_BROWSER_FUZZ_ITERATIONS"] ?? 300);
const SEED = Number(process.env["OM_BROWSER_FUZZ_SEED"] ?? 0x0b0e);
const INPUT_LIMIT_MS = 2000;
const RSS_GROWTH_LIMIT = 768 << 20;
const HOME: HomeSummary = { tier: "T1", observers: ["host_sees_ciphertext"], migrationAvailable: true, clientOriginChoice: ["default_origin"] };

function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
type Rand = () => number;
const below = (r: Rand, n: number): number => Math.floor(r() * n);
const pick = <T,>(r: Rand, xs: readonly T[]): T => xs[below(r, xs.length)] as T;

type Seeds = Record<string, Record<string, string>>;
const seeds: Seeds = present ? JSON.parse(gunzipSync(readFileSync(SEEDS)).toString("utf8")) as Seeds : {};
const corpus = (target: string): Uint8Array<ArrayBuffer>[] => Object.values(seeds[target] ?? {}).map((hex) => fromHex(hex));

function mutate(r: Rand, input: Uint8Array, pool: readonly Uint8Array[]): Uint8Array<ArrayBuffer> {
  let out = Uint8Array.from(input);
  for (let n = 1 + below(r, 4); n > 0; n--) {
    switch (below(r, 7)) {
      case 0: if (out.length > 0) out[below(r, out.length)] = (out[0] ?? 0) ^ (1 << below(r, 8)) ^ below(r, 256); break;
      case 1: if (out.length > 0) out[below(r, out.length)] = pick(r, [0, 1, 0x7f, 0x80, 0xff]); break;
      case 2: out = out.slice(0, below(r, out.length + 1)); break;
      case 3: out = Uint8Array.from([...out, ...Array.from({ length: below(r, 32) }, () => below(r, 256))]); break;
      case 4: if (out.length >= 4) new DataView(out.buffer).setUint32(below(r, out.length - 3), pick(r, [0, 1, 0xffff, 0xffffff, 0x7fffffff, 0xffffffff]), r() < 0.5); break;
      case 5: { const at = below(r, out.length + 1); const len = below(r, Math.min(64, out.length - at) + 1); out = Uint8Array.from([...out.slice(0, at + len), ...out.slice(at, at + len), ...out.slice(at + len)]); break; }
      default: { const other = pick(r, pool); out = Uint8Array.from([...out.slice(0, below(r, out.length + 1)), ...other.slice(below(r, other.length + 1))]); }
    }
  }
  return out.slice(0, 1 << 18);
}

const HOSTILE_STRINGS = ["", "00", "zz", "0", "ab".repeat(31), "ab".repeat(32), "ab".repeat(33), "ff".repeat(1 << 12), "\u0000", "é", "../x", "x".repeat(1 << 10), "A".repeat(64)];
const HOSTILE_NUMBERS = [0, 1, -1, 2 ** 31, 2 ** 32, 2 ** 32 - 1, 2 ** 53, 0.5, 65536, 1e300];

interface Tally { inputs: number; ok: number; refused: number; coreErrors: number; maxMs: number; refusals: Map<string, number> }
const tally = (): Tally => ({ inputs: 0, ok: 0, refused: 0, coreErrors: 0, maxMs: 0, refusals: new Map() });
const note = (t: Tally, reason: string): void => { t.refusals.set(reason, (t.refusals.get(reason) ?? 0) + 1); };

describe.skipIf(!present)("browser-path fuzz: expressive Core (real wasm)", () => {
  async function boot() {
    const core = await instantiateCore(readFileSync(WASM));
    const rt = await createCoreRuntime({ core, provider: { perform: () => Promise.resolve({ result: "error", error: "unreachable" }) as never, poll: () => Promise.resolve([]) }, storage: () => createMemoryKv(), home: HOME, autoDrain: false, expressiveMedia: true });
    const ctx = await rt.openProfile(asProfileId("fuzz"));
    await rt.dispatch({ kind: "CreateIdentityProfile", ctx, operationId: asOperationId("fuzz-create"), label: "Fuzz" });
    return { core, rt, ctx };
  }

  it("is enabled in this artifact", async () => {
    const { rt } = await boot();
    expect(rt.expressive.available()).toBe(true);
  });

  it(`refuses every mutated portable pack file with a typed value and no trap (${String(ITERATIONS)} inputs, seed ${String(SEED)})`, async () => {
    const { core, rt, ctx } = await boot();
    const r = rng(SEED);
    const pool = corpus("pack_file");
    const t = tally();
    const rss0 = process.memoryUsage().rss;
    for (let i = 0; i < ITERATIONS; i++) {
      const input = i < pool.length ? pool[i] as Uint8Array<ArrayBuffer> : mutate(r, pick(r, pool), pool);
      const started = performance.now();
      let result: Awaited<ReturnType<typeof rt.expressive.reviewPackFile>>;
      try {
        result = await rt.expressive.reviewPackFile(ctx, input);
      } catch (error) {
        throw new Error(`reviewPackFile threw at index ${String(i)} (${String(input.length)} bytes, seed ${String(SEED)}): ${String(error)}`, { cause: error });
      }
      const ms = performance.now() - started;
      t.inputs++;
      t.maxMs = Math.max(t.maxMs, ms);
      expect(ms, `input ${String(i)} took ${ms.toFixed(0)} ms`).toBeLessThan(INPUT_LIMIT_MS);
      if ("ticket" in result) {
        t.ok++;
        await rt.expressive.discardPackFile(ctx, result.ticket);
      } else {
        t.refused++;
        expect(result.refused, `untyped refusal at index ${String(i)}`).toMatch(/^[a-z][a-z0-9_]*$/);
        note(t, result.refused);
      }
    }
    expect(core.call({ cmd: "expressive_available" }), "instance no longer answers").toBe(true);
    expect(process.memoryUsage().rss - rss0).toBeLessThan(RSS_GROWTH_LIMIT);
    expect(t.refused).toBeGreaterThan(0);
    if (process.env["OM_BROWSER_FUZZ_REPORT"] !== undefined) console.log(`OM_BROWSER_FUZZ_PACK ${JSON.stringify({ seed: SEED, ...t, refusals: Object.fromEntries(t.refusals) })}`);
  });

  it(`answers hostile release, control-state, ref and decode commands with typed results and no trap (seed ${String(SEED)})`, async () => {
    const { core, ctx } = await boot();
    const r = rng(SEED + 1);
    const releases = corpus("release");
    const controls = corpus("control_state");
    const refs = corpus("asset_ref");
    const t = tally();
    const hex = (b: Uint8Array): string => toHex(b);
    const anyString = (): string => r() < 0.5 ? pick(r, HOSTILE_STRINGS) : hex(mutate(r, pick(r, refs), refs));
    const source = (): unknown => r() < 0.7 ? { kind: pick(r, ["inline", "reaction", "x", ""]), index: pick(r, HOSTILE_NUMBERS) } : pick(r, [null, 1, "inline", []]);
    const observed = (): unknown => r() < 0.3 ? null : { width: pick(r, HOSTILE_NUMBERS), height: pick(r, HOSTILE_NUMBERS), frameCount: r() < 0.3 ? null : pick(r, HOSTILE_NUMBERS) };
    const commands: (() => Record<string, unknown>)[] = [
      () => ({ cmd: "expressive_query", ctx, query: { kind: "preview_pack", input: { release: hex(mutate(r, pick(r, releases), releases)), control: r() < 0.3 ? null : hex(mutate(r, pick(r, controls), controls)) } } }),
      () => ({ cmd: "expressive_query", ctx, query: { kind: "picker", packId: anyString() } }),
      () => ({ cmd: "expressive_query", ctx, query: { kind: "fetch", messageId: anyString(), source: source() } }),
      () => ({ cmd: "expressive_query", ctx, query: { kind: "open", messageId: anyString(), source: source(), ciphertext: anyString() } }),
      () => ({ cmd: "expressive_decode", ctx, messageId: anyString(), source: source(), ciphertext: anyString(), observed: observed() }),
      () => ({ cmd: "pack_cache_open", ctx, packId: anyString(), releaseId: anyString(), itemId: anyString(), ciphertext: anyString(), observed: observed() }),
      () => ({ cmd: "pack_file_append", ctx, offset: pick(r, HOSTILE_NUMBERS), bytes: anyString() }),
    ];
    for (let i = 0; i < ITERATIONS; i++) {
      const command = pick(r, commands)();
      const started = performance.now();
      t.inputs++;
      try {
        const value = core.call(command);
        t.ok++;
        if (typeof value === "object" && value !== null && "refused" in value) { t.refused++; note(t, String(value.refused)); }
      } catch (error) {
        if (!(error instanceof CoreCallError)) throw error;
        expect(error.code, `wasm trap at index ${String(i)} for ${String(command["cmd"])} (seed ${String(SEED)})`).not.toBe("trapped");
        expect(error.code).toMatch(/^[a-z][a-z0-9_]*$/);
        t.coreErrors++;
        note(t, `error:${error.code}`);
      }
      const ms = performance.now() - started;
      t.maxMs = Math.max(t.maxMs, ms);
      expect(ms, `command ${String(i)} (${String(command["cmd"])}) took ${ms.toFixed(0)} ms`).toBeLessThan(INPUT_LIMIT_MS);
    }
    expect(core.call({ cmd: "expressive_available" })).toBe(true);
    expect(t.refused + t.coreErrors).toBeGreaterThan(0);
    if (process.env["OM_BROWSER_FUZZ_REPORT"] !== undefined) console.log(`OM_BROWSER_FUZZ_COMMANDS ${JSON.stringify({ seed: SEED, ...t, refusals: Object.fromEntries(t.refusals) })}`);
  });

  it("refuses a pack file above the size ceiling before it reaches Core", async () => {
    const { rt, ctx } = await boot();
    expect(await rt.expressive.reviewPackFile(ctx, new Uint8Array((32 << 20) + 1))).toEqual({ refused: "size_limit" });
  });
});

export type { ProfileExecutionContext };
