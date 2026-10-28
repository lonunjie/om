// SPDX-License-Identifier: MPL-2.0
// RelayEntry (spec/cddl/delivery.cddl) and the sealed-delivery route binding of SEALED_SENDER_DELIVERY_PROFILE.md section 3:
//   route_binding = SHA-256(OM-CBOR ["OM-SEALED-DELIVERY-ROUTE-v1", relay_uri, deposit_capability])
import { CborError, decode, encode, map } from "./cbor";
import { sha256 } from "./bytes";
import { Obj } from "./fields";

export const ROUTE_LABEL = "OM-SEALED-DELIVERY-ROUTE-v1";

export interface RelayEntry {
  /** Where the relay is: `https://<host>/` for the reference binding (the Home API paths hang off it). */
  readonly relayUri: string;
  /** The deposit capability for exactly this relationship and relay. */
  readonly capability: Uint8Array;
  readonly priority: number;
  readonly weight?: number;
  readonly expiresAt: number;
  readonly transports?: readonly string[];
  readonly failureDomain?: string;
}

export function encodeRelayEntry(e: RelayEntry) {
  return encode(map([
    [0, e.relayUri], [1, e.capability], [2, e.priority], [3, e.weight], [4, e.expiresAt],
    [5, e.transports === undefined ? undefined : [...e.transports].sort()], [6, e.failureDomain],
  ]));
}

export function decodeRelayEntry(b: Uint8Array): RelayEntry {
  const o = new Obj(decode(b));
  const relayUri = o.text(0, 1, 2048);
  const capability = o.bytes(1, 16, 64);
  const priority = o.uint(2);
  if (priority > 255) throw new CborError("priority out of range");
  const weight = o.optUint(3);
  if (weight !== undefined && weight > 65535) throw new CborError("weight out of range");
  const expiresAt = o.uint(4);
  const transports = o.optTextList(5, 1, 8, 1, 64);
  const failureDomain = o.optText(6, 1, 128);
  o.done();
  return {
    relayUri, capability, priority, ...(weight === undefined ? {} : { weight }), expiresAt,
    ...(transports === undefined ? {} : { transports }), ...(failureDomain === undefined ? {} : { failureDomain }),
  };
}

export async function routeBinding(relayUri: string, depositCapability: Uint8Array): Promise<Uint8Array> {
  return sha256(encode([ROUTE_LABEL, relayUri, depositCapability]));
}

export const routeBindingOf = (e: Pick<RelayEntry, "relayUri" | "capability">): Promise<Uint8Array> => routeBinding(e.relayUri, e.capability);
