// SPDX-License-Identifier: AGPL-3.0-or-later
// OM reference Home on Cloudflare: Worker front door for the Home API v1 reference HTTP binding (spec/protocol/home-api.md).
// The Worker routes by an opaque capability prefix or object id to a SQLite Durable Object and keeps nothing itself.
// It never reads a network address or any identifying header, writes no log, and sets no cookie.
import {
  CborError, HomeError, MEDIA_TURN_PATH, MEDIA_MAX_REPLY_BYTES, parseTurnLease, decodeTurnRequest, encodeTurnLease, decodeAckRequest, decodeDepositRequest, decodeDescriptorPut, decodeDrainRequest, decodeIntroDrainRequest, decodeIntroTicketRequest, decodeKeyFetch, decodeKeyFetchGrant,
  ADMISSION_HEADER, BLOB_DIGEST_HEADER, RECOVERY_CAP_BYTES, RECOVERY_RECORD_MAX_BYTES, RECOVERY_SIGNED_OBJECT_MAX_BYTES, REQUEST_ID_HEADER, checkRecordEnvelope, checkSignedRequest, checkSignedVeto, decodeFanout, decodeInboxAck, decodeInboxDrain, decodeInboxRegister, decodeRecordPut, toBase64Url, USAGE_PATH, quotaDetail, type UsageStateName, admissionHash, blobLocatorPattern, decodeBlobPutRequest, parseBlobDigestHeader, fromBase64Url, decodeKeyPublish, decodeMintRequest, decodeQueueCreate, decodeQueueRetireRequest, decodeRotateRequest, parseWakeProtocols, encodeKeyFetchResponse, decodeKeyFetchResponse, PUSH_PATHS, parsePushRequest, fromHex, decode, Obj, type HomeErrorName,
} from "@om/home-wire";
import { admissionConfigured, admissionHashes, blobsOn, capabilitiesDocument, mediaCapabilities, pushOn, recoveryOn, wakeOn } from "./capabilities";
import type { Env } from "./env";
import { bearer, fail, fromReply, json, ok, okEtag, preflight, privateJson, readBody } from "./http";
import { LIMITS } from "./limits";
import { UsageMeter, ceilings, classify, usageDocument, usageOn, type LeaseOutcome, type LeaseRecord, type UsageSnapshot } from "./usage";
import type { Reply } from "./util";
import { BlobSpool } from "./blobs";
import { DescriptorStore } from "./descriptors";
import { IntroGate } from "./intro";
import { KeyPool } from "./keys";
import { MailboxQueue } from "./queue";
import { PushHub } from "./push";
import { RecoveryStore } from "./recovery";

export { BlobSpool, DescriptorStore, IntroGate, KeyPool, MailboxQueue, PushHub, RecoveryStore, UsageMeter };

const hex = (b: Uint8Array): string => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
const queueOf = (env: Env, cap: Uint8Array): DurableObjectStub<MailboxQueue> => env.QUEUES.get(env.QUEUES.idFromName(`q:${hex(cap.slice(0, 16))}`));
const introOf = (env: Env, cap: Uint8Array): DurableObjectStub<IntroGate> => env.INTRO.get(env.INTRO.idFromName(`i:${hex(cap.slice(0, 8))}`));

const BODY_MAX = LIMITS.maxDepositBytes + 2048;
const BLOB_BODY_MAX = LIMITS.maxBlobBytes + 2048;
/** One spool for the whole Home: the spool quota is the Home's, and the owner is the only one who can create into it. */
const spoolOf = (env: Env): DurableObjectStub<BlobSpool> | undefined => env.BLOBS?.get(env.BLOBS.idFromName("spool"));

/** One RecoveryStore for the whole Home: the per-Home caps and the Home-wide rate buckets need one view (ADR-0201). */
const recoveryOf = (env: Env): DurableObjectStub<RecoveryStore> | undefined => env.RECOVERY?.get(env.RECOVERY.idFromName("store"));
/** The public scope id of the store is made once by the store and cached for the life of this isolate. */
let recoveryScope: string | undefined;
async function scopeOf(env: Env): Promise<string | undefined> {
  if (!recoveryOn(env)) return undefined;
  if (recoveryScope !== undefined) return recoveryScope;
  const store = recoveryOf(env);
  if (!store) return undefined;
  try {
    recoveryScope = toBase64Url(await store.scopeId());
    await chargeCalls(env, 1); // this capabilities read made a Durable Object call the front door did not count up front
  } catch { /* the document is still served; a client that needs the scope asks again */ }
  return recoveryScope;
}

/** A capability carries its routing prefix (16 bytes for a queue, 8 for an IntroGate service) plus at least 16 secret bytes. */
const wellFormed = (cap: Uint8Array, prefix: number): boolean => cap.length >= prefix + 16 && cap.length <= 96;

class Bad extends Error {
  constructor(readonly code: HomeErrorName) { super(code); }
}

async function body(request: Request, max: number): Promise<Uint8Array> {
  if (request.method !== "POST" && request.method !== "PUT") throw new Bad("invalid_request");
  const b = await readBody(request, max);
  if (b === undefined) throw new Bad("payload_too_large");
  return b;
}

function parse<T>(f: (b: Uint8Array) => T, b: Uint8Array): T {
  try { return f(b); } catch (e) { if (e instanceof CborError) throw new Bad("invalid_request"); throw e; }
}

/** Constant-time equality of two same-length hex strings. */
function sameHex(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}

/**
 * Owner admission (om.admission.v1). Where the deployment lists admission hashes, creating a resource (queue, descriptor, key
 * pool, intro service) needs the matching secret in the `om-admission` header; otherwise the answer is the same `unauthorized`
 * as for any bad capability. Peers never need it: deposit, drain, fetch and first-contact submission use their own capabilities.
 */
async function admitted(request: Request, env: Env): Promise<boolean> {
  const hashes = admissionHashes(env);
  if (hashes.length === 0) return !admissionConfigured(env); // set but unusable closes creation; only an unset variable leaves it open
  const h = request.headers.get(ADMISSION_HEADER);
  if (h === null) return false;
  let secret: Uint8Array;
  try { secret = fromBase64Url(h.trim()); } catch { return false; }
  if (secret.length !== 32) return false;
  const got = await admissionHash(secret);
  let ok = false;
  for (const want of hashes) ok = sameHex(got, want) || ok;
  return ok;
}

const KEY_FETCH_PATH = "/v1/keys/fetch";

/**
 * What the usage counter is told about a request up front: the Durable Object calls it causes besides the meter's own (see usage.ts).
 * A key fetch fans out to one Durable Object per requested device, so it is told none here and charged what it really made (`chargeCalls`).
 */
const routedCalls = (request: Request, path: string): number => (request.method !== "OPTIONS" && path.startsWith("/v1/") && path !== KEY_FETCH_PATH && path !== MEDIA_TURN_PATH ? 1 : 0);

/** Charges the meter for Durable Object calls a request made after it was counted, plus this charge's own call. A meter that cannot answer is ignored (fail open, as in `count`). */
async function chargeCalls(env: Env, made: number): Promise<void> {
  if (made <= 0 || !usageOn(env) || env.USAGE === undefined) return;
  try { await env.USAGE.get(env.USAGE.idFromName("meter")).charge(made + 1); } catch { /* unknown never sheds */ }
}

/** Work that adds to what the Home holds. At `hard` these are shed with a typed `quota`; drains, acks, rotations, reads and revocations never are. */
function addsWork(request: Request, path: string): boolean {
  return request.method === "POST" && (path === "/v1/relay/queues" || path === "/v1/relay/deposit" || path === "/v1/intro/requests" || path === "/v1/blobs");
}

interface Counted { readonly state: UsageStateName; readonly snapshot: UsageSnapshot | undefined }

/** Counts this request in the usage meter (personal profile only). A meter that cannot answer is `unknown`, and unknown never sheds: the free plan's own hard stop is the backstop, and a counter fault must not strand a person's mail. */
async function count(request: Request, env: Env): Promise<Counted | undefined> {
  if (!usageOn(env) || env.USAGE === undefined) return undefined;
  try {
    const snapshot = await env.USAGE.get(env.USAGE.idFromName("meter")).note(1 + routedCalls(request, new URL(request.url).pathname));
    return { state: classify(snapshot, ceilings(env)), snapshot };
  } catch {
    return { state: "unknown", snapshot: undefined };
  }
}

async function route(request: Request, env: Env, counted: Counted | undefined): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname;
  const m = request.method;

  if (m === "OPTIONS") return preflight();
  if (path === "/.well-known/om-provider-capabilities") return m === "GET" ? json(capabilitiesDocument(env, await scopeOf(env))) : fail("invalid_request");
  if (counted?.state === "hard" && addsWork(request, path)) return fail("quota", { detail: quotaDetail(3, true) });
  if (path === MEDIA_TURN_PATH) {
    const backend = env.MEDIA_TURN;
    if (!backend || !mediaCapabilities(env)) return fail("not_found");
    if (m !== "POST") return fail("invalid_request");
    if (!(await admitted(request, env))) return fail("unauthorized");
    const id = hex(parse(decodeTurnRequest, await body(request, 128)));
    // ADR-0228 s4: an attempt the Home has seen is answered from its own table and never gated by the budget again, so a
    // reservation made before the meter turned hard stays retrievable by its exact retry and costs nothing further.
    const meter = usageOn(env) && env.USAGE !== undefined ? env.USAGE.get(env.USAGE.idFromName("meter")) : undefined;
    let made = 0; // meter calls made here, charged after the fact like any other fan-out (the front door counted the request itself)
    const record = async (outcome: LeaseOutcome, expiresAt: number): Promise<void> => {
      if (meter === undefined) return;
      made++;
      try { await meter.leaseNote(id, outcome, expiresAt); } catch { /* an unrecorded attempt is only gated again on retry */ }
    };
    try {
      let known: LeaseRecord | null = null;
      if (meter !== undefined) {
        made++;
        try { known = await meter.leaseSeen(id); } catch { known = null; }
      }
      if (known === null) {
        if (counted?.state !== "healthy" && counted?.state !== "soft") return fail("quota", { detail: quotaDetail(3, true) });
        await record("pending", 0); // the reservation may exist from here on, whether or not the reply reaches the client
      } else if (known.outcome === "quota") {
        return fail("quota", { detail: quotaDetail(3, true) });
      } else if (known.outcome === "expired" || (known.outcome === "issued" && known.expires_at <= Math.floor(Date.now() / 1000))) {
        return fail("expired");
      }
      // A service binding or Unix socket handles this internal request. No DNS or public fetch. workerd has no redirect:"error", so "manual" is used and any 3xx falls through to the non-ok refusal below.
      const lease = await backend.fetch(new Request(new URL("/lease", url.origin), {
        method: "POST", redirect: "manual", signal: AbortSignal.timeout(5000),
        headers: { "content-type": "application/json" }, body: JSON.stringify({ request_id: id }),
      }));
      if (lease.status === 429) { await record("quota", 0); return fail("quota", { detail: quotaDetail(3, true) }); }
      if (lease.status === 410) { await record("expired", 0); return fail("expired"); }
      if (!lease.ok) return fail("retryable");
      const bytes = await readBody(lease, MEDIA_MAX_REPLY_BYTES);
      const result = bytes === undefined ? undefined
        : parseTurnLease(JSON.parse(new TextDecoder().decode(bytes)), Math.floor(Date.now() / 1000));
      if (result === undefined) return fail("retryable");
      await record("issued", result.expires_at);
      return ok(encodeTurnLease(result));
    } catch { return fail("retryable"); } finally { await chargeCalls(env, made); }
  }
  if (path === USAGE_PATH) {
    // Owner-only: the admin capability of any queue this Home holds. Never public, never in a URL.
    if (!usageOn(env)) return fail("not_found");
    if (m !== "GET") return fail("invalid_request");
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    const r = await queueOf(env, admin).adminCheck(admin);
    return r.ok ? privateJson(usageDocument(env, counted?.snapshot)) : fromReply(r);
  }
  if (path === "/" || path === "/healthz") return m === "GET" ? ok(new TextEncoder().encode("OM reference Home\n"), "text/plain; charset=utf-8") : fail("invalid_request");

  // ----------------------------------------------------------------------------------------------- relay
  if (path === "/v1/relay/queues") {
    if (!(await admitted(request, env))) return fail("unauthorized");
    const req = parse(decodeQueueCreate, await body(request, 1024));
    if (![req.receiveCap, req.depositCap, ...(req.adminCap ? [req.adminCap] : [])].every((c) => wellFormed(c, 16))) return fail("invalid_request");
    const prefix = hex(req.receiveCap.slice(0, 16));
    if (hex(req.depositCap.slice(0, 16)) !== prefix || (req.adminCap && hex(req.adminCap.slice(0, 16)) !== prefix)) return fail("invalid_request");
    return fromReply(await queueOf(env, req.receiveCap).create(req));
  }
  if (path === "/v1/relay/wake") {
    // The queue is named by a routing label in the subprotocol header, never by the URL; authority arrives in the first frame.
    if (!wakeOn(env)) return fail("not_found");
    if (m !== "GET" || request.headers.get("upgrade")?.toLowerCase() !== "websocket") return fail("invalid_request");
    const label = parseWakeProtocols(request.headers.get("sec-websocket-protocol"));
    if (!label) return fail("invalid_request");
    return env.QUEUES.get(env.QUEUES.idFromName(`q:${hex(label.queueId)}`)).fetch(request);
  }
  if (path === "/v1/relay/deposit") {
    const req = parse(decodeDepositRequest, await body(request, BODY_MAX));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).deposit(req));
  }
  if (path === "/v1/relay/drain") {
    const req = parse(decodeDrainRequest, await body(request, 1024));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).drain(req));
  }
  if (path === "/v1/relay/ack") {
    const req = parse(decodeAckRequest, await body(request, 8192));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).ack(req));
  }
  if (path === "/v1/relay/rotate") {
    const req = parse(decodeRotateRequest, await body(request, 1024));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).rotate(req));
  }
  if (path === "/v1/relay/deposit-capabilities") {
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).mint(admin, parse(decodeMintRequest, await body(request, 1024))));
  }
  const retireMatch = /^\/v1\/relay\/queues\/([0-9a-f]{32})\/retire$/.exec(path);
  if (retireMatch) {
    // ADR-0229 section 3: the queue is named in the path, the bearer is its admin capability and must carry the same routing prefix.
    if (m !== "POST") return fail("invalid_request");
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    if (hex(admin.slice(0, 16)) !== retireMatch[1]) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).retire(admin, parse(decodeQueueRetireRequest, await body(request, 1024))));
  }
  const revokeMatch = /^\/v1\/relay\/deposit-capabilities\/([0-9a-f]{32})$/.exec(path);
  if (revokeMatch) {
    if (m !== "DELETE") return fail("invalid_request");
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).revokeDepositCapability(admin, fromHex(revokeMatch[1] ?? "")));
  }

  // ----------------------------------------------------------------------------------------------- blob spool (om.blob.spool.v1)
  if (path === "/v1/blobs" || path.startsWith("/v1/blobs/")) {
    const spool = blobsOn(env) ? spoolOf(env) : undefined;
    if (!spool) return fail("not_found"); // not offered here (no binding, switched off, or no owner admission)
    if (path === "/v1/blobs") {
      if (m !== "POST") return fail("invalid_request");
      if (!(await admitted(request, env))) return fail("unauthorized"); // before the body is read: a stranger costs no memory
      const expected = parseBlobDigestHeader(request.headers.get(BLOB_DIGEST_HEADER));
      if (!expected) return fail("invalid_request");
      const req = parse(decodeBlobPutRequest, await body(request, BLOB_BODY_MAX));
      if (!wellFormed(req.cap, 16)) return fail("invalid_request");
      return fromReply(await spool.put(req, expected));
    }
    const locator = path.slice("/v1/blobs/".length);
    if (!blobLocatorPattern.test(locator)) return fail("not_found");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if (m === "GET") {
      const r = await spool.get(fromHex(locator), cap);
      return r.ok ? ok(r.body, "application/octet-stream") : fromReply(r);
    }
    if (m === "DELETE") return fromReply(await spool.remove(fromHex(locator), cap));
    return fail("invalid_request");
  }

  // ----------------------------------------------------------------------------------------------- push wake (om.relay.push.v1, ADR-0210)
  if (path === PUSH_PATHS.key || path === PUSH_PATHS.subscription) {
    const hub = pushOn(env) ? env.PUSH?.get(env.PUSH.idFromName("hub")) : undefined;
    if (!hub) return fail("not_found"); // not offered here (no binding, switched off, or no owner admission)
    if (path === PUSH_PATHS.key) return m === "GET" ? privateJson({ key: await hub.publicKey() }) : fail("invalid_request");
    if (m !== "PUT" && m !== "DELETE") return fail("invalid_request");
    // Registering is an owner act (admission) by the holder of the mailbox's receive capability; removing needs the capability only, so a device can always leave.
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 16) || (m === "PUT" && !(await admitted(request, env)))) return fail("unauthorized");
    const raw = await readBody(request, 1024);
    if (raw === undefined) return fail("payload_too_large");
    const endpoint = parsePushRequest(new TextDecoder().decode(raw));
    const armed = await queueOf(env, cap).pushArm(cap);
    if (!armed.ok) return fromReply(armed);
    if (endpoint === undefined) return fail("invalid_request");
    const label = hex(cap.slice(0, 16));
    if (m === "DELETE") { await hub.unregister(label, endpoint); return ok(new Uint8Array(0)); }
    const r = await hub.register(label, endpoint, env.OM_PUSH_SUBJECT ?? url.origin);
    return r.ok ? privateJson({ id: r.id, expires_at: r.expiresAt }) : fail(r.error);
  }
  // ----------------------------------------------------------------------------------------------- RecoveryStore (om.recovery.store.v1, ADR-0201)
  if (path.startsWith("/v1/recovery/")) {
    const store = recoveryOn(env) ? recoveryOf(env) : undefined;
    if (!store) return fail("not_found"); // not offered here (no binding, switched off, or no owner admission)
    return recovery(request, path, store, env);
  }

  // ----------------------------------------------------------------------------------------------- descriptors
  const desc = /^\/v1\/discovery\/descriptors\/([0-9a-f]{32})$/.exec(path);
  if (desc) {
    const stub = env.DESCRIPTORS.get(env.DESCRIPTORS.idFromName(`d:${desc[1] ?? ""}`));
    if (m === "GET") return fromReply(await stub.get());
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if ((m === "PUT" || m === "DELETE") && !(await admitted(request, env))) return fail("unauthorized");
    if (m === "PUT") return fromReply(await stub.put(cap, parse(decodeDescriptorPut, await body(request, LIMITS.maxDescriptorBytes + 1024))));
    if (m === "DELETE") return fromReply(await stub.revoke(cap));
    return fail("invalid_request");
  }

  // ----------------------------------------------------------------------------------------------- keys
  const bundle = /^\/v1\/keys\/bundles\/([0-9a-f]{32})$/.exec(path);
  if (bundle && m === "PUT") {
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if (!(await admitted(request, env))) return fail("unauthorized");
    const device = fromHex(bundle[1] ?? "");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${bundle[1] ?? ""}`)).publish(cap, device, parse(decodeKeyPublish, await body(request, 4 * LIMITS.maxBundleBytes))));
  }
  const grant = /^\/v1\/keys\/bundles\/([0-9a-f]{32})\/fetch-caps$/.exec(path);
  if (grant) {
    if (m !== "PUT") return fail("invalid_request");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    const req = parse(decodeKeyFetchGrant, await body(request, 2048));
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${grant[1] ?? ""}`)).grantFetch(cap, req.caps));
  }
  const gen = /^\/v1\/keys\/bundles\/([0-9a-f]{32})\/generations\/(\d{1,15})$/.exec(path);
  if (gen) {
    if (m !== "DELETE") return fail("invalid_request");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${gen[1] ?? ""}`)).revokeGeneration(cap, Number(gen[2])));
  }
  if (path === KEY_FETCH_PATH) {
    const req = parse(decodeKeyFetch, await body(request, 4096));
    // One flat list: for each requested device, its signed bundle then the one-time items consumed for this fetch.
    const items: Uint8Array[] = [];
    let made = 0; // per-device Durable Object calls actually issued, counted before each so a failing one is charged too
    try {
      for (const d of req.devices) {
        if (d.length !== 16) return fail("invalid_request");
        made++;
        const r: Reply = await env.KEYS.get(env.KEYS.idFromName(`k:${hex(d)}`)).fetchItems(req.cap);
        if (!r.ok) return fromReply(r);
        items.push(...decodeKeyFetchResponse(r.body).items);
      }
      return ok(encodeKeyFetchResponse({ items }));
    } finally {
      await chargeCalls(env, made);
    }
  }

  // ----------------------------------------------------------------------------------------------- IntroGate
  if (path === "/v1/intro/tickets") {
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 8) || !(await admitted(request, env))) return fail("unauthorized");
    return fromReply(await introOf(env, cap).issueTickets(cap, parse(decodeIntroTicketRequest, await body(request, 1024))));
  }
  // Revoking an invite revokes its ticket at this gate, so a request that still holds the link fails closed (INVITES_INTROGATE_AND_SPAM 3, 10).
  const ticket = /^\/v1\/intro\/tickets\/([0-9a-f]{32})$/.exec(path);
  if (ticket && m === "DELETE") {
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 8) || !(await admitted(request, env))) return fail("unauthorized");
    return fromReply(await introOf(env, cap).revokeTicket(cap, fromHex(ticket[1] ?? "")));
  }
  if (path === "/v1/intro/requests") {
    const b = await body(request, LIMITS.maxTicketPayload + 4096);
    // The ticket id sits at a fixed place of the canonical MessageRequest; reading it only routes, the gate validates it.
    const prefix = ticketPrefix(b);
    if (!prefix) return fail("invalid_request");
    return fromReply(await env.INTRO.get(env.INTRO.idFromName(`i:${hex(prefix)}`)).submit(b));
  }
  if (path === "/v1/intro/drain") {
    const req = parse(decodeIntroDrainRequest, await body(request, 1024));
    if (!wellFormed(req.cap, 8)) return fail("unauthorized");
    return fromReply(await introOf(env, req.cap).drain(req));
  }
  return fail("not_found");
}

// ------------------------------------------------------------------------------------------------------------------- RecoveryStore routes

/** base64url, no padding, exactly 32 bytes in canonical form (one path per value). */
function id32(s: string): Uint8Array | undefined {
  if (!/^[A-Za-z0-9_-]{43}$/.test(s)) return undefined;
  try {
    const b = fromBase64Url(s);
    return b.length === 32 && toBase64Url(b) === s ? b : undefined;
  } catch { return undefined; }
}

/** The capability of a store operation: exactly 256 bits in the Authorization header, never in a URL (home-api.md 3.1). */
const storeCap = (request: Request): Uint8Array | undefined => {
  const c = bearer(request);
  return c !== undefined && c.length === RECOVERY_CAP_BYTES ? c : undefined;
};

/** A body whose payload has a CDDL bound: the raw size is capped before decoding, and a payload past its bound is `payload_too_large`, not a malformed body. */
async function boundedBody(request: Request, payloadMax: number): Promise<Uint8Array> {
  const b = await body(request, payloadMax + 64);
  if (b.length > payloadMax + 27) throw new Bad("payload_too_large"); // the wrapper around the payload is at most 27 bytes
  return b;
}

/** `If-Match: "<base64url SHA-256>"` as the 32 bytes it names. */
function parseEtag(h: string): Uint8Array | undefined {
  const m = /^"([A-Za-z0-9_-]{43})"$/.exec(h.trim());
  return m?.[1] === undefined ? undefined : id32(m[1]);
}

/** The optional `OM-Request-Id` header of a body-less GET: 16 bytes, base64url. A malformed one is refused; its value is not otherwise used by a read. */
function requestIdHeaderOk(request: Request): boolean {
  const h = request.headers.get(REQUEST_ID_HEADER);
  if (h === null) return true;
  try { return fromBase64Url(h.trim()).length === 16; } catch { return false; }
}

async function recovery(request: Request, path: string, store: DurableObjectStub<RecoveryStore>, env: Env): Promise<Response> {
  const m = request.method;
  const rec = /^\/v1\/recovery\/records\/([^/]+)$/.exec(path);
  if (rec) {
    const locator = id32(rec[1] ?? "");
    if (!locator) return fail("not_found");
    if (m === "GET") {
      const cap = storeCap(request);
      if (!cap) return fail("not_found"); // a missing or malformed capability is answered like a wrong one
      if (!requestIdHeaderOk(request)) return fail("invalid_request");
      const r = await store.getRecord(locator, cap);
      return r.ok ? okEtag(r.body.slice(32), toBase64Url(r.body.slice(0, 32))) : fromReply(r);
    }
    if (m === "PUT") {
      const cap = storeCap(request);
      if (!cap) return fail("unauthorized");
      const inm = request.headers.get("if-none-match");
      const im = request.headers.get("if-match");
      if ((inm === null) === (im === null)) return fail("invalid_request"); // exactly one precondition: create or replace
      let ifMatch: Uint8Array | undefined;
      if (inm !== null) {
        if (inm.trim() !== "*") return fail("invalid_request");
        if (!(await admitted(request, env))) return fail("admission_denied"); // before the body is read: a stranger costs no memory
      } else {
        ifMatch = parseEtag(im ?? "");
        if (!ifMatch) return fail("invalid_request");
      }
      const req = parse(decodeRecordPut, await boundedBody(request, RECOVERY_RECORD_MAX_BYTES));
      parse(checkRecordEnvelope, req.envelope);
      const r = await store.putRecord({ locator, writeCap: cap, requestId: req.requestId, envelope: req.envelope, create: inm !== null, ...(ifMatch ? { ifMatch } : {}) });
      return r.ok ? okEtag(new Uint8Array(0), toBase64Url(r.body)) : fromReply(r);
    }
    if (m === "DELETE") {
      const cap = storeCap(request);
      return cap ? fromReply(await store.deleteRecord(locator, cap)) : fail("not_found");
    }
    return fail("invalid_request");
  }

  if (path === "/v1/recovery/requests") {
    if (m !== "POST") return fail("invalid_request");
    const req = parse(decodeFanout, await boundedBody(request, RECOVERY_SIGNED_OBJECT_MAX_BYTES));
    const shape = parse(checkSignedRequest, req.signed);
    return fromReply(await store.fanout({ ...req, ...shape }));
  }
  if (path === "/v1/recovery/vetoes") {
    if (m !== "POST") return fail("invalid_request");
    const req = parse(decodeFanout, await boundedBody(request, RECOVERY_SIGNED_OBJECT_MAX_BYTES));
    const shape = parse(checkSignedVeto, req.signed);
    const cap = storeCap(request);
    return fromReply(await store.postVeto({ ...req, ...shape, ...(cap === undefined ? {} : { cap }) }));
  }
  const board = /^\/v1\/recovery\/requests\/([^/]+)\/vetoes$/.exec(path);
  if (board) {
    if (m !== "GET") return fail("invalid_request");
    const digest = id32(board[1] ?? "");
    if (!digest) return fail("not_found");
    const r = await store.vetoes(digest);
    return r.ok ? ok(r.body) : fromReply(r);
  }
  const inbox = /^\/v1\/recovery\/inbox\/([^/]+?)(\/drain|\/ack)?$/.exec(path);
  if (inbox) {
    const inboxId = id32(inbox[1] ?? "");
    if (!inboxId) return fail("not_found");
    const sub = inbox[2];
    if (sub === undefined) {
      if (m !== "PUT") return fail("invalid_request");
      if (!(await admitted(request, env))) return fail("admission_denied");
      const req = parse(decodeInboxRegister, await body(request, 512));
      if (req.cap.length !== RECOVERY_CAP_BYTES) return fail("invalid_request");
      return fromReply(await store.registerInbox(inboxId, req.cap, req.requestId));
    }
    if (m !== "POST") return fail("invalid_request");
    if (sub === "/drain") {
      const req = parse(decodeInboxDrain, await body(request, 512));
      return req.cap.length === RECOVERY_CAP_BYTES ? fromReply(await store.drain(inboxId, req.cap, req.pageSize)) : fail("unauthorized");
    }
    const req = parse(decodeInboxAck, await body(request, 2048));
    return req.cap.length === RECOVERY_CAP_BYTES ? fromReply(await store.ack(inboxId, req.cap, req.itemIds)) : fail("unauthorized");
  }
  return fail("not_found");
}

function ticketPrefix(b: Uint8Array): Uint8Array | undefined {
  try {
    const o = new Obj(decode(b));
    o.uint(0); o.uint(1);
    return o.bytes(2, 16, 16).slice(0, 8);
  } catch { return undefined; }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      return await route(request, env, await count(request, env));
    } catch (e) {
      if (e instanceof Bad) return fail(e.code);
      if (e instanceof HomeError) return fail(e.errorName);
      // A bug or a Durable Object that is briefly unavailable. Say retryable, reveal nothing, log nothing.
      return fail("retryable", { retryAfterSeconds: 2 });
    }
  },
} satisfies ExportedHandler<Env>;
