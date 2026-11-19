// SPDX-License-Identifier: MPL-2.0
// Feature om.recovery.store.v1 (ADR-0201, home-api.md section 8): the bodies and shape checks of the RecoveryStore, the recovery inbox
// and the veto board. A Home applies only the shape checks below to the signed objects it carries: it holds no recovery key, reads no
// account state and never counts a signature as valid (every consumer verifies against its own pinned state).
import { sha256 } from "./bytes";
import { CborError, decode, encode, map, type CborValue } from "./cbor";
import { Obj } from "./fields";
import { API_MAJOR } from "./version";

/** Advertised in `ProviderCapabilities.features`, only where the store is enabled. */
export const RECOVERY_FEATURE = "om.recovery.store.v1";

/** The three object bounds of home-api.md 8.1 and 8.4 (wire bounds; the operational limits live in the Home). */
export const RECOVERY_RECORD_MAX_BYTES = 1_048_576;
export const RECOVERY_SIGNED_OBJECT_MAX_BYTES = 16_384;
/** Capabilities of the store (record read and write, inbox) are exactly 256 bits. */
export const RECOVERY_CAP_BYTES = 32;
/** The reference path prefixes, kept together so a rename is one edit. */
export const RECOVERY_PATHS = { records: "/v1/recovery/records", requests: "/v1/recovery/requests", vetoes: "/v1/recovery/vetoes", inbox: "/v1/recovery/inbox" } as const;
export const REQUEST_ID_HEADER = "om-request-id";

const ID = 16;
const LABEL_LOCATOR = "OM-RECOVERY-RECORD-LOCATOR-v1";
const LABEL_INBOX = "OM-RECOVERY-INBOX-ID-v2";
const LABEL_REQUEST = "OM-RECOVERY-REQUEST-HASH-v1";

function open(bytes: Uint8Array): Obj {
  const o = new Obj(decode(bytes));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  return o;
}

// ------------------------------------------------------------------------------------------------ derived identifiers

/** `DS-HASH(label, body)` digest value: SHA-256 over the canonical CBOR array `[label, body]` (om-domain-separation.json). */
export const dsHashValue = async (label: string, body: Uint8Array): Promise<Uint8Array> => sha256(encode([label, body]));

/** `opaque_locator = DS-HASH(OM-RECOVERY-RECORD-LOCATOR-v1, read_cap)` (32 bytes). */
export const recoveryLocator = (readCap: Uint8Array): Promise<Uint8Array> => dsHashValue(LABEL_LOCATOR, readCap);
/** ADR-0226 §1. The Home API still carries a 32-byte opaque inbox identifier. */
export const recoveryInboxId = (accountId: Uint8Array, recoveryEpoch: number): Promise<Uint8Array> =>
  dsHashValue(LABEL_INBOX, encode([accountId, recoveryEpoch]));
/** `OM-RECOVERY-REQUEST-HASH-v1` of the RecoveryRequest body: what a RecoveryVeto names. */
export const recoveryRequestDigest = (requestBody: Uint8Array): Promise<Uint8Array> => dsHashValue(LABEL_REQUEST, requestBody);
/** The ETag of a stored envelope: SHA-256 of its bytes (the value `If-Match` takes, base64url, quoted on the wire). */
export const recoveryEtag = (envelope: Uint8Array): Promise<Uint8Array> => sha256(envelope);
export const sha256Of = (b: Uint8Array): Promise<Uint8Array> => sha256(b);

// ------------------------------------------------------------------------------------------------ record bodies (frozen CDDL)

export interface RecoveryRecordPutRequest { readonly requestId: Uint8Array; readonly envelope: Uint8Array }
export const encodeRecordPut = (r: RecoveryRecordPutRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.envelope]]));
export function decodeRecordPut(b: Uint8Array): RecoveryRecordPutRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const envelope = o.bytes(2, 1, RECOVERY_RECORD_MAX_BYTES);
  o.done();
  return { requestId, envelope };
}

export const encodeRecordGetResponse = (envelope: Uint8Array) => encode(map([[0, API_MAJOR], [1, envelope]]));
export function decodeRecordGetResponse(b: Uint8Array): Uint8Array {
  const o = open(b);
  const envelope = o.bytes(1, 1, RECOVERY_RECORD_MAX_BYTES);
  o.done();
  return envelope;
}

/**
 * Shape of a RecoveryRecordEnvelope (object type 35, recovery-record.cddl): type, version, profile id, 12-byte nonce, ciphertext of
 * 16,400 to 1,048,016 bytes, optional extensions. Nothing is opened; the Home cannot.
 */
export function checkRecordEnvelope(b: Uint8Array): void {
  if (b.length > RECOVERY_RECORD_MAX_BYTES) throw new CborError("envelope too large");
  const o = new Obj(decode(b));
  if (o.uint(0) !== 35 || o.uint(1) !== 1) throw new CborError("not a RecoveryRecordEnvelope");
  const profile = o.uint(2);
  if (profile < 1 || profile > 65535) throw new CborError("profile id");
  o.bytes(3, 12, 12);
  o.bytes(4, 16_400, 1_048_016);
  o.optExtensions();
  o.done();
}

// ------------------------------------------------------------------------------------------------ signed objects (shape only)

function signedParts(bytes: Uint8Array): Uint8Array {
  if (bytes.length < 1 || bytes.length > RECOVERY_SIGNED_OBJECT_MAX_BYTES) throw new CborError("signed object size");
  const o = new Obj(decode(bytes));
  const body = o.bytes(1, 1, RECOVERY_SIGNED_OBJECT_MAX_BYTES);
  const sigs = o.mapList(2, 16);
  if (sigs.length < 1) throw new CborError("no signatures");
  o.done();
  return body;
}

export interface RecoveryRequestShape { readonly accountId: Uint8Array; readonly recoveryEpoch: number; readonly body: Uint8Array }
/** `signed-RecoveryRequest` outer envelope and the type, version, sizes and account_id of its body (account.cddl). Never a signature. */
export function checkSignedRequest(bytes: Uint8Array): RecoveryRequestShape {
  const body = signedParts(bytes);
  const o = new Obj(decode(body));
  if (o.uint(0) !== 4 || o.uint(1) !== 1) throw new CborError("not a RecoveryRequest");
  const accountId = o.bytes(2, 32, 32);
  const recoveryEpoch = o.uint(3);
  o.bytes(4, 16, 16);
  o.digest(5);
  o.digest(6);
  o.bytes(7, 16, 16);
  o.optExtensions();
  o.done();
  return { accountId, recoveryEpoch, body };
}

export interface RecoveryVetoShape { readonly accountId: Uint8Array; readonly recoveryEpoch: number; readonly requestDigest: Uint8Array; readonly body: Uint8Array }
export function checkSignedVeto(bytes: Uint8Array): RecoveryVetoShape {
  const body = signedParts(bytes);
  const o = new Obj(decode(body));
  if (o.uint(0) !== 5 || o.uint(1) !== 1) throw new CborError("not a RecoveryVeto");
  const accountId = o.bytes(2, 32, 32);
  const recoveryEpoch = o.uint(3);
  const requestDigest = o.digest(4);
  o.optExtensions();
  o.done();
  return { accountId, recoveryEpoch, requestDigest, body };
}

export interface RecoveryRequestFanout { readonly requestId: Uint8Array; readonly signed: Uint8Array }
export const encodeFanout = (r: RecoveryRequestFanout) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.signed]]));
export function decodeFanout(b: Uint8Array): RecoveryRequestFanout {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const signed = o.bytes(2, 1, RECOVERY_SIGNED_OBJECT_MAX_BYTES);
  o.done();
  return { requestId, signed };
}
export interface RecoveryVetoPostRequest { readonly requestId: Uint8Array; readonly signed: Uint8Array }
export const encodeVetoPost = (r: RecoveryVetoPostRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.signed]]));
export const decodeVetoPost = decodeFanout;

// ------------------------------------------------------------------------------------------------ inbox bodies (home-api.cddl additions)

export interface InboxRegisterRequest { readonly requestId: Uint8Array; readonly cap: Uint8Array }
export const encodeInboxRegister = (r: InboxRegisterRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap]]));
export function decodeInboxRegister(b: Uint8Array): InboxRegisterRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, 16, 96);
  o.done();
  return { requestId, cap };
}

export const INBOX_DRAIN_PAGE_MAX = 16;
export interface InboxDrainRequest { readonly requestId: Uint8Array; readonly cap: Uint8Array; readonly pageSize?: number }
export const encodeInboxDrain = (r: InboxDrainRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, r.pageSize]]));
export function decodeInboxDrain(b: Uint8Array): InboxDrainRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, 16, 96);
  const pageSize = o.optUint(3);
  if (pageSize !== undefined && (pageSize < 1 || pageSize > INBOX_DRAIN_PAGE_MAX)) throw new CborError("page size");
  o.done();
  return { requestId, cap, ...(pageSize === undefined ? {} : { pageSize }) };
}

/** `kind`: 1 a signed RecoveryRequest, 2 a signed RecoveryVeto. No timestamp, ever (a Home is not a time authority). */
export interface InboxItem { readonly id: Uint8Array; readonly kind: 1 | 2; readonly bytes: Uint8Array }
export interface InboxDrainResponse { readonly items: readonly InboxItem[]; readonly cursor?: Uint8Array }
export function encodeInboxDrainResponse(r: InboxDrainResponse) {
  const items: CborValue[] = r.items.map((i) => map([[0, i.id], [1, i.kind], [2, i.bytes]]));
  return encode(map([[0, API_MAJOR], [1, items], [2, r.cursor]]));
}
export function decodeInboxDrainResponse(b: Uint8Array): InboxDrainResponse {
  const o = open(b);
  const items = o.mapList(1, INBOX_DRAIN_PAGE_MAX).map((m): InboxItem => {
    const id = m.bytes(0, ID, ID);
    const kind = m.uint(1);
    if (kind !== 1 && kind !== 2) throw new CborError("item kind");
    const bytes = m.bytes(2, 1, RECOVERY_SIGNED_OBJECT_MAX_BYTES);
    m.done();
    return { id, kind, bytes };
  });
  const cursor = o.optBytes(2, 1, 64);
  o.done();
  return { items, ...(cursor === undefined ? {} : { cursor }) };
}

export interface InboxAckRequest { readonly requestId: Uint8Array; readonly cap: Uint8Array; readonly itemIds: readonly Uint8Array[] }
export const encodeInboxAck = (r: InboxAckRequest) => encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, [...r.itemIds]]]));
export function decodeInboxAck(b: Uint8Array): InboxAckRequest {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, 16, 96);
  const itemIds = o.byteList(3, 1, INBOX_DRAIN_PAGE_MAX, ID, ID);
  o.done();
  return { requestId, cap, itemIds };
}

export const encodeVetoList = (vetoes: readonly Uint8Array[]) => encode(map([[0, API_MAJOR], [1, [...vetoes]]]));
export const RECOVERY_VETO_LIST_MAX = 24;
export function decodeVetoList(b: Uint8Array): Uint8Array[] {
  const o = open(b);
  const vetoes = o.byteList(1, 0, RECOVERY_VETO_LIST_MAX, 1, RECOVERY_SIGNED_OBJECT_MAX_BYTES);
  o.done();
  return vetoes;
}
