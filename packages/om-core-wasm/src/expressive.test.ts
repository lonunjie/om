// SPDX-License-Identifier: MPL-2.0
import { describe, expect, it, vi } from "vitest";
import { asMessageId, asProfileGeneration, asProfileId, captureProfileContext } from "@om/platform-api";
import type { ExpressiveMediaApi, HomeSummary } from "@om/platform-api";
import { createCoreRuntime } from "./runtime";
import type { NetCall } from "./types";
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

const oldDigest = "aa".repeat(32);
const firstDigest = "bb".repeat(32);
const secondDigest = "cc".repeat(32);
const packType = "application/vnd.om.content-pack";
const expected = { releaseId: "ab".repeat(32), controlDigest: "ac".repeat(32) };
const attachmentLocation = "https://sender-home.example/v1/blob/pack";

async function packRuntime(fail?: "first" | "second" | "inventory", stale = false) {
  const kv = createMemoryKv();
  await kv.put(`pack-cache/${oldDigest}`, Uint8Array.of(5));
  const oldInventory = Uint8Array.of(1);
  const newInventory = Uint8Array.of(2);
  await kv.put("core/expressive-packs", oldInventory);
  const stamp = async (bytes: Uint8Array<ArrayBuffer>) => {
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
  };
  const previous = await stamp(oldInventory);
  const next = await stamp(newInventory);
  const writes: string[] = [];
  let active = false;
  let stage = 0;
  const put = vi.fn<typeof kv.put>((key, bytes) => {
    writes.push(key);
    if ((fail === "first" && key.endsWith(firstDigest)) || (fail === "second" && key.endsWith(secondDigest))
      || (fail === "inventory" && key === "core/expressive-packs")) return Promise.resolve({ ok: false, reason: "storage_failed" });
    return kv.put(key, bytes);
  });
  const other = asProfileId("other");
  const isolated = createMemoryKv();
  const requests: string[] = [];
  const provider = {
    perform: vi.fn((op: NetCall) => {
      if (op.call !== "get_blob") throw new Error("unexpected provider call");
      requests.push(...op.locations);
      return Promise.resolve({ result: "bytes" as const, bytes: "aabbcc" });
    }),
    poll: () => Promise.resolve([]),
  };
  const call = vi.fn((request: Readonly<Record<string, unknown>>): unknown => {
    switch (request["cmd"]) {
      case "init": return null;
      case "expressive_available": return true;
      case "context_for": return ctx;
      case "attachment_fetch": return { call: { call: "get_blob", locations: [attachmentLocation] } };
      case "open_attachment": return { opened: { plaintext: "cafebabe", mediaType: packType } };
      case "pack_file_append": return { accepted: true };
      case "pack_file_review": return { ticket: 7, preview: { ...expected, sourceUrls: ["https://creator.invalid/pack"] } };
      case "pack_install_abort": return null;
      case "pack_cache_digests": return {
        inventory: (request["ctx"] as typeof ctx).profileId !== other
          ? (active ? next : stale ? "00".repeat(32) : previous) : null,
        digests: active ? [firstDigest, secondDigest] : [oldDigest],
      };
      case "pack_install_begin": stage = 0; return { count: 2 };
      case "pack_install_stage": return { digest: stage++ === 0 ? firstDigest : secondDigest, ciphertext: "aa" };
      case "pack_install_commit": return { accepted: true, commit: { id: 1, profileId: ctx.profileId, writes: [{ key: "expressive-packs", value: "02" }] } };
      case "finish_commit": active = request["ok"] === true; return { profileId: ctx.profileId, changedOperations: [] };
      case "pack_cache_open": return request["observed"] === undefined
        ? { state: "renderable", plaintext: "aa", mediaProfileId: "om.expressive.webp-v1", kind: "sticker", facts, altText }
        : true;
      default: throw new Error(`unexpected command ${String(request["cmd"])}`);
    }
  });
  const runtime = await createCoreRuntime({
    core: { call }, home, provider, storage: (id) => id === other ? isolated : { ...kv, put },
    expressiveMedia: true, autoDrain: false,
  });
  return { runtime, call, requests, kv, writes, other };
}

describe("portable pack install", () => {
  it.each(["message", "file"])("limits requests for a pack opened from %s", async (source) => {
    const { runtime, requests, kv, call, writes, other } = await packRuntime();
    const opened = source === "message" ? await runtime.attachments.open(ctx, id, 0) : { ok: true, bytes: Uint8Array.of(0xca, 0xfe, 0xba, 0xbe) };
    if (!opened.ok) throw new Error("attachment refused");
    const review = await runtime.expressive.reviewPackFile(ctx, opened.bytes);
    expect(writes).toEqual([]);
    if (!("ticket" in review)) throw new Error("review refused");
    expect(await runtime.expressive.do(ctx, { kind: "install_pack_file", ticket: review.ticket, confirmation: expected })).toEqual({ accepted: true });
    expect(requests).toEqual(source === "message" ? [attachmentLocation] : []);
    expect(writes).toEqual([`pack-cache/${firstDigest}`, `pack-cache/${secondDigest}`, "core/expressive-packs"]);
    expect(call).toHaveBeenCalledWith({ cmd: "finish_commit", commitId: 1, ok: true });
    expect(kv.snapshot().has(`pack-cache/${oldDigest}`)).toBe(false);
    expect(await runtime.expressive.readCached(ctx, firstDigest)).toEqual(Uint8Array.of(0xaa));
    expect(await runtime.expressive.readCached(captureProfileContext(other, ctx.generation), firstDigest)).toBeNull();
    const decode = vi.fn<ExpressiveMediaApi["decode"]>(() => Promise.resolve({
      state: "decoded", facts: observed, frame: { rgba: new Uint8ClampedArray(4), durationUs: null },
    }));
    const selection = { packId: "pack", releaseId: expected.releaseId, itemId: "item" };
    expect(await runtime.expressive.decodeCached(ctx, selection, Uint8Array.of(0xaa), { decode }, { reducedMotion: true })).toMatchObject({ state: "decoded", altText });
    expect(call).toHaveBeenCalledWith({ cmd: "pack_cache_open", ctx, ...selection, ciphertext: "aa", observed });
    expect(requests).toEqual(source === "message" ? [attachmentLocation] : []);
  });

  it.each(["first", "second", "inventory"] as const)("preserves the prior install when the %s write fails", async (fail) => {
    const { runtime, requests, kv, call } = await packRuntime(fail);
    const r = await runtime.expressive.do(ctx, { kind: "install_pack_file", ticket: 7, confirmation: expected });
    expect(r).toEqual({ refused: "pack_storage" });
    expect(requests).toEqual([]);
    expect([...kv.snapshot().keys()]).toEqual([`pack-cache/${oldDigest}`, "core/expressive-packs"]);
    expect(await runtime.expressive.readCached(ctx, oldDigest)).toEqual(Uint8Array.of(5));
    expect(call).not.toHaveBeenCalledWith({ cmd: "finish_commit", commitId: 1, ok: true });
  });

  it("refuses a stale inventory without staging or deleting any cache bytes", async () => {
    const { runtime, writes, kv } = await packRuntime(undefined, true);
    expect(await runtime.expressive.do(ctx, { kind: "install_pack_file", ticket: 7, confirmation: expected })).toEqual({ refused: "confirmation_required" });
    expect(writes).toEqual([]);
    expect(kv.snapshot().has(`pack-cache/${oldDigest}`)).toBe(true);
  });
});
