// SPDX-License-Identifier: MPL-2.0
// Client of the RecoveryStore of a Home (feature om.recovery.store.v1, ADR-0201, home-api.md section 8): the encrypted record (put, get,
// delete), the recovery request fan-out, the veto board, and the controller's inbox (register, drain, ack). It speaks only the HTTP
// binding and knows nothing of Core: Core hands it sealed envelopes and signed objects as bytes and gets bytes back.
//
// Rules this client keeps (home-api.md 3.1, ADR-0141): a capability travels only in `Authorization: Bearer` or the frozen CBOR slot,
// never in a URL; no cookie, no redirect, no referrer. The record locator in the path is a hash of the read capability, not a
// capability. The owner's admission secret is sent only on the two operations that create a resource (a record create and an inbox
// register), and only to this Home. A mutating request has a 128-bit request id; a caller that retries an unknown outcome passes the
// same id and the same bytes (Home API v1 section 5), and a network failure after sending is reported as `network`, never as a rejection.
import {
  ADMISSION_HEADER, FEATURES, RECOVERY_CAP_BYTES, RECOVERY_PATHS, REQUEST_ID_HEADER, decodeInboxDrainResponse, decodeRecordGetResponse, decodeVetoList, encodeFanout, encodeInboxAck, encodeInboxDrain, encodeInboxRegister, encodeRecordPut, encodeVetoPost,
  fromBase64Url, recoveryEtag, recoveryInboxId, recoveryLocator, sha256, toBase64Url, utf8, concat,
  type HomeErrorName, type InboxItem,
} from "@om/home-wire";
import { checkProviderUrl, type ProviderConfig } from "./config";
import { exchange, type HttpOptions } from "./http";

export interface RecoveryStoreOptions {
  readonly config: ProviderConfig;
  readonly fetch?: typeof fetch;
  readonly timeoutMs?: number;
  /** The owner's admission secret for THIS Home (om.admission.v1); sent only to create a record or register an inbox. */
  readonly admission?: Uint8Array;
}

/** What a Home that offers the store says about it (capabilities document). */
export interface RecoveryStoreInfo {
  /** The 32-byte public scope id the record capabilities derive from. */
  readonly scopeId: Uint8Array;
  readonly limits: Readonly<Record<string, unknown>>;
}

/**
 * `home`: a typed Home answer (`name`, with `retryAfterSeconds` for `rate_limited`). `network`: no answer, so a mutation's outcome is
 * unknown and is retried with the same request id and bytes. `gateway`: an answer that is not a typed Home error (outcome unknown too).
 */
export type RecoveryFailure =
  | { readonly kind: "home"; readonly name: HomeErrorName | "unknown"; readonly retryable: boolean; readonly retryAfterSeconds?: number }
  | { readonly kind: "network" }
  | { readonly kind: "gateway"; readonly status: number };
export type RecoveryResult<T> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly failure: RecoveryFailure };

export interface RecordWrite {
  /** `DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`; see `recordLocator`. */
  readonly locator: Uint8Array;
  readonly writeCap: Uint8Array;
  /** The sealed RecoveryRecordEnvelope (type 35), at most 1,048,576 bytes. Sealed once per staged write; every retry sends these exact bytes. */
  readonly envelope: Uint8Array;
  /** Replace only when the stored envelope's ETag still equals this (compare-and-swap); absent means create. */
  readonly ifMatch?: Uint8Array;
  /** The fixed 128-bit id of the staged write; random when absent. */
  readonly requestId?: Uint8Array;
}

export interface InboxAuth { readonly accountId: Uint8Array; readonly cap: Uint8Array }
export interface DrainPage { readonly items: readonly InboxItem[]; /** Present only when more items wait; ack what was handled and drain again. */ readonly more: boolean }

export interface RecoveryStoreClient {
  readonly baseUrl: string;
  /** `undefined` when the Home does not advertise the store (or says something unusable): "no RecoveryStore here". */
  info(): Promise<RecoveryStoreInfo | undefined>;
  /** Create (no `ifMatch`) or compare-and-swap replace. The value is the new ETag. */
  putRecord(w: RecordWrite): Promise<RecoveryResult<Uint8Array>>;
  /** The sealed envelope and its ETag. A wrong capability and an absent record are the same `not_found`. */
  getRecord(readCap: Uint8Array): Promise<RecoveryResult<{ readonly envelope: Uint8Array; readonly etag: Uint8Array }>>;
  /** Idempotent: `not_found` (absent, or not this capability) counts as done. */
  deleteRecord(locator: Uint8Array, writeCap: Uint8Array): Promise<RecoveryResult<undefined>>;
  /** Posts a signed RecoveryRequest to this Home. The Home checks shape only; the answer is the same whether or not an inbox exists. */
  fanOut(signedRequest: Uint8Array): Promise<RecoveryResult<undefined>>;
  postVeto(signedVeto: Uint8Array): Promise<RecoveryResult<undefined>>;
  /** The vetoes stored for one request digest (the 32-byte `OM-RECOVERY-REQUEST-HASH-v1` digest value); no capability. */
  readVetoes(requestDigest: Uint8Array): Promise<RecoveryResult<readonly Uint8Array[]>>;
  registerInbox(a: InboxAuth): Promise<RecoveryResult<undefined>>;
  drainInbox(a: InboxAuth, pageSize?: number): Promise<RecoveryResult<DrainPage>>;
  ackInbox(a: InboxAuth, itemIds: readonly Uint8Array[]): Promise<RecoveryResult<undefined>>;
}

/** `DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)`, the locator a record is stored under. */
export const recordLocator = recoveryLocator;
export { recoveryEtag, recoveryInboxId };

const rand16 = (): Uint8Array => crypto.getRandomValues(new Uint8Array(16));
const cap32 = (c: Uint8Array): boolean => c.length === RECOVERY_CAP_BYTES;
const NETWORK: RecoveryFailure = { kind: "network" };
const bad = (): RecoveryResult<never> => ({ ok: false, failure: { kind: "home", name: "invalid_request", retryable: false } });

/** Content-derived request id for posts whose retry must be exact (the same bytes always carry the same id). */
async function idFor(label: string, bytes: Uint8Array): Promise<Uint8Array> {
  return (await sha256(concat(utf8(label), bytes))).slice(0, 16);
}

function etagOf(headers: Headers): Uint8Array | undefined {
  const m = /^"([A-Za-z0-9_-]{43})"$/.exec((headers.get("etag") ?? "").trim());
  if (m?.[1] === undefined) return undefined;
  try { const b = fromBase64Url(m[1]); return b.length === 32 ? b : undefined; } catch { return undefined; }
}

export function createRecoveryStoreClient(opts: RecoveryStoreOptions): RecoveryStoreClient {
  const base = opts.config.baseUrl;
  const http: HttpOptions = { fetch: opts.fetch ?? ((...a) => fetch(...a)), timeoutMs: opts.timeoutMs ?? 20_000 }; // no admission here: it is attached by hand, to creations only
  const owner: Record<string, string> = opts.admission === undefined ? {} : { [ADMISSION_HEADER]: toBase64Url(opts.admission) };

  async function call<T>(path: string, init: Parameters<typeof exchange>[2], ok: (body: Uint8Array, headers: Headers) => T | undefined): Promise<RecoveryResult<T>> {
    const r = await exchange(http, `${base}${path}`, init);
    if (r.kind === "network") return { ok: false, failure: NETWORK };
    if (r.kind === "gateway") return { ok: false, failure: { kind: "gateway", status: r.status } };
    if (r.kind === "home") {
      const e = r.error;
      return { ok: false, failure: { kind: "home", name: e.name, retryable: e.retryable, ...(e.retryAfterSeconds === undefined ? {} : { retryAfterSeconds: e.retryAfterSeconds }) } };
    }
    let value: T | undefined;
    try { value = ok(r.body, r.headers); } catch { value = undefined; }
    // A success that does not look like the operation's answer is not a typed Home answer: the outcome is unknown.
    return value === undefined ? { ok: false, failure: { kind: "gateway", status: r.status } } : { ok: true, value };
  }
  const empty = (): undefined | null => null;
  const done = (r: RecoveryResult<null>): RecoveryResult<undefined> => (r.ok ? { ok: true, value: undefined } : r);

  return {
    baseUrl: base,

    async info() {
      const r = await exchange(http, `${base}/.well-known/om-provider-capabilities`, { method: "GET" });
      if (r.kind !== "ok" || r.body.length > 64 * 1024) return undefined;
      let doc: unknown;
      try { doc = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(r.body)); } catch { return undefined; }
      if (typeof doc !== "object" || doc === null || Array.isArray(doc)) return undefined;
      const d = doc as Record<string, unknown>;
      const features = d["features"];
      if (!Array.isArray(features) || !features.includes(FEATURES.recoveryStore)) return undefined;
      const scope = d["recovery_scope_id"];
      if (typeof scope !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(scope)) return undefined;
      let scopeId: Uint8Array;
      try { scopeId = fromBase64Url(scope); } catch { return undefined; }
      if (scopeId.length !== 32 || toBase64Url(scopeId) !== scope) return undefined;
      const limits = d["limits"];
      return { scopeId, limits: typeof limits === "object" && limits !== null && !Array.isArray(limits) ? (limits as Record<string, unknown>) : {} };
    },

    async putRecord(w) {
      if (!cap32(w.writeCap) || w.locator.length !== 32 || (w.ifMatch !== undefined && w.ifMatch.length !== 32)) return bad();
      const body = encodeRecordPut({ requestId: w.requestId ?? rand16(), envelope: w.envelope });
      const headers: Record<string, string> = w.ifMatch === undefined ? { "if-none-match": "*", ...owner } : { "if-match": `"${toBase64Url(w.ifMatch)}"` };
      return call(`${RECOVERY_PATHS.records}/${toBase64Url(w.locator)}`, { method: "PUT", body, bearer: w.writeCap, headers }, (_b, h) => etagOf(h));
    },

    async getRecord(readCap) {
      if (!cap32(readCap)) return bad();
      const locator = await recoveryLocator(readCap);
      return call(`${RECOVERY_PATHS.records}/${toBase64Url(locator)}`, { method: "GET", bearer: readCap, headers: { [REQUEST_ID_HEADER]: toBase64Url(rand16()) } }, (b, h) => {
        const etag = etagOf(h);
        return etag === undefined ? undefined : { envelope: decodeRecordGetResponse(b), etag };
      });
    },

    async deleteRecord(locator, writeCap) {
      if (!cap32(writeCap) || locator.length !== 32) return bad();
      const r = await call(`${RECOVERY_PATHS.records}/${toBase64Url(locator)}`, { method: "DELETE", bearer: writeCap }, empty);
      if (!r.ok && r.failure.kind === "home" && r.failure.name === "not_found") return { ok: true, value: undefined };
      return done(r);
    },

    async fanOut(signed) {
      const requestId = await idFor("recovery-fanout", signed);
      return done(await call(RECOVERY_PATHS.requests, { method: "POST", body: encodeFanout({ requestId, signed }) }, empty));
    },

    async postVeto(signed) {
      const requestId = await idFor("recovery-veto", signed);
      return done(await call(RECOVERY_PATHS.vetoes, { method: "POST", body: encodeVetoPost({ requestId, signed }) }, empty));
    },

    async readVetoes(requestDigest) {
      if (requestDigest.length !== 32) return bad();
      return call(`${RECOVERY_PATHS.requests}/${toBase64Url(requestDigest)}/vetoes`, { method: "GET" }, (b) => decodeVetoList(b));
    },

    async registerInbox(a) {
      if (!cap32(a.cap) || a.accountId.length !== 32) return bad();
      const inbox = toBase64Url(await recoveryInboxId(a.accountId));
      return done(await call(`${RECOVERY_PATHS.inbox}/${inbox}`, { method: "PUT", body: encodeInboxRegister({ requestId: rand16(), cap: a.cap }), headers: owner }, empty));
    },

    async drainInbox(a, pageSize) {
      if (!cap32(a.cap) || a.accountId.length !== 32) return bad();
      const inbox = toBase64Url(await recoveryInboxId(a.accountId));
      return call(`${RECOVERY_PATHS.inbox}/${inbox}/drain`, { method: "POST", body: encodeInboxDrain({ requestId: rand16(), cap: a.cap, ...(pageSize === undefined ? {} : { pageSize }) }) }, (b) => {
        const r = decodeInboxDrainResponse(b);
        return { items: r.items, more: r.cursor !== undefined };
      });
    },

    async ackInbox(a, itemIds) {
      if (!cap32(a.cap) || a.accountId.length !== 32 || itemIds.length < 1 || itemIds.length > 16) return bad();
      const inbox = toBase64Url(await recoveryInboxId(a.accountId));
      return done(await call(`${RECOVERY_PATHS.inbox}/${inbox}/ack`, { method: "POST", body: encodeInboxAck({ requestId: rand16(), cap: a.cap, itemIds }) }, empty));
    },
  };
}

/**
 * Fan-out of one signed request to every Home of the person's provider snapshot: each is told independently, one Home failing never
 * stops the others, and the per-Home results come back in order. `ok` is true when at least one Home took it (a Home answers alike
 * whether or not it holds an inbox, so "took it" is all a poster can know).
 */
export async function fanOutToAll(clients: readonly RecoveryStoreClient[], signedRequest: Uint8Array): Promise<{ readonly ok: boolean; readonly results: readonly RecoveryResult<undefined>[] }> {
  const results = await Promise.all(clients.map((c) => c.fanOut(signedRequest)));
  return { ok: results.some((r) => r.ok), results };
}

/** Validates a Home address for the store the way the rest of the adapter does (https, or an opted-in loopback). */
export function recoveryStoreFor(baseUrl: string, opts: Omit<RecoveryStoreOptions, "config"> & { readonly allowLoopbackHttp?: boolean }): RecoveryStoreClient | undefined {
  const c = checkProviderUrl(baseUrl, opts.allowLoopbackHttp === undefined ? {} : { allowLoopbackHttp: opts.allowLoopbackHttp });
  return c.ok ? createRecoveryStoreClient({ ...opts, config: c.config }) : undefined;
}
