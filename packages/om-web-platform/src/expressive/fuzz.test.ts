// Browser-path fuzz of the expressive decode adapter (V-012, ADR-0251): hostile WebP headers, hostile header facts and
// hostile decoder-worker behaviour go through budget.ts, driver.ts and native.ts. Deterministic (seeded) and offline.
// It checks: no exception escapes, every refusal is a typed fallback, the pixel, frame, size and duration ceilings are
// enforced before a worker starts, no decode outlives the time budget, and a native decoder never returns a frame whose
// size differs from the validated facts. Smoke by default; OM_BROWSER_FUZZ_ITERATIONS scales it for a long run.
import type { ExpressiveDecodeFallback, ValidatedExpressiveAsset } from "@om/platform-api";
import { afterEach, describe, expect, it, vi } from "vitest";
import { checkBudget, DECODE_TIME_MS } from "./budget";
import type { DecodeReply, DecodeRequest } from "./decoder.worker";
import { decodeExpressiveAsset } from "./driver";
import { corpus } from "./fuzz-corpus";
import { DecoderMismatch, openFrameDecoder } from "./native";
import { budget, kind_resource_profile, resource_profiles } from "../../../../spec/registries/om-expressive-media.json";

type Facts = ValidatedExpressiveAsset["facts"];
type Kind = ValidatedExpressiveAsset["kind"];

const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};
const ITERATIONS = Number(env["OM_BROWSER_FUZZ_ITERATIONS"] ?? 400);
const SEED = Number(env["OM_BROWSER_FUZZ_SEED"] ?? 0x0b0e);
const KINDS: readonly Kind[] = ["sticker", "custom_emoji", "reaction_asset", "loop_animation"];
const REASONS: readonly ExpressiveDecodeFallback["reason"][] = [
  "unsupported_profile", "invalid_budget", "decode_failed", "decode_timeout", "decoder_mismatch", "unavailable",
];

export const stats = { inputs: 0, accepted: 0, refused: 0, workers: 0, timeouts: 0 };

// mulberry32: a small deterministic generator, so a failing input replays from (seed, index).
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

function fromHex(hex: string): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

const seeds = Object.entries(corpus.webp).map(([name, hex]) => ({ name, bytes: fromHex(hex) }));

function mutate(r: Rand, input: Uint8Array): Uint8Array<ArrayBuffer> {
  let out = Uint8Array.from(input);
  for (let n = 1 + below(r, 4); n > 0; n--) {
    switch (below(r, 7)) {
      case 0: if (out.length > 0) { const at = below(r, out.length); out[at] = (out[at] ?? 0) ^ (1 << below(r, 8)); } break;
      case 1: if (out.length > 0) out[below(r, out.length)] = pick(r, [0, 1, 0x7f, 0x80, 0xff]); break;
      case 2: out = out.slice(0, below(r, out.length + 1)); break;
      case 3: out = Uint8Array.from([...out, ...Array.from({ length: below(r, 16) }, () => below(r, 256))]); break;
      case 4: { // a hostile little-endian 32-bit length or size at a random offset
        if (out.length >= 4) new DataView(out.buffer).setUint32(below(r, out.length - 3), pick(r, [0, 1, 0xffff, 0xffffff, 0x7fffffff, 0xffffffff]), true);
        break;
      }
      case 5: { // duplicate a slice in place (chunk repetition)
        const at = below(r, out.length + 1);
        const len = below(r, Math.min(64, out.length - at) + 1);
        out = Uint8Array.from([...out.slice(0, at + len), ...out.slice(at, at + len), ...out.slice(at + len)]);
        break;
      }
      default: { // splice the tail of another seed
        const other = pick(r, seeds).bytes;
        out = Uint8Array.from([...out.slice(0, below(r, out.length + 1)), ...other.slice(below(r, other.length + 1))]);
      }
    }
  }
  return out.slice(0, 1 << 16);
}

// A defensive, independent reader that claims facts from header bytes. It is the harness's stand-in for Core's walker: it
// need not agree with Core, only produce plausible and hostile facts for the adapter to refuse or accept.
function claim(bytes: Uint8Array, r: Rand): Facts {
  const u8 = (at: number): number => bytes[at] ?? 0;
  const u24 = (at: number): number => u8(at) | (u8(at + 1) << 8) | (u8(at + 2) << 16);
  const u32 = (at: number): number => (u24(at) | (u8(at + 3) << 24)) >>> 0;
  let width = 0;
  let height = 0;
  let frames = 0;
  let duration = 0;
  let animated = false;
  for (let at = 12, guard = 0; at + 8 <= bytes.length && guard < 512; guard++) {
    const tag = String.fromCharCode(u8(at), u8(at + 1), u8(at + 2), u8(at + 3));
    const len = u32(at + 4);
    if (tag === "VP8X") {
      animated = (u8(at + 8) & 2) !== 0;
      width = u24(at + 12) + 1;
      height = u24(at + 15) + 1;
      at += 8 + Math.min(len + (len & 1), bytes.length);
    } else if (tag === "ANMF") {
      frames++;
      duration += u24(at + 8 + 12);
      at += 8 + 16; // the frame's sub-chunks follow its 16-byte header
    } else {
      at += 8 + Math.min(len + (len & 1), bytes.length);
    }
  }
  if (frames === 0) frames = 1;
  if (width === 0) { width = 1 + below(r, 600); height = 1 + below(r, 600); }
  return {
    width, height, frames,
    durationMs: animated || frames > 1 ? Math.max(1, duration) : null,
    encodedSize: bytes.length,
    totalPixels: width * height * frames,
  };
}

const EXTREMES = [0, 1, 2, 3, 29, 30, 31, 89, 90, 91, 255, 256, 257, 300, 301, 511, 512, 513, 1023, 1024, 1025, 65535, 65536, 2 ** 24, 2 ** 31 - 1, 2 ** 31, 2 ** 32 - 1, 2 ** 32, 2 ** 53 - 1, 2 ** 53, -1, 0.5, Number.NaN, Infinity, -Infinity, 1e300];
function hostileFacts(r: Rand, base: Facts): Facts {
  const out: { -readonly [K in keyof Facts]: Facts[K] } = { ...base };
  for (let n = 1 + below(r, 3); n > 0; n--) {
    const key = pick(r, ["width", "height", "frames", "durationMs", "encodedSize", "totalPixels"] as const);
    const value = r() < 0.7 ? pick(r, EXTREMES) : below(r, 4) === 0 ? null : below(r, 2000);
    if (key === "durationMs") out[key] = value;
    else if (value !== null) out[key] = value;
  }
  return out;
}

/** Independent statement of the ADR-0251 ceilings: true only when every ceiling holds. */
function withinCeilings(kind: Kind, f: Facts, byteLength: number): boolean {
  const id = kind_resource_profile[kind] as keyof typeof budget.profiles;
  const profile = resource_profiles.find((p) => p.id === id);
  if (profile === undefined) return false;
  const ceiling = budget.profiles[id];
  const ints = [f.width, f.height, f.frames, f.encodedSize, f.totalPixels];
  if (!ints.every((n) => Number.isSafeInteger(n) && n > 0)) return false;
  if (f.encodedSize !== byteLength || f.encodedSize > profile.max_encoded_bytes) return false;
  if (f.width > profile.max_canvas || f.height > profile.max_canvas || f.frames > ceiling.max_frames) return false;
  if (f.width * f.height > ceiling.max_pixels_per_frame) return false;
  if (f.totalPixels > ceiling.max_total_pixels || f.totalPixels > f.width * f.height * f.frames) return false;
  if (f.width * f.height * 16 > budget.resident_decode_ceiling_bytes) return false;
  if (f.durationMs === null) return f.frames === 1 && profile.animated !== "required";
  return Number.isSafeInteger(f.durationMs) && f.durationMs >= 1 && f.durationMs <= profile.max_duration_ms;
}

const asset = (kind: Kind, bytes: Uint8Array<ArrayBuffer>, facts: Facts, mediaProfileId = "om.expressive.webp-v1"): ValidatedExpressiveAsset =>
  ({ mediaProfileId, kind, bytes, facts });

function fakeWorker(behaviour: number, r: Rand) {
  const worker = {
    onerror: null as Worker["onerror"],
    onmessageerror: null as Worker["onmessageerror"],
    onmessage: null as Worker["onmessage"],
    terminated: 0,
    posted: 0,
    postMessage(message: DecodeRequest): void {
      worker.posted++;
      const reply = (data: unknown): void => { queueMicrotask(() => { worker.onmessage?.call(worker as unknown as Worker, new MessageEvent("message", { data })); }); };
      switch (behaviour) {
        case 0: break; // silent: the adapter's own timer must end it
        case 1: reply({ state: "fallback", reason: "decode_failed" } satisfies DecodeReply); break;
        case 2: queueMicrotask(() => { worker.onerror?.call(worker as unknown as Worker, { preventDefault() {} } as ErrorEvent); }); break;
        case 3: queueMicrotask(() => { worker.onmessageerror?.call(worker as unknown as Worker, new MessageEvent("messageerror")); }); break;
        case 4: throw new Error("postMessage refused");
        case 5: reply({ state: "fallback", reason: "decoder_mismatch" } satisfies DecodeReply); break;
        default: {
          const frameCount = message.type === "open" ? pick(r, [null, 1, 2, 90, 301]) : 5;
          reply({ state: "frame", frame: { rgba: new Uint8ClampedArray(4), durationUs: below(r, 3) === 0 ? null : below(r, 100000) }, facts: { width: 1, height: 1, frameCount } } satisfies DecodeReply);
        }
      }
    },
    terminate(): void { worker.terminated++; },
  };
  return worker;
}

/** Advance fake time until the promise settles; a promise that outlives the budget plus slack is a hang. */
async function settle<T>(p: Promise<T>): Promise<{ value: T } | "hang"> {
  let out: { value: T } | undefined;
  void p.then((value) => { out = { value }; });
  for (let waited = 0; waited <= DECODE_TIME_MS + 2000 && out === undefined; waited += 250) await vi.advanceTimersByTimeAsync(250);
  return out ?? "hang";
}

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("browser-path fuzz: budget preflight", () => {
  it(`never throws, and accepts only inputs within every ceiling (${String(ITERATIONS)} x 3 shapes, seed ${String(SEED)})`, () => {
    const r = rng(SEED);
    for (let i = 0; i < ITERATIONS; i++) {
      for (const { bytes } of [pick(r, seeds)]) {
        const kind = pick(r, KINDS);
        const mutated = mutate(r, bytes);
        for (const facts of [claim(mutated, r), hostileFacts(r, claim(mutated, r)), hostileFacts(r, { ...claim(mutated, r), encodedSize: mutated.length })]) {
          const input = asset(kind, mutated, facts);
          let verdict: ReturnType<typeof checkBudget>;
          try { verdict = checkBudget(input); } catch (error) { throw new Error(`checkBudget threw at index ${String(i)}: ${String(error)}`, { cause: error }); }
          stats.inputs++;
          if (verdict === undefined) {
            stats.accepted++;
            expect(withinCeilings(kind, facts, mutated.length), `accepted outside a ceiling at index ${String(i)}: ${JSON.stringify({ kind, facts })}`).toBe(true);
          } else {
            stats.refused++;
            expect(["unsupported_profile", "invalid_budget"]).toContain(verdict);
          }
        }
      }
    }
  });

  it("refuses an unknown or non-WebP media profile without reading the facts", () => {
    const r = rng(SEED + 1);
    for (let i = 0; i < 50; i++) {
      const id = pick(r, ["", "om.expressive.webp-v2", "image/webp", "om.expressive.gif-v1", "\u0000", "om.expressive.webp-v1 ", "x".repeat(1 << 12)]);
      expect(checkBudget(asset(pick(r, KINDS), new Uint8Array(1), { width: 1, height: 1, frames: 1, durationMs: null, encodedSize: 1, totalPixels: 1 }, id))).toBe("unsupported_profile");
    }
  });
});

describe("browser-path fuzz: decode driver against a hostile worker", () => {
  it(`starts a worker only for an accepted asset and always resolves to a typed result within the time budget (seed ${String(SEED)})`, async () => {
    vi.useFakeTimers();
    const r = rng(SEED + 2);
    const rejections: unknown[] = [];
    const onRejection = (reason: unknown): void => { rejections.push(reason); };
    const proc = (globalThis as { process?: { on(e: string, f: (r: unknown) => void): void; off(e: string, f: (r: unknown) => void): void } }).process;
    proc?.on("unhandledRejection", onRejection);
    try {
      for (let i = 0; i < Math.max(40, ITERATIONS / 2); i++) {
        const kind = pick(r, KINDS);
        const bytes = mutate(r, pick(r, seeds).bytes);
        const facts = r() < 0.5 ? claim(bytes, r) : hostileFacts(r, claim(bytes, r));
        const input = asset(kind, bytes, facts);
        const accepted = checkBudget(input) === undefined;
        const worker = fakeWorker(below(r, 7), r);
        let created = 0;
        const started = Date.now();
        const settled = await settle(decodeExpressiveAsset(input, r() < 0.5, () => { created++; return worker; }));
        expect(settled, `decode hung at index ${String(i)}`).not.toBe("hang");
        if (settled === "hang") return;
        const result = settled.value;
        stats.workers += created;
        expect(created, `worker started for a refused asset at index ${String(i)}`).toBe(accepted ? 1 : 0);
        expect(["decoded", "fallback"]).toContain(result.state);
        if (result.state === "fallback") {
          expect(REASONS).toContain(result.reason);
          if (result.reason === "decode_timeout") stats.timeouts++;
          expect(worker.terminated, `fallback left a worker running at index ${String(i)}`).toBe(created);
        } else {
          expect(accepted).toBe(true);
          if (result.playback !== undefined) {
            // Frames: every outcome is typed and a worker that goes quiet ends in the shared time budget.
            for (let n = 0; n < 4; n++) {
              const next = await settle(result.playback.nextFrame());
              expect(next, `nextFrame hung at index ${String(i)}`).not.toBe("hang");
              if (next === "hang") return;
              expect(["frame", "paused", "closed", "fallback"]).toContain(next.value.state);
            }
            result.playback.close();
            expect(worker.terminated).toBe(1);
          }
        }
        expect(Date.now() - started, "virtual time past the decode budget").toBeLessThanOrEqual(DECODE_TIME_MS * 3);
      }
    } finally {
      proc?.off("unhandledRejection", onRejection);
    }
    expect(rejections).toEqual([]);
  });
});

describe("browser-path fuzz: native decoder size agreement", () => {
  it("never returns a frame whose size differs from the validated facts, and closes every bitmap", async () => {
    const r = rng(SEED + 3);
    const bytes = new Uint8Array(32);
    for (let i = 0; i < Math.max(40, ITERATIONS / 2); i++) {
      const expected: Facts = { width: 1 + below(r, 64), height: 1 + below(r, 64), frames: 1, durationMs: null, encodedSize: bytes.length, totalPixels: 0 };
      const dims = [pick(r, [expected.width, expected.width + 1, 0, 1, 65536, 2 ** 31]), pick(r, [expected.height, expected.height + 1, 0, 1, 65536, 2 ** 31])] as const;
      const matches = dims[0] === expected.width && dims[1] === expected.height;
      let closed = 0;
      vi.stubGlobal("ImageDecoder", undefined);
      vi.stubGlobal("createImageBitmap", () => Promise.resolve({ width: dims[0], height: dims[1], close: () => { closed++; } }));
      vi.stubGlobal("OffscreenCanvas", class {
        getContext() { return { drawImage() {}, clearRect() {}, getImageData: (_x: number, _y: number, w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4) }) }; }
      });
      const decoder = await openFrameDecoder(bytes, expected);
      if (decoder === undefined) throw new Error("expected a bitmap decoder");
      if (matches) {
        const { frame, facts } = await decoder.read(0);
        expect(frame.rgba.length).toBe(expected.width * expected.height * 4);
        expect(facts).toEqual({ width: expected.width, height: expected.height, frameCount: null });
      } else {
        await expect(decoder.read(0)).rejects.toBeInstanceOf(DecoderMismatch);
      }
      expect(closed).toBe(1);
      decoder.close();
      vi.unstubAllGlobals();
    }
  });
});

describe("browser-path fuzz: coverage of the run", () => {
  it("exercised acceptance, refusal, worker start and timeout", () => {
    // Guards against a vacuous run: the generators must reach both sides of every decision.
    expect(stats.accepted).toBeGreaterThan(0);
    expect(stats.refused).toBeGreaterThan(0);
    expect(stats.workers).toBeGreaterThan(0);
    expect(stats.timeouts).toBeGreaterThan(0);
    if (env["OM_BROWSER_FUZZ_REPORT"] !== undefined) console.log(`OM_BROWSER_FUZZ_WEB ${JSON.stringify({ seed: SEED, iterations: ITERATIONS, ...stats })}`);
  });
});
