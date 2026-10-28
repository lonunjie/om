// SPDX-License-Identifier: MPL-2.0
// Raw HTTP client for the conformance tests. It speaks the wire directly (not through @om/provider-http), so the Home is
// tested independently of the reference adapter.
import {
  concat, decodeDepositCapabilityRecord, decodeDepositReceipt, decodeDrainResponse, decodeHomeError, decodeRotateResponse, encode, encodeAckRequest, encodeDepositRequest, encodeDrainRequest,
  encodeMintRequest, encodeQueueCreate, encodeRotateRequest, map, toBase64Url, type HomeErrorBody, type RelayDrainResponse,
} from "@om/home-wire";
import type { Home } from "./harness";

export const rand = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n));
export const nowS = (): number => Math.floor(Date.now() / 1000);

export interface Caps { prefix: Uint8Array; receive: Uint8Array; deposit: Uint8Array; admin: Uint8Array }
export const newCaps = (): Caps => {
  const prefix = rand(16);
  return { prefix, receive: concat(prefix, rand(32)), deposit: concat(prefix, rand(32)), admin: concat(prefix, rand(32)) };
};

/** A structurally valid profile-1 SealedDeliveryEnvelope (random content; nothing can open it). */
export function sealed(opts: { expiresAt?: number; keyId?: Uint8Array; deliveryId?: Uint8Array; ct?: number } = {}): Uint8Array {
  return encode(map([
    [0, 23], [1, 1], [2, 1], [3, opts.keyId ?? rand(16)], [4, opts.deliveryId ?? rand(16)], [5, rand(1120)], [6, rand(opts.ct ?? 1040)], [7, opts.expiresAt ?? nowS() + 3600],
  ]));
}

/** A structurally valid MessageRequest (object type 19). */
export function messageRequest(ticketId: Uint8Array, ct = 600): Uint8Array {
  return encode(map([[0, 19], [1, 1], [2, ticketId], [3, map([[0, 1], [1, rand(32)]])], [4, 1], [5, rand(1120)], [6, rand(ct)]]));
}

export type Result<T> = { ok: true; value: T; status: number; headers: Headers } | { ok: false; error: HomeErrorBody; status: number; headers: Headers };

async function call<T>(home: Home, path: string, init: RequestInit, decodeOk: (b: Uint8Array) => T): Promise<Result<T>> {
  const r = await home.fetch(path, init);
  const b = new Uint8Array(await r.arrayBuffer());
  if (r.status >= 200 && r.status < 300) return { ok: true, value: decodeOk(b), status: r.status, headers: r.headers };
  return { ok: false, error: decodeHomeError(b), status: r.status, headers: r.headers };
}

/** `Uint8Array` is a valid fetch body; the DOM typings only know the `ArrayBuffer`-backed form. */
export const asBody = (b: Uint8Array): BodyInit => b as Uint8Array<ArrayBuffer>;

const post = (body: Uint8Array, headers: Record<string, string> = {}): RequestInit => ({ method: "POST", body: asBody(body), headers: { "content-type": "application/cbor", ...headers } });
export const bearer = (cap: Uint8Array): Record<string, string> => ({ authorization: `Bearer ${toBase64Url(cap)}` });

export const relay = {
  create: (home: Home, c: Caps, requestId = rand(16)) => call(home, "/v1/relay/queues", post(encodeQueueCreate({ requestId, receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })), () => undefined),
  deposit: (home: Home, cap: Uint8Array, envelope: Uint8Array, requestId = rand(16)) => call(home, "/v1/relay/deposit", post(encodeDepositRequest({ requestId, cap, envelope })), decodeDepositReceipt),
  drain: (home: Home, cap: Uint8Array, after?: number) => call<RelayDrainResponse>(home, "/v1/relay/drain", post(encodeDrainRequest({ requestId: rand(16), cap, ...(after === undefined ? {} : { after }) })), decodeDrainResponse),
  ack: (home: Home, cap: Uint8Array, ids: Uint8Array[], requestId = rand(16)) => call(home, "/v1/relay/ack", post(encodeAckRequest({ requestId, cap, itemIds: ids })), () => undefined),
  rotate: (home: Home, cap: Uint8Array, requestId = rand(16)) => call(home, "/v1/relay/rotate", post(encodeRotateRequest({ requestId, cap })), decodeRotateResponse),
  mint: (home: Home, admin: Uint8Array, opts: { ttlSeconds?: number; maxItems?: number } = {}, requestId = rand(16)) =>
    call(home, "/v1/relay/deposit-capabilities", post(encodeMintRequest({ requestId, label: new Uint8Array(0), ...opts }), bearer(admin)), decodeDepositCapabilityRecord),
  revoke: (home: Home, admin: Uint8Array, id: Uint8Array) => call(home, `/v1/relay/deposit-capabilities/${Array.from(id, (x) => x.toString(16).padStart(2, "0")).join("")}`, { method: "DELETE", headers: bearer(admin) }, () => undefined),
};

/** Drains every page after `after` and returns all items in order. */
export async function drainAll(home: Home, cap: Uint8Array, after = 0): Promise<{ seq: number; id: Uint8Array; bytes: Uint8Array }[]> {
  const out: { seq: number; id: Uint8Array; bytes: Uint8Array }[] = [];
  let cursor = after;
  for (let i = 0; i < 100; i++) {
    const r = await relay.drain(home, cap, cursor);
    if (!r.ok) throw new Error(`drain failed: ${r.error.name}`);
    for (const it of r.value.items) out.push({ seq: it.seq ?? 0, id: it.id, bytes: it.bytes });
    if (r.value.next === undefined) return out;
    cursor = Number(new DataView(r.value.next.buffer, r.value.next.byteOffset, 8).getBigUint64(0));
  }
  throw new Error("drain did not terminate");
}

/** The typed error of a failed result; throws if the call unexpectedly succeeded. */
export function errorOf<T>(r: Result<T>): HomeErrorBody {
  if (r.ok) throw new Error("expected a typed error, got success");
  return r.error;
}
