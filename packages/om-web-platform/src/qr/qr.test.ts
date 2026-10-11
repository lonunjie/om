import { describe, expect, it } from "vitest";
import { QR_MAX_PAYLOAD_CHARS, decodeQrFrame, qrModules, scanLoop, type QrFrame } from "./qr";

/** Renders modules to an RGBA frame with a quiet zone, the way a camera would see a printed code. */
function frameOf(modules: readonly (readonly boolean[])[], scale = 6, quiet = 4): QrFrame {
  const n = modules.length + quiet * 2;
  const size = n * scale;
  const data = new Uint8ClampedArray(size * size * 4).fill(255);
  for (let y = 0; y < modules.length; y++) {
    for (let x = 0; x < modules.length; x++) {
      if (!modules[y]?.[x]) continue;
      for (let dy = 0; dy < scale; dy++) for (let dx = 0; dx < scale; dx++) {
        const at = (((y + quiet) * scale + dy) * size + (x + quiet) * scale + dx) * 4;
        data[at] = 0; data[at + 1] = 0; data[at + 2] = 0;
      }
    }
  }
  return { width: size, height: size, data };
}

const LINK = "om:invite/3fK9a2Q8x1Zp7Lr4Wt6Yb0Nc5HdEu2Vm";

describe("qr", () => {
  it("round-trips an invite-sized payload", () => {
    const m = qrModules(LINK);
    expect(m).not.toBeNull();
    expect(decodeQrFrame(frameOf(m ?? []))).toBe(LINK);
  });

  it("refuses empty and oversized payloads when encoding", () => {
    expect(qrModules("")).toBeNull();
    expect(qrModules("a".repeat(QR_MAX_PAYLOAD_CHARS + 1))).toBeNull();
  });

  it("returns null for blank, undersized and malformed frames instead of throwing", () => {
    const blank = { width: 64, height: 64, data: new Uint8ClampedArray(64 * 64 * 4).fill(255) };
    expect(decodeQrFrame(blank)).toBeNull();
    expect(decodeQrFrame({ width: 10, height: 10, data: new Uint8ClampedArray(4) })).toBeNull();
    expect(decodeQrFrame({ width: 0, height: 0, data: new Uint8ClampedArray(0) })).toBeNull();
  });

  it("scanLoop skips empty frames, returns the first decoded text, and stops polling", async () => {
    const good = frameOf(qrModules(LINK) ?? []);
    const frames: (QrFrame | null)[] = [null, { width: 8, height: 8, data: new Uint8ClampedArray(8 * 8 * 4).fill(255) }, good, good];
    let polled = 0;
    const text = await scanLoop({
      nextFrame: () => frames[polled++] ?? null, signal: new AbortController().signal, sleep: () => Promise.resolve(),
    });
    expect(text).toBe(LINK);
    expect(polled).toBe(3);
  });

  it("scanLoop resolves null on abort and on timeout", async () => {
    const ac = new AbortController();
    let n = 0;
    const aborted = await scanLoop({ nextFrame: () => { if (++n === 2) ac.abort(); return null; }, signal: ac.signal, sleep: () => Promise.resolve() });
    expect(aborted).toBeNull();
    expect(n).toBe(2);
    const timedOut = await scanLoop({ nextFrame: () => null, signal: new AbortController().signal, timeoutMs: 0, sleep: () => Promise.resolve() });
    expect(timedOut).toBeNull();
  });
});
