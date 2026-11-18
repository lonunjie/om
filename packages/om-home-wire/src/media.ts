// SPDX-License-Identifier: MPL-2.0
import { API_MAJOR } from "./version";
import { CborError, decode, encode, map } from "./cbor";
import { Obj } from "./fields";

export const MEDIA_FEATURE = "om.media.turn.v1";
export const MEDIA_TURN_PATH = "/v1/media/turn";
export const MEDIA_MAX_LEASE_SECONDS = 600;
export const MEDIA_MAX_REPLY_BYTES = 4096;

export interface MediaCapabilities {
  readonly stun_urls: readonly string[];
}

export interface TurnLease {
  readonly urls: readonly string[];
  readonly username: string;
  readonly credential: string;
  readonly expires_at: number;
}

const fields = (value: unknown): Record<string, unknown> | undefined =>
  typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : undefined;

function urls(value: unknown, scheme: "stun" | "turn"): string[] | undefined {
  if (!Array.isArray(value) || value.length > 8) return undefined;
  const pattern = scheme === "stun"
    ? /^stuns?:(?:[a-z0-9.-]+|\[[0-9a-f:]+\])(?::[0-9]{1,5})?$/i
    : /^turns?:(?:[a-z0-9.-]+|\[[0-9a-f:]+\])(?::[0-9]{1,5})?(?:\?transport=(?:udp|tcp))?$/i;
  const result: string[] = [];
  for (const url of value as unknown[]) {
    if (typeof url !== "string" || url.length > 256 || !pattern.test(url)) return undefined;
    result.push(url);
  }
  return result;
}

export function parseMediaCapabilities(value: unknown): MediaCapabilities | undefined {
  const stun = urls(fields(value)?.["stun_urls"], "stun");
  return stun === undefined ? undefined : { stun_urls: stun };
}

export function parseTurnLease(value: unknown, nowSeconds: number): TurnLease | undefined {
  const doc = fields(value);
  const servers = urls(doc?.["urls"], "turn");
  const username = doc?.["username"];
  const credential = doc?.["credential"];
  const expires = doc?.["expires_at"];
  if (!servers?.length || typeof username !== "string" || typeof credential !== "string" ||
      !/^[!-~]{1,256}$/.test(username) || !/^[!-~]{1,256}$/.test(credential) ||
      typeof expires !== "number" || !Number.isSafeInteger(expires) || expires <= nowSeconds || expires > nowSeconds + MEDIA_MAX_LEASE_SECONDS) return undefined;
  return { urls: servers, username, credential, expires_at: expires };
}

export const encodeTurnRequest = (id: Uint8Array): Uint8Array => encode(map([[0, API_MAJOR], [1, id]]));

export function decodeTurnRequest(bytes: Uint8Array): Uint8Array {
  const body = new Obj(decode(bytes));
  if (body.uint(0) !== API_MAJOR) throw new CborError("unsupported media version");
  const id = body.bytes(1, 16, 16);
  body.done();
  return id;
}

export const encodeTurnLease = (lease: TurnLease): Uint8Array => encode(map([
  [0, API_MAJOR], [1, [...lease.urls]], [2, lease.username], [3, lease.credential], [4, lease.expires_at],
]));

export function decodeTurnLease(bytes: Uint8Array, nowSeconds: number): TurnLease | undefined {
  const body = new Obj(decode(bytes));
  if (body.uint(0) !== API_MAJOR) return undefined;
  const lease = parseTurnLease({
    urls: body.optTextList(1, 1, 8, 1, 256),
    username: body.text(2, 1, 256),
    credential: body.text(3, 1, 256),
    expires_at: body.uint(4),
  }, nowSeconds);
  body.done();
  return lease;
}
