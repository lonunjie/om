// SPDX-License-Identifier: MPL-2.0
import { decode, type CborValue } from "./cbor";
import { Obj } from "./fields";

export interface BundleFacts { readonly deviceId: Uint8Array; readonly generation: number; readonly expiresAt: number }

/**
 * Best-effort structural read of a signed-AsyncKeyBundle, `{1: bstr .cbor AsyncKeyBundle, 2: [signatures]}`
 * (spec/cddl/keys.cddl). It verifies nothing: a Home may use the device id, generation and expiry only as availability
 * aids; clients decide validity and freshness (home-api.md section 3.3).
 */
export function readBundleFacts(bundle: Uint8Array): BundleFacts | undefined {
  try {
    const outer = new Obj(decode(bundle));
    const body = outer.bytes(1, 1, 65536);
    const inner = decode(body);
    if (!(inner instanceof Map)) return undefined;
    const m = inner as ReadonlyMap<number | string, CborValue>;
    const [type, deviceId, generation, expiresAt] = [m.get(0), m.get(2), m.get(6), m.get(13)];
    if (type !== 9 || !(deviceId instanceof Uint8Array) || typeof generation !== "number" || typeof expiresAt !== "number") return undefined;
    return { deviceId, generation, expiresAt };
  } catch {
    return undefined;
  }
}
