// SPDX-License-Identifier: MPL-2.0
// spec/registries/om-home-errors.json, v1. Codes are permanent; the retryable flag mirrors the registry and is checked
// against the committed JSON by errors.test.ts. Unknown codes are non-retryable protocol failures.
import { decode, encode, map, type CborMap, type CborValue } from "./cbor";
import { CborError } from "./cbor";
import { API_MAJOR } from "./version";
import { Obj } from "./fields";

export const HOME_ERROR_CODES = {
  invalid_request: 1,
  unsupported_version: 2,
  unauthorized: 3,
  revoked: 4,
  expired: 5,
  not_found: 6,
  stale_generation: 7,
  invalid_signature: 8,
  quota: 9,
  rate_limited: 10,
  retryable: 11,
  moved: 12,
  payload_too_large: 13,
  policy_denied: 14,
  admission_denied: 15,
  spent_token: 16,
} as const;

export type HomeErrorName = keyof typeof HOME_ERROR_CODES;

const RETRYABLE: ReadonlySet<HomeErrorName> = new Set<HomeErrorName>(["rate_limited", "retryable"]);
const NAME_OF = new Map<number, HomeErrorName>((Object.entries(HOME_ERROR_CODES) as [HomeErrorName, number][]).map(([n, c]) => [c, n]));

export const isRetryable = (name: HomeErrorName): boolean => RETRYABLE.has(name);

/** HTTP status for the reference binding. The status is a hint; the typed body is authoritative. */
export const HTTP_STATUS: Readonly<Record<HomeErrorName, number>> = {
  invalid_request: 400,
  unsupported_version: 400,
  unauthorized: 401,
  revoked: 403,
  expired: 410,
  not_found: 404,
  stale_generation: 409,
  invalid_signature: 422,
  quota: 429,
  rate_limited: 429,
  retryable: 503,
  moved: 410,
  payload_too_large: 413,
  policy_denied: 403,
  admission_denied: 403,
  spent_token: 409,
};

export interface HomeErrorBody {
  readonly name: HomeErrorName | "unknown";
  readonly code: number;
  readonly retryable: boolean;
  readonly hint?: string;
  readonly retryAfterSeconds?: number;
  readonly detail?: Uint8Array;
}

/** Thrown inside the reference Home and by the client when a typed error is received. */
export class HomeError extends Error {
  constructor(
    readonly errorName: HomeErrorName,
    readonly options: { readonly hint?: string; readonly retryAfterSeconds?: number; readonly detail?: Uint8Array } = {},
  ) {
    super(errorName);
    this.name = "HomeError";
  }
}

/** `quota` detail (code-specific, ours): class 1 items, 2 bytes, 3 per-capability rate, 4 storage; `hard` = free-tier hard stop. */
export function quotaDetail(cls: 1 | 2 | 3 | 4, hard: boolean): Uint8Array {
  return encode(map([[0, cls], [1, hard]]));
}

export function encodeHomeError(e: HomeError): Uint8Array {
  const retryable = isRetryable(e.errorName);
  const retryAfter = retryable ? e.options.retryAfterSeconds : undefined; // retry_after_seconds only where the registry permits
  return encode(map([
    [0, API_MAJOR],
    [1, HOME_ERROR_CODES[e.errorName]],
    [2, e.options.hint],
    [3, retryAfter],
    [4, e.options.detail],
  ]));
}

export function decodeHomeError(bytes: Uint8Array): HomeErrorBody {
  let v: CborValue;
  try { v = decode(bytes); } catch (err) { throw err instanceof CborError ? err : new CborError("bad error body"); }
  const o = new Obj(v);
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  const code = o.uint(1);
  const hint = o.optText(2, 1, 256);
  const retryAfter = o.optUint(3);
  const detail = o.optBytes(4, 0, 4096);
  o.done();
  const name = NAME_OF.get(code);
  const body = { name: name ?? "unknown", code, retryable: name !== undefined && isRetryable(name) } as const;
  return { ...body, ...(hint === undefined ? {} : { hint }), ...(retryAfter === undefined || !body.retryable ? {} : { retryAfterSeconds: retryAfter }), ...(detail === undefined ? {} : { detail }) };
}

export type { CborMap };
