// TEST AND DEV ONLY: an in-process stand-in for the direct byte channel of ADR-0158 section 4 (D120: the real one, a WebRTC data
// channel signalled by sealed envelopes, is not built because `local_delivery` is not an authorized scope). Two profiles register
// as listeners; `transportFor(profile)` connects to the other one. It keeps frame order, never shares memory between ends, and
// adds what a real channel does: a cut at any frame, silent loss of one kind of frame, no route at all. Never import it from
// product code (the production guards cover `test-support`, which re-exports it).
import type { DirectChannel, DirectTransport } from "./direct-driver";
import type { DirectFrame } from "./types";

export type Direction = "toSender" | "toReceiver";

export interface DirectFaults {
  /** The transport for one profile: `connect` reaches the other registered listener, `listen` registers this profile. */
  transportFor(profile: string): DirectTransport;
  /** While down, `connect` finds no way (`null`). */
  setDown(down: boolean): void;
  /** Cut the next channel after `n` frames have gone toward the receiver (both ends then see a close). */
  cutAfter(n: number): void;
  /** Silently lose the next frame of this kind in this direction (the sender of it sees a normal send). */
  loseNext(kind: DirectFrame["kind"], dir: Direction): void;
  /** Flip one bit of the next chunk sent toward the receiver (a faulty or hostile path). */
  corruptNextChunk(): void;
  /** Open a channel to the other listener and push these frames from the connecting end, as a hostile peer would. Resolves after delivery. */
  forge(from: string, frames: readonly DirectFrame[]): Promise<void>;
  /** The frames of every channel in order, with their direction. */
  readonly log: readonly { readonly dir: Direction; readonly frame: DirectFrame }[];
  /** Channels opened so far. */
  readonly opened: () => number;
}

class End implements DirectChannel {
  peer!: End;
  private frame: ((f: DirectFrame) => void) | undefined;
  private closeH: (() => void) | undefined;
  private drain: (() => void) | undefined;
  closed = false; // close() was called here: nothing more can be sent
  private gone = false; // the close reached this end: frames already in flight were delivered first, as on a graceful close
  readonly buffered = 0;
  constructor(readonly dir: Direction, private readonly net: Net) {}
  send(f: DirectFrame): Promise<void> {
    if (this.closed) return Promise.reject(new Error("closed"));
    this.net.deliver(this, f);
    return Promise.resolve();
  }
  onFrame(h: (f: DirectFrame) => void): void { this.frame = h; }
  onClose(h: () => void): void { this.closeH = h; }
  onDrain(h: () => void): void { this.drain = h; }
  close(): void {
    if (this.closed) return;
    this.closed = true;
    queueMicrotask(() => { this.gone = true; this.closeH?.(); });
    this.peer.close();
  }
  receive(f: DirectFrame): void { if (!this.gone) this.frame?.(f); }
  signalDrain(): void { this.drain?.(); }
}

class Net {
  down = false;
  cut: number | null = null;
  corrupt = false;
  toReceiver = 0;
  readonly lose: { kind: string; dir: Direction }[] = [];
  readonly log: { dir: Direction; frame: DirectFrame }[] = [];
  readonly listeners = new Map<string, (c: DirectChannel) => void>();
  count = 0;
  deliver(from: End, f: DirectFrame): void {
    // `from.dir` names where a frame sent from this end goes.
    const dir = from.dir;
    const at = this.lose.findIndex((l) => l.kind === f.kind && l.dir === dir);
    this.log.push({ dir, frame: f });
    if (at >= 0) { this.lose.splice(at, 1); return; }
    if (dir === "toReceiver") {
      this.toReceiver++;
      if (this.cut !== null && this.toReceiver > this.cut) { this.cut = null; from.close(); return; }
    }
    const to = from.peer;
    // A copy, as a wire would make: the two ends never share an object.
    let copy = JSON.parse(JSON.stringify(f)) as DirectFrame;
    if (this.corrupt && copy.kind === "chunk") {
      this.corrupt = false;
      const first = (parseInt(copy.bytes.slice(0, 2), 16) ^ 1).toString(16).padStart(2, "0");
      copy = { ...copy, bytes: first + copy.bytes.slice(2) };
    }
    queueMicrotask(() => { to.receive(copy); });
  }
}

export function createDirectFaults(): DirectFaults {
  const net = new Net();
  const connectTo = (from: string): DirectChannel | null => {
    if (net.down) return null;
    const target = [...net.listeners].find(([p]) => p !== from);
    if (target === undefined) return null;
    net.count++;
    net.toReceiver = 0;
    // Both ends named by where THEIR sends go: the connecting end is the receiver of the file, so it sends toSender.
    const client = new End("toSender", net);
    const server = new End("toReceiver", net);
    client.peer = server;
    server.peer = client;
    target[1](server);
    return client;
  };
  return {
    transportFor: (profile) => ({
      connect: () => Promise.resolve(connectTo(profile)),
      listen: (_ctx, onChannel) => { net.listeners.set(profile, onChannel); return () => { net.listeners.delete(profile); }; },
    }),
    setDown: (d) => { net.down = d; },
    cutAfter: (n) => { net.cut = n; },
    corruptNextChunk: () => { net.corrupt = true; },
    loseNext: (kind, dir) => { net.lose.push({ kind, dir }); },
    async forge(from, frames) {
      const ch = connectTo(from);
      if (ch === null) return;
      for (const f of frames) await ch.send(f);
      await new Promise<void>((r) => { setTimeout(r, 0); });
      ch.close();
    },
    log: net.log,
    opened: () => net.count,
  };
}
