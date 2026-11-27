// SPDX-License-Identifier: MPL-2.0
// Moves one object over an ordered, reliable LAN channel (`om-lan-v1`, ADR-0158) using the Core state machines and the
// frozen `LanFrameV1` codec of the engineering-only `scope-engines` module. The channel is an interface, so the same driver
// runs over a WebRTC DataChannel, a QR/manual relay or the in-memory pair the tests use. Local delivery is a transport
// binding: transport ACKs move the window only and only an authenticated receipt confirms delivery. Keys, the receipt MAC
// and the digest check are injected by the host; this file holds none.
import { fromHex, toHex } from "./types";
import type { ScopeEngines } from "./scope-engines";
import type { WasmCore } from "./wasm";

export interface LanChannel {
  /** One message is exactly one canonical frame. */
  send(bytes: Uint8Array): void;
}

type Core = Pick<WasmCore, "call">;
const frame = (core: Core, f: Record<string, unknown>): Uint8Array => {
  const r = core.call({ cmd: "lan_frame_encode", frame: f }) as { frame?: string; refused?: string };
  if (r.frame === undefined) throw new Error(`lan frame refused: ${r.refused ?? "unknown"}`);
  return fromHex(r.frame);
};
interface Decoded { kind: string; transferId?: string; offset?: number; bytes?: string; totalBytes?: number; tag?: string; refused?: string }
const decode = (core: Core, b: Uint8Array): Decoded => core.call({ cmd: "lan_frame_decode", bytes: toHex(b) }) as Decoded;

export interface LanSenderOptions {
  readonly core: Core;
  readonly engines: ScopeEngines;
  readonly channel: LanChannel;
  readonly handle: string;
  readonly transferId: string;
  readonly blobId: string;
  readonly digest: string;
  readonly objectKind: number;
  readonly total: number;
  readonly read: (offset: number, len: number) => Uint8Array;
  /** Host check of the receipt MAC (OM-DIRECT-RECEIPT-MAC-v1): true only for an authentic tag. */
  readonly verifyReceipt: (tag: string) => boolean;
}

export interface LanSender {
  /** Sends object_begin and as many chunks as the window allows; call again after frames arrive. */
  start(resumeFrom?: number): void;
  onFrame(bytes: Uint8Array): void;
  readonly confirmed: boolean;
}

export function createLanSender(o: LanSenderOptions): LanSender {
  const lt = o.engines.lanTransfer;
  let sentEnd = false;
  let sentTotal = 0;
  let confirmed = false;
  const pump = (): void => {
    for (;;) {
      const r = lt.nextChunk(o.handle);
      if ("refused" in r || r.chunk === null) break;
      const bytes = o.read(r.chunk.offset, r.chunk.len);
      o.channel.send(frame(o.core, { kind: "chunk", transferId: o.transferId, offset: r.chunk.offset, bytes: toHex(bytes) }));
      sentTotal = Math.max(sentTotal, r.chunk.offset + r.chunk.len);
    }
    if (!sentEnd && sentTotal >= o.total) {
      sentEnd = true;
      o.channel.send(frame(o.core, { kind: "end", transferId: o.transferId }));
    }
  };
  return {
    start(resumeFrom = 0) {
      const opened = lt.openSender(o.handle, { blobId: o.blobId, digest: o.digest, total: o.total });
      if ("refused" in opened) throw new Error(opened.refused);
      if (resumeFrom > 0) {
        const resumed = lt.resumeSender(o.handle, resumeFrom);
        if ("refused" in resumed) throw new Error(resumed.refused);
        sentTotal = resumeFrom;
      }
      o.channel.send(frame(o.core, { kind: "begin", transferId: o.transferId, objectKind: o.objectKind, totalBytes: o.total, sha256: o.digest }));
      pump();
    },
    onFrame(bytes) {
      const f = decode(o.core, bytes);
      if (f.kind === "ack" && f.transferId === o.transferId && f.offset !== undefined) {
        lt.transportAck(o.handle, f.offset);
        pump();
      } else if (f.kind === "receipt" && f.transferId === o.transferId && f.tag !== undefined) {
        lt.receipt(o.handle, { authenticated: o.verifyReceipt(f.tag), blobId: o.blobId, digest: o.digest, length: o.total });
        const view = lt.senderView(o.handle);
        confirmed = "confirmed" in view && view.confirmed;
      }
      // Anything else (a refused or unexpected frame) is ignored: a peer cannot move the sender with a frame it cannot form.
    },
    get confirmed() {
      return confirmed;
    },
  };
}

export interface LanReceiverOptions {
  readonly core: Core;
  readonly engines: ScopeEngines;
  readonly channel: LanChannel;
  readonly handle: string;
  /** Durable prefix already stored for this transfer (a resumed transfer), else 0. */
  readonly storedOffset: number;
  /** Durably store the bytes at the offset before the driver acknowledges them. */
  readonly write: (offset: number, bytes: Uint8Array) => void;
  /** After the last byte: true when the SHA-256 of the stored bytes equals the digest from object_begin. */
  readonly digestMatches: (declaredDigest: string) => boolean;
  /** The authenticated receipt tag the host computes for this transfer. */
  readonly receiptTag: (transferId: string) => string;
}

export interface LanReceiver {
  onFrame(bytes: Uint8Array): void;
  readonly complete: boolean;
}

export function createLanReceiver(o: LanReceiverOptions): LanReceiver {
  const lt = o.engines.lanTransfer;
  let transferId = "";
  let digest = "";
  let acknowledged = o.storedOffset;
  let complete = false;
  const ack = (): void => {
    o.channel.send(frame(o.core, { kind: "ack", transferId, offset: acknowledged }));
  };
  return {
    onFrame(bytes) {
      const f = decode(o.core, bytes);
      if (f.kind === "begin" && f.transferId !== undefined && f.totalBytes !== undefined) {
        transferId = f.transferId;
        digest = (f as { sha256?: string }).sha256 ?? "";
        const opened = lt.openReceiver(o.handle, f.totalBytes, o.storedOffset > 0 ? o.storedOffset : undefined);
        if ("refused" in opened) return;
        ack();
      } else if (f.kind === "chunk" && f.transferId === transferId && f.offset !== undefined && f.bytes !== undefined) {
        const data = fromHex(f.bytes);
        const r = lt.accept(o.handle, f.offset, data.length);
        if ("refused" in r) return; // out of order or over length: nothing stored, nothing acknowledged
        o.write(f.offset, data);
        acknowledged = r.acknowledged;
        ack();
      } else if (f.kind === "end" && f.transferId === transferId) {
        const view = lt.receiverView(o.handle);
        if ("complete" in view && view.complete && o.digestMatches(digest)) {
          complete = true;
          o.channel.send(frame(o.core, { kind: "receipt", transferId, tag: o.receiptTag(transferId) }));
        }
      }
    },
    get complete() {
      return complete;
    },
  };
}
