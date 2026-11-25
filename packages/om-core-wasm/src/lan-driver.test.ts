// SPDX-License-Identifier: MPL-2.0
// Two real Core instances (the engineering `scope-engines` module) move a multi-chunk object over an in-memory ordered channel.
// Skipped when the module is not built (`pnpm --filter @om/core-wasm build:wasm:scope`). Not a WebRTC or real-LAN test.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createLanReceiver, createLanSender } from "./lan-driver";
import type { LanChannel } from "./lan-driver";
import { createScopeEngines } from "./scope-engines";
import { toHex } from "./types";
import { instantiateCore } from "./wasm";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const TOTAL = 70_000;
const payload = Uint8Array.from({ length: TOTAL }, (_, i) => (i * 31 + 7) & 0xff);
const digest = createHash("sha256").update(payload).digest("hex");
const blobId = "aa".repeat(32);
const transferId = "07".repeat(16);
const tag = "5c".repeat(32);

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
    core: a, engines: createScopeEngines(a), channel: chan(toReceiver), handle: "t", transferId, blobId, digest, objectKind: 8, total: TOTAL,
    read: (o, l) => payload.subarray(o, o + l), verifyReceipt: (t) => t === tag,
  });
  const receiver = createLanReceiver({
    core: b, engines: createScopeEngines(b), channel: chan(toSender), handle: "t", storedOffset,
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
