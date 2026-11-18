// The host driver for direct large-file transfer (ADR-0158 section 8, ADR-0194, D109 to D112). Core owns every decision: the
// sender and receiver state machines, both digests, the receipt MAC, resume offsets and the honest states. This driver only
// moves `DirectFrame`s between Core and an ordered, reliable byte channel, and does the two host jobs Core names:
//
//   * receiver: when a fetch is verified, write the ciphertext to the local encrypted blob cache, THEN tell Core the commit is
//     durable (which releases the receipt frame), send that frame, and only then open the file (commit before open, D109);
//   * sender: nothing is kept here. Core keeps the sealed object until a valid receipt arrives; the driver feeds it frames,
//     pumps chunks while the channel drains, and never reports a state of its own (every state shown comes from Core).
//
// It fails closed: an ack or a receipt is never believed here, only handed to Core, which treats an ack as a claim and checks a
// receipt's tag. Retries are bounded; a dropped channel resumes from the bytes Core still holds; a refusal or a failed digest ends
// the fetch. No plaintext is written anywhere by this driver: it sees ciphertext frames, hands ciphertext to the cache, and gives the
// opened bytes back to the caller in memory.
//
// The transport is abstract. ADR-0158 section 4 defines the real one (a WebRTC data channel `om-lan-v1`, signalled by sealed
// `LanSignalEnvelopeV1`) but `local_delivery` is not an authorized scope and no signalling codec exists (D120), so this package
// ships the interface and a loopback in tests; no product transport is wired.
import type { AttachmentMeta, AttachmentOpenResult, MessageId, OperationId, ProfileExecutionContext, ProfileId } from "@om/platform-api";
import { asConversationId, asOperationId } from "@om/platform-api";
import type { DirectFrame } from "./types";
import type { WasmCoreClient } from "./runtime";

/** An ordered, reliable stream of whole frames to one other device. The host adapts a real channel to this. */
export interface DirectChannel {
  /** Queue one frame. Rejects when the channel is closed or cannot take it; resolves once queued (not once delivered). */
  send(frame: DirectFrame): Promise<void>;
  /** Bytes queued but not yet on the wire (`RTCDataChannel.bufferedAmount`). */
  readonly buffered: number;
  /** Register once, before the first frame can arrive. */
  onFrame(handler: (frame: DirectFrame) => void): void;
  onClose(handler: () => void): void;
  /** The buffer fell to or below the low-water mark: more chunks may be sent. */
  onDrain(handler: () => void): void;
  close(): void;
}

export interface DirectTransport {
  /** Receiver: open a channel to the device that sent this message's file. `null`: no way to reach it now. */
  connect(ctx: ProfileExecutionContext, target: { readonly messageId: string; readonly index: number }): Promise<DirectChannel | null>;
  /** Sender: be reachable for this profile. Each incoming channel is handed over; returns a function that stops listening. */
  listen(ctx: ProfileExecutionContext, onChannel: (channel: DirectChannel) => void): () => void;
}

/** The host's local encrypted blob cache for a received object. `put` resolves only when the bytes are durable; a rejection means they are not. */
export interface DirectCiphertextCache {
  put(profileId: ProfileId, messageId: string, index: number, ciphertext: Uint8Array): Promise<void>;
}

export interface PickedFile { readonly bytes: Uint8Array; readonly meta: AttachmentMeta }

export interface DirectDriverOptions {
  readonly core: Pick<WasmCoreClient, "direct" | "attachments" | "dispatch" | "query">;
  readonly transport: DirectTransport;
  readonly cache: DirectCiphertextCache;
  /** Lets the person choose the file again for "Send again". The driver does not keep it. Absent: `sendAgain` answers `no_picker`. */
  readonly pickFile?: () => Promise<PickedFile | null>;
  /** Attempts per fetch (a drop, an idle channel or an unreachable peer each use one). Default 5. */
  readonly maxAttempts?: number;
  /** First wait before a repeat attempt; doubles each time, capped at 30 s. Default 1000 ms; 0 retries at once. */
  readonly retryMs?: number;
  /** A channel that says nothing for this long is closed and the attempt ends. Default 30 s. */
  readonly idleMs?: number;
  /** More frames than this waiting to be handled closes the channel (a flood). Default 256. */
  readonly maxQueuedFrames?: number;
  /** Time between sender pumps while a channel is open. Default 1000 ms. */
  readonly tickMs?: number;
  readonly setTimer?: (fn: () => void, ms: number) => unknown;
  readonly clearTimer?: (handle: unknown) => void;
}

export type DirectFetchFailure = "no_channel" | "refused" | "failed" | "commit_failed" | "gave_up" | "stopped";
export type DirectFetchResult =
  | { readonly ok: true; readonly bytes: Uint8Array; readonly mediaType: string; readonly filename?: string; /** The receipt frame was handed to the channel (not proof it arrived). */ readonly receiptSent: boolean }
  | { readonly ok: false; readonly reason: DirectFetchFailure };
export type SendAgainResult =
  | { readonly ok: true; readonly operationId: OperationId }
  | { readonly ok: false; readonly reason: "no_picker" | "cancelled" | "not_resendable" | "refused" };

export interface DirectDriver {
  /** Be reachable for this profile (sender side) and keep pumping while a channel is open. Call when the profile opens. */
  start(ctx: ProfileExecutionContext): void;
  /** The profile locked or closed: stop listening, close its channel, drop nothing durable. */
  stop(profileId: ProfileId): void;
  /** Receiver: fetch one attachment, commit it to the cache, send the receipt, then open it. One fetch per file at a time. */
  fetch(ctx: ProfileExecutionContext, messageId: MessageId, index: number): Promise<DirectFetchResult>;
  /** After a restart: hand the original file back so a live offer is served again (Core checks it is the same file). */
  rearm(ctx: ProfileExecutionContext, blobId: string, plaintext: Uint8Array): Promise<boolean>;
  /** "Send again" (D112): choose the file again and offer it as a new direct message in the same conversation. */
  sendAgain(ctx: ProfileExecutionContext, messageId: MessageId): Promise<SendAgainResult>;
  /** The page came back (D88, D118): pump now instead of waiting for a timer the browser held back. */
  resume(): void;
  /** The `ConversationServices.sendAgain` for one profile context. */
  sendAgainFor(ctx: ProfileExecutionContext): (messageId: MessageId, index: number) => void;
  dispose(): void;
}

interface Serving {
  readonly ctx: ProfileExecutionContext;
  readonly channel: DirectChannel;
  readonly id: number;
  chain: Promise<void>;
  dead: boolean;
  detached: boolean;
}
let nextChannelId = 1;
interface Timers { readonly set: (fn: () => void, ms: number) => unknown; readonly clear: (h: unknown) => void }

const MAX_BACKOFF_MS = 30_000;

export function createDirectDriver(o: DirectDriverOptions): DirectDriver {
  const { core, transport, cache } = o;
  const maxAttempts = Math.max(1, Math.floor(o.maxAttempts ?? 5));
  const retryMs = o.retryMs ?? 1000;
  const idleMs = o.idleMs ?? 30_000;
  const maxQueued = o.maxQueuedFrames ?? 256;
  const tickMs = o.tickMs ?? 1000;
  const timers: Timers = {
    set: o.setTimer ?? ((fn, ms) => setTimeout(fn, ms)),
    clear: o.clearTimer ?? ((h) => { clearTimeout(h as ReturnType<typeof setTimeout>); }),
  };
  const listening = new Map<ProfileId, () => void>();
  const serving = new Map<ProfileId, Set<Serving>>();
  const tickers = new Map<ProfileId, { timer: unknown; busy: boolean }>();
  const fetching = new Map<string, Promise<DirectFetchResult>>();
  const waiting = new Set<() => void>(); // retry waits that dispose() must release
  let disposed = false;
  const stopped = (): boolean => disposed; // read afresh after every await (TypeScript narrows the flag itself)

  // ---- sender: serve a channel ----------------------------------------------------------------------------------------------

  const closed = (s: Serving): boolean => s.dead;
  const detached = (s: Serving): boolean => s.detached;

  async function pumpAll(s: Serving): Promise<void> {
    if (closed(s)) return;
    for (const f of core.direct.pump(s.ctx, s.channel.buffered, s.id)) {
      if (closed(s)) break;
      await s.channel.send(f);
    }
  }

  function forget(s: Serving): void {
    s.dead = true;
    const channels = serving.get(s.ctx.profileId);
    channels?.delete(s);
    if (channels?.size === 0) serving.delete(s.ctx.profileId);
  }

  function detach(s: Serving): void {
    s.detached = true;
    try { core.direct.disconnected(s.ctx, s.id); } catch { /* the profile has already closed */ }
  }

  function drop(s: Serving): void {
    if (s.detached) return;
    forget(s);
    detach(s);
    try { s.channel.close(); } catch { /* already closed */ }
  }

  function run(s: Serving, work: () => Promise<void>): Promise<void> {
    s.chain = s.chain.then(work).catch(() => { drop(s); });
    return s.chain;
  }

  function serveChannel(ctx: ProfileExecutionContext, channel: DirectChannel): void {
    if (disposed) { channel.close(); return; }
    const s: Serving = { ctx, channel, id: nextChannelId++, chain: Promise.resolve(), dead: false, detached: false };
    const channels = serving.get(ctx.profileId) ?? new Set<Serving>();
    channels.add(s);
    serving.set(ctx.profileId, channels);
    let queued = 0;
    channel.onFrame((frame) => {
      if (s.dead) return;
      if (++queued > maxQueued) { drop(s); return; }
      void run(s, async () => {
        queued--;
        // A receipt already queued before the channel closed still belongs to this transfer.
        if (detached(s) || (closed(s) && frame.kind === "fetch_blob")) return;
        const frames = await core.direct.serve(s.ctx, frame, s.id);
        if (detached(s)) { detach(s); return; } // close during the runtime's commit
        for (const f of frames) {
          if (closed(s)) break;
          await channel.send(f);
        }
        await pumpAll(s);
      });
    });
    channel.onDrain(() => { if (!s.dead) void run(s, () => pumpAll(s)); });
    channel.onClose(() => {
      forget(s);
      void run(s, () => { detach(s); return Promise.resolve(); });
    });
    wake(ctx.profileId);
  }

  // ---- ticks (the D88 pattern: a timer while something is open, resume() reads again now) ------------------------------------

  function wake(p: ProfileId): void {
    if (disposed || tickers.has(p)) return;
    const t = { timer: undefined as unknown, busy: false };
    tickers.set(p, t);
    arm(p, t);
  }
  function arm(p: ProfileId, t: { timer: unknown; busy: boolean }): void {
    t.timer = timers.set(() => { t.timer = undefined; void tick(p, t); }, tickMs);
  }
  async function tick(p: ProfileId, t: { timer: unknown; busy: boolean }): Promise<void> {
    t.busy = true;
    await Promise.all([...(serving.get(p) ?? [])].map((s) => run(s, () => pumpAll(s))));
    t.busy = false;
    if (disposed || tickers.get(p) !== t) return;
    if (serving.get(p) === undefined) { tickers.delete(p); return; } // nothing open: no timer, nothing retained
    arm(p, t);
  }

  // ---- receiver: one fetch ---------------------------------------------------------------------------------------------------

  type Attempt = "done" | "closed" | "idle" | "refused" | "failed" | "commit_failed" | "no_channel";
  interface FetchRun { committed: boolean; receiptSent: boolean }

  async function attempt(ctx: ProfileExecutionContext, messageId: MessageId, index: number, run: FetchRun): Promise<Attempt> {
    let channel: DirectChannel | null;
    try { channel = await transport.connect(ctx, { messageId, index }); } catch { return "no_channel"; }
    if (channel === null) return "no_channel";
    const ch = channel;
    let first: DirectFrame | null;
    try { first = core.direct.fetchStart(ctx, messageId, index); } catch { ch.close(); return "failed"; }
    if (first === null) { ch.close(); return "refused"; } // a stored object, a withdrawn fetch: not a direct one
    return new Promise<Attempt>((resolve) => {
      let finished = false;
      let timer: unknown;
      let queued = 0;
      let chain: Promise<void> = Promise.resolve();
      const finish = (r: Attempt): void => {
        if (finished) return;
        finished = true;
        if (timer !== undefined) timers.clear(timer);
        if (r !== "done" || run.receiptSent) { try { ch.close(); } catch { /* closed */ } }
        resolve(r);
      };
      const bump = (): void => {
        if (timer !== undefined) timers.clear(timer);
        timer = timers.set(() => { finish("idle"); }, idleMs);
      };
      const sendAll = async (frames: readonly DirectFrame[]): Promise<void> => {
        for (const f of frames) {
          await ch.send(f);
          if (f.kind === "receipt") run.receiptSent = true;
        }
      };
      ch.onClose(() => { finish(run.committed && run.receiptSent ? "done" : "closed"); });
      ch.onDrain(() => { /* the receiver sends only small acks */ });
      ch.onFrame((frame) => {
        if (finished) return;
        if (++queued > maxQueued) { finish("closed"); return; }
        chain = chain.then(async () => {
          queued--;
          if (finished) return;
          bump();
          await sendAll(core.direct.fetchHandle(ctx, messageId, index, frame));
          const st = core.direct.fetchProgress(ctx, messageId, index);
          if (st === null) { finish("failed"); return; }
          if (st.state === "failed") { finish("failed"); return; }
          // The sender answered the request with an error (withdrawn, lapsed, not armed, wrong capability: one uniform answer). Core keeps what it holds.
          if (frame.kind === "error" && st.state !== "complete" && st.state !== "committed") { finish("refused"); return; }
          if (frame.kind === "end" && (st.state === "complete" || st.state === "committed")) {
            // The sender's transfer ended and both digests held. Durable commit first (D109), then the receipt Core releases.
            if (!run.committed) {
              const ct = core.direct.fetchCiphertext(ctx, messageId, index);
              if (ct === null) { finish("failed"); return; }
              try { await cache.put(ctx.profileId, messageId, index, ct); } catch { finish("commit_failed"); return; }
              run.committed = true;
              await sendAll(core.direct.fetchCommitted(ctx, messageId, index));
            }
            finish("done");
          }
        }).catch(() => { finish("closed"); }); // a send that failed: the channel is gone; the bytes held stay in Core
      });
      bump();
      ch.send(first).catch(() => { finish("closed"); });
    });
  }

  const wait = (ms: number): Promise<void> => new Promise((resolve) => {
    if (ms <= 0 || disposed) { resolve(); return; }
    const release = (): void => { timers.clear(h); waiting.delete(release); resolve(); };
    const h = timers.set(release, ms);
    waiting.add(release);
  });

  async function fetchOnce(ctx: ProfileExecutionContext, messageId: MessageId, index: number): Promise<DirectFetchResult> {
    const run: FetchRun = { committed: false, receiptSent: false };
    let last: Attempt = "no_channel";
    for (let n = 1; n <= maxAttempts; n++) {
      if (stopped()) return { ok: false, reason: "stopped" };
      if (n > 1) await wait(Math.min(MAX_BACKOFF_MS, retryMs * 2 ** (n - 2)));
      if (stopped()) return { ok: false, reason: "stopped" };
      last = await attempt(ctx, messageId, index, run);
      if (last === "refused") return { ok: false, reason: "refused" };
      if (last === "failed") return { ok: false, reason: "failed" };
      if (last === "commit_failed") return { ok: false, reason: "commit_failed" }; // the verified bytes stay in Core: fetching again retries the write
      if (last === "done" && run.receiptSent) break;
      // done without a receipt on the wire (the channel dropped first): connect again, Core answers from what it holds and releases the receipt
    }
    if (!run.committed) return { ok: false, reason: last === "no_channel" ? "no_channel" : "gave_up" };
    // Committed. Open now: if the receipt never went out the sender keeps saying "they say they have it" until a later fetch.
    let opened: AttachmentOpenResult;
    try { opened = core.direct.fetchOpen(ctx, messageId, index); } catch { return { ok: false, reason: "failed" }; }
    if (!opened.ok) return { ok: false, reason: "failed" };
    return { ok: true, bytes: opened.bytes, mediaType: opened.mediaType, ...(opened.filename === undefined ? {} : { filename: opened.filename }), receiptSent: run.receiptSent };
  }

  // ---- send again --------------------------------------------------------------------------------------------------------------

  async function sendAgain(ctx: ProfileExecutionContext, messageId: MessageId): Promise<SendAgainResult> {
    if (o.pickFile === undefined) return { ok: false, reason: "no_picker" };
    // Only a lapsed, unconfirmed offer: anything else is either still live (nothing to resend) or not ours.
    let offer;
    try { offer = (await core.query({ kind: "GetDirectOffers", ctx })).find((x) => x.messageId === messageId); } catch { return { ok: false, reason: "not_resendable" }; }
    if (offer === undefined || offer.state !== "not_confirmed") return { ok: false, reason: "not_resendable" };
    let picked: PickedFile | null;
    try { picked = await o.pickFile(); } catch { picked = null; }
    if (picked === null) return { ok: false, reason: "cancelled" };
    // The file is sealed afresh (a new key and BlobID: the lapsed offer's key was shown to the peer, a new message carries a new one).
    const prep = await core.attachments.prepareDirect?.(ctx, picked.bytes, picked.meta);
    if (prep?.ok !== true) return { ok: false, reason: "refused" };
    const operationId = asOperationId(`direct-again-${globalThis.crypto.randomUUID()}`);
    await core.dispatch({ kind: "SendAttachmentsDirect", ctx, operationId, conversationId: asConversationId(offer.conversationId), draft: { text: "" }, attachments: [{ blobId: prep.blobId }] });
    return { ok: true, operationId };
  }

  return {
    start(ctx) {
      if (disposed || listening.has(ctx.profileId)) return;
      listening.set(ctx.profileId, transport.listen(ctx, (channel) => { serveChannel(ctx, channel); }));
    },
    stop(profileId) {
      listening.get(profileId)?.();
      listening.delete(profileId);
      for (const s of serving.get(profileId) ?? []) drop(s);
      const t = tickers.get(profileId);
      if (t !== undefined && t.timer !== undefined) timers.clear(t.timer);
      tickers.delete(profileId);
    },
    fetch(ctx, messageId, index) {
      const key = `${ctx.profileId}/${messageId}/${String(index)}`;
      const cur = fetching.get(key);
      if (cur !== undefined) return cur;
      const p = fetchOnce(ctx, messageId, index).finally(() => { fetching.delete(key); });
      fetching.set(key, p);
      return p;
    },
    rearm: (ctx, blobId, plaintext) => core.attachments.arm(ctx, blobId, plaintext),
    sendAgain,
    resume() {
      if (disposed) return;
      for (const [p, t] of [...tickers]) {
        if (t.busy) continue; // a pump is in flight: it re-arms itself
        if (t.timer !== undefined) timers.clear(t.timer);
        t.timer = undefined;
        void tick(p, t);
      }
    },
    sendAgainFor: (ctx) => (messageId) => { void sendAgain(ctx, messageId); },
    dispose() {
      disposed = true;
      for (const off of listening.values()) off();
      listening.clear();
      for (const channels of [...serving.values()]) for (const s of channels) drop(s);
      for (const t of tickers.values()) if (t.timer !== undefined) timers.clear(t.timer);
      tickers.clear();
      for (const r of [...waiting]) r();
    },
  };
}
