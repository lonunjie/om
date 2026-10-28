// SPDX-License-Identifier: MPL-2.0
// The Core transport capability (`Provider` of @om/core-wasm: perform + poll) over the OM Home API v1 HTTP binding.
//
// Mapping of Core's stand-in calls to Home operations (deviations D1..D8 are recorded in
// docs/reports/2026-10-28-relay-status.md):
//   deposit / poll / ack            -> POST /v1/relay/deposit | drain | ack   (+ /v1/relay/queues on first poll)
//   publish_key_material            -> PUT  /v1/keys/bundles/{device}         (feature om.keys.pool.v1)
//   fetch_key_material              -> POST /v1/keys/fetch
//   put_invite / get_invite / revoke_invite -> PUT | GET | DELETE /v1/discovery/descriptors/{id}
//
// Failure classes follow Core's contract: `unavailable` = definitely not performed; `timeout` = may have been performed
// (every mutating request carries a content-derived request_id, so Core's exact retry is idempotent at the Home).
import type { Inbound, NetCall, NetError, NetResult, Provider, RouteOffer } from "@om/core-wasm";
import {
  decodeDescriptorGet, decodeDrainResponse, decodeKeyFetchResponse, encodeAckRequest, encodeDepositRequest, encodeDescriptorPut,
  encodeDrainRequest, encodeKeyFetch, encodeKeyPublish, encodeQueueCreate, fromHex, toHex, type HomeErrorBody, type RelayEntry,
} from "@om/home-wire";
import { exchange, type Exchange, type HttpOptions } from "./http";
import { depositCap, descriptorPublishCap, installSecret, keyPublishCap, mailboxCaps, requestId } from "./derive";
import type { SecretStore } from "./secrets";
import type { ProviderConfig } from "./config";

export interface HttpProviderOptions {
  readonly config: ProviderConfig;
  readonly secrets: SecretStore;
  readonly fetch?: typeof fetch;
  /** Per-request timeout (default 20 s). A timeout after a mutating request is `timeout` (unknown), never a rejection. */
  readonly timeoutMs?: number;
}

export type Reachability = "unknown" | "reachable" | "unreachable";

export interface HttpProvider extends Provider {
  /** Routes for new mailboxes at this Home (Provider.allocateRoutes, always present here). */
  allocateRoutes(count: number): Promise<readonly RouteOffer[]>;
  readonly baseUrl: string;
  reachability(): Reachability;
  /** Mailboxes this installation could not claim because another party already holds the queue (squatted). */
  lostMailboxes(): readonly string[];
  /** The spec's RelayEntry (route object) for a mailbox's deposit capability at this Home. */
  routeEntry(mailbox: Uint8Array, expiresAt: number): Promise<RelayEntry>;
  /** The Home's public ProviderCapabilities document, or undefined if it cannot be fetched. */
  capabilities(): Promise<Readonly<Record<string, unknown>> | undefined>;
}

/** What remains of an exchange once network failure and gateway noise have been turned into a Core failure class. */
type Settled = Extract<Exchange, { kind: "ok" | "home" }> | NetResult;

const MAX_PAGES = 100;
const ACK_WINDOW = 256;
const UNAVAILABLE: NetResult = { result: "err", error: { kind: "unavailable" } };
const TIMEOUT: NetResult = { result: "err", error: { kind: "timeout" } };
const rejected = (reason: Extract<NetError, { kind: "rejected" }>["reason"]): NetResult => ({ result: "err", error: { kind: "rejected", reason } });
const toMs = (seconds: number): number => seconds * 1000;
/** Core hands expiry as milliseconds; the Home API speaks POSIX seconds (spec/registries/om-time-fields.json). */
const toSeconds = (t: number): number => (t > 100_000_000_000 ? Math.floor(t / 1000) : t);

export function createHttpProvider(opts: HttpProviderOptions): HttpProvider {
  const base = opts.config.baseUrl;
  const http: HttpOptions = { fetch: opts.fetch ?? ((...a) => fetch(...a)), timeoutMs: opts.timeoutMs ?? 20_000 };
  let secretP: Promise<Uint8Array> | undefined;
  const secret = (): Promise<Uint8Array> => (secretP ??= installSecret(opts.secrets));
  let reach: Reachability = "unknown";
  const ensured = new Set<string>();
  const lost = new Set<string>();
  const ackIds = new Map<string, Map<number, Uint8Array>>();
  const backoff = new Map<string, { until: number; step: number }>();

  // ------------------------------------------------------------------------------------------ plumbing

  const blocked = (key: string): boolean => (backoff.get(key)?.until ?? 0) > Date.now();
  function backOff(key: string, retryAfterSeconds?: number): void {
    const step = Math.min((backoff.get(key)?.step ?? 1000) * 2, 300_000);
    backoff.set(key, { until: Date.now() + (retryAfterSeconds === undefined ? step : toMs(retryAfterSeconds)), step });
  }
  const clear = (key: string): void => { backoff.delete(key); };

  async function probe(): Promise<boolean> {
    const r = await exchange({ ...http, timeoutMs: Math.min(http.timeoutMs, 5000) }, `${base}/.well-known/om-provider-capabilities`, { method: "GET" });
    reach = r.kind === "ok" ? "reachable" : "unreachable";
    return r.kind === "ok";
  }

  /** A mutating request that got no answer: if the Home is reachable the request may have landed (unknown), otherwise it was not sent. */
  async function unknownOutcome(): Promise<NetResult> {
    return (await probe()) ? TIMEOUT : UNAVAILABLE;
  }

  /** Home error -> Core failure class. `retryLater` is for a mailbox/queue whose deposit failed on a transient limit. */
  function mapError(e: HomeErrorBody, key?: string): NetResult {
    switch (e.name) {
      case "quota": // also the answer for a queue that does not exist yet (no existence oracle): retry with backoff
      case "rate_limited":
        if (key !== undefined) backOff(key, e.retryAfterSeconds);
        return UNAVAILABLE;
      case "retryable": return UNAVAILABLE;
      case "unauthorized": return rejected("unauthorized");
      case "revoked": return rejected("revoked");
      case "expired": return rejected("expired");
      case "not_found": return { result: "err", error: { kind: "not_found" } };
      case "stale_generation": return rejected("stale_state");
      case "invalid_signature": return rejected("integrity_failure");
      case "payload_too_large":
      case "invalid_request": return rejected("invalid");
      case "policy_denied":
      case "admission_denied": return rejected("policy_denied");
      case "spent_token": return rejected("conflict");
      case "unsupported_version":
      case "moved": // never followed without a verified continuity proof, which Core cannot check yet
      case "unknown": return rejected("unsupported");
    }
  }

  async function mutate(url: string, init: Parameters<typeof exchange>[2], key?: string): Promise<Settled> {
    if (key !== undefined && blocked(key)) return UNAVAILABLE;
    const r = await exchange(http, url, init);
    if (r.kind === "network" || r.kind === "gateway") return unknownOutcome();
    reach = "reachable";
    return r;
  }

  const isResult = (x: Settled): x is NetResult => "result" in x;

  async function read(url: string, init: Parameters<typeof exchange>[2]): Promise<Settled> {
    const r = await exchange(http, url, init);
    if (r.kind === "network" || r.kind === "gateway") { reach = "unreachable"; return UNAVAILABLE; }
    reach = "reachable";
    return r;
  }

  // ------------------------------------------------------------------------------------------ queues

  /** First poll of an own mailbox claims the queue at the Home (idempotent for identical capabilities). */
  async function ensureQueue(mailbox: Uint8Array, mailboxHex: string): Promise<"ok" | "later" | "lost"> {
    if (ensured.has(mailboxHex)) return "ok";
    if (lost.has(mailboxHex)) return "lost";
    const caps = await mailboxCaps(await secret(), mailbox);
    const r = await mutate(`${base}/v1/relay/queues`, {
      method: "POST",
      body: encodeQueueCreate({ requestId: await requestId("create", mailbox, caps.receive), receiveCap: caps.receive, depositCap: caps.deposit, adminCap: caps.admin }),
    });
    if (isResult(r)) return "later";
    if (r.kind === "ok") { ensured.add(mailboxHex); return "ok"; }
    if (r.error.name === "unauthorized") { lost.add(mailboxHex); return "lost"; }
    return "later";
  }

  async function drainPage(caps: Uint8Array, after: number): Promise<{ items: { seq: number; id: Uint8Array; bytes: Uint8Array }[]; next: number | undefined } | undefined> {
    const r = await read(`${base}/v1/relay/drain`, { method: "POST", body: encodeDrainRequest({ requestId: crypto.getRandomValues(new Uint8Array(16)), cap: caps, after }) });
    if (isResult(r) || r.kind !== "ok") return undefined;
    const page = decodeDrainResponse(r.body);
    const next = page.next ? Number(new DataView(page.next.buffer, page.next.byteOffset, page.next.byteLength).getBigUint64(0)) : undefined;
    return { items: page.items.map((i) => ({ seq: i.seq ?? 0, id: i.id, bytes: i.bytes })), next };
  }

  async function poll(mailboxHex: string, after: number): Promise<readonly Inbound[]> {
    const mailbox = fromHex(mailboxHex);
    if ((await ensureQueue(mailbox, mailboxHex)) !== "ok") return [];
    const caps = await mailboxCaps(await secret(), mailbox);
    const out: Inbound[] = [];
    const ids = ackIds.get(mailboxHex) ?? new Map<number, Uint8Array>();
    let cursor = after;
    for (let i = 0; i < MAX_PAGES; i++) {
      const page = await drainPage(caps.receive, cursor);
      if (!page) break;
      for (const it of page.items) {
        if (it.seq <= after) continue;
        ids.set(it.seq, it.id);
        out.push({ seq: it.seq, bytes: toHex(it.bytes) });
      }
      if (page.next === undefined) break;
      cursor = page.next;
    }
    ackIds.set(mailboxHex, ids);
    return out;
  }

  /** Core acknowledges cumulatively ("everything up to seq is processed"); the Home acknowledges item ids. */
  async function ack(mailboxHex: string, seq: number): Promise<NetResult> {
    const mailbox = fromHex(mailboxHex);
    const caps = await mailboxCaps(await secret(), mailbox);
    const known = ackIds.get(mailboxHex) ?? new Map<number, Uint8Array>();
    let ids = [...known.entries()].filter(([s]) => s <= seq).sort(([a], [b]) => a - b);
    if (ids.length === 0) {
      // After a restart the id cache is empty: re-read the window below `seq` to learn the ids (rare; bounded by ACK_WINDOW).
      let cursor = Math.max(0, seq - ACK_WINDOW);
      for (let i = 0; i < MAX_PAGES; i++) {
        const page = await drainPage(caps.receive, cursor);
        if (!page) return UNAVAILABLE;
        for (const it of page.items) if (it.seq <= seq) known.set(it.seq, it.id);
        if (page.next === undefined || page.next >= seq) break;
        cursor = page.next;
      }
      ids = [...known.entries()].filter(([s]) => s <= seq).sort(([a], [b]) => a - b);
    }
    for (let at = 0; at < ids.length; at += ACK_WINDOW) {
      const chunk = ids.slice(at, at + ACK_WINDOW);
      const r = await mutate(`${base}/v1/relay/ack`, {
        method: "POST",
        body: encodeAckRequest({ requestId: await requestId("ack", caps.queueId, ...chunk.map(([, id]) => id)), cap: caps.receive, itemIds: chunk.map(([, id]) => id) }),
      });
      if (isResult(r)) return r;
      if (r.kind === "home" && r.error.name !== "not_found") return mapError(r.error);
      for (const [s] of chunk) known.delete(s); // not_found means already acknowledged: the same end state
    }
    ackIds.set(mailboxHex, known);
    return { result: "ok" };
  }

  /**
   * `target` is the destination's deposit capability as its RelayEntry carried it, with that entry's `relayUri`. A
   * destination at another Home cannot be reached from here and is refused, not deposited at the wrong place. A legacy
   * destination (no `relayUri`, the bare 16-byte mailbox) has its capability derived, as before routes existed.
   */
  async function deposit(targetHex: string, bytesHex: string, relayUri: string | undefined): Promise<NetResult> {
    const target = fromHex(targetHex);
    const bytes = fromHex(bytesHex);
    let cap: Uint8Array;
    if (relayUri === undefined) cap = await depositCap(target);
    else if (relayUri !== `${base}/`) return rejected("unsupported");
    else if (target.length < 16 || target.length > 96) return rejected("invalid");
    else cap = target;
    const key = `deposit:${targetHex}`;
    const r = await mutate(`${base}/v1/relay/deposit`, {
      method: "POST",
      body: encodeDepositRequest({ requestId: await requestId("deposit", target, bytes), cap, envelope: bytes }),
    }, key);
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error, key);
    clear(key);
    return { result: "ok" };
  }

  // ------------------------------------------------------------------------------------------ key material, descriptors

  async function publishKeys(c: Extract<NetCall, { call: "publish_key_material" }>): Promise<NetResult> {
    const device = fromHex(c.deviceId);
    const bundle = fromHex(c.bundle);
    const opks = c.opks.map(fromHex);
    const pqopks = c.pqopks.map(fromHex);
    const r = await mutate(`${base}/v1/keys/bundles/${c.deviceId}`, {
      method: "PUT",
      body: encodeKeyPublish({ requestId: await requestId("keys", device, bundle, ...opks, ...pqopks), bundle, opks, pqopks }),
      bearer: await keyPublishCap(await secret(), device),
    });
    if (isResult(r)) return r;
    return r.kind === "home" ? mapError(r.error) : { result: "ok" };
  }

  async function fetchKeys(c: Extract<NetCall, { call: "fetch_key_material" }>): Promise<NetResult> {
    const r = await read(`${base}/v1/keys/fetch`, {
      method: "POST",
      // No relationship capability exists in Core yet (D5): a fixed anonymous placeholder; the Home throttles one-time items.
      body: encodeKeyFetch({ requestId: crypto.getRandomValues(new Uint8Array(16)), cap: new Uint8Array(16), devices: [fromHex(c.deviceId)] }),
    });
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error);
    return { result: "many", items: decodeKeyFetchResponse(r.body).items.map(toHex) };
  }

  async function putInvite(c: Extract<NetCall, { call: "put_invite" }>): Promise<NetResult> {
    const id = fromHex(c.inviteId);
    const body = fromHex(c.descriptor);
    const expiresAt = toSeconds(c.expiresAt);
    const r = await mutate(`${base}/v1/discovery/descriptors/${c.inviteId}`, {
      method: "PUT",
      body: encodeDescriptorPut({ requestId: await requestId("descriptor", id, body), body, expiresAt }),
      bearer: await descriptorPublishCap(await secret(), id),
    });
    if (isResult(r)) return r;
    return r.kind === "home" ? mapError(r.error) : { result: "ok" };
  }

  async function getInvite(c: Extract<NetCall, { call: "get_invite" }>): Promise<NetResult> {
    const r = await read(`${base}/v1/discovery/descriptors/${c.inviteId}`, { method: "GET" });
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error, `get:${c.inviteId}`);
    return { result: "bytes", bytes: toHex(decodeDescriptorGet(r.body).body) };
  }

  async function revokeInvite(c: Extract<NetCall, { call: "revoke_invite" }>): Promise<NetResult> {
    const id = fromHex(c.inviteId);
    const r = await mutate(`${base}/v1/discovery/descriptors/${c.inviteId}`, { method: "DELETE", bearer: await descriptorPublishCap(await secret(), id) });
    if (isResult(r)) return r;
    return r.kind === "home" ? mapError(r.error) : { result: "ok" };
  }

  // ------------------------------------------------------------------------------------------ Provider

  return {
    baseUrl: base,
    reachability: () => reach,
    lostMailboxes: () => [...lost],
    routeEntry: async (mailbox, expiresAt) => ({ relayUri: `${base}/`, capability: await depositCap(mailbox), priority: 0, expiresAt, transports: ["https"] }),
    async allocateRoutes(count) {
      const out: RouteOffer[] = [];
      for (let i = 0; i < count; i++) {
        const mailbox = crypto.getRandomValues(new Uint8Array(16));
        const entry = await depositCap(mailbox);
        out.push({ mailbox: toHex(mailbox), relayUri: `${base}/`, capability: toHex(entry) });
      }
      return out;
    },
    async capabilities() {
      const r = await exchange(http, `${base}/.well-known/om-provider-capabilities`, { method: "GET" });
      if (r.kind !== "ok") return undefined;
      try { return JSON.parse(new TextDecoder().decode(r.body)) as Record<string, unknown>; } catch { return undefined; }
    },
    async perform(call: NetCall): Promise<NetResult> {
      try {
        switch (call.call) {
          case "deposit": return await deposit(call.mailbox, call.bytes, call.relayUri);
          case "ack": return await ack(call.mailbox, call.seq);
          case "publish_key_material": return await publishKeys(call);
          case "fetch_key_material": return await fetchKeys(call);
          case "put_invite": return await putInvite(call);
          case "get_invite": return await getInvite(call);
          case "revoke_invite": return await revokeInvite(call);
        }
      } catch {
        return UNAVAILABLE; // a local failure (secret store, bad hex): nothing was sent
      }
    },
    poll: (mailbox, after) => poll(mailbox, after).catch(() => []),
  };
}
