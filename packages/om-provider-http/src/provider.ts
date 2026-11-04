// SPDX-License-Identifier: MPL-2.0
// The Core transport capability (`Provider` of @om/core-wasm: perform + poll) over the OM Home API v1 HTTP binding.
//
// Mapping of Core's stand-in calls to Home operations (deviations D1..D8 are recorded in
// docs/reports/2026-10-28-relay-status.md):
//   deposit / poll / ack            -> POST /v1/relay/deposit | drain | ack   (+ /v1/relay/queues on first poll)
//   publish_key_material            -> PUT  /v1/keys/bundles/{device}         (feature om.keys.pool.v1)
//   fetch_key_material              -> POST /v1/keys/fetch
//   put_invite / get_invite / revoke_invite -> PUT | GET | DELETE /v1/discovery/descriptors/{id}
//                                              (+ POST | DELETE /v1/intro/tickets[/{id}]: the invite's single-use ticket, om.intro.v1)
//   submit_intro                   -> POST /v1/intro/requests at the inviter's Home;   poll/ack of an invite -> POST /v1/intro/drain
//   grant_key_fetch                -> PUT  /v1/keys/bundles/{device}/fetch-caps         (om.keys.relationship-caps.v1)
//
// Homes. The configured Home is the person's own: it is where they poll, publish and create resources, and the only one
// that ever sees their admission secret. A peer's Home (named by an invite link, a signed descriptor, a route or a
// relationship) is only ever sent what a stranger may send (a deposit, a descriptor read, a MessageRequest, a fetch),
// with the capability the peer gave, over https (loopback http only when the host opts in), without redirects, and after
// one public capabilities read that must look like an OM Home. Nothing is stored about it.
//
// Failure classes follow Core's contract: `unavailable` = definitely not performed; `timeout` = may have been performed
// (every mutating request carries a content-derived request_id, so Core's exact retry is idempotent at the Home).
import type { BlobLimits, Inbound, MailboxKind, NetCall, NetError, NetResult, PerformExtra, Provider, RouteOffer } from "@om/core-wasm";
import {
  API_MAJOR, BLOB_DIGEST_HEADER, blobLocator, blobMatches, decodeBlobPutResponse, encodeBlobDigestHeader, encodeBlobPutRequest, decodeDescriptorGet, decodeDrainResponse, decodeKeyFetchResponse, encodeAckRequest, encodeDepositRequest, encodeDescriptorPut,
  encodeDrainRequest, encodeIntroTicketRequest, encodeKeyFetch, encodeKeyFetchGrant, encodeKeyPublish, encodeQueueCreate, encodeTicketParams, fromHex, toHex,
  FEATURES, encode, map, type HomeErrorBody, type RelayEntry,
} from "@om/home-wire";
import { exchange, type Exchange, type HttpOptions } from "./http";
import { depositCap, descriptorPublishCap, installSecret, introServiceCap, keyPublishCap, mailboxCaps, requestId } from "./derive";
import type { SecretStore } from "./secrets";
import { checkProviderUrl, type ProviderConfig } from "./config";
import { createWakeHub, type WakeHubOptions, type WakeSocketFactory, type WakeStats } from "./wake";

export interface HttpProviderOptions {
  readonly config: ProviderConfig;
  readonly secrets: SecretStore;
  readonly fetch?: typeof fetch;
  /** Per-request timeout (default 20 s). A timeout after a mutating request is `timeout` (unknown), never a rejection. */
  readonly timeoutMs?: number;
  /**
   * Wake channel (om.relay.wake.v1): how to open a WebSocket. Default is the platform's `WebSocket` when it has one. `false`
   * turns the channel off for this provider; the caller then polls on its timer exactly as before.
   */
  readonly webSocket?: WakeSocketFactory | false;
  /** The owner's admission secret for this Home (om.admission.v1), 32 random bytes made for this Home only; absent for a Home that does not ask. */
  readonly admission?: Uint8Array;
  /**
   * Accept a peer's Home at plain http on a loopback host (local development and the two-Home tests). Like the plain-http
   * rule for the configured Home, a production build never sets it.
   */
  readonly allowLoopbackHttp?: boolean;
  /** Tuning of the wake channel (timers, socket cap); tests use short values. */
  readonly wake?: Pick<WakeHubOptions, "pingMs" | "pongMs" | "safetyMs" | "connectMs" | "maxSockets">;
}

export type Reachability = "unknown" | "reachable" | "unreachable";

export interface HttpProvider extends Provider {
  /** Routes for new mailboxes at this Home (Provider.allocateRoutes, always present here). */
  allocateRoutes(count: number, held?: readonly string[]): Promise<readonly RouteOffer[]>;
  readonly baseUrl: string;
  /** True once this Home refused to create a resource because it asks for the owner's admission secret and this client's was missing or wrong. */
  admissionRefused(): boolean;
  reachability(): Reachability;
  /** Mailboxes this installation could not claim because another party already holds the queue (squatted). */
  lostMailboxes(): readonly string[];
  /** The spec's RelayEntry (route object) for a mailbox's deposit capability at this Home. */
  routeEntry(mailbox: Uint8Array, expiresAt: number): Promise<RelayEntry>;
  /** The Home's public ProviderCapabilities document, or undefined if it cannot be fetched. */
  capabilities(): Promise<Readonly<Record<string, unknown>> | undefined>;
  /**
   * The wake channel may hold sockets only while the app is on screen: the host calls this with its visibility. While a
   * socket is live for a mailbox, `poll` answers from it without asking the Home; otherwise it drains as before.
   */
  setWakeActive(active: boolean): void;
  /** Called when the Home reports something newer than this client holds (Provider.onWake). */
  onWake(listener: () => void): () => void;
  wakeStats(): WakeStats;
  /** Closes every socket and timer. The provider keeps working by polling. */
  dispose(): void;
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
  const http: HttpOptions = { fetch: opts.fetch ?? ((...a) => fetch(...a)), timeoutMs: opts.timeoutMs ?? 20_000, ...(opts.admission ? { admission: opts.admission } : {}) };
  let secretP: Promise<Uint8Array> | undefined;
  const secret = (): Promise<Uint8Array> => (secretP ??= installSecret(opts.secrets));
  let reach: Reachability = "unknown";
  const ensured = new Set<string>();
  const lost = new Set<string>();
  const ackIds = new Map<string, Map<number, Uint8Array>>();
  const backoff = new Map<string, { until: number; step: number }>();
  const socketFactory: WakeSocketFactory | undefined = opts.webSocket === false ? undefined : opts.webSocket ?? (typeof globalThis.WebSocket === "function" ? (u, p) => new globalThis.WebSocket(u, p) : undefined);

  // ------------------------------------------------------------------------------------------ plumbing

  const blocked = (key: string): boolean => (backoff.get(key)?.until ?? 0) > Date.now();
  function backOff(key: string, retryAfterSeconds?: number): void {
    const step = Math.min((backoff.get(key)?.step ?? 1000) * 2, 300_000);
    backoff.set(key, { until: Date.now() + (retryAfterSeconds === undefined ? step : toMs(retryAfterSeconds)), step });
  }
  const clear = (key: string): void => { backoff.delete(key); };

  // ------------------------------------------------------------------------------------------ Homes

  /** Where a request goes: the configured Home, or a peer's. Only the configured Home is sent the admission secret. */
  interface Target { readonly base: string; readonly http: HttpOptions; readonly own: boolean }
  const ownTarget: Target = { base, http, own: true };
  const peerHttp: HttpOptions = { fetch: http.fetch, timeoutMs: http.timeoutMs };
  type PeerView = { readonly kind: "ok" } | { readonly kind: "refused" } | { readonly kind: "unreachable" };
  const vetted = new Map<string, Promise<PeerView>>();

  /** A Home named by a link, descriptor, route or relationship: https (or opted-in loopback http), no credentials, nothing else in the URL. */
  function homeOf(uri: string | undefined): Target | undefined {
    if (uri === undefined) return ownTarget;
    // The configured Home was accepted when it was configured; a name for it is always fine. Any other must pass the rules now.
    const same = checkProviderUrl(uri, { allowLoopbackHttp: true });
    if (same.ok && same.config.baseUrl === base) return ownTarget;
    const c = checkProviderUrl(uri, { allowLoopbackHttp: opts.allowLoopbackHttp === true });
    return c.ok ? { base: c.config.baseUrl, http: peerHttp, own: false } : undefined;
  }

  async function capabilitiesOf(t: Target, timeoutMs = 5000): Promise<PeerView> {
    const r = await exchange({ ...t.http, timeoutMs: Math.min(t.http.timeoutMs, timeoutMs) }, `${t.base}/.well-known/om-provider-capabilities`, { method: "GET" });
    if (r.kind === "network") return { kind: "unreachable" };
    if (r.kind !== "ok" || r.body.length > 64 * 1024) return { kind: "refused" };
    try {
      const d = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(r.body)) as { schema_version?: unknown; api_versions?: unknown };
      if (d.schema_version !== 1 || !Array.isArray(d.api_versions) || !d.api_versions.includes(String(API_MAJOR))) return { kind: "refused" };
      return { kind: "ok" };
    } catch { return { kind: "refused" }; }
  }

  /**
   * The first request to a Home the person did not configure follows one public read of its capabilities document: it must
   * answer like an OM Home at this API version and offer what the call needs. Nothing identifying is sent before that. A Home
   * that cannot be reached is retried on the next call; one that answered wrongly is not asked again this session.
   */
  async function ready(t: Target): Promise<NetResult | undefined> {
    if (t.own) return undefined;
    let p = vetted.get(t.base);
    if (!p) { p = capabilitiesOf(t); vetted.set(t.base, p); }
    const v = await p;
    if (v.kind === "unreachable") { vetted.delete(t.base); return UNAVAILABLE; }
    if (v.kind === "refused") return rejected("unsupported");
    return undefined;
  }

  async function probe(t: Target = ownTarget): Promise<boolean> {
    const r = await exchange({ ...t.http, timeoutMs: Math.min(t.http.timeoutMs, 5000) }, `${t.base}/.well-known/om-provider-capabilities`, { method: "GET" });
    if (t.own) reach = r.kind === "ok" ? "reachable" : "unreachable";
    return r.kind === "ok";
  }

  /** A mutating request that got no answer: if the Home is reachable the request may have landed (unknown), otherwise it was not sent. */
  async function unknownOutcome(t: Target): Promise<NetResult> {
    return (await probe(t)) ? TIMEOUT : UNAVAILABLE;
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

  async function mutate(url: string, init: Parameters<typeof exchange>[2], key?: string, t: Target = ownTarget): Promise<Settled> {
    if (key !== undefined && blocked(key)) return UNAVAILABLE;
    const r = await exchange(t.http, url, init);
    if (r.kind === "network" || r.kind === "gateway") return unknownOutcome(t);
    if (t.own) reach = "reachable";
    return r;
  }

  const isResult = (x: Settled): x is NetResult => "result" in x;

  async function read(url: string, init: Parameters<typeof exchange>[2], t: Target = ownTarget): Promise<Settled> {
    const r = await exchange(t.http, url, init);
    if (r.kind === "network" || r.kind === "gateway") { if (t.own) reach = "unreachable"; return UNAVAILABLE; }
    if (t.own) reach = "reachable";
    return r;
  }

  async function advertisedFeatures(): Promise<readonly unknown[] | undefined> {
    const r = await exchange(http, `${base}/.well-known/om-provider-capabilities`, { method: "GET" });
    if (r.kind !== "ok") return undefined;
    try { const f = (JSON.parse(new TextDecoder().decode(r.body)) as { features?: unknown }).features; return Array.isArray(f) ? f : undefined; } catch { return undefined; }
  }

  const hub = createWakeHub({
    base,
    webSocket: socketFactory,
    supported: async () => (await advertisedFeatures())?.includes(FEATURES.relayWake) === true,
    credentials: async (mailboxHex) => {
      const caps = await mailboxCaps(await secret(), fromHex(mailboxHex));
      return { queueId: caps.queueId, receive: caps.receive };
    },
    ...opts.wake,
  });

  // ------------------------------------------------------------------------------------------ queues

  let admissionDenied = false;

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
    if (r.error.name === "unauthorized") {
      // A Home that asks for the owner's admission refuses a client without it for a reason that has nothing to do with another
      // party holding the queue: say so (reported by `admissionRefused`) and keep the mailbox, instead of calling it squatted.
      if ((await advertisedFeatures())?.includes(FEATURES.admission)) { admissionDenied = true; return "later"; }
      lost.add(mailboxHex);
      return "lost";
    }
    return "later";
  }

  async function drainPage(caps: Uint8Array, after: number, t: Target = ownTarget): Promise<{ items: { seq: number; id: Uint8Array; bytes: Uint8Array }[]; next: number | undefined } | undefined> {
    const r = await read(`${t.base}/v1/relay/drain`, { method: "POST", body: encodeDrainRequest({ requestId: crypto.getRandomValues(new Uint8Array(16)), cap: caps, after }) }, t);
    if (isResult(r) || r.kind !== "ok") return undefined;
    const page = decodeDrainResponse(r.body);
    const next = page.next ? Number(new DataView(page.next.buffer, page.next.byteOffset, page.next.byteLength).getBigUint64(0)) : undefined;
    return { items: page.items.map((i) => ({ seq: i.seq ?? 0, id: i.id, bytes: i.bytes })), next };
  }

  /**
   * An invite's requests wait in its introduction service at the person's own Home. The drain acknowledges what the caller
   * already holds (`after`), so polling and acknowledging are the same request (om.intro.v1, drain with implicit ack).
   */
  async function introDrain(inviteHex: string, after: number): Promise<{ items: Inbound[]; ok: boolean }> {
    const cap = await introServiceCap(await secret(), fromHex(inviteHex));
    const out: Inbound[] = [];
    let cursor = after;
    for (let i = 0; i < MAX_PAGES; i++) {
      const r = await read(`${base}/v1/intro/drain`, { method: "POST", body: encodeDrainRequest({ requestId: crypto.getRandomValues(new Uint8Array(16)), cap, after: cursor }) });
      if (isResult(r) || r.kind !== "ok") return { items: out, ok: false };
      const page = decodeDrainResponse(r.body);
      for (const it of page.items) if ((it.seq ?? 0) > after) out.push({ seq: it.seq ?? 0, bytes: toHex(it.bytes) });
      if (!page.next) break;
      cursor = Number(new DataView(page.next.buffer, page.next.byteOffset, page.next.byteLength).getBigUint64(0));
    }
    return { items: out, ok: true };
  }

  /**
   * A queue another install opened for a device-link ceremony (D24): drained with the receive capability the link carried,
   * at the Home the link names. The existing queue endpoints, nothing new; no wake socket, no queue creation.
   */
  async function pollWithCapability(mailboxHex: string, after: number, relayUri: string | undefined, capability: string): Promise<readonly Inbound[]> {
    const t = homeOf(relayUri);
    if (!t || (await ready(t))) return [];
    const cap = fromHex(capability);
    const out: Inbound[] = [];
    const ids = ackIds.get(mailboxHex) ?? new Map<number, Uint8Array>();
    let cursor = after;
    for (let i = 0; i < MAX_PAGES; i++) {
      const page = await drainPage(cap, cursor, t);
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

  /** Make sure a ceremony queue exists at our own Home and answer its receive capability, so the other device can drain it. */
  async function openQueue(c: Extract<NetCall, { call: "open_queue" }>): Promise<NetResult> {
    const t = homeOf(c.relayUri);
    if (!t || !t.own) return rejected("unsupported");
    const mailbox = fromHex(c.mailbox);
    if ((await ensureQueue(mailbox, c.mailbox)) !== "ok") return UNAVAILABLE;
    return { result: "bytes", bytes: toHex((await mailboxCaps(await secret(), mailbox)).receive) };
  }

  async function poll(mailboxHex: string, after: number, kind: MailboxKind = "relay", relayUri?: string, capability?: string): Promise<readonly Inbound[]> {
    if (kind === "intro") return (await introDrain(mailboxHex, after)).items;
    if (capability !== undefined && capability !== "") return pollWithCapability(mailboxHex, after, relayUri, capability);
    const mailbox = fromHex(mailboxHex);
    if ((await ensureQueue(mailbox, mailboxHex)) !== "ok") return [];
    // A live wake socket for this mailbox vouches that nothing newer exists: skip the request. Otherwise drain as always.
    hub.watch(mailboxHex, after);
    if (hub.quiet(mailboxHex, after)) return [];
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
    hub.drained(mailboxHex, out.reduce((m, i) => Math.max(m, i.seq), 0));
    return out;
  }

  /** Core acknowledges cumulatively ("everything up to seq is processed"); the Home acknowledges item ids. */
  async function ack(mailboxHex: string, seq: number, kind: MailboxKind = "relay", relayUri?: string, capability?: string): Promise<NetResult> {
    if (kind === "intro") return (await introDrain(mailboxHex, seq)).ok ? { result: "ok" } : UNAVAILABLE;
    // A ceremony queue another install opened is acknowledged with the capability its link carried, at the Home it names.
    const explicit = capability !== undefined && capability !== "" ? fromHex(capability) : undefined;
    const t = explicit ? homeOf(relayUri) : ownTarget;
    if (!t) return rejected("unsupported");
    if (explicit) { const not = await ready(t); if (not) return not; }
    const caps = explicit ? { queueId: explicit.slice(0, 16), receive: explicit } : await mailboxCaps(await secret(), fromHex(mailboxHex));
    const known = ackIds.get(mailboxHex) ?? new Map<number, Uint8Array>();
    let ids = [...known.entries()].filter(([s]) => s <= seq).sort(([a], [b]) => a - b);
    if (ids.length === 0) {
      // After a restart the id cache is empty: re-read the window below `seq` to learn the ids (rare; bounded by ACK_WINDOW).
      let cursor = Math.max(0, seq - ACK_WINDOW);
      for (let i = 0; i < MAX_PAGES; i++) {
        const page = await drainPage(caps.receive, cursor, t);
        if (!page) return UNAVAILABLE;
        for (const it of page.items) if (it.seq <= seq) known.set(it.seq, it.id);
        if (page.next === undefined || page.next >= seq) break;
        cursor = page.next;
      }
      ids = [...known.entries()].filter(([s]) => s <= seq).sort(([a], [b]) => a - b);
    }
    for (let at = 0; at < ids.length; at += ACK_WINDOW) {
      const chunk = ids.slice(at, at + ACK_WINDOW);
      const r = await mutate(`${t.base}/v1/relay/ack`, {
        method: "POST",
        body: encodeAckRequest({ requestId: await requestId("ack", caps.queueId, ...chunk.map(([, id]) => id)), cap: caps.receive, itemIds: chunk.map(([, id]) => id) }),
      }, undefined, t);
      if (isResult(r)) return r;
      if (r.kind === "home" && r.error.name !== "not_found") return mapError(r.error);
      for (const [s] of chunk) known.delete(s); // not_found means already acknowledged: the same end state
    }
    ackIds.set(mailboxHex, known);
    return { result: "ok" };
  }

  /**
   * `target` is the destination's deposit capability as its RelayEntry carried it, with that entry's `relayUri`: the deposit
   * goes to the Home that entry names, which is the recipient's own (MULTI_RELAY section 1: the sender deposits to the
   * route the recipient gave). A legacy destination (no `relayUri`, the bare 16-byte mailbox) is at the configured Home and
   * has its capability derived, as before routes existed.
   */
  async function deposit(targetHex: string, bytesHex: string, relayUri: string | undefined): Promise<NetResult> {
    const target = fromHex(targetHex);
    const bytes = fromHex(bytesHex);
    let cap: Uint8Array;
    let t = ownTarget;
    if (relayUri === undefined) cap = await depositCap(target);
    else {
      const h = homeOf(relayUri);
      if (!h) return rejected("unsupported");
      t = h;
      if (target.length < 16 || target.length > 96) return rejected("invalid");
      cap = target;
    }
    const not = await ready(t);
    if (not) return not;
    const key = `deposit:${t.base}:${targetHex}`;
    const r = await mutate(`${t.base}/v1/relay/deposit`, {
      method: "POST",
      body: encodeDepositRequest({ requestId: await requestId("deposit", target, bytes), cap, envelope: bytes }),
    }, key, t);
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

  /** The device owner lets the holder of `capability` fetch this device's bundle: the grant is made at the owner's own Home. */
  async function grantKeyFetch(c: Extract<NetCall, { call: "grant_key_fetch" }>): Promise<NetResult> {
    const device = fromHex(c.deviceId);
    const cap = fromHex(c.capability);
    const r = await mutate(`${base}/v1/keys/bundles/${c.deviceId}/fetch-caps`, {
      method: "PUT",
      body: encodeKeyFetchGrant({ requestId: await requestId("fetch-cap", device, cap), caps: [cap] }),
      bearer: await keyPublishCap(await secret(), device),
    });
    if (isResult(r)) return r;
    return r.kind === "home" ? mapError(r.error) : { result: "ok" };
  }

  /**
   * Fetch a peer device's bundle and one-time items at the Home that holds its pool, with the per-relationship capability the
   * peer issued (ASYNC_KEY_MATERIAL 2.1; D4). A peer from before capabilities existed sends none: the Home then answers
   * `unauthorized`, which is a refusal, not a downgrade to anonymous fetch.
   */
  async function fetchKeys(c: Extract<NetCall, { call: "fetch_key_material" }>): Promise<NetResult> {
    const t = homeOf(c.service);
    if (!t) return rejected("unsupported");
    const not = await ready(t);
    if (not) return not;
    const cap = c.capability === undefined || c.capability === "" ? new Uint8Array(16) : fromHex(c.capability);
    const r = await read(`${t.base}/v1/keys/fetch`, {
      method: "POST",
      body: encodeKeyFetch({ requestId: crypto.getRandomValues(new Uint8Array(16)), cap, devices: [fromHex(c.deviceId)] }),
    }, t);
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error);
    return { result: "many", items: decodeKeyFetchResponse(r.body).items.map(toHex) };
  }

  /**
   * The inviter's own Home is its introduction service: the descriptor is stored under the invite id, and the invite's single
   * ticket is opened under the id Core signed (ADR-0181 section 4), in a service only this install can drain.
   */
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
    if (r.kind === "home") return mapError(r.error);
    const ttlSeconds = Math.max(1, Math.min(expiresAt - Math.floor(Date.now() / 1000), 14 * 86_400));
    const params = encodeTicketParams({ ttlSeconds, maxUses: 1, payloadLimitBytes: c.payloadLimit && c.payloadLimit > 0 ? c.payloadLimit : 32_768, allowAttachments: false, allowCalls: false, ticketId: id });
    const t = await mutate(`${base}/v1/intro/tickets`, {
      method: "POST",
      body: encodeIntroTicketRequest({ requestId: await requestId("ticket", id), params, count: 1 }),
      bearer: await introServiceCap(await secret(), id),
    });
    if (isResult(t)) return t;
    return t.kind === "home" ? mapError(t.error) : { result: "ok" };
  }

  /** The descriptor comes from the introduction service the link names; the person's tap on Accept is what sends this. */
  async function getInvite(c: Extract<NetCall, { call: "get_invite" }>): Promise<NetResult> {
    const t = homeOf(c.service);
    if (!t) return rejected("unsupported");
    const not = await ready(t);
    if (not) return not;
    const r = await read(`${t.base}/v1/discovery/descriptors/${c.inviteId}`, { method: "GET" }, t);
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error, `get:${t.base}:${c.inviteId}`);
    return { result: "bytes", bytes: toHex(decodeDescriptorGet(r.body).body) };
  }

  /** A sealed MessageRequest for the inviter's introduction service. The gate admits by the ticket inside it; nothing else is sent. */
  async function submitIntro(c: Extract<NetCall, { call: "submit_intro" }>): Promise<NetResult> {
    const t = homeOf(c.service);
    if (!t) return rejected("unsupported");
    const not = await ready(t);
    if (not) return not;
    const bytes = fromHex(c.bytes);
    const key = `intro:${t.base}:${toHex(bytes.slice(0, 64))}`;
    const r = await mutate(`${t.base}/v1/intro/requests`, { method: "POST", body: bytes }, key, t);
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error, key);
    clear(key);
    return { result: "ok" };
  }

  async function revokeInvite(c: Extract<NetCall, { call: "revoke_invite" }>): Promise<NetResult> {
    const id = fromHex(c.inviteId);
    const r = await mutate(`${base}/v1/discovery/descriptors/${c.inviteId}`, { method: "DELETE", bearer: await descriptorPublishCap(await secret(), id) });
    if (isResult(r)) return r;
    if (r.kind === "home") return mapError(r.error);
    // The gate stops admitting requests for this invite too; a Home that never opened a ticket for it treats this as done.
    const t = await mutate(`${base}/v1/intro/tickets/${c.inviteId}`, { method: "DELETE", bearer: await introServiceCap(await secret(), id) });
    if (isResult(t)) return t;
    return t.kind === "home" ? mapError(t.error) : { result: "ok" };
  }

  // ------------------------------------------------------------------------------------------ blobs (om.blob.spool.v1)
  //
  // Files are sealed on the device (Core); the Home holds only ciphertext for a few days and never sees a name, a type or a key.
  //   put_blob    -> POST   /v1/blobs            owner admission + `om-ciphertext-digest`; the capability goes in the CBOR body
  //   get_blob    -> GET    /v1/blobs/{locator}  the read capability as bearer, at the Home the sender's reference names
  //   delete_blob -> DELETE /v1/blobs/{locator}  the owner capability as bearer
  // A reference carries https locations only (Core drops anything else). On a loopback Home reached over plain http, which only a
  // test build allows, the location is written as https and read back as http here, so Core's rule stays as it is.

  const [PLAIN, SECURE] = ["http", "https"] as const;
  const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "[::1]"]);
  const BLOB_PATH = /^\/v1\/blobs\/([0-9a-f]{32})$/;
  /** The name a peer is given for an object of ours: the Home's public address, always https. */
  const locationOf = (locator: string): string => `${opts.allowLoopbackHttp === true && base.startsWith(`${PLAIN}:`) ? `${SECURE}:${base.slice(PLAIN.length + 1)}` : base}/v1/blobs/${locator}`;
  /** Where a reference's location is fetched from (https as written, or the loopback Home's plain http in a test build), and its locator. */
  function placeOf(location: string): { readonly origin: string; readonly locator: string } | undefined {
    let u: URL;
    try { u = new URL(location); } catch { return undefined; }
    if (u.protocol !== "https:" || u.username !== "" || u.password !== "" || u.search !== "" || u.hash !== "") return undefined;
    const m = BLOB_PATH.exec(u.pathname);
    if (!m) return undefined;
    const plain = opts.allowLoopbackHttp === true && LOOPBACK_HOSTS.has(u.hostname);
    return { origin: plain ? `${PLAIN}://${u.host}` : u.origin, locator: m[1] ?? "" };
  }

  async function putBlob(c: Extract<NetCall, { call: "put_blob" }>, ciphertext: Uint8Array | undefined): Promise<NetResult> {
    const ownerCap = fromHex(c.ownerCap);
    const digest = fromHex(c.digest);
    // The staged bytes must be exactly what Core sealed: Core committed to this digest and length before anything left.
    if (ciphertext === undefined || ciphertext.length !== c.length || !(await blobMatches(ciphertext, digest))) return rejected("integrity_failure");
    const r = await mutate(`${base}/v1/blobs`, {
      method: "POST",
      body: encodeBlobPutRequest({ requestId: fromHex(c.requestId), cap: ownerCap, ciphertext }),
      headers: { [BLOB_DIGEST_HEADER]: encodeBlobDigestHeader(digest) },
    });
    if (isResult(r)) return r;
    if (r.kind === "home") {
      switch (r.error.name) {
        case "payload_too_large": return rejected("too_large"); // this one file, not the spool: said before sending, in plain words
        case "quota": return rejected("quota"); // the spool is full right now; the person's own retry may succeed later
        case "not_found": return rejected("unsupported"); // this Home does not hold files
        case "unauthorized": admissionDenied = true; return rejected("unauthorized");
        default: return mapError(r.error);
      }
    }
    let put: ReturnType<typeof decodeBlobPutResponse>;
    try { put = decodeBlobPutResponse(r.body); } catch { return rejected("integrity_failure"); }
    const locator = blobLocator(ownerCap);
    // The Home must have stored the bytes we hashed, under the locator we chose: anything else is removed and refused.
    if (toHex(put.digest) !== c.digest || toHex(put.readCap.slice(0, 16)) !== locator) {
      void mutate(`${base}/v1/blobs/${locator}`, { method: "DELETE", bearer: ownerCap });
      return rejected("integrity_failure");
    }
    const receipt = encode(map([[0, put.readCap], [1, [locationOf(locator)]], [2, put.digest], [3, put.expiresAt]]));
    return { result: "bytes", bytes: toHex(receipt) };
  }

  async function getBlob(c: Extract<NetCall, { call: "get_blob" }>): Promise<NetResult> {
    const readCap = fromHex(c.readCap);
    const digest = fromHex(c.digest);
    let unreachable = false;
    for (const location of c.locations) {
      const place = placeOf(location);
      const t = place === undefined ? undefined : homeOf(place.origin);
      if (place === undefined || t === undefined) continue;
      const not = await ready(t);
      if (not) { if (not === UNAVAILABLE) unreachable = true; continue; }
      const r = await read(`${t.base}/v1/blobs/${place.locator}`, { method: "GET", bearer: readCap }, t);
      if (isResult(r)) { unreachable = true; continue; }
      if (r.kind === "home") continue; // not_found, expired, unauthorized: try the next location, else the object is gone
      // Never hand bytes to Core that are not the object the message committed to (Core checks again before it decrypts).
      if (r.body.length !== c.length || !(await blobMatches(r.body, digest))) return rejected("integrity_failure");
      return { result: "bytes", bytes: toHex(r.body) };
    }
    return unreachable ? UNAVAILABLE : { result: "err", error: { kind: "not_found" } };
  }

  async function deleteBlob(c: Extract<NetCall, { call: "delete_blob" }>): Promise<NetResult> {
    const place = placeOf(c.location);
    const t = place === undefined ? undefined : homeOf(place.origin);
    if (place === undefined || t === undefined || !t.own) return rejected("unsupported"); // only our own Home holds objects of ours
    const r = await mutate(`${t.base}/v1/blobs/${place.locator}`, { method: "DELETE", bearer: fromHex(c.ownerCap) });
    if (isResult(r)) return r;
    if (r.kind === "home") return r.error.name === "not_found" ? { result: "ok" } : mapError(r.error);
    return { result: "ok" };
  }

  /** What the Home advertises for files, read from its public capabilities document; null when it holds none. */
  async function blobLimits(): Promise<BlobLimits | null> {
    const r = await exchange(http, `${base}/.well-known/om-provider-capabilities`, { method: "GET" });
    if (r.kind !== "ok") throw new Error("capabilities unavailable"); // could not find out: the caller keeps its last answer
    let doc: { features?: unknown; limits?: Record<string, unknown> };
    try { doc = JSON.parse(new TextDecoder().decode(r.body)) as typeof doc; } catch { throw new Error("capabilities unreadable"); }
    if (!Array.isArray(doc.features) || !doc.features.includes(FEATURES.blobSpool)) return null;
    const n = (k: string): number | undefined => { const v = doc.limits?.[k]; return typeof v === "number" && Number.isSafeInteger(v) && v > 0 ? v : undefined; };
    const maxBlobBytes = n("max_blob_bytes");
    if (maxBlobBytes === undefined) return null; // a feature without its bound is not usable: fail closed
    const spool = n("max_blob_spool_bytes");
    const count = n("max_blobs");
    return { maxBlobBytes, ...(spool === undefined ? {} : { maxSpoolBytes: spool }), ...(count === undefined ? {} : { maxBlobs: count }) };
  }

  // ------------------------------------------------------------------------------------------ Provider

  return {
    baseUrl: base,
    reachability: () => reach,
    admissionRefused: () => admissionDenied,
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
    setWakeActive: (active) => { hub.setActive(active); },
    onWake: (listener) => hub.onWake(listener),
    wakeStats: () => hub.stats(),
    dispose: () => { hub.dispose(); },
    blobLimits,
    async perform(call: NetCall, extra?: PerformExtra): Promise<NetResult> {
      try {
        switch (call.call) {
          case "deposit": return await deposit(call.mailbox, call.bytes, call.relayUri);
          case "ack": return await ack(call.mailbox, call.seq, call.kind, call.relayUri, call.capability);
          case "open_queue": return await openQueue(call);
          case "publish_key_material": return await publishKeys(call);
          case "grant_key_fetch": return await grantKeyFetch(call);
          case "fetch_key_material": return await fetchKeys(call);
          case "submit_intro": return await submitIntro(call);
          case "put_invite": return await putInvite(call);
          case "get_invite": return await getInvite(call);
          case "revoke_invite": return await revokeInvite(call);
          case "put_blob": return await putBlob(call, extra?.ciphertext);
          case "get_blob": return await getBlob(call);
          case "delete_blob": return await deleteBlob(call);
        }
      } catch {
        return UNAVAILABLE; // a local failure (secret store, bad hex): nothing was sent
      }
    },
    poll: (mailbox, after, kind, relayUri, capability) => poll(mailbox, after, kind, relayUri, capability).catch(() => []),
  };
}
