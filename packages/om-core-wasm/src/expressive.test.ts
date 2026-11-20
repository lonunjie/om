// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it, vi } from "vitest";
import { asMessageId, asProfileGeneration, asProfileId, captureProfileContext } from "@om/platform-api";
import type { ExpressiveMediaApi, HomeSummary } from "@om/platform-api";
import { createCoreRuntime } from "./runtime";
import { createMemoryKv } from "./test-support";

const home: HomeSummary = { tier: "T1", observers: [], migrationAvailable: true, clientOriginChoice: ["default_origin"] };
const ctx = captureProfileContext(asProfileId("test"), asProfileGeneration("one"));
const id = asMessageId("expressive");
const source = { kind: "inline", index: 0 } as const;
const facts = { width: 1, height: 1, frames: 1, durationMs: null, encodedSize: 1, totalPixels: 1 };
const observed = { width: 1, height: 1, frameCount: null };
const altText = "A small smile";

async function setup({ enabled = true, renderable = true, accepted = true } = {}) {
  const call = vi.fn((command: Readonly<Record<string, unknown>>): unknown => {
    if (command["cmd"] === "init") return null;
    if (command["cmd"] === "expressive_available") return true;
    if (command["cmd"] === "expressive_decode") return accepted;
    if (command["cmd"] === "expressive_query") {
      const query = command["query"] as { kind: string };
      if (query.kind === "fetch") return renderable ? { altText, call: { call: "get_blob" } } : { altText };
      if (query.kind === "open") return { state: "renderable", altText, facts, kind: "custom_emoji", mediaProfileId: "om.expressive.webp-v1", plaintext: "aa" };
    }
    throw new Error(`unexpected command ${String(command["cmd"])}`);
  });
  const perform = vi.fn(() => Promise.resolve({ result: "bytes" as const, bytes: "ab" }));
  const runtime = await createCoreRuntime({ core: { call }, home, provider: { perform, poll: () => Promise.resolve([]) }, storage: () => createMemoryKv(), autoDrain: false, expressiveMedia: enabled });
  return { runtime, call, perform };
}

describe("expressive decoder handoff", () => {
  it("passes only validated plaintext and facts to the decoder and returns its observations to Core", async () => {
    const { runtime, call, perform } = await setup();
    const decode = vi.fn<ExpressiveMediaApi["decode"]>((asset) => {
      expect(asset.bytes).toEqual(Uint8Array.of(0xaa));
      expect(asset.facts).toEqual(facts);
      return Promise.resolve({ state: "decoded", facts: observed, frame: { rgba: new Uint8ClampedArray(4), durationUs: null } });
    });
    expect(await runtime.expressive.decode(ctx, id, source, { decode }, { reducedMotion: true })).toMatchObject({ state: "decoded", altText });
    expect(call).toHaveBeenCalledWith({ cmd: "expressive_decode", ctx, messageId: id, source, ciphertext: "ab", observed });
    expect(perform).toHaveBeenCalledOnce();
    expect(decode.mock.calls[0]?.[1]).toEqual({ reducedMotion: true });
  });

  it("closes playback and uses the placeholder when Core refuses decoder observations", async () => {
    const { runtime } = await setup({ accepted: false });
    const close = vi.fn();
    const decoder: ExpressiveMediaApi = {
      decode: () => Promise.resolve({
        state: "decoded", facts: { ...observed, width: 2 }, frame: { rgba: new Uint8ClampedArray(8), durationUs: null },
        playback: { close, pause() {}, play() {}, nextFrame: () => Promise.resolve({ state: "closed" }) },
      }),
    };
    expect(await runtime.expressive.decode(ctx, id, source, decoder, { reducedMotion: false })).toEqual({ state: "placeholder", altText });
    expect(close).toHaveBeenCalledOnce();
  });

  it("passes native failure back and retains the asset's alt text", async () => {
    const { runtime, call } = await setup();
    const decode: ExpressiveMediaApi["decode"] = () => Promise.resolve({ state: "fallback", reason: "decode_timeout" });
    expect(await runtime.expressive.decode(ctx, id, source, { decode }, { reducedMotion: false })).toEqual({ state: "placeholder", altText });
    expect(call).toHaveBeenCalledWith({ cmd: "expressive_decode", ctx, messageId: id, source, ciphertext: "ab", observed: null });
  });

  it("does not fetch or decode for a Core placeholder or a disabled client", async () => {
    const decode = vi.fn<ExpressiveMediaApi["decode"]>();
    const unsupported = await setup({ renderable: false });
    expect(await unsupported.runtime.expressive.decode(ctx, id, source, { decode }, { reducedMotion: false })).toEqual({ state: "placeholder", altText });
    const disabled = await setup({ enabled: false });
    expect(disabled.runtime.expressive.available()).toBe(false);
    expect(await disabled.runtime.expressive.decode(ctx, id, source, { decode }, { reducedMotion: false })).toBeNull();
    expect(disabled.call).toHaveBeenCalledExactlyOnceWith({ cmd: "init", home });
    expect(unsupported.perform).not.toHaveBeenCalled();
    expect(disabled.perform).not.toHaveBeenCalled();
    expect(decode).not.toHaveBeenCalled();
  });
});
