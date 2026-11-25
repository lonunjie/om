// SPDX-License-Identifier: MPL-2.0
// Adapts an `om-lan-v1` byte channel (one canonical `LanFrameV1` per message, ADR-0158 sections 4 and 6) to the
// `DirectChannel` that the direct-transfer driver speaks. The mapping of blob frames to `LanFrameV1` is fixed in Core
// (`DirectFrame::to_lan`); the bytes are produced and checked by the frozen codec in the engineering module, so a frame the
// codec refuses never reaches the driver, and a frame that is not part of a blob transfer is dropped here, not guessed at.
import type { DirectChannel } from "./direct-driver";
import type { DirectFrame } from "./types";
import { fromHex, toHex } from "./types";
import type { WasmCore } from "./wasm";

/** What this needs of the byte channel; `LanDataChannel` of `@om/web-platform/lan` satisfies it. */
export interface LanBytes {
  send(bytes: Uint8Array): void;
  canSend(): boolean;
  onDrain(cb: () => void): void;
  onMessage(cb: (bytes: Uint8Array) => void): void;
  onClose(cb: () => void): void;
  close(): void;
}

type Core = Pick<WasmCore, "call">;

const OBJECT_KIND_BLOB = 8;
const ABOVE_HIGH_WATER = 262_145; // reported while the channel will not take more (its high mark is 262,144)

function toWire(f: DirectFrame): Record<string, unknown> {
  switch (f.kind) {
    case "fetch_blob": return { kind: "fetch_blob", requestId: f.requestId, capability: f.readCap, blobId: f.blobId, ciphertextDigest: f.digest };
    case "begin": return { kind: "begin", transferId: f.transferId, objectKind: OBJECT_KIND_BLOB, totalBytes: f.totalBytes, sha256: f.sha256 };
    default: return { ...f };
  }
}

interface Wire { kind: string; requestId?: string; capability?: string; blobId?: string; ciphertextDigest?: string; transferId?: string; objectKind?: number; totalBytes?: number; sha256?: string; offset?: number; bytes?: string; id?: string; code?: number; tag?: string; refused?: string }

function fromWire(w: Wire): DirectFrame | null {
  switch (w.kind) {
    case "fetch_blob": return { kind: "fetch_blob", requestId: w.requestId ?? "", readCap: w.capability ?? "", blobId: w.blobId ?? "", digest: w.ciphertextDigest ?? "" };
    case "begin": return w.objectKind === OBJECT_KIND_BLOB ? { kind: "begin", transferId: w.transferId ?? "", totalBytes: w.totalBytes ?? 0, sha256: w.sha256 ?? "" } : null;
    case "chunk": return { kind: "chunk", transferId: w.transferId ?? "", offset: w.offset ?? 0, bytes: w.bytes ?? "" };
    case "end": return { kind: "end", transferId: w.transferId ?? "" };
    case "ack": return { kind: "ack", transferId: w.transferId ?? "", offset: w.offset ?? 0 };
    case "error": return w.code === 1 || w.code === 2 || w.code === 3 ? { kind: "error", id: w.id ?? "", code: w.code } : null;
    case "receipt": return { kind: "receipt", transferId: w.transferId ?? "", tag: w.tag ?? "" };
    default: return null; // fetch_async_key and anything else: not part of a blob transfer
  }
}

export function lanDirectChannel(core: Core, lan: LanBytes): DirectChannel {
  let frameHandler: ((f: DirectFrame) => void) | undefined;
  lan.onMessage((bytes) => {
    const w = core.call({ cmd: "lan_frame_decode", bytes: toHex(bytes) }) as Wire;
    if (w.refused !== undefined) return; // the codec refused it: it never reaches the driver
    const f = fromWire(w);
    if (f) frameHandler?.(f);
  });
  return {
    send: (f) => {
      const r = core.call({ cmd: "lan_frame_encode", frame: toWire(f) }) as { frame?: string; refused?: string };
      if (r.frame === undefined) return Promise.reject(new Error(`lan frame refused: ${r.refused ?? "unknown"}`));
      lan.send(fromHex(r.frame));
      return Promise.resolve();
    },
    get buffered() { return lan.canSend() ? 0 : ABOVE_HIGH_WATER; },
    onFrame: (h) => { frameHandler = h; },
    onClose: (h) => { lan.onClose(h); },
    onDrain: (h) => { lan.onDrain(h); },
    close: () => { lan.close(); },
  };
}
