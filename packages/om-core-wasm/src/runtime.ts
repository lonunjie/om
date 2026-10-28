// The async driver around the sans-IO Core: persists every Commit through the profile's EncryptedKv BEFORE telling
// Core it may release anything, performs the persisted network outbox through a Provider (exact retry, same
// identity), polls mailboxes, and fans changes out to observers and subscribers.
import { captureProfileContext, isTerminalOutcome } from "@om/platform-api";
import type {
  CoreClient, CoreIntent, CoreQuery, EncryptedKv, HomeSummary, OperationHandle, OperationId, OutcomeClass, PendingOperationView,
  ProfileExecutionContext, ProfileId, QueryResult, Unsubscribe,
} from "@om/platform-api";
import { asProfileGeneration, asProfileId } from "@om/platform-api";
import type { Commit, Inbound, NetOp, NetResult, PollTarget, Provider, Record as CoreRecord } from "./types";
import { CoreCallError, CoreStorageError, fromHex, toHex } from "./types";
import type { WasmCore } from "./wasm";

const KEY_PREFIX = "core/";

export interface CoreRuntimeOptions {
  readonly core: WasmCore;
  readonly provider: Provider;
  /** The profile's encrypted store (DeviceVault `EncryptedKv`); the profile's vault must already be unlocked. */
  readonly storage: (profileId: ProfileId) => EncryptedKv;
  readonly home: HomeSummary;
  /** Perform the outbox and poll in the background after every change (default true). Tests may drive `flush` by hand. */
  readonly autoDrain?: boolean;
  /** Delay before retrying a network operation that was unavailable (default 2000 ms). */
  readonly retryMs?: number;
}

export interface WasmCoreClient extends CoreClient {
  /** Loads the profile's records, opens it in Core and returns its (frozen) execution context. */
  openProfile(profileId: ProfileId): Promise<ProfileExecutionContext>;
  closeProfile(profileId: ProfileId): void;
  /** Satisfies the UI's ProfileContextSource: the current context of an open profile. */
  contextFor(profileId: ProfileId): ProfileExecutionContext;
  /** Perform owed network operations and poll mailboxes until nothing more moves. Resolves when quiescent. */
  flush(profileId: ProfileId): Promise<void>;
  /** Resolves when no dispatch or drain is in flight. */
  idle(): Promise<void>;
  /** Release timers. */
  dispose(): void;
}

interface StepJson { readonly commit: Commit | null; readonly handle: OperationHandle | null }
interface ReleasedJson { readonly profileId: string; readonly changedOperations: readonly string[] }

const ctxOf = (x: ProfileExecutionContext): ProfileExecutionContext => captureProfileContext(x.profileId, x.generation);

export function createCoreRuntime(opts: CoreRuntimeOptions): Promise<WasmCoreClient> {
  const { core, provider } = opts;
  const autoDrain = opts.autoDrain ?? true;
  const retryMs = opts.retryMs ?? 2000;
  core.call({ cmd: "init", home: opts.home });

  let tail: Promise<unknown> = Promise.resolve();
  /** All state-changing calls run strictly one at a time (Core allows one outstanding commit). */
  const exclusive = <T>(fn: () => Promise<T>): Promise<T> => {
    const run = tail.then(fn, fn);
    tail = run.catch(() => undefined);
    return run;
  };

  const observers = new Map<string, Set<(o: OutcomeClass) => void>>();
  const subscribers = new Set<{ readonly selector: CoreQuery; readonly listener: (r: never) => void; last: string }>();
  const timers = new Set<ReturnType<typeof setTimeout>>();
  const draining = new Map<string, Promise<void>>();
  const inflight = new Set<string>();
  let disposed = false;

  const key = (profileId: string, op: string): string => `${profileId}\u0000${op}`;
  const query = (q: CoreQuery): unknown => core.call({ cmd: "query", query: q });

  function emitQueries(profileId: string): void {
    for (const s of [...subscribers]) {
      if (s.selector.kind !== "GetProfileList" && s.selector.ctx.profileId !== profileId) continue;
      let result: unknown;
      try { result = query(s.selector); } catch { continue; } // the profile closed or the context went stale
      const json = JSON.stringify(result);
      if (json === s.last) continue;
      s.last = json;
      (s.listener as (r: unknown) => void)(result);
    }
  }

  function release(released: ReleasedJson, ctx: ProfileExecutionContext): void {
    for (const op of released.changedOperations) {
      const set = observers.get(key(released.profileId, op));
      if (!set || set.size === 0) continue;
      const outcome = core.call({ cmd: "outcome", ctx, operationId: op }) as OutcomeClass | null;
      if (outcome) for (const l of [...set]) l(outcome);
    }
    emitQueries(released.profileId);
  }

  /** Make a Commit durable, then (and only then) let Core release its effects. */
  async function commit(c: Commit | null, ctx: ProfileExecutionContext): Promise<void> {
    if (!c) return;
    const kv = opts.storage(asProfileId(c.profileId));
    let failure: string | undefined;
    try {
      for (const w of c.writes) {
        const r = w.value === null ? await kv.delete(KEY_PREFIX + w.key) : await kv.put(KEY_PREFIX + w.key, fromHex(w.value));
        if (!r.ok) { failure = r.reason; break; }
      }
    } catch (e) {
      failure = e instanceof Error ? e.message : "storage";
    }
    const released = core.call({ cmd: "finish_commit", commitId: c.id, ok: failure === undefined }) as ReleasedJson;
    if (failure !== undefined) throw new CoreStorageError(`could not persist (${failure}); nothing was released`);
    release(released, ctx);
  }

  async function readRecords(profileId: ProfileId): Promise<CoreRecord[]> {
    const kv = opts.storage(profileId);
    const listed = await kv.list();
    if (!listed.ok) {
      if (listed.reason === "not_found") return [];
      throw new CoreStorageError(`cannot list profile records (${listed.reason})`);
    }
    const out: CoreRecord[] = [];
    for (const k of listed.keys.filter((x) => x.startsWith(KEY_PREFIX))) {
      const r = await kv.get(k);
      if (!r.ok) throw new CoreStorageError(`cannot read ${k} (${r.reason})`);
      if (r.value !== undefined) out.push({ key: k.slice(KEY_PREFIX.length), value: toHex(r.value) });
    }
    return out;
  }

  // ---------------------------------------------------------------------------------------------- network

  function scheduleRetry(profileId: ProfileId): void {
    if (disposed || !autoDrain || retryMs <= 0) return;
    const t = setTimeout(() => { timers.delete(t); void flush(profileId); }, retryMs);
    timers.add(t);
  }

  async function runOp(ctx: ProfileExecutionContext, op: NetOp): Promise<boolean> {
    const id = key(ctx.profileId, String(op.id));
    if (inflight.has(id)) return false;
    inflight.add(id);
    try {
      let result: NetResult;
      try { result = await provider.perform(op.call); } catch { result = { result: "err", error: { kind: "unavailable" } }; }
      const progressed = !(result.result === "err" && (result.error.kind === "unavailable" || result.error.kind === "timeout"));
      await exclusive(async () => {
        const step = core.call({ cmd: "net_result", ctx, netId: op.id, result }) as StepJson;
        await commit(step.commit, ctx);
      });
      return progressed;
    } finally {
      inflight.delete(id);
    }
  }

  async function pollOnce(ctx: ProfileExecutionContext): Promise<boolean> {
    let any = false;
    const targets = core.call({ cmd: "poll_targets", ctx }) as PollTarget[];
    for (const t of targets) {
      let items: readonly Inbound[];
      try { items = await provider.poll(t.mailbox, t.after); } catch { continue; }
      if (items.length === 0) continue;
      any = true;
      await exclusive(async () => {
        const step = core.call({ cmd: "inbound", ctx, mailbox: t.mailbox, items }) as StepJson;
        await commit(step.commit, ctx);
      });
    }
    return any;
  }

  async function drainLoop(profileId: ProfileId): Promise<void> {
    for (let round = 0; round < 50 && !disposed; round++) {
      const ctx = ctxOf(core.call({ cmd: "context_for", profileId }) as ProfileExecutionContext);
      const ops = core.call({ cmd: "net_ops", ctx }) as NetOp[];
      let moved = false;
      for (const op of ops) if (await runOp(ctx, op)) moved = true;
      if (await pollOnce(ctx)) moved = true;
      if (!moved) {
        if ((core.call({ cmd: "net_ops", ctx }) as NetOp[]).length > 0) scheduleRetry(profileId);
        return;
      }
    }
  }

  function flush(profileId: ProfileId): Promise<void> {
    const running = draining.get(profileId);
    if (running) return running.then(() => flush(profileId)); // run once more so late work is not missed
    const p = drainLoop(profileId).catch(() => undefined).finally(() => { draining.delete(profileId); });
    draining.set(profileId, p);
    return p;
  }

  const kick = (profileId: ProfileId): void => { if (autoDrain && !disposed) void flush(profileId); };

  // ---------------------------------------------------------------------------------------------- client

  const client: WasmCoreClient = {
    async openProfile(profileId) {
      const records = await readRecords(profileId);
      const ctx = await exclusive(() => Promise.resolve(core.call({ cmd: "open_profile", profileId, records }) as ProfileExecutionContext));
      emitQueries(profileId);
      kick(profileId);
      return ctxOf(ctx);
    },
    closeProfile(profileId) {
      core.call({ cmd: "close_profile", profileId });
      emitQueries(profileId);
    },
    contextFor(profileId) {
      const c = core.call({ cmd: "context_for", profileId }) as { profileId: string; generation: string };
      return captureProfileContext(asProfileId(c.profileId), asProfileGeneration(c.generation));
    },
    dispatch(intent: CoreIntent) {
      return exclusive(async () => {
        const step = core.call({ cmd: "dispatch", intent }) as StepJson;
        await commit(step.commit, intent.ctx);
        if (!step.handle) throw new CoreCallError("internal", "dispatch returned no handle");
        kick(intent.ctx.profileId);
        return step.handle;
      });
    },
    observe(ctx, operationId, listener): Unsubscribe {
      const current = core.call({ cmd: "outcome", ctx, operationId }) as OutcomeClass | null; // validates the context
      const k = key(ctx.profileId, operationId);
      const set = observers.get(k) ?? new Set();
      set.add(listener);
      observers.set(k, set);
      if (current) listener(current);
      return () => { set.delete(listener); };
    },
    query<Q extends CoreQuery>(q: Q): Promise<QueryResult<Q>> {
      try { return Promise.resolve(query(q) as QueryResult<Q>); } catch (e) { return Promise.reject(e instanceof Error ? e : new CoreCallError("internal", "query failed")); }
    },
    subscribe<Q extends CoreQuery>(selector: Q, listener: (result: QueryResult<Q>) => void): Unsubscribe {
      const last = JSON.stringify(query(selector)); // validates the context; the initial value comes from `query`
      const entry = { selector, listener: listener as (r: never) => void, last };
      subscribers.add(entry);
      return () => { subscribers.delete(entry); };
    },
    pendingOperations(ctx) {
      try { return Promise.resolve(core.call({ cmd: "pending_operations", ctx }) as PendingOperationView[]); } catch (e) { return Promise.reject(e instanceof Error ? e : new CoreCallError("internal", "failed")); }
    },
    flush,
    async idle() {
      for (let i = 0; i < 100; i++) {
        const pending = [tail, ...draining.values()];
        await Promise.all(pending);
        if (draining.size === 0) { await tail; return; }
      }
    },
    dispose() {
      disposed = true;
      for (const t of timers) clearTimeout(t);
      timers.clear();
    },
  };
  return Promise.resolve(client);
}

export { isTerminalOutcome };
export type { OperationId };
