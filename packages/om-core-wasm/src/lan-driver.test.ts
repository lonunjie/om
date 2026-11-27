// SPDX-License-Identifier: MPL-2.0
// Two real Core instances (the engineering `scope-engines` module) move a multi-chunk object over an in-memory ordered channel.
// Skipped when the module is not built (`pnpm --filter @om/core-wasm build:wasm:scope`). Not a WebRTC or real-LAN test.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import type { ProfileExecutionContext } from "@om/platform-api";
import { describe, expect, it, vi } from "vitest";
import { createLanReceiver, createLanSender } from "./lan-driver";
import type { LanAsyncKeyResponder, LanChannel } from "./lan-driver";
import { createScopeEngines } from "./scope-engines";
import { fromHex, toHex } from "./types";
import { instantiateCore, type WasmCore } from "./wasm";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const TOTAL = 70_000;
const payload = Uint8Array.from({ length: TOTAL }, (_, i) => (i * 31 + 7) & 0xff);
const digest = createHash("sha256").update(payload).digest("hex");
const blobId = "aa".repeat(32);
const transferId = "07".repeat(16);
const tag = "5c".repeat(32);

function scoped(core: WasmCore) {
  core.call({ cmd: "init", home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] } });
  const ctx = core.call({ cmd: "open_profile", profileId: "transfer-test", records: [] }) as ProfileExecutionContext;
  return createScopeEngines(core, ctx);
}

async function rig(opts: { sentTag?: string; storedOffset?: number } = {}) {
  const a = await instantiateCore(readFileSync(wasm));
  const b = await instantiateCore(readFileSync(wasm));
  const toReceiver: Uint8Array[] = [];
  const toSender: Uint8Array[] = [];
  const chan = (q: Uint8Array[]): LanChannel => ({ send: (m) => { q.push(m); } });
  const stored = new Uint8Array(TOTAL);
  const storedOffset = opts.storedOffset ?? 0;
  stored.set(payload.subarray(0, storedOffset));
  const sender = createLanSender({
    core: a, engines: scoped(a), channel: chan(toReceiver), handle: "t", transferId, blobId, digest, objectKind: 8, total: TOTAL,
    read: (o, l) => payload.subarray(o, o + l), verifyReceipt: (t) => t === tag,
  });
  const receiver = createLanReceiver({
    core: b, engines: scoped(b), channel: chan(toSender), handle: "t", storedOffset,
    write: (o, bytes) => { stored.set(bytes, o); },
    digestMatches: (d) => createHash("sha256").update(stored).digest("hex") === d,
    receiptTag: () => opts.sentTag ?? tag,
  });
  const run = (): void => {
    for (let i = 0; i < 10_000 && (toReceiver.length > 0 || toSender.length > 0); i++) {
      const r = toReceiver.shift();
      if (r) receiver.onFrame(r);
      const s = toSender.shift();
      if (s) sender.onFrame(s);
    }
  };
  return { sender, receiver, stored, run, a, b };
}

describe.skipIf(!existsSync(wasm))("LAN transfer driver over an in-memory channel", () => {
  it("delivers an object in 16 KiB frames and confirms only on the authenticated receipt", async () => {
    const t = await rig();
    t.sender.start();
    t.run();
    expect(t.receiver.complete).toBe(true);
    expect(toHex(t.stored)).toBe(toHex(payload));
    expect(t.sender.confirmed).toBe(true);
  });

  it("does not confirm on a receipt the host cannot authenticate", async () => {
    const t = await rig({ sentTag: "ee".repeat(32) });
    t.sender.start();
    t.run();
    expect(t.receiver.complete).toBe(true);
    expect(t.sender.confirmed).toBe(false);
  });

  it("resumes from the receiver's durable offset", async () => {
    const t = await rig({ storedOffset: 32_768 });
    t.sender.start(32_768);
    t.run();
    expect(t.receiver.complete).toBe(true);
    expect(t.sender.confirmed).toBe(true);
    expect(toHex(t.stored)).toBe(toHex(payload));
  });
});

describe("LAN driver async-key routing over a fake channel", () => {
  it.each(["sender", "receiver"] as const)("answers fetch frames on the %s only when a responder is supplied", (side) => {
    const fetch = Uint8Array.of(1);
    const refused = Uint8Array.of(2);
    const requestId = "03".repeat(16);
    const error = `a4000101070250${requestId}0d01`;
    const bundle = { kind: "bundle", requestId, sealedBundle: "cafe" } as const;
    const core = {
      call: vi.fn((cmd: Record<string, unknown>) => cmd.bytes === toHex(fetch)
        ? { kind: "fetch_async_key" }
        : { refused: "wrong_type" }),
    };
    const channel = { send: vi.fn<(bytes: Uint8Array) => void>() };
    const engines = createScopeEngines(core);
    const handleFrame = vi.fn<LanAsyncKeyResponder["handleFrame"]>()
      .mockReturnValueOnce({ kind: "error", frame: error })
      .mockReturnValueOnce(bundle);
    const sendBundle = vi.fn<LanAsyncKeyResponder["sendBundle"]>();
    const driver = (asyncKeyResponder?: LanAsyncKeyResponder) => {
      const options = { core, channel, engines, handle: "transfer", ...(asyncKeyResponder && { asyncKeyResponder }) };
      return side === "sender"
        ? createLanSender({ ...options, transferId, blobId, digest, objectKind: 8, total: TOTAL, read: () => payload, verifyReceipt: () => false })
        : createLanReceiver({ ...options, storedOffset: 0, write: () => {}, digestMatches: () => false, receiptTag: () => tag });
    };

    driver().onFrame(fetch);
    expect(channel.send).not.toHaveBeenCalled();
    const enabled = driver({ handleFrame, sendBundle });
    enabled.onFrame(refused);
    expect(handleFrame).not.toHaveBeenCalled();
    enabled.onFrame(fetch);
    expect(handleFrame).toHaveBeenCalledWith(fetch);
    expect(channel.send).toHaveBeenCalledExactlyOnceWith(fromHex(error));
    enabled.onFrame(fetch);
    expect(sendBundle).toHaveBeenCalledExactlyOnceWith(channel, bundle);
    expect(channel.send).toHaveBeenCalledTimes(1);
  });
});
