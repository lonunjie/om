// SPDX-License-Identifier: MPL-2.0
// The `om-lan-v1` byte channel of LanDeliveryProfile-v1 (ADR-0158 section 4): an ordered, reliable RTCDataChannel between
// two peers on one network, host candidates only, no STUN, TURN or project-run signalling. The offer and answer are returned
// and accepted as text so the host can seal them in a `LanSignalEnvelopeV1` and carry them by copy/paste or QR; this file
// never sees the seed and holds no identity. A runtime that cannot build the channel reports `unavailable`, and there is no
// weaker fallback (ADR-0158 section 4). The channel is a transport: it names no peer and grants no authority.

export const LAN_CHANNEL_LABEL = "om-lan-v1";
/** Sender stops enqueueing above this many buffered bytes and resumes at or below the low mark (ADR-0158 section 1). */
export const HIGH_WATER = 262_144;
export const LOW_WATER = 65_536;
const GATHER_MS = 3000;
const CONNECT_MS = 20_000;
const MAX_SDP_BYTES = 12_288; // the signal envelope carries at most 12,288 bytes of SDP (lan.cddl)

export type LanChannelFailure = "unavailable" | "connection" | "signal";
export class LanChannelError extends Error {
  constructor(readonly reason: LanChannelFailure) { super(reason); }
}

export interface LanChannelEnvironment {
  /** Absent when the runtime has no RTCPeerConnection or is not a secure context. */
  readonly peer?: (config: RTCConfiguration) => RTCPeerConnection;
}

export function browserLanEnvironment(): LanChannelEnvironment {
  const ok = globalThis.isSecureContext && typeof RTCPeerConnection === "function";
  return ok ? { peer: (config) => new RTCPeerConnection(config) } : {};
}

/** Static precondition only: a peer connection can be constructed. It does not show the engine can establish a channel;
 *  the capability row comes from `probeLanChannel` (PLATFORM_CAPABILITY_MATRIX: report unavailable if the browser cannot
 *  establish it). */
export function lanChannelCapability(env: LanChannelEnvironment): "available" | "unavailable" {
  return env.peer ? "available" : "unavailable";
}

const PROBE_MS = 8000;

/** The capability row, from an actual establish attempt: two peers of this runtime are connected to each other on the
 *  host (host candidates only, nothing leaves the device) and a channel must open on both. Any failure or timeout is
 *  `unavailable`; there is no weaker answer. It proves the engine can build the channel, not that a given network or
 *  permission setting allows reaching another device. */
export async function probeLanChannel(env: LanChannelEnvironment): Promise<"available" | "unavailable"> {
  if (lanChannelCapability(env) === "unavailable") return "unavailable";
  let a: LanPeer | undefined;
  let b: LanPeer | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    a = createLanPeer(env);
    b = createLanPeer(env);
    const [pa, pb] = [a, b];
    const attempt = (async (): Promise<boolean> => {
      const answer = await pb.accept(await pa.offer());
      await pa.finish(answer);
      const [x, y] = await Promise.all([pa.opened(), pb.opened()]);
      x.close();
      y.close();
      return true;
    })();
    const limit = new Promise<boolean>((resolve) => { timer = setTimeout(() => { resolve(false); }, PROBE_MS); });
    void attempt.catch(() => undefined);
    return (await Promise.race([attempt.catch(() => false), limit])) ? "available" : "unavailable";
  } catch {
    return "unavailable";
  } finally {
    clearTimeout(timer);
    a?.close();
    b?.close();
  }
}

export interface LanDataChannel {
  /** One message is exactly one canonical `LanFrameV1`. */
  send(bytes: Uint8Array): void;
  /** True while the buffered amount is at or below the high mark. */
  canSend(): boolean;
  /** Called once each time the buffer drains to the low mark after having been above it. */
  onDrain(cb: () => void): void;
  onMessage(cb: (bytes: Uint8Array) => void): void;
  onClose(cb: () => void): void;
  close(): void;
}

export interface LanPeer {
  /** Offer SDP (plain text) to seal and hand to the other peer. */
  offer(): Promise<string>;
  /** Takes the other peer's offer, returns the answer SDP to seal and return. */
  accept(offerSdp: string): Promise<string>;
  /** The offerer takes the answer. */
  finish(answerSdp: string): Promise<void>;
  /** Resolves with the open channel, or rejects with `connection` after the timeout. */
  opened(): Promise<LanDataChannel>;
  close(): void;
}

function bounded(sdp: string): string {
  if (new TextEncoder().encode(sdp).length > MAX_SDP_BYTES) throw new LanChannelError("signal");
  return sdp;
}

function gathered(pc: RTCPeerConnection): Promise<void> {
  return new Promise((resolve) => {
    if (pc.iceGatheringState === "complete") return resolve();
    const done = (): void => { clearTimeout(timer); pc.removeEventListener("icegatheringstatechange", check); resolve(); };
    const check = (): void => { if (pc.iceGatheringState === "complete") done(); };
    const timer = setTimeout(done, GATHER_MS);
    pc.addEventListener("icegatheringstatechange", check);
  });
}

function wrap(dc: RTCDataChannel, pc: RTCPeerConnection): LanDataChannel {
  dc.binaryType = "arraybuffer";
  dc.bufferedAmountLowThreshold = LOW_WATER;
  let drained: (() => void) | undefined;
  // Hysteresis (ADR-0158 section 6): once the buffer passes the high mark, sending stays paused until it is at or below the low mark.
  let paused = false;
  dc.addEventListener("bufferedamountlow", () => { paused = false; drained?.(); });
  // Frames that arrive before the consumer attaches are kept, not dropped.
  let listener: ((bytes: Uint8Array) => void) | undefined;
  const early: Uint8Array[] = [];
  dc.addEventListener("message", (e: MessageEvent<ArrayBuffer>) => {
    if (!(e.data instanceof ArrayBuffer)) return;
    const bytes = new Uint8Array(e.data);
    if (listener) listener(bytes); else early.push(bytes);
  });
  return {
    send: (bytes) => {
      dc.send(bytes as Uint8Array<ArrayBuffer>);
      if (dc.bufferedAmount > HIGH_WATER) paused = true;
    },
    canSend: () => {
      if (paused && dc.bufferedAmount <= LOW_WATER) paused = false;
      return !paused && dc.bufferedAmount <= HIGH_WATER;
    },
    onDrain: (cb) => { drained = cb; },
    onMessage: (cb) => { listener = cb; for (const b of early.splice(0)) cb(b); },
    onClose: (cb) => { dc.addEventListener("close", cb); },
    // Closing the channel also tears down the peer connection (ICE and DTLS state, ADR-0158 section 9).
    close: () => { dc.close(); pc.close(); },
  };
}

/** Resolves with the wrapped channel once `dc` is open; rejects on timeout or a failed connection. */
function opening(pc: RTCPeerConnection, attach: (onChannel: (dc: RTCDataChannel) => void) => void): Promise<LanDataChannel> {
  const result = new Promise<LanDataChannel>((resolve, reject) => {
    const fail = (): void => { clearTimeout(timer); pc.close(); reject(new LanChannelError("connection")); };
    const timer = setTimeout(fail, CONNECT_MS);
    pc.addEventListener("connectionstatechange", () => {
      if (pc.connectionState === "failed" || pc.connectionState === "closed") fail();
    });
    attach((dc) => {
      // The channel must be ordered and fully reliable (ADR-0158 section 4); any other shape is refused.
      if (!dc.ordered || dc.maxRetransmits != null || dc.maxPacketLifeTime != null) return fail();
      const ready = (): void => { clearTimeout(timer); resolve(wrap(dc, pc)); };
      if (dc.readyState === "open") ready(); else dc.addEventListener("open", ready, { once: true });
    });
  });
  void result.catch(() => undefined); // the caller may await later; a rejection must not be unhandled meanwhile
  return result;
}

export function createLanPeer(env: LanChannelEnvironment): LanPeer {
  if (!env.peer) throw new LanChannelError("unavailable");
  // Host candidates only: no ICE servers, no candidate pool.
  const pc = env.peer({ iceServers: [], iceCandidatePoolSize: 0 });
  let open: Promise<LanDataChannel> | undefined;
  // One attempt per peer: fresh ICE and DTLS credentials every time (ADR-0158 sections 3 and 9).
  const once = (): void => { if (open) throw new LanChannelError("signal"); };
  const sdp = (): string => {
    const d = pc.localDescription;
    if (!d) throw new LanChannelError("signal");
    return bounded(d.sdp);
  };
  return {
    async offer() {
      once();
      const dc = pc.createDataChannel(LAN_CHANNEL_LABEL, { ordered: true });
      open = opening(pc, (on) => { on(dc); });
      await pc.setLocalDescription(await pc.createOffer());
      await gathered(pc);
      return sdp();
    },
    async accept(offerSdp) {
      once();
      bounded(offerSdp);
      open = opening(pc, (on) => {
        pc.addEventListener("datachannel", (e) => { if (e.channel.label === LAN_CHANNEL_LABEL) on(e.channel); });
      });
      await pc.setRemoteDescription({ type: "offer", sdp: offerSdp });
      await pc.setLocalDescription(await pc.createAnswer());
      await gathered(pc);
      return sdp();
    },
    async finish(answerSdp) {
      await pc.setRemoteDescription({ type: "answer", sdp: bounded(answerSdp) });
    },
    opened() {
      return open ?? Promise.reject(new LanChannelError("signal"));
    },
    close() { pc.close(); },
  };
}
