// The host driver for ordinary groups (ADR-0177, ADR-0189). Core seals and opens everything: Invite, Accept and Decline are
// events on the 1:1 (the relationship lane), every other group wire is a sealed `GroupTransportFrame` for one destination
// device's per-Room route (the device lane). This driver only moves opaque bytes between Core and the provider:
//
//   * after each durable group step and at start-up it asks Core to `settle` (Core accepts its own Commit itself: no host
//     report, no timer, no relay acknowledgement, ADR-0189 section 2);
//   * it drains Core's outbox in order: seal an item, deposit each sealed wrapper at its entry, then tell Core the item is
//     done. A crash or an outage repeats the item (a duplicate is harmless: MLS drops it);
//   * Invite, Accept and Decline are kept by Core itself, in the commit of the step that produced them, until its 1:1 queue
//     takes them (`group_lane_flush`); this driver only asks Core to move them and keeps nothing of its own;
//   * it polls each per-Room mailbox Core names, hands each wrapper to Core, and acknowledges it after the step is durable;
//   * after a poll it retries held frames and asks for missing account heads; after a removal it asks Core for a fresh route
//     (Core queues the `route_update` frames in its own outbox, drained like any other item); it tops up the route offers Core needs to mint a route.
//
// It never reads a wire, never decides anything about a group and never builds one. Nothing here is a transport of its own:
// the carriage is the sealed delivery the provider already offers (deposit, poll, acknowledge).
import type {
  EncryptedKv, GroupApi, GroupEvent, GroupId, GroupIntent, GroupSender, GroupStep, GroupTarget, GroupWire, HeldGroupInvitations, ProfileExecutionContext, ProfileId,
} from "@om/platform-api";
import type { Provider } from "./types";

/** One sealed frame for one destination device: deposit `wrapper` (hex) at `entry`. */
export interface SealedFrame {
  readonly device: string;
  readonly wrapper: string;
  readonly entry: { readonly relayUri: string; readonly capability: string };
}
/** One frame Core owns until it is deposited. `to` and `wire` go back to `frameSend` unchanged. */
export interface GroupOutItem { readonly id: number; readonly intentId: string | null; readonly to: GroupTarget; readonly wire: GroupWire }
/** What Core says the host must watch for the device lane. */
export interface GroupCarriageStatus {
  readonly mailboxes: readonly { readonly mailbox: string; readonly relayUri: string }[];
  /** Rooms that owe a route update after a removal. */
  readonly routeOwed: readonly string[];
  readonly outbox: number;
  readonly pending: number;
  /** Invite, Accept and Decline Core keeps until its 1:1 queue takes them. */
  readonly laneOut: number;
  /** Such items Core dropped for good (refused for a reason that cannot change, or too old); never silent. */
  readonly laneLost: number;
}
export type FrameSend = { readonly ok: true; readonly sealed: readonly SealedFrame[] } | { readonly ok: false; readonly refused: string };

/** The low-level Core calls the driver needs; `createCoreRuntime` implements them over the wasm module (each one durable before it resolves). */
export interface GroupCore extends GroupApi {
  held(ctx: ProfileExecutionContext): HeldGroupInvitations;
  ackHeldOverflow(ctx: ProfileExecutionContext): Promise<void>;
  outbox(ctx: ProfileExecutionContext): readonly GroupOutItem[];
  status(ctx: ProfileExecutionContext): GroupCarriageStatus;
  frameSend(ctx: ProfileExecutionContext, to: GroupTarget, wire: GroupWire): Promise<FrameSend>;
  outboxDone(ctx: ProfileExecutionContext, ids: readonly number[]): Promise<void>;
  /** Send one Invite, Accept or Decline directly; it also takes the matching kept item, so nothing goes twice. */
  laneSend(ctx: ProfileExecutionContext, relationshipId: string, wire: GroupWire): Promise<{ readonly ok: true } | { readonly ok: false; readonly refused: string }>;
  /** Move the oldest kept Invite, Accept or Decline into Core's 1:1 queue; `moved` says one went (call again). */
  laneFlush(ctx: ProfileExecutionContext): Promise<{ readonly ok: true; readonly moved: boolean } | { readonly ok: false; readonly refused: string }>;
  lanePump(ctx: ProfileExecutionContext): Promise<GroupStep>;
  frameReceive(ctx: ProfileExecutionContext, wrapper: string): Promise<GroupStep>;
  pendingRefresh(ctx: ProfileExecutionContext): Promise<void>;
  pendingRetry(ctx: ProfileExecutionContext): Promise<GroupStep>;
  /** Mint this device's fresh route and queue the `route_update` frames in Core's outbox (`queued` of them). */
  routeUpdate(ctx: ProfileExecutionContext, conversationId: string): Promise<{ readonly ok: true; readonly queued: number } | { readonly ok: false; readonly refused: string }>;
  /** Retired routes whose overlap ended; the host revokes each deposit capability. */
  routesExpire(ctx: ProfileExecutionContext): Promise<readonly { readonly relayUri: string; readonly capability: string }[]>;
}

export interface GroupDriverOptions {
  readonly core: GroupCore;
  readonly provider: Provider;
  readonly storage: (profileId: ProfileId) => EncryptedKv;
  readonly contextFor: (profileId: ProfileId) => ProfileExecutionContext;
  /** Keep a few route offers in Core (a route is minted for each Accept, Welcome and Room route update). */
  readonly topUp: () => Promise<void>;
  /** Called after a step that queued something on the 1:1 (the runtime then performs it). */
  readonly wake?: (profileId: ProfileId) => void;
  /** Wait before repeating a pass that could not finish (default 2000 ms; 0 = only when `flush` is called). */
  readonly retryMs?: number;
}

export interface GroupDriver extends GroupApi {
  /** One full pass for the profile: settle, poll, pump, retry, route updates, then everything owed. True if anything moved. */
  flush(ctx: ProfileExecutionContext): Promise<boolean>;
  /** Resolves when no step or pass is in flight. */
  idle(): Promise<void>;
  /** Items still owed: Core's outbox plus the lane items Core keeps for its 1:1 queue. */
  pending(ctx: ProfileExecutionContext): Promise<number>;
  /** Frames this driver gave up on because the relay refused them for good (a revoked or unknown route). */
  readonly dropped: () => number;
  dispose(): void;
}

const CUR = "gdrv/cur/";

const enc = new TextEncoder();
const dec = new TextDecoder();
const destKey = (t: GroupTarget): string => ("relationship" in t ? `r:${t.relationship}` : `d:${t.device}`);
const refused = (r: string): GroupStep => ({ ok: false, refused: r });

export function createGroupDriver(opts: GroupDriverOptions): GroupDriver {
  const { core, provider } = opts;
  const retryMs = opts.retryMs ?? 2000;
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const listeners = new Set<(profileId: ProfileId, events: readonly GroupEvent[]) => void>();
  let disposed = false;
  let dropped = 0;
  let reported = 0;
  let tail: Promise<unknown> = Promise.resolve();
  let inflight = 0;
  const exclusive = <T>(fn: () => Promise<T>): Promise<T> => {
    inflight++;
    const run = tail.then(fn, fn);
    tail = run.catch(() => undefined).finally(() => { inflight--; });
    return run;
  };
  const available = (): boolean => core.available();
  const emit = (profileId: ProfileId, events: readonly GroupEvent[]): void => {
    if (events.length === 0) return;
    for (const l of [...listeners]) { try { l(profileId, events); } catch { /* a listener must not break the driver */ } }
  };
  /** After a durable step: Core accepts its own Commit once its frames are queued (ADR-0189 section 2). Events are merged into `step`. */
  async function settled(ctx: ProfileExecutionContext, step: Extract<GroupStep, { ok: true }>): Promise<Extract<GroupStep, { ok: true }>> {
    let events: readonly GroupEvent[] = step.events;
    for (let i = 0; i < 8; i++) {
      const s = await core.settle(ctx);
      if (!s.ok || (s.events.length === 0 && s.out.length === 0)) break;
      events = [...events, ...s.events];
    }
    return { ...step, events };
  }

  // ------------------------------------------------------------------------------------------ deposit

  type Deposited = "done" | "retry" | "refused";
  /** Deposit one sealed wrapper. A relay that refuses it for good (a revoked or unknown route) ends it; an outage repeats it. */
  async function deposit(s: SealedFrame): Promise<Deposited> {
    let r;
    try { r = await provider.perform({ call: "deposit", mailbox: s.entry.capability, relayUri: s.entry.relayUri, bytes: s.wrapper }); } catch { return "retry"; }
    if (r.result !== "err") return "done";
    if (r.error.kind === "unavailable" || r.error.kind === "timeout") return "retry";
    return "refused";
  }

  /** Core's outbox, oldest first: seal, deposit every wrapper, then report the item done. Order is kept per destination. */
  async function drainOutbox(ctx: ProfileExecutionContext): Promise<{ moved: boolean; retry: boolean }> {
    let moved = false;
    let retry = false;
    const blocked = new Set<string>();
    for (const item of core.outbox(ctx)) {
      const key = destKey(item.to);
      if (blocked.has(key)) continue; // nothing later goes ahead of an earlier frame for the same destination
      let sealed = await core.frameSend(ctx, item.to, item.wire);
      if (!sealed.ok && sealed.refused === "no_route_offer") { await opts.topUp(); sealed = await core.frameSend(ctx, item.to, item.wire); }
      if (!sealed.ok) {
        // A route that is missing or stale waits for the member's route update (ADR-0142 section 9: queue, never fall back).
        blocked.add(key);
        retry = true;
        continue;
      }
      let ok = true;
      for (const s of sealed.sealed) {
        const result = await deposit(s);
        if (result === "retry" || (result === "refused" && item.wire.t === "route_update")) { ok = false; break; }
        if (result === "refused") dropped++;
      }
      if (!ok) { retry = true; blocked.add(key); break; } // the provider is down: every later deposit would fail alike
      await core.outboxDone(ctx, [item.id]);
      moved = true;
    }
    return { moved, retry };
  }

  /** Invite, Accept and Decline: ask Core to move each kept one into its 1:1 queue (Core removes it in the same transaction). */
  async function drainLane(ctx: ProfileExecutionContext): Promise<{ moved: boolean; retry: boolean }> {
    let moved = false;
    for (let i = 0; i < 128; i++) {
      let r = await core.laneFlush(ctx);
      if (!r.ok && r.refused === "no_route_offer") { await opts.topUp(); r = await core.laneFlush(ctx); }
      // Every kept item waits (no route offer, relationship not usable yet): Core keeps them and we try again later.
      if (!r.ok) return { moved, retry: true };
      if (!r.moved) return { moved, retry: false };
      moved = true;
      opts.wake?.(ctx.profileId);
    }
    return { moved, retry: true };
  }

  // ------------------------------------------------------------------------------------------ receive

  /** Poll each per-Room mailbox, give each wrapper to Core, and acknowledge only what Core has made durable. */
  async function pollRooms(ctx: ProfileExecutionContext): Promise<boolean> {
    const kv = opts.storage(ctx.profileId);
    let moved = false;
    for (const m of core.status(ctx).mailboxes) {
      const ck = CUR + m.mailbox;
      const cur = await kv.get(ck);
      let after = cur.ok && cur.value !== undefined ? Number(dec.decode(cur.value)) : 0;
      let items;
      try { items = await provider.poll(m.mailbox, after, "relay", m.relayUri); } catch { continue; }
      for (const it of [...items].sort((a, b) => a.seq - b.seq)) {
        if (it.seq <= after) continue;
        const s = await core.frameReceive(ctx, it.bytes); // a refusal is a drop that changed nothing: the frame is not repeated
        if (s.ok) { emit(ctx.profileId, (await settled(ctx, s)).events); moved = true; }
        after = it.seq;
        await kv.put(ck, enc.encode(String(after)));
        try { await provider.perform({ call: "ack", mailbox: m.mailbox, seq: after, kind: "relay", relayUri: m.relayUri }); } catch { /* the cursor already keeps it from being handled twice */ }
      }
    }
    return moved;
  }

  async function pass(ctx: ProfileExecutionContext): Promise<{ moved: boolean; retry: boolean }> {
    let moved = false;
    let retry = false;
    const note = (r: { moved: boolean; retry: boolean }): void => { moved = moved || r.moved; retry = retry || r.retry; };
    await opts.topUp();
    // Start-up and after a crash: a Commit that was durable is finished before anything else.
    const first = await core.settle(ctx);
    if (first.ok && (first.events.length > 0 || first.out.length > 0)) { emit(ctx.profileId, first.events); moved = true; }
    if (await pollRooms(ctx)) moved = true;
    const pumped = await core.lanePump(ctx);
    if (pumped.ok && (pumped.events.length > 0 || pumped.out.length > 0)) { emit(ctx.profileId, (await settled(ctx, pumped)).events); moved = true; }
    // Held frames: ask for the heads they wait for, then retry what a poll or a head may have unblocked.
    if (core.status(ctx).pending > 0) {
      await core.pendingRefresh(ctx);
      opts.wake?.(ctx.profileId);
      const r = await core.pendingRetry(ctx);
      if (r.ok && (r.events.length > 0 || r.out.length > 0)) { emit(ctx.profileId, (await settled(ctx, r)).events); moved = true; }
    }
    // After a removal each remaining device mints a fresh route and tells the new roster (ADR-0189 section 1.3).
    for (const room of core.status(ctx).routeOwed) {
      const u = await core.routeUpdate(ctx, room); // route generation and per-destination distribution debt are committed together
      if (!u.ok) { if (u.refused === "no_route_offer") retry = true; continue; }
      moved = true;
    }
    for (const e of await core.routesExpire(ctx)) { try { await provider.revokeDeposit?.(e.relayUri, e.capability); } catch { /* a capability that is not revoked expires on its own */ } }
    note(await drainLane(ctx));
    note(await drainOutbox(ctx));
    // A deposit a relay refused for good ended its item: tell the client so it can say "could not deliver" (never silent).
    const lost = core.status(ctx).laneLost;
    if (dropped + lost > reported) { emit(ctx.profileId, [{ kind: "undeliverable", count: dropped + lost - reported }]); reported = dropped + lost; }
    return { moved, retry };
  }

  function schedule(ctx: ProfileExecutionContext): void {
    if (disposed || retryMs <= 0) return;
    const t = setTimeout(() => { timers.delete(t); void flush(ctx); }, retryMs);
    timers.add(t);
  }

  function flush(ctx: ProfileExecutionContext): Promise<boolean> {
    if (!available()) return Promise.resolve(false);
    return exclusive(async () => {
      let any = false;
      for (let i = 0; i < 20 && !disposed; i++) {
        let r;
        try { r = await pass(ctx); } catch { schedule(ctx); return any; }
        any = any || r.moved;
        if (r.retry) { schedule(ctx); return any; }
        if (!r.moved) return any;
      }
      return any;
    }).catch(() => false);
  }

  const kick = (ctx: ProfileExecutionContext): void => { if (!disposed) void flush(ctx); };

  async function run(ctx: ProfileExecutionContext, step: () => Promise<GroupStep>): Promise<GroupStep> {
    const s = await exclusive(async () => {
      let r = await step();
      if (r.ok) {
        r = await settled(ctx, r);
        emit(ctx.profileId, r.events);
      }
      return r;
    });
    kick(ctx);
    return s;
  }

  return {
    available,
    do: async (ctx, intent: GroupIntent) => {
      if (!available()) return refused("groups_unavailable");
      await opts.topUp(); // an Accept, an Invite or a Welcome mints a route and needs an offer
      return run(ctx, () => core.do(ctx, intent));
    },
    receive: (ctx, from: GroupSender, wire) => (available() ? run(ctx, () => core.receive(ctx, from, wire)) : Promise.resolve(refused("groups_unavailable"))),
    // Base-module queries: they work whether or not the group engine is present (that is their point).
    held: (ctx) => core.held(ctx),
    ackHeldOverflow: (ctx) => core.ackHeldOverflow(ctx),
    list: (ctx) => (available() ? core.list(ctx) : []),
    get: (ctx, id: GroupId) => (available() ? core.get(ctx, id) : null),
    invites: (ctx) => (available() ? core.invites(ctx) : []),
    messages: (ctx, id) => (available() ? core.messages(ctx, id) : []),
    notices: (ctx, id) => (available() ? core.notices(ctx, id) : []),
    settle: (ctx) => (available() ? run(ctx, () => core.settle(ctx)) : Promise.resolve(refused("groups_unavailable"))),
    onEvents(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    flush,
    async idle() {
      for (let i = 0; i < 100; i++) {
        const t = tail;
        await t;
        if (inflight === 0) return;
      }
    },
    pending(ctx) {
      return Promise.resolve(core.outbox(ctx).length + core.status(ctx).laneOut);
    },
    dropped: () => dropped,
    dispose() {
      disposed = true;
      for (const t of timers) clearTimeout(t);
      timers.clear();
      listeners.clear();
    },
  };
}
