// SPDX-License-Identifier: MPL-2.0
// Optional feature om.blob.spool.v1: the bounded encrypted blob spool of a Home (STORAGE_AND_BLOB_ARCHITECTURE "Zero-payment
// personal bootstrap", CLOUDFLARE_REFERENCE_DEPLOYMENT "File/data routing"). The bodies are the frozen Home API ones
// (spec/cddl/home-api.cddl BlobPutRequest / BlobPutResponse); this file only gives their keys a meaning and adds the digest rule.
//
// Reading of the frozen bodies (the spec names the shapes, not the field meanings; recorded in docs/reports/2026-11-04-attachments-home-status.md):
//   BlobPutRequest  {0 api_major, 1 request_id, 2 owner blob_cap, 3 ciphertext, ?4 retention_seconds}
//   BlobPutResponse {0 api_major, 1 read blob_cap minted by the Home, 2 ciphertext digest the Home computed, ?3 expires_at}
// A blob_cap is a 16-byte routing prefix plus at least 16 secret bytes; the prefix is the opaque locator in the URL
// (`GET /v1/blobs/{locator}`), the capability itself only ever travels in the Authorization header or the CBOR slot.
import { equalBytes, fromBase64Url, sha256, toBase64Url, toHex } from "./bytes";
import { CborError, decode, encode, map } from "./cbor";
import { Obj } from "./fields";
import { API_MAJOR } from "./version";

/** Registered label of AttachmentRef.ciphertext_digest (spec/registries/om-domain-separation.json). */
export const BLOB_CIPHERTEXT_LABEL = "OM-BLOB-CIPHERTEXT-HASH-v1";
/** Transport binding of the reference HTTP profile: the expected digest of a put (base64url of 32 bytes), and the stored digest of a get. */
export const BLOB_DIGEST_HEADER = "om-ciphertext-digest";
export const BLOB_ROUTING_BYTES = 16;
const ID = 16;
const CAP_MIN = 32;
const CAP_MAX = 96;
/** The wire never carries more than this; a Home enforces its own, smaller bound (limits.max_blob_bytes). */
export const BLOB_WIRE_MAX = 16 * 1024 * 1024;

/** CBOR head of a byte string of length `n` (major type 2), without allocating `n` bytes. */
function bstrHead(n: number): Uint8Array {
  if (n < 24) return Uint8Array.of(0x40 | n);
  if (n < 0x100) return Uint8Array.of(0x58, n);
  if (n < 0x10000) return Uint8Array.of(0x59, n >> 8, n & 255);
  if (n < 0x100000000) return Uint8Array.of(0x5a, (n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255);
  throw new CborError("blob too large");
}

/**
 * `AttachmentRef.ciphertext_digest` hash bytes: SHA-256 over DS-HASH(label, ciphertext), where DS-HASH is the canonical CBOR
 * array [label, body]. The framing is written out so a multi-megabyte body is hashed without building a CBOR copy of it.
 */
export async function blobCiphertextDigest(ciphertext: Uint8Array): Promise<Uint8Array> {
  return sha256(Uint8Array.of(0x82), encode(BLOB_CIPHERTEXT_LABEL), bstrHead(ciphertext.length), ciphertext);
}

export const encodeBlobDigestHeader = (digest: Uint8Array): string => toBase64Url(digest);
/** The 32-byte digest of a header value, or undefined when it is not exactly that. */
export function parseBlobDigestHeader(value: string | null): Uint8Array | undefined {
  if (value === null) return undefined;
  try { const d = fromBase64Url(value.trim()); return d.length === 32 ? d : undefined; } catch { return undefined; }
}

/** The opaque locator of a blob: lowercase hex of the capability's routing prefix. It names the object and grants nothing. */
export const blobLocator = (cap: Uint8Array): string => toHex(cap.slice(0, BLOB_ROUTING_BYTES));
export const blobLocatorPattern = /^[0-9a-f]{32}$/;

export interface BlobPutRequest { readonly requestId: Uint8Array; readonly cap: Uint8Array; readonly ciphertext: Uint8Array; readonly retentionSeconds?: number }
export const encodeBlobPutRequest = (r: BlobPutRequest) =>
  encode(map([[0, API_MAJOR], [1, r.requestId], [2, r.cap], [3, r.ciphertext], [4, r.retentionSeconds]]));
export function decodeBlobPutRequest(b: Uint8Array): BlobPutRequest {
  const o = new Obj(decode(b));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const ciphertext = o.bytes(3, 1, BLOB_WIRE_MAX);
  const retentionSeconds = o.optUint(4);
  o.done();
  return { requestId, cap, ciphertext, ...(retentionSeconds === undefined ? {} : { retentionSeconds }) };
}

export interface BlobPutResponse { readonly readCap: Uint8Array; readonly digest: Uint8Array; readonly expiresAt?: number }
export const encodeBlobPutResponse = (r: BlobPutResponse) => encode(map([[0, API_MAJOR], [1, r.readCap], [2, r.digest], [3, r.expiresAt]]));
export function decodeBlobPutResponse(b: Uint8Array): BlobPutResponse {
  const o = new Obj(decode(b));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  const readCap = o.bytes(1, CAP_MIN, CAP_MAX);
  const digest = o.bytes(2, 32, 32);
  const expiresAt = o.optUint(3);
  o.done();
  return { readCap, digest, ...(expiresAt === undefined ? {} : { expiresAt }) };
}

/** A fresh capability for a new blob: routing prefix plus 32 CSPRNG bytes (home-api.md section 3.2: at least 256 bits of secret). */
export function newBlobCap(): Uint8Array {
  const b = new Uint8Array(BLOB_ROUTING_BYTES + 32);
  crypto.getRandomValues(b);
  return b;
}

/** True when `digest` is the digest of `ciphertext`; the client's check of a put response and of a fetched blob. */
export async function blobMatches(ciphertext: Uint8Array, digest: Uint8Array): Promise<boolean> {
  return equalBytes(await blobCiphertextDigest(ciphertext), digest);
}
