// The host driver for ordinary groups (ADR-0177): it carries what Core's group steps hand out (`out`) to a transport,
// gives what the transport receives back to Core, and walks a prepared Commit through the host's two reports
// (`commit_published`, `commit_accepted`). It never reads a wire, never decides anything about a group and never builds one.
//
// What this driver does NOT decide (groups plan Q2, G10: the design is silent, see docs/reports/2026-11-14-groups-driver-status.md):
// which 1:1 object type carries a wire (a `GroupCarriage` is supplied by the host; no host has one yet, so `available()` is
// false in a real build), and when a committer may call its own Commit accepted (the optional `acceptance` policy; with none,
// a prepared Commit stays prepared and the group stays paused, which is the safe answer).
//
// Durability (ADR-0163, K1/K2): Core's step is durable before the driver sees `out`. The driver then writes each outbound as an
// opaque sealed blob in the profile's EncryptedKv (`gdrv/out/`) BEFORE the step is returned, retries the exact same bytes, and
// deletes a blob only after the transport took it. The blob is opaque pending-delivery data, never interpreted, never a protocol.
import type {
  EncryptedKv, GroupApi, GroupEvent, GroupIntent, GroupOutbound, GroupSender, GroupStep, GroupWire, ProfileExecutionContext, ProfileId,
} from "@om/platform-api";

/** What the transport says about one outbound. `sent` is transport progress only (a relay ack is never group acceptance). */
export type CarriageResult = "sent" | "unavailable" | "undeliverable";

/**
 * The transport for group wires, supplied by the host. Real carriage (a 1:1 object type per outbound, sealed per recipient
 * device) is not specified yet (plan Q2); a host without one passes none and groups stay unavailable.
 */
export interface GroupCarriage {
  /** False when no transport can carry group wires. */
  available(): boolean;
  /** Carry one outbound for `profileId`. Must be safe to repeat: the driver retries the same bytes after `unavailable` or a crash. */
  send(profileId: ProfileId, out: GroupOutbound): Promise<CarriageResult>;
  /** Wires that arrived, with what the transport authenticated about the sender. Returns the unsubscribe. */
  onReceive(handler: (profileId: ProfileId, from: GroupSender, wire: GroupWire) => Promise<void>): () => void;
}

/** Whether the committer may report its prepared Commit as accepted by the ordering profile (G10). `false`: not yet. */
export type CommitAcceptance = (info: { readonly profileId: ProfileId; readonly intentId: string }) => boolean | Promise<boolean>;

/** TEST AND LOOPBACK ONLY: accept once the Commit was handed to the transport. The design says a relay ack is not acceptance. */
export const acceptAfterDelivery: CommitAcceptance = () => true;

export interface GroupDriverOptions {
  /** The group steps of the Core client (`client.groups`). */
  readonly engine: GroupApi;
  readonly storage: (profileId: ProfileId) => EncryptedKv;
  readonly contextFor: (profileId: ProfileId) => ProfileExecutionContext;
  readonly carriage: GroupCarriage | null;
  readonly acceptance?: CommitAcceptance;
  /** Wait before retrying an unavailable transport (default 2000 ms; 0 = only when `flush` is called). */
  readonly retryMs?: number;
}

export interface GroupDriver extends GroupApi {
  /** Send what is owed and advance every prepared Commit as far as the policy allows. Resolves when nothing more moves. */
  flush(ctx: ProfileExecutionContext): Promise<void>;
  /** Resolves when no step or flush is in flight. */
  idle(): Promise<void>;
  /** Outbounds written but not yet taken by the transport. */
  pending(ctx: ProfileExecutionContext): Promise<number>;
  dispose(): void;
}

const OUT = "gdrv/out/";
const PEND = "gdrv/pend/";

interface OutRec { readonly out: GroupOutbound; readonly tag?: string }
interface PendRec { readonly phase: "sending" | "published" }

const enc = new TextEncoder();
const dec = new TextDecoder();
const toBytes = (v: unknown): Uint8Array => enc.encode(JSON.stringify(v));
const parseOut = (b: Uint8Array): OutRec => JSON.parse(dec.decode(b)) as OutRec;
const parsePend = (b: Uint8Array): PendRec => JSON.parse(dec.decode(b)) as PendRec;
const rand = (): string => Array.from(globalThis.crypto.getRandomValues(new Uint8Array(6)), (b) => b.toString(16).padStart(2, "0")).join("");

export function createGroupDriver(opts: GroupDriverOptions): GroupDriver {
  const { engine, carriage } = opts;
  const retryMs = opts.retryMs ?? 2000;
  const profiles = new Set<ProfileId>();
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let disposed = false;
  let counter = 0;
  let tail: Promise<unknown> = Promise.resolve();
  let inflight = 0;
  const exclusive = <T>(fn: () => Promise<T>): Promise<T> => {
    inflight++;
    const run = tail.then(fn, fn);
    tail = run.catch(() => undefined).finally(() => { inflight--; });
    return run;
  };

  const available = (): boolean => engine.available() && carriage !== null && carriage.available();

  /** Write the step's outbounds (and a marker per prepared Commit) durably; false if the store refused (nothing was claimed sent). */
  async function enqueue(ctx: ProfileExecutionContext, step: Extract<GroupStep, { ok: true }>): Promise<void> {
    const kv = opts.storage(ctx.profileId);
    const prepared = step.events.filter((e): e is Extract<GroupEvent, { kind: "commit_prepared" }> => e.kind === "commit_prepared");
    for (const p of prepared) await kv.put(PEND + p.intentId, toBytes({ phase: "sending" } satisfies PendRec));
    const tag = prepared[0]?.intentId;
    for (const out of step.out) {
      const rec: OutRec = tag !== undefined && out.wire.t === "commit" ? { out, tag } : { out };
      const key = `${OUT}${String(Date.now()).padStart(15, "0")}-${String(counter++).padStart(6, "0")}-${rand()}`;
      await kv.put(key, toBytes(rec));
    }
    profiles.add(ctx.profileId);
  }

  async function ownedOut(kv: EncryptedKv): Promise<string[]> {
    const l = await kv.list();
    return l.ok ? l.keys.filter((k) => k.startsWith(OUT)).sort() : [];
  }

  /** One pass: returns true if anything moved. */
  async function pass(ctx: ProfileExecutionContext): Promise<{ moved: boolean; retry: boolean }> {
    const kv = opts.storage(ctx.profileId);
    let moved = false;
    let retry = false;
    if (carriage?.available() === true) {
      for (const key of await ownedOut(kv)) {
        const got = await kv.get(key);
        if (!got.ok || got.value === undefined) continue;
        let result: CarriageResult;
        try { result = await carriage.send(ctx.profileId, parseOut(got.value).out); } catch { result = "unavailable"; }
        if (result === "unavailable") { retry = true; break; } // keep order: nothing later goes ahead of an earlier outbound
        await kv.delete(key);
        moved = true;
      }
    } else retry = (await ownedOut(kv)).length > 0;
    // Commits whose wires are all out: report published, then ask the policy about accepted.
    const l = await kv.list();
    const pend = l.ok ? l.keys.filter((k) => k.startsWith(PEND)) : [];
    for (const pk of pend) {
      const intentId = pk.slice(PEND.length);
      const rec = await kv.get(pk);
      if (!rec.ok || rec.value === undefined) continue;
      const reported = parsePend(rec.value).phase === "published";
      const stillOut = await Promise.all((await ownedOut(kv)).map(async (k) => { const g = await kv.get(k); return g.ok && g.value !== undefined && parseOut(g.value).tag === intentId; }));
      if (stillOut.some(Boolean)) continue;
      if (!reported) {
        const s = await engine.do(ctx, { kind: "commit_published", intentId });
        // A refusal means Core no longer holds this candidate (superseded or already published): nothing more to report.
        if (!s.ok) { await kv.delete(pk); continue; }
        await kv.put(pk, toBytes({ phase: "published" } satisfies PendRec));
        moved = true;
      }
      if (opts.acceptance !== undefined && (await opts.acceptance({ profileId: ctx.profileId, intentId }))) {
        const s = await engine.do(ctx, { kind: "commit_accepted", intentId });
        await kv.delete(pk);
        moved = true;
        if (s.ok) await enqueue(ctx, s);
      }
    }
    return { moved, retry };
  }

  function schedule(ctx: ProfileExecutionContext): void {
    if (disposed || retryMs <= 0) return;
    const t = setTimeout(() => { timers.delete(t); void flush(ctx); }, retryMs);
    timers.add(t);
  }

  function flush(ctx: ProfileExecutionContext): Promise<void> {
    return exclusive(async () => {
      for (let i = 0; i < 20 && !disposed; i++) {
        const r = await pass(ctx);
        if (r.retry) { schedule(ctx); return; }
        if (!r.moved) return;
      }
    }).catch(() => undefined);
  }

  const kick = (ctx: ProfileExecutionContext): void => { if (!disposed) void flush(ctx); };

  async function run(ctx: ProfileExecutionContext, step: () => Promise<GroupStep>): Promise<GroupStep> {
    const s = await exclusive(async () => {
      const r = await step();
      if (r.ok) await enqueue(ctx, r);
      return r;
    });
    kick(ctx);
    return s;
  }

  // Queued, not awaited: a transport that delivers to the very driver that is sending (two profiles in one runtime) would
  // otherwise wait on the step that is waiting on it. `idle()` still covers the queued step.
  const unsubscribe = carriage?.onReceive((profileId, from, wire) => {
    try {
      const ctx = opts.contextFor(profileId);
      void run(ctx, () => engine.receive(ctx, from, wire)).catch(() => undefined);
    } catch { /* a profile that is not open here: nothing to give it */ }
    return Promise.resolve();
  });

  return {
    available,
    do: (ctx, intent: GroupIntent) => (available() ? run(ctx, () => engine.do(ctx, intent)) : Promise.resolve({ ok: false, refused: "groups_unavailable" } as const)),
    receive: (ctx, from, wire) => (available() ? run(ctx, () => engine.receive(ctx, from, wire)) : Promise.resolve({ ok: false, refused: "groups_unavailable" } as const)),
    list: (ctx) => (available() ? engine.list(ctx) : []),
    get: (ctx, id) => (available() ? engine.get(ctx, id) : null),
    invites: (ctx) => (available() ? engine.invites(ctx) : []),
    messages: (ctx, id) => (available() ? engine.messages(ctx, id) : []),
    flush,
    async idle() {
      for (let i = 0; i < 100; i++) {
        const t = tail;
        await t;
        if (inflight === 0) return;
      }
    },
    async pending(ctx) { return (await ownedOut(opts.storage(ctx.profileId))).length; },
    dispose() {
      disposed = true;
      unsubscribe?.();
      for (const t of timers) clearTimeout(t);
      timers.clear();
    },
  };
}
