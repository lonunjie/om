// SPDX-License-Identifier: MPL-2.0
// Semantic bodies of the OM Home API v1 (spec/cddl/home-api.cddl), plus the additive bodies of the optional features the
// reference Home advertises (docs: see FEATURES). Every decoder checks the CDDL size bounds and rejects unknown keys.
import { CborError, decode, encode, map, type CborValue } from "./cbor";
import { Obj } from "./fields";
import { API_MAJOR } from "./version";

const ID = 16;
const CAP_MIN = 16;
const CAP_MAX = 96;

function open(bytes: Uint8Array): Obj {
  const o = new Obj(decode(bytes));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  return o;
}

export type Id16 = Uint8Array;
export type Cap = Uint8Array;

// ------------------------------------------------------------------------------------------------ relay (normative)

export interface RelayDepositRequest { readonly requestId: Id16; readonly cap: Cap; readonly envelope: Uint8Array; readonly expiresAt?: number }
export const encodeDepositRequest = (r: RelayDepositRequest) =>
  encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, r.envelope], [4, r.expiresAt]]));
export function decodeDepositRequest(b: Uint8Array): RelayDepositRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const envelope = o.bytes(3, 1, 262144);
  const expiresAt = o.optUint(4);
  o.done();
  return { requestId, cap, envelope, ...(expiresAt === undefined ? {} : { expiresAt }) };
}

export interface RelayDepositReceipt { readonly itemId: Id16; readonly seq?: number }
export const encodeDepositReceipt = (r: RelayDepositReceipt) => encode(map([[0, API_MAJOR], [1, r.itemId], [2, r.seq]]));
export function decodeDepositReceipt(b: Uint8Array): RelayDepositReceipt {
  const o = open(b);
  const itemId = o.bytes(1, ID, ID);
  const seq = o.optUint(2);
  o.done();
  return { itemId, ...(seq === undefined ? {} : { seq }) };
}

/** `after` is the highest sequence the caller already holds (items with a greater sequence are returned). */
export interface RelayDrainRequest { readonly requestId: Id16; readonly cap: Cap; readonly after?: number }
export const encodeDrainRequest = (r: RelayDrainRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, r.after]]));
export function decodeDrainRequest(b: Uint8Array): RelayDrainRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const after = o.optUint(3);
  o.done();
  return { requestId, cap, ...(after === undefined ? {} : { after }) };
}

export interface RelayItem { readonly id: Id16; readonly bytes: Uint8Array; readonly seq?: number }
/** `next` is present only when more items remain: an opaque continuation (8-byte big-endian sequence in this Home). */
export interface RelayDrainResponse { readonly items: readonly RelayItem[]; readonly next?: Uint8Array }
export function encodeDrainResponse(r: RelayDrainResponse) {
  const items: CborValue[] = r.items.map((i) => map([[0, i.id], [1, i.bytes], [2, i.seq]]));
  return encode(map([[0, API_MAJOR], [1, items], [2, r.next]]));
}
export function decodeDrainResponse(b: Uint8Array): RelayDrainResponse {
  const o = open(b);
  const items = o.mapList(1, 512).map((m): RelayItem => {
    const id = m.bytes(0, ID, ID);
    const bytes = m.bytes(1, 1, 262144);
    const seq = m.optUint(2);
    m.done();
    return { id, bytes, ...(seq === undefined ? {} : { seq }) };
  });
  const next = o.optBytes(2, 0, 64);
  o.done();
  return { items, ...(next === undefined ? {} : { next }) };
}

export interface RelayAckRequest { readonly requestId: Id16; readonly cap: Cap; readonly itemIds: readonly Id16[] }
export const encodeAckRequest = (r: RelayAckRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, [...r.itemIds]]]));
export function decodeAckRequest(b: Uint8Array): RelayAckRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const itemIds = o.byteList(3, 1, 256, ID, ID);
  o.done();
  return { requestId, cap, itemIds };
}

export interface RelayRotateRequest { readonly requestId: Id16; readonly cap: Cap }
export const encodeRotateRequest = (r: RelayRotateRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap]]));
export function decodeRotateRequest(b: Uint8Array): RelayRotateRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  o.done();
  return { requestId, cap };
}

/** The new receive capability and the new deposit capability; `overlapUntil` is when the old pair stops working. */
export interface RelayRotateResponse { readonly receiveCap: Cap; readonly depositCap: Cap; readonly overlapUntil?: number }
export const encodeRotateResponse = (r: RelayRotateResponse) => encode(map([[0, API_MAJOR], [1, r.receiveCap], [2, r.depositCap], [3, r.overlapUntil]]));
export function decodeRotateResponse(b: Uint8Array): RelayRotateResponse {
  const o = open(b);
  const receiveCap = o.bytes(1, CAP_MIN, CAP_MAX);
  const depositCap = o.bytes(2, CAP_MIN, CAP_MAX);
  const overlapUntil = o.optUint(3);
  o.done();
  return { receiveCap, depositCap, ...(overlapUntil === undefined ? {} : { overlapUntil }) };
}

/** `label` is an opaque client note kept only so the owner can tell its capabilities apart (never interpreted). */
export interface MintDepositCapabilityRequest { readonly requestId: Id16; readonly label: Uint8Array; readonly ttlSeconds?: number; readonly maxItems?: number }
export const encodeMintRequest = (r: MintDepositCapabilityRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.label], [3, r.ttlSeconds], [4, r.maxItems]]));
export function decodeMintRequest(b: Uint8Array): MintDepositCapabilityRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const label = o.bytes(2, 0, 64);
  const ttlSeconds = o.optUint(3);
  const maxItems = o.optUint(4);
  o.done();
  return { requestId, label, ...(ttlSeconds === undefined ? {} : { ttlSeconds }), ...(maxItems === undefined ? {} : { maxItems }) };
}

export interface DepositCapabilityRecord { readonly id: Id16; readonly cap: Cap; readonly expiresAt?: number }
export const encodeDepositCapabilityRecord = (r: DepositCapabilityRecord) => encode(map([[0, API_MAJOR], [1, r.id], [2, r.cap], [3, r.expiresAt]]));
export function decodeDepositCapabilityRecord(b: Uint8Array): DepositCapabilityRecord {
  const o = open(b);
  const id = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const expiresAt = o.optUint(3);
  o.done();
  return { id, cap, ...(expiresAt === undefined ? {} : { expiresAt }) };
}

// --------------------------------------------------------------------------------------- relay (feature om.relay.queues.v1)

/** Create a queue with caller-chosen capabilities (the relay stores only their hashes). Idempotent for identical input. */
export interface QueueCreateRequest { readonly requestId: Id16; readonly receiveCap: Cap; readonly depositCap: Cap; readonly adminCap?: Cap }
export const encodeQueueCreate = (r: QueueCreateRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.receiveCap], [3, r.depositCap], [4, r.adminCap]]));
export function decodeQueueCreate(b: Uint8Array): QueueCreateRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const receiveCap = o.bytes(2, CAP_MIN, CAP_MAX);
  const depositCap = o.bytes(3, CAP_MIN, CAP_MAX);
  const adminCap = o.optBytes(4, CAP_MIN, CAP_MAX);
  o.done();
  return { requestId, receiveCap, depositCap, ...(adminCap === undefined ? {} : { adminCap }) };
}

// ------------------------------------------------------------------------------------------------ keys (normative + ext)

export interface KeyBundleFetchRequest { readonly requestId: Id16; readonly cap: Cap; readonly devices: readonly Uint8Array[] }
export const encodeKeyFetch = (r: KeyBundleFetchRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, [...r.devices]]]));
export function decodeKeyFetch(b: Uint8Array): KeyBundleFetchRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const devices = o.byteList(3, 1, 64, 16, 64);
  o.done();
  return { requestId, cap, devices };
}
export interface KeyBundleFetchResponse { readonly items: readonly Uint8Array[] }
export const encodeKeyFetchResponse = (r: KeyBundleFetchResponse) => encode(map([[0, API_MAJOR], [1, [...r.items]]]));
export function decodeKeyFetchResponse(b: Uint8Array): KeyBundleFetchResponse {
  const o = open(b);
  const items = o.byteList(1, 0, 400, 1, 262144);
  o.done();
  return { items };
}

/**
 * Feature om.keys.pool.v1. The spec's `PUT /v1/keys/bundles/{device}` carries one signed AsyncKeyBundle. Core keeps its
 * one-time prekeys outside the bundle (pass-2 shape), so a publish carries the signed bundle plus the one-time items
 * as separate byte strings. The Home never opens them.
 */
export interface KeyPublishRequest { readonly requestId: Id16; readonly bundle: Uint8Array; readonly opks: readonly Uint8Array[]; readonly pqopks: readonly Uint8Array[] }
export const encodeKeyPublish = (r: KeyPublishRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.bundle], [3, [...r.opks]], [4, [...r.pqopks]]]));
export function decodeKeyPublish(b: Uint8Array): KeyPublishRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const bundle = o.bytes(2, 1, 65536);
  const opks = o.byteList(3, 0, 100, 1, 8192);
  const pqopks = o.byteList(4, 0, 100, 1, 8192);
  o.done();
  return { requestId, bundle, opks, pqopks };
}

// ------------------------------------------------------------------------------------------------ descriptors (ext)

/** Feature om.discovery.put.v1: replace the descriptor body; `expiresAt` (POSIX seconds) bounds retention. */
export interface DescriptorPutRequest { readonly requestId: Id16; readonly body: Uint8Array; readonly expiresAt: number }
export const encodeDescriptorPut = (r: DescriptorPutRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.body], [3, r.expiresAt]]));
export function decodeDescriptorPut(b: Uint8Array): DescriptorPutRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const body = o.bytes(2, 1, 32768);
  const expiresAt = o.uint(3);
  o.done();
  return { requestId, body, expiresAt };
}
export interface DescriptorGetResponse { readonly body: Uint8Array; readonly expiresAt: number }
export const encodeDescriptorGet = (r: DescriptorGetResponse) => encode(map([[0, API_MAJOR], [1, r.body], [2, r.expiresAt]]));
export function decodeDescriptorGet(b: Uint8Array): DescriptorGetResponse {
  const o = open(b);
  const body = o.bytes(1, 1, 32768);
  const expiresAt = o.uint(2);
  o.done();
  return { body, expiresAt };
}
/** Feature om.discovery.delete.v1: an idempotent revoke. */
export interface RequestOnly { readonly requestId: Id16 }
export const encodeRequestOnly = (r: RequestOnly) => encode(map([[0, API_MAJOR], [1, r.requestId]]));
export function decodeRequestOnly(b: Uint8Array): RequestOnly {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  o.done();
  return { requestId };
}

// ------------------------------------------------------------------------------------------------ IntroGate

/**
 * `POST /v1/intro/tickets`. Body slot 2 is a canonical CBOR map of ticket parameters (TicketParams below); key 3 is the
 * number of tickets requested (default 1). The Home chooses `ticket_id` and `nonce` and returns IntroductionTickets.
 */
export interface IntroTicketRequest { readonly requestId: Id16; readonly params: Uint8Array; readonly count?: number }
export const encodeIntroTicketRequest = (r: IntroTicketRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.params], [3, r.count]]));
export function decodeIntroTicketRequest(b: Uint8Array): IntroTicketRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const params = o.bytes(2, 1, 512);
  const count = o.optUint(3);
  o.done();
  return { requestId, params, ...(count === undefined ? {} : { count }) };
}

export interface TicketParams { readonly ttlSeconds: number; readonly maxUses: number; readonly payloadLimitBytes: number; readonly allowAttachments?: boolean; readonly allowCalls?: boolean }
export const encodeTicketParams = (p: TicketParams) => encode(map([[0, p.ttlSeconds], [1, p.maxUses], [2, p.payloadLimitBytes], [3, p.allowAttachments], [4, p.allowCalls]]));
export function decodeTicketParams(b: Uint8Array): TicketParams {
  const o = new Obj(decode(b));
  const ttlSeconds = o.uint(0);
  const maxUses = o.uint(1);
  const payloadLimitBytes = o.uint(2);
  const allowAttachments = o.optBool(3);
  const allowCalls = o.optBool(4);
  o.done();
  return { ttlSeconds, maxUses, payloadLimitBytes, ...(allowAttachments === undefined ? {} : { allowAttachments }), ...(allowCalls === undefined ? {} : { allowCalls }) };
}

/** IntroductionTicket (object type 15, spec/cddl/discovery.cddl). Unsigned here: the Home's record is the authority for admission. */
export interface IntroductionTicket {
  readonly ticketId: Id16; readonly serviceBinding: Uint8Array; readonly expiresAt: number; readonly maxUses: number; readonly payloadLimitBytes: number;
  readonly allowAttachments?: boolean; readonly allowCalls?: boolean; readonly nonce: Id16;
}
export const encodeIntroductionTicket = (t: IntroductionTicket) =>
  encode(map([[0, 15], [1, 1], [2, t.ticketId], [3, t.serviceBinding], [4, t.expiresAt], [5, t.maxUses], [6, t.payloadLimitBytes], [7, t.allowAttachments], [8, t.allowCalls], [9, t.nonce]]));
export function decodeIntroductionTicket(b: Uint8Array): IntroductionTicket {
  const o = new Obj(decode(b));
  if (o.uint(0) !== 15 || o.uint(1) !== 1) throw new CborError("not an IntroductionTicket");
  const ticketId = o.bytes(2, ID, ID);
  const serviceBinding = o.bytes(3, 16, 32);
  const expiresAt = o.uint(4);
  const maxUses = o.uint(5);
  const payloadLimitBytes = o.uint(6);
  const allowAttachments = o.optBool(7);
  const allowCalls = o.optBool(8);
  const nonce = o.bytes(9, ID, ID);
  o.done();
  return { ticketId, serviceBinding, expiresAt, maxUses, payloadLimitBytes, nonce, ...(allowAttachments === undefined ? {} : { allowAttachments }), ...(allowCalls === undefined ? {} : { allowCalls }) };
}
export const encodeTicketList = (tickets: readonly IntroductionTicket[]) =>
  encode(map([[0, API_MAJOR], [1, tickets.map((t) => encodeIntroductionTicket(t))]]));
export function decodeTicketList(b: Uint8Array): readonly IntroductionTicket[] {
  const o = open(b);
  const items = o.byteList(1, 1, 16, 1, 1024);
  o.done();
  return items.map(decodeIntroductionTicket);
}

/** Feature om.intro.v1: `POST /v1/intro/requests` carries the MessageRequest (type 19) bytes as the whole body, no auth header. */
export interface IntroDrainRequest { readonly requestId: Id16; readonly cap: Cap; readonly after?: number }
export const encodeIntroDrainRequest = encodeDrainRequest;
export const decodeIntroDrainRequest = decodeDrainRequest;
