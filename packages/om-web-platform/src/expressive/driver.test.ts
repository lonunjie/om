import type { ValidatedExpressiveAsset } from "@om/platform-api";
import { afterEach, describe, expect, it, vi } from "vitest";
import { checkBudget } from "./budget";
import type { DecodeReply, DecodeRequest } from "./decoder.worker";
import { decodeExpressiveAsset } from "./driver";

const asset = (facts: Partial<ValidatedExpressiveAsset["facts"]> = {}): ValidatedExpressiveAsset => {
  const bytes = new Uint8Array(26);
  return {
    mediaProfileId: "om.expressive.webp-v1",
    kind: "sticker",
    bytes,
    facts: { width: 1, height: 1, frames: 1, durationMs: null, encodedSize: bytes.byteLength, totalPixels: 1, ...facts },
  };
};

const animation = asset({ frames: 3, durationMs: 100, totalPixels: 3 });

function fakeWorker() {
  const worker = {
    onerror: null as Worker["onerror"],
    onmessageerror: null as Worker["onmessageerror"],
    onmessage: null as Worker["onmessage"],
    postMessage: vi.fn<(message: DecodeRequest) => void>(),
    terminate: vi.fn(),
  };
  const factory = vi.fn(() => worker);
  const reply = (data: DecodeReply): void => {
    const listener = worker.onmessage;
    if (listener === null) throw new Error("No message listener");
    listener.call(worker as unknown as Worker, new MessageEvent("message", { data }));
  };
  const frame = (index: number, frameCount: number | null = 3): void => {
    reply({
      state: "frame",
      frame: { rgba: Uint8ClampedArray.of(index, 0, 0, 255), durationUs: 33_333 },
      facts: { width: 1, height: 1, frameCount },
    });
  };
  return { worker, factory, reply, frame };
}

afterEach(() => { vi.useRealTimers(); });

describe("expressive preflight", () => {
  // The bytes and the rejected header facts are represented together. The adapter does
  // not parse the RIFF container. Core exercises that boundary in hostile_inputs.rs.
  const hugeCanvas = Uint8Array.of(
    82, 73, 70, 70, 18, 0, 0, 0, 87, 69, 66, 80, 86, 80, 56, 76, 6, 0, 0, 0, 47, 255, 255, 255, 15, 0,
  );
  const onePixelFrame = Uint8Array.of(
    65, 78, 77, 70, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 0, 0, 0,
    86, 80, 56, 76, 6, 0, 0, 0, 47, 0, 0, 0, 0, 0,
  );
  const manyFrames = new Uint8Array(44 + 91 * onePixelFrame.length);
  manyFrames.set([
    82, 73, 70, 70, 0, 0, 0, 0, 87, 69, 66, 80,
    86, 80, 56, 88, 10, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    65, 78, 73, 77, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  ]);
  new DataView(manyFrames.buffer).setUint32(4, manyFrames.byteLength - 8, true);
  for (let i = 0; i < 91; i++) manyFrames.set(onePixelFrame, 44 + i * onePixelFrame.length);

  it.each([
    ["26-byte WebP declaring a 16384 × 16384 canvas", hugeCanvas, { width: 16384, height: 16384, totalPixels: 16384 ** 2 }],
    ["91 one-pixel frames", manyFrames, { frames: 91, totalPixels: 91, durationMs: 91 * 34 }],
  ] as const)("refuses %s before starting the worker", async (_name, bytes, facts) => {
    const factory = vi.fn(() => { throw new Error("must not start"); });
    const request = { ...asset({ ...facts, encodedSize: bytes.byteLength }), bytes };
    expect(await decodeExpressiveAsset(request, false, factory)).toEqual({ state: "fallback", reason: "invalid_budget" });
    expect(factory).not.toHaveBeenCalled();
  });

  it.each([
    ["sticker", 512, 90, 3000],
    ["custom_emoji", 256, 90, 3000],
    ["reaction_asset", 256, 90, 3000],
    ["loop_animation", 1024, 300, 10000],
  ] as const)("applies the %s budget at its exact ceiling and one over", (kind, canvas, frames, durationMs) => {
    const totalPixels = canvas ** 2 * frames;
    const facts = { width: canvas, height: canvas, frames, durationMs, totalPixels };
    const input = { ...asset(facts), kind };
    expect(checkBudget(input)).toBeUndefined();
    for (const excess of [{ width: canvas + 1 }, { height: canvas + 1 }, { frames: frames + 1 }, { totalPixels: totalPixels + 1 }]) {
      expect(checkBudget({ ...input, facts: { ...input.facts, ...excess } })).toBe("invalid_budget");
    }
  });

  it.each([
    { width: 0 }, { height: Number.NaN }, { frames: 0 }, { frames: 1.5 }, { width: 2 ** 32, height: 2 ** 32 },
    { totalPixels: 0 }, { durationMs: -1 }, { encodedSize: 25 }, { frames: 2, durationMs: 66, totalPixels: 2 },
  ])("refuses malformed or inconsistent facts", (facts) => {
    expect(checkBudget(asset(facts))).toBe("invalid_budget");
  });

  it("leaves unknown media profiles at the inert fallback", async () => {
    const { factory } = fakeWorker();
    const result = await decodeExpressiveAsset({ ...asset(), mediaProfileId: "unknown" }, false, factory);
    expect(result).toEqual({ state: "fallback", reason: "unsupported_profile" });
    expect(factory).not.toHaveBeenCalled();
  });
});

describe("expressive playback", () => {
  it("holds a static representative frame in Reduced Motion until play", async () => {
    const { worker, factory, frame } = fakeWorker();
    const opened = decodeExpressiveAsset(animation, true, factory);
    frame(0);
    const result = await opened;
    expect(result).toMatchObject({ state: "decoded", facts: { width: 1, height: 1, frameCount: 3 } });
    if (result.state !== "decoded" || result.playback === undefined) throw new Error("expected playback");
    expect(result.frame.rgba[0]).toBe(0);
    expect(await result.playback.nextFrame()).toEqual({ state: "paused" });
    expect(worker.postMessage).toHaveBeenCalledTimes(1);
    result.playback.play();
    const next = result.playback.nextFrame();
    expect(worker.postMessage.mock.lastCall?.[0]).toEqual({ type: "frame", index: 1 });
    frame(1);
    expect(await next).toMatchObject({ state: "frame", frame: { rgba: Uint8ClampedArray.of(1, 0, 0, 255) } });
    result.playback.close();
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it("suppresses a frame that finishes after pause and resumes from the same index", async () => {
    const { worker, factory, frame } = fakeWorker();
    const opened = decodeExpressiveAsset(animation, false, factory);
    frame(0);
    const result = await opened;
    if (result.state !== "decoded" || result.playback === undefined) throw new Error("expected playback");
    const playback = result.playback;
    const next = playback.nextFrame();
    expect(playback.nextFrame()).toBe(next);
    playback.pause();
    frame(1);
    expect(await next).toEqual({ state: "paused" });
    expect(await playback.nextFrame()).toEqual({ state: "paused" });
    playback.play();
    const resumed = playback.nextFrame();
    expect(worker.postMessage.mock.lastCall?.[0]).toEqual({ type: "frame", index: 1 });
    playback.close();
    expect(await resumed).toEqual({ state: "closed" });
    expect(await playback.nextFrame()).toEqual({ state: "closed" });
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it("releases a bitmap-only worker once the static frame arrives", async () => {
    const { worker, factory, frame } = fakeWorker();
    const opened = decodeExpressiveAsset(animation, true, factory);
    frame(0, null);
    const result = await opened;
    expect(result).toMatchObject({ state: "decoded", facts: { frameCount: null } });
    expect(result).not.toHaveProperty("playback");
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it("resolves decoder failures to inert fallback and stops the worker", async () => {
    const { worker, factory, reply } = fakeWorker();
    const opened = decodeExpressiveAsset(asset(), false, factory);
    const failed = { state: "fallback", reason: "decode_failed" } as const;
    reply(failed);
    expect(await opened).toEqual(failed);
    expect(worker.terminate).toHaveBeenCalledOnce();
  });

  it("terminates a hung worker using one time budget across frame requests", async () => {
    vi.useFakeTimers();
    const { worker, factory, frame } = fakeWorker();
    const opened = decodeExpressiveAsset(animation, false, factory);
    await vi.advanceTimersByTimeAsync(4000);
    frame(0);
    const result = await opened;
    if (result.state !== "decoded" || result.playback === undefined) throw new Error("expected playback");
    const next = result.playback.nextFrame();
    await vi.advanceTimersByTimeAsync(1001);
    expect(await next).toEqual({ state: "fallback", reason: "decode_timeout" });
    expect(worker.terminate).toHaveBeenCalledOnce();
  });
});
