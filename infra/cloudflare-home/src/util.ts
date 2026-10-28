// SPDX-License-Identifier: MPL-2.0
import { HomeError, encodeHomeError, HTTP_STATUS, sha256, toHex, type HomeErrorName } from "@om/home-wire";

export type Reply =
  | { readonly ok: true; readonly body: Uint8Array }
  | { readonly ok: false; readonly error: HomeErrorName; readonly retryAfterSeconds?: number; readonly detail?: Uint8Array };

export const nowSeconds = (): number => Math.floor(Date.now() / 1000);

export function randomBytes(n: number): Uint8Array<ArrayBuffer> {
  const b = new Uint8Array(n);
  crypto.getRandomValues(b);
  return b;
}

/** Hex SHA-256 of a capability: all a Home stores of one. Equal hashes are compared as plain strings (the hash is not secret-dependent timing). */
export async function capHash(cap: Uint8Array): Promise<string> {
  return toHex(await sha256(cap));
}

export async function fingerprint(...parts: readonly Uint8Array[]): Promise<string> {
  return toHex(await sha256(...parts));
}

/** Runs an operation and turns a `HomeError` into a typed reply. Anything else is a bug and propagates (the Worker answers `retryable`). */
export async function reply(fn: () => Promise<Uint8Array>): Promise<Reply> {
  try {
    return { ok: true, body: await fn() };
  } catch (e) {
    if (e instanceof HomeError) {
      return {
        ok: false, error: e.errorName,
        ...(e.options.retryAfterSeconds === undefined ? {} : { retryAfterSeconds: e.options.retryAfterSeconds }),
        ...(e.options.detail === undefined ? {} : { detail: e.options.detail }),
      };
    }
    throw e;
  }
}

export function errorResponse(name: HomeErrorName, opts: { retryAfterSeconds?: number; detail?: Uint8Array } = {}): Uint8Array {
  return encodeHomeError(new HomeError(name, opts));
}

export const statusOf = (name: HomeErrorName): number => HTTP_STATUS[name];

/** Token bucket stored as two numbers: returns the wait in seconds (0 = allowed) and the new state. */
export function takeToken(tokens: number, at: number, now: number, burst: number, refillPerSecond: number): { allowed: boolean; tokens: number; waitSeconds: number } {
  const t = Math.min(burst, tokens + Math.max(0, now - at) * refillPerSecond);
  if (t >= 1) return { allowed: true, tokens: t - 1, waitSeconds: 0 };
  return { allowed: false, tokens: t, waitSeconds: Math.max(1, Math.ceil((1 - t) / refillPerSecond)) };
}

export const MAX_SAFE = Number.MAX_SAFE_INTEGER;
