// The async driver around the sans-IO Core: persists every Commit through the profile's EncryptedKv BEFORE telling
// Core it may release anything, performs the persisted network outbox through a Provider (exact retry, same
// identity), polls mailboxes, and fans changes out to observers and subscribers.
import { captureProfileContext, isTerminalOutcome } from "@om/platform-api";
import type {
  AttachmentApi, AttachmentLimits, AttachmentMeta, AttachmentOpenResult, AttachmentPrepareResult, AttachmentRefusal,
  CoreClient, CoreIntent, CoreQuery, EncryptedKv, GroupApi, GroupDetailView, GroupEvent, GroupOutbound, GroupInviteView, GroupMessageView, GroupStep, GroupSummaryView, HomeSummary, OperationHandle, OperationId, OutcomeClass, PendingOperationView,
  ProfileExecutionContext, ProfileId, QueryResult, Unsubscribe,
} from "@om/platform-api";
import { asProfileGeneration, asProfileId } from "@om/platform-api";
import type { MessageId } from "@om/platform-api";
import type { BackupHeader, BackupPlan, BlobLimits, Commit, DirectFetchProgress, DirectFrame, ImportSummary, Inbound, NetOp, NetResult, PollTarget, Provider, Record as CoreRecord } from "./types";
import { CoreCallError, CoreStorageError, fromHex, toHex } from "./types";
import type { WasmCore } from "./wasm";
import { createGroupDriver } from "./groups-driver";
import type { CommitAcceptance, GroupCarriage, GroupDriver } from "./groups-driver";

const KEY_PREFIX = "core/";
/** Sealed attachment bytes waiting for their upload, in the profile's encrypted store (never under `core/`, which is Core's own records). */
const STAGE_PREFIX = "stage/";
/** How long an advertised-limits answer is trusted before the Home is asked again. */
const LIMITS_TTL_MS = 5 * 60_000;

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
  /** Poll open profiles' mailboxes at this interval while autoDrain is on (default 0 = only after changes). A function is asked again before every poll and returns the wait in ms, so the host can slow down while nobody is looking (a non-positive answer means 5 s). */
  readonly pollMs?: number | (() => number);
  /**
   * The host driver for groups (ADR-0177). Given, `client.groups` carries each step's wires over `carriage` and reports Commits
   * (see `createGroupDriver`); with a null `carriage`, or none given at all, the host cannot carry a group wire and groups are
   * unavailable even when the module has the engine. Absent, `client.groups` is the bare engine (tests that carry wires by hand).
   */
  readonly groupDriver?: { readonly carriage: GroupCarriage | null; readonly acceptance?: CommitAcceptance };
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
  /** Chat-history backup file (ADR-0184). The host derives the key from the person's password; Core never sees the password. */
  /** The Argon2id settings and a fresh salt for the next file. */
  backupPlan(): BackupPlan;
  /** The profile's history, sealed under `key` (32 bytes derived with `plan`). Refuses with a `backup_*` CoreCallError. */
  exportBackup(ctx: ProfileExecutionContext, plan: BackupPlan, key: Uint8Array): Uint8Array;
  /** A file's public header (settings and salt), without any key. Refuses a non-backup, an unknown version, a short file. */
  inspectBackup(file: Uint8Array): BackupHeader;
  /** Restores a file as read-only history: all of it or none of it, durable before this resolves. */
  importBackup(ctx: ProfileExecutionContext, file: Uint8Array, key: Uint8Array): Promise<ImportSummary>;
  /**
   * The person stopped using one of their Homes (`relayUri` is its route name, `RouteOffer.relayUri`). Every relationship that has
   * another route publishes a new RelaySet version without it and keeps collecting from the dropped route for an overlap
   * (closed at once when `emergency`). Resolves with how many relationships had no other route and so stay on that Home.
   */
  retireHome(ctx: ProfileExecutionContext, relayUri: string, emergency?: boolean): Promise<number>;
  /** Files: seal, stage, open. Always present on the real client; `limits()` is `null` until the Home advertises a file store. */
  readonly attachments: AttachmentApi & {
    /** Ask the Home again what it advertises and tell Core (also done lazily before `prepare`). */
    refreshLimits(): Promise<AttachmentLimits | null>;
    /** The ciphertext of an outgoing attachment again from its plaintext, staged under the same id (a host that lost its staged bytes). */
    restage(ctx: ProfileExecutionContext, blobId: string, plaintext: Uint8Array): Promise<boolean>;
    /**
     * After a restart a direct offer shows `needsFile` and serves nothing; hand the original file back to serve it again.
     * False when the file is not the one that was offered, or the offer is over.
     */
    arm(ctx: ProfileExecutionContext, blobId: string, plaintext: Uint8Array): Promise<boolean>;
  };
  /**
   * Ordinary groups (ADR-0177), typed. Real only when the module was built with `groups-openmls` (ADR-0187, proposed):
   * the shipped module is not, and `available()` is then false. The group wire in each step's `out` is Core-internal
   * JSON (groups plan Q2): the host carries it, never reads it.
   */
  readonly groups: GroupApi;
  /**
   * The two ends of a direct transfer, as frames for a host-supplied channel (the host driver is not built: no transport is
   * chosen yet). Sender: `serve` each frame received, `pump` while the channel drains. Receiver: `fetchStart`, feed each
   * frame to `fetchHandle` and send what it returns, then `fetchOpen` when the progress says `complete`.
   */
  readonly direct: {
    serve(ctx: ProfileExecutionContext, frame: DirectFrame): Promise<readonly DirectFrame[]>;
    pump(ctx: ProfileExecutionContext, buffered: number): readonly DirectFrame[];
    fetchStart(ctx: ProfileExecutionContext, messageId: MessageId, index: number): DirectFrame | null;
    fetchHandle(ctx: ProfileExecutionContext, messageId: MessageId, index: number, frame: DirectFrame): readonly DirectFrame[];
    fetchProgress(ctx: ProfileExecutionContext, messageId: MessageId, index: number): DirectFetchProgress | null;
    fetchOpen(ctx: ProfileExecutionContext, messageId: MessageId, index: number): AttachmentOpenResult;
  };
}

interface PrepareJson {
  readonly prepared?: { readonly blobId: string; readonly ciphertext: string };
  readonly refused?: { readonly error: string; readonly max?: number };
  readonly commit: Commit | null;
}
interface OpenJson {
  readonly opened?: { readonly plaintext: string; readonly mediaType: string; readonly filename?: string | null };
  readonly refused?: { readonly error: string; readonly max?: number };
  readonly call?: NetCallBlob;
}
type NetCallBlob = Extract<NetOp["call"], { call: "get_blob" }>;
const REFUSALS: readonly string[] = ["too_large", "storage_full", "no_blob_store", "empty", "invalid", "not_found", "integrity_failure", "unsupported"];
function refusalOf(r: { readonly error: string; readonly max?: number } | undefined): AttachmentRefusal {
  if (r?.error === "too_large") return { error: "too_large", max: r.max ?? 0 };
  const e = r?.error ?? "invalid";
  return { error: (REFUSALS.includes(e) ? e : "invalid") as Exclude<AttachmentRefusal["error"], "too_large"> };
}

interface GroupJson { readonly refused?: string; readonly reply?: { readonly events: readonly GroupEvent[]; readonly out: readonly GroupOutbound[] }; readonly commit?: Commit | null }
interface StepJson { readonly commit: Commit | null; readonly handle: OperationHandle | null }
interface ReleasedJson { readonly profileId: string; readonly changedOperations: readonly string[] }

const ctxOf = (x: ProfileExecutionContext): ProfileExecutionContext => captureProfileContext(x.profileId, x.generation);

export function createCoreRuntime(opts: CoreRuntimeOptions): Promise<WasmCoreClient> {
  const { core, provider } = opts;
  const autoDrain = opts.autoDrain ?? true;
  const retryMs = opts.retryMs ?? 2000;
  const pollMs = opts.pollMs ?? 0;
  const pollEvery = (): number => (typeof pollMs === "function" ? pollMs() : pollMs);
  const openProfiles = new Set<ProfileId>();
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
  let wakeOff: (() => void) | undefined;

  const key = (profileId: string, op: string): string => `${profileId}\u0000${op}`;

  /** Keep a few route offers in Core so an intent that needs a new mailbox never waits on the provider adapter. */
  const OFFER_TARGET = 4;
  async function topUpRoutes(): Promise<void> {
    if (!provider.allocateRoutes) return;
    try {
      const held = core.call({ cmd: "supply_routes", routes: [] }) as number;
      if (held >= OFFER_TARGET) return;
      core.call({ cmd: "supply_routes", routes: await provider.allocateRoutes(OFFER_TARGET - held, core.call({ cmd: "offer_homes" }) as string[]) });
    } catch { /* no routes: Core refuses what needs a mailbox as unavailable; nothing is invented */ }
  }
  const query = (q: CoreQuery): unknown => core.call({ cmd: "query", query: q });

  // ---------------------------------------------------------------------------------------------- files
  let limits: AttachmentLimits | null = null;
  let limitsAt = 0;
  async function refreshLimits(): Promise<AttachmentLimits | null> {
    if (!provider.blobLimits) return limits;
    let found: BlobLimits | null;
    try { found = await provider.blobLimits(); } catch { return limits; } // could not find out: the last answer stays in force
    limitsAt = Date.now();
    limits = found === null ? null : { maxBlobBytes: found.maxBlobBytes, ...(found.maxSpoolBytes === undefined ? {} : { maxSpoolBytes: found.maxSpoolBytes }), ...(found.maxBlobs === undefined ? {} : { maxBlobs: found.maxBlobs }) };
    core.call({ cmd: "set_blob_limits", limits: found === null ? null : { maxBlobBytes: found.maxBlobBytes, maxSpoolBytes: found.maxSpoolBytes ?? 0, maxBlobs: found.maxBlobs ?? 0 } });
    return limits;
  }
  const ensureLimits = async (): Promise<void> => { if (limits === null || Date.now() - limitsAt > LIMITS_TTL_MS) await refreshLimits(); };
  async function stage(profileId: ProfileId, blobId: string, ciphertext: Uint8Array): Promise<boolean> {
    try { return (await opts.storage(profileId).put(STAGE_PREFIX + blobId, ciphertext)).ok; } catch { return false; }
  }
  async function unstage(profileId: ProfileId, blobId: string): Promise<void> {
    try { await opts.storage(profileId).delete(STAGE_PREFIX + blobId); } catch { /* a leftover is sealed ciphertext and is swept with the profile */ }
  }

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
      try {
        if (op.call.call === "put_blob") {
          const staged = await opts.storage(ctx.profileId).get(STAGE_PREFIX + op.call.blobId);
          if (!staged.ok) result = { result: "err", error: { kind: "unavailable" } }; // the store could not be read now: retry, never reject
          else if (staged.value === undefined) result = { result: "err", error: { kind: "rejected", reason: "not_found" } }; // the sealed bytes are gone: say so, do not upload nothing
          else result = await provider.perform(op.call, { ciphertext: staged.value });
        } else result = await provider.perform(op.call);
      } catch { result = { result: "err", error: { kind: "unavailable" } }; }
      // Once the store has the object (or definitely refused it) the staged copy has no further use.
      if (op.call.call === "put_blob" && (result.result !== "err" || result.error.kind === "rejected")) await unstage(ctx.profileId, op.call.blobId);
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
      try { items = await provider.poll(t.mailbox, t.after, t.kind, t.relayUri, t.capability); } catch { continue; }
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

  /** One group command: the typed refusal or `{reply, commit}`. */
  const groupCall = (cmd: Record<string, unknown>): Promise<GroupJson> => Promise.resolve(core.call(cmd) as GroupJson);
  async function groupStep(r: GroupJson, ctx: ProfileExecutionContext): Promise<GroupStep> {
    if (r.refused !== undefined || r.reply === undefined) return { ok: false, refused: r.refused ?? "invalid" };
    await commit(r.commit ?? null, ctx); // the step is durable before any outbound is handed to the host
    return { ok: true, events: r.reply.events, out: r.reply.out };
  }
  const groupView = (cmd: Record<string, unknown>): Record<string, unknown> => {
    const v = core.call(cmd) as Record<string, unknown>;
    return v["refused"] === undefined ? v : {};
  };

  const groupEngine: GroupApi = {
    available: () => core.call({ cmd: "groups_available" }) === true,
    do: (ctx, intent) => exclusive(async () => groupStep(await groupCall({ cmd: "group_do", ctx: ctxOf(ctx), intent }), ctx)),
    receive: (ctx, from, wire) => exclusive(async () => groupStep(await groupCall({ cmd: "group_receive", ctx: ctxOf(ctx), from, wire }), ctx)),
    list: (ctx) => (groupView({ cmd: "group_query", ctx: ctxOf(ctx), query: { kind: "groups" } }) as { groups?: GroupSummaryView[] }).groups ?? [],
    get: (ctx, conversationId) => (groupView({ cmd: "group_query", ctx: ctxOf(ctx), query: { kind: "group", conversationId } }) as { group?: GroupDetailView }).group ?? null,
    invites: (ctx) => ((groupView({ cmd: "group_query", ctx: ctxOf(ctx), query: { kind: "invites" } }) as { invites?: (GroupInviteView & { releases?: unknown; intentId?: unknown })[] }).invites ?? [])
      .map(({ releases: _r, intentId: _i, ...rest }) => rest),
    messages: (ctx, conversationId) => (groupView({ cmd: "group_query", ctx: ctxOf(ctx), query: { kind: "messages", conversationId } }) as { messages?: GroupMessageView[] }).messages ?? [],
  };
  const groupDriver: GroupDriver | undefined = opts.groupDriver === undefined ? undefined : createGroupDriver({
    engine: groupEngine,
    storage: opts.storage,
    contextFor: (profileId) => { const c = core.call({ cmd: "context_for", profileId }) as { profileId: string; generation: string }; return captureProfileContext(asProfileId(c.profileId), asProfileGeneration(c.generation)); },
    carriage: opts.groupDriver.carriage,
    ...(opts.groupDriver.acceptance === undefined ? {} : { acceptance: opts.groupDriver.acceptance }),
  });

  const client: WasmCoreClient = {
    async openProfile(profileId) {
      const records = await readRecords(profileId);
      const ctx = await exclusive(() => Promise.resolve(core.call({ cmd: "open_profile", profileId, records }) as ProfileExecutionContext));
      emitQueries(profileId);
      openProfiles.add(profileId);
      void refreshLimits();
      kick(profileId);
      if (groupDriver !== undefined) void groupDriver.flush(ctxOf(ctx)); // what a previous run still owed over the group carriage
      return ctxOf(ctx);
    },
    closeProfile(profileId) {
      openProfiles.delete(profileId);
      core.call({ cmd: "close_profile", profileId });
      emitQueries(profileId);
    },
    contextFor(profileId) {
      const c = core.call({ cmd: "context_for", profileId }) as { profileId: string; generation: string };
      return captureProfileContext(asProfileId(c.profileId), asProfileGeneration(c.generation));
    },
    dispatch(intent: CoreIntent) {
      return exclusive(async () => {
        await topUpRoutes();
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
    backupPlan() { return core.call({ cmd: "backup_plan" }) as BackupPlan; },
    exportBackup(ctx, plan, key) {
      return fromHex(core.call({ cmd: "backup_export", ctx, plan, key: toHex(key) }) as string);
    },
    inspectBackup(file) { return core.call({ cmd: "backup_inspect", file: toHex(file) }) as BackupHeader; },
    importBackup(ctx, file, key) {
      return exclusive(async () => {
        const r = core.call({ cmd: "backup_import", ctx: ctxOf(ctx), file: toHex(file), key: toHex(key) }) as { summary: ImportSummary; commit: Commit | null };
        await commit(r.commit, ctx);
        return r.summary;
      });
    },
    retireHome(ctx, relayUri, emergency = false) {
      return exclusive(async () => {
        const r = core.call({ cmd: "retire_home", ctx: ctxOf(ctx), relayUri, emergency }) as { stuck: number; commit: Commit | null };
        await commit(r.commit, ctx);
        return r.stuck;
      });
    },
    attachments: {
      limits: () => limits,
      refreshLimits,
      async prepare(ctx, bytes, meta: AttachmentMeta): Promise<AttachmentPrepareResult> {
        await ensureLimits();
        return exclusive(async () => {
          const r = core.call({ cmd: "prepare_attachment", ctx: ctxOf(ctx), plaintext: toHex(bytes), meta }) as PrepareJson;
          await commit(r.commit, ctx); // the identities are durable before the sealed bytes are used
          if (r.prepared === undefined) return { ok: false, refusal: refusalOf(r.refused) };
          if (!(await stage(ctx.profileId, r.prepared.blobId, fromHex(r.prepared.ciphertext)))) return { ok: false, refusal: { error: "unavailable" } };
          return { ok: true, blobId: r.prepared.blobId };
        });
      },
      async prepareDirect(ctx, bytes, meta: AttachmentMeta): Promise<AttachmentPrepareResult> {
        await ensureLimits();
        return exclusive(async () => {
          const r = core.call({ cmd: "prepare_direct_attachment", ctx: ctxOf(ctx), plaintext: toHex(bytes), meta }) as PrepareJson;
          await commit(r.commit, ctx); // the identities are durable before the offer is used; nothing is staged: Core holds the object
          if (r.prepared === undefined) return { ok: false, refusal: refusalOf(r.refused) };
          return { ok: true, blobId: r.prepared.blobId };
        });
      },
      arm(ctx, blobId, plaintext) {
        const r = core.call({ cmd: "arm_direct_offer", ctx: ctxOf(ctx), blobId, plaintext: toHex(plaintext) }) as { armed?: boolean };
        return Promise.resolve(r.armed === true);
      },
      async restage(ctx, blobId, plaintext) {
        const r = core.call({ cmd: "restage_attachment", ctx: ctxOf(ctx), blobId, plaintext: toHex(plaintext) }) as { ciphertext?: string };
        return r.ciphertext !== undefined && (await stage(ctx.profileId, blobId, fromHex(r.ciphertext)));
      },
      async open(ctx, messageId: MessageId, index): Promise<AttachmentOpenResult> {
        const plan = core.call({ cmd: "attachment_fetch", ctx: ctxOf(ctx), messageId, index }) as OpenJson;
        if (plan.call === undefined) return { ok: false, refusal: refusalOf(plan.refused) };
        let got: NetResult;
        try { got = await provider.perform(plan.call); } catch { got = { result: "err", error: { kind: "unavailable" } }; }
        if (got.result !== "bytes") {
          const e = got.result === "err" ? got.error : { kind: "unavailable" as const };
          return { ok: false, refusal: { error: e.kind === "not_found" ? "not_found" : "unavailable" } };
        }
        const o = core.call({ cmd: "open_attachment", ctx: ctxOf(ctx), messageId, index, ciphertext: got.bytes }) as OpenJson;
        if (o.opened === undefined) return { ok: false, refusal: refusalOf(o.refused) };
        return { ok: true, bytes: fromHex(o.opened.plaintext), mediaType: o.opened.mediaType, ...(o.opened.filename ? { filename: o.opened.filename } : {}) };
      },
    },
    direct: {
      serve: (ctx, frame) => exclusive(async () => {
        const r = core.call({ cmd: "direct_serve", ctx: ctxOf(ctx), frame }) as { frames: DirectFrame[]; commit: Commit | null };
        await commit(r.commit, ctx); // the one durable effect of serving is "delivered"
        return r.frames;
      }),
      pump: (ctx, buffered) => core.call({ cmd: "direct_pump", ctx: ctxOf(ctx), buffered }) as DirectFrame[],
      fetchStart(ctx, messageId, index) {
        const r = core.call({ cmd: "direct_fetch_start", ctx: ctxOf(ctx), messageId, index }) as { frame?: DirectFrame };
        return r.frame ?? null;
      },
      fetchHandle(ctx, messageId, index, frame) {
        const r = core.call({ cmd: "direct_fetch_handle", ctx: ctxOf(ctx), messageId, index, frame }) as { frames?: DirectFrame[] };
        return r.frames ?? [];
      },
      fetchProgress(ctx, messageId, index) {
        const r = core.call({ cmd: "direct_fetch_progress", ctx: ctxOf(ctx), messageId, index }) as { state: DirectFetchProgress["state"]; held: number } | null;
        return r === null ? null : { state: r.state, held: r.held };
      },
      fetchOpen(ctx, messageId, index): AttachmentOpenResult {
        const o = core.call({ cmd: "direct_fetch_open", ctx: ctxOf(ctx), messageId, index }) as OpenJson;
        if (o.opened === undefined) return { ok: false, refusal: refusalOf(o.refused) };
        return { ok: true, bytes: fromHex(o.opened.plaintext), mediaType: o.opened.mediaType, ...(o.opened.filename ? { filename: o.opened.filename } : {}) };
      },
    },
    groups: groupDriver ?? groupEngine,
    flush,
    async idle() {
      for (let i = 0; i < 100; i++) {
        const pending = [tail, ...draining.values()];
        await Promise.all(pending);
        if (draining.size === 0) { await tail; await groupDriver?.idle(); return; }
      }
    },
    dispose() {
      disposed = true;
      groupDriver?.dispose();
      wakeOff?.();
      for (const t of timers) { clearTimeout(t); clearInterval(t); }
      timers.clear();
    },
  };
  if (autoDrain && provider.onWake) {
    // The transport heard that mail is waiting: collect now for every open profile (a flush already running is followed by one more).
    wakeOff = provider.onWake(() => { if (!disposed) for (const p of [...openProfiles]) void flush(p); });
  }
  if (autoDrain && (typeof pollMs === "function" || pollMs > 0)) {
    // Background mailbox polling: a message from another device arrives with no local change to trigger a drain.
    const tick = (): void => {
      if (disposed) return;
      const wait = pollEvery();
      const t = setTimeout(() => {
        timers.delete(t);
        for (const p of [...openProfiles]) if (!draining.has(p)) void flush(p);
        tick();
      }, wait > 0 ? wait : 5000);
      timers.add(t);
    };
    tick();
  }
  return Promise.resolve(client);
}

export { isTerminalOutcome };
export type { OperationId };
