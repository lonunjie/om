// SPDX-License-Identifier: MPL-2.0
// Blob frames cross an in-memory byte channel as canonical `LanFrameV1` bytes and come back unchanged; frames outside a blob
// transfer are dropped. Needs `pnpm --filter @om/core-wasm build:wasm:scope`. Not a WebRTC test.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { lanDirectChannel } from "./lan-direct-channel";
import type { LanBytes } from "./lan-direct-channel";
import type { DirectFrame } from "./types";
import { fromHex, toHex } from "./types";
import { instantiateCore } from "./wasm";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);

function pair(): [LanBytes, LanBytes] {
  let ma: ((b: Uint8Array) => void) | undefined;
  let mb: ((b: Uint8Array) => void) | undefined;
  const end = (peer: () => ((b: Uint8Array) => void) | undefined, set: (cb: (b: Uint8Array) => void) => void): LanBytes => ({
    send: (b) => { peer()?.(b); }, canSend: () => true, onDrain: () => undefined, onMessage: set, onClose: () => undefined, close: () => undefined,
  });
  return [end(() => mb, (cb) => { ma = cb; }), end(() => ma, (cb) => { mb = cb; })];
}

const id = "07".repeat(16);
const frames: DirectFrame[] = [
  { kind: "fetch_blob", requestId: id, readCap: "11".repeat(32), blobId: "22".repeat(32), digest: "33".repeat(32) },
  { kind: "begin", transferId: id, totalBytes: 40_000, sha256: "44".repeat(32) },
  { kind: "chunk", transferId: id, offset: 16_384, bytes: "99".repeat(100) },
  { kind: "end", transferId: id },
  { kind: "ack", transferId: id, offset: 100 },
  { kind: "error", id, code: 3 },
  { kind: "receipt", transferId: id, tag: "55".repeat(32) },
];

describe.skipIf(!existsSync(wasm))("om-lan-v1 byte channel as a direct-transfer channel", () => {
  it("carries every blob frame as LanFrameV1 and drops what is not a blob transfer", async () => {
    const core = await instantiateCore(readFileSync(wasm));
    const [x, y] = pair();
    const wire: Uint8Array[] = [];
    const spy: LanBytes = { ...x, send: (b) => { wire.push(b); x.send(b); } };
    const sender = lanDirectChannel(core, spy);
    const receiver = lanDirectChannel(core, y);
    const got: DirectFrame[] = [];
    receiver.onFrame((f) => { got.push(f); });
    for (const f of frames) await sender.send(f);
    expect(got).toEqual(frames);
    expect(wire).toHaveLength(frames.length);
    // The bytes are the codec's, not a private encoding: they decode as the matching LanFrameV1 kind.
    expect((core.call({ cmd: "lan_frame_decode", bytes: toHex(wire[0] ?? new Uint8Array()) }) as { kind: string }).kind).toBe("fetch_blob");
    // An async-key fetch, a transfer of another object kind and garbage never reach the driver.
    const enc = (frame: object): Uint8Array => fromHex((core.call({ cmd: "lan_frame_encode", frame }) as { frame: string }).frame);
    const [p, q] = pair();
    const r2 = lanDirectChannel(core, q);
    const seen: DirectFrame[] = [];
    r2.onFrame((f) => { seen.push(f); });
    p.send(enc({ kind: "fetch_async_key", requestId: id, deviceId: "01".repeat(16), profile: 1, capability: "aa".repeat(32) }));
    p.send(enc({ kind: "begin", transferId: id, objectKind: 1, totalBytes: 1, sha256: "00".repeat(32) }));
    p.send(Uint8Array.of(0xff, 0x00));
    expect(seen).toEqual([]);
    // Backpressure is reported from the channel.
    let open = true;
    const blocked = lanDirectChannel(core, { ...x, canSend: () => open });
    expect(blocked.buffered).toBe(0);
    open = false;
    expect(blocked.buffered).toBeGreaterThan(262_144);
  });
});
