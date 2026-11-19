// SPDX-License-Identifier: AGPL-3.0-or-later
// Builders for the RecoveryStore tests: structurally valid (never cryptographically valid) envelopes and signed objects. The Home
// checks shape only, so random bytes stand in for ciphertext and signatures: a Home that cared about either would fail these tests.
import { encode, map, recoveryLocator, recoveryRequestDigest, sha256, toBase64Url } from "@om/home-wire";

export const rand = (n: number): Uint8Array => {
  const out = new Uint8Array(n);
  for (let i = 0; i < n; i += 65536) crypto.getRandomValues(out.subarray(i, Math.min(n, i + 65536)));
  return out;
};

/** A RecoveryRecordEnvelope (type 35) whose ciphertext has `ct` bytes (16,400 is the smallest bucket plus tag). */
export const envelope = (ct = 16_400, extra: Record<number, number | Uint8Array> = {}): Uint8Array =>
  encode(map([[0, 35], [1, 1], [2, 1], [3, rand(12)], [4, rand(ct)], ...Object.entries(extra).map(([k, v]): [number, number | Uint8Array] => [Number(k), v])]));

/**
 * The largest conforming envelope: the CDDL bounds the ciphertext at 1,048,016 bytes (the 1,048,000-byte padded plaintext bucket plus
 * the 16-byte tag), so it encodes to a little over 1,048,000 bytes, inside the 1,048,576-byte bound that is checked first.
 */
export const maxEnvelope = (): Uint8Array => envelope(1_048_016);

const digestOf = (b: Uint8Array) => map([[0, 1], [1, b]]);

export interface Req { readonly signed: Uint8Array; readonly body: Uint8Array; readonly accountId: Uint8Array; readonly recoveryEpoch: number }
/** A signed-RecoveryRequest (type 4) for `accountId`; every call is a different request (fresh nonce). */
export function signedRequest(accountId: Uint8Array = rand(32), recoveryEpoch = 1): Req {
  const body = encode(map([[0, 4], [1, 1], [2, accountId], [3, recoveryEpoch], [4, rand(16)], [5, digestOf(rand(32))], [6, digestOf(rand(32))], [7, rand(16)]]));
  const signed = encode(map([[1, body], [2, [map([[0, 1], [1, rand(64)]])]]]));
  return { signed, body, accountId, recoveryEpoch };
}

/** A signed-RecoveryVeto (type 5) naming `requestDigest`; the test extension makes distinct bodies within one epoch. */
export function signedVeto(accountId: Uint8Array, requestDigest: Uint8Array, n = Math.floor(Math.random() * 1e9), recoveryEpoch = 1): Uint8Array {
  const body = encode(map([[0, 5], [1, 1], [2, accountId], [3, recoveryEpoch], [4, digestOf(requestDigest)], [23, new Map([["test.veto", map([[0, false], [1, n]])]])]]));
  return encode(map([[1, body], [2, [map([[0, 1], [1, rand(64)]])]]]));
}

export const requestDigestOf = (r: Req): Promise<Uint8Array> => recoveryRequestDigest(r.body);
export { recoveryLocator, sha256, toBase64Url };

/** A fresh pair of record capabilities and the locator they give. */
export async function recordCaps(): Promise<{ readCap: Uint8Array; writeCap: Uint8Array; locator: Uint8Array }> {
  const readCap = rand(32);
  return { readCap, writeCap: rand(32), locator: await recoveryLocator(readCap) };
}
