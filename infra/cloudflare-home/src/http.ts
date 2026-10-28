// SPDX-License-Identifier: MPL-2.0
// HTTP reference binding helpers (home-api.md section 3.1): bearer capabilities only in the Authorization header or the
// frozen CBOR slot, responses never cacheable, no referrer, no cookies, and no read of any network-address header.
import { HTTP_STATUS, fromBase64Url, type HomeErrorName } from "@om/home-wire";
import { errorResponse, type Reply } from "./util";

export const CBOR = "application/cbor";

const BASE_HEADERS: Readonly<Record<string, string>> = {
  "Cache-Control": "no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Expose-Headers": "Retry-After",
  "Vary": "Origin",
};

export const preflight = (): Response =>
  new Response(null, {
    status: 204,
    headers: { ...BASE_HEADERS, "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS", "Access-Control-Allow-Headers": "authorization, content-type", "Access-Control-Max-Age": "86400" },
  });

export function ok(body: Uint8Array, contentType = CBOR): Response {
  return new Response(body.length === 0 ? null : body, { status: body.length === 0 ? 204 : 200, headers: { ...BASE_HEADERS, "Content-Type": contentType } });
}

export function json(value: unknown): Response {
  return new Response(JSON.stringify(value), { status: 200, headers: { ...BASE_HEADERS, "Content-Type": "application/json", "Cache-Control": "public, max-age=300" } });
}

export function fail(name: HomeErrorName, opts: { retryAfterSeconds?: number; detail?: Uint8Array } = {}): Response {
  const headers: Record<string, string> = { ...BASE_HEADERS, "Content-Type": CBOR };
  if (opts.retryAfterSeconds !== undefined && (name === "rate_limited" || name === "retryable")) headers["Retry-After"] = String(opts.retryAfterSeconds);
  return new Response(errorResponse(name, opts), { status: HTTP_STATUS[name], headers });
}

export function fromReply(r: Reply): Response {
  return r.ok ? ok(r.body) : fail(r.error, { ...(r.retryAfterSeconds === undefined ? {} : { retryAfterSeconds: r.retryAfterSeconds }), ...(r.detail === undefined ? {} : { detail: r.detail }) });
}

/** How much of an oversize body is read and discarded so the client sees the typed error instead of a reset connection. */
const DISCARD_BUDGET = 1024 * 1024;

/** Reads at most `max` bytes; `undefined` means the body is larger than allowed. */
export async function readBody(request: Request, max: number): Promise<Uint8Array | undefined> {
  if (request.body === null) return new Uint8Array(0);
  const reader = request.body.getReader() as ReadableStreamDefaultReader<Uint8Array>;
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > max) {
      // Bounded discard: a few more bytes are read so a well-behaved client receives `payload_too_large`; a hostile one is cut off.
      while (total < DISCARD_BUDGET) {
        const rest = await reader.read();
        if (rest.done) break;
        total += rest.value.length;
      }
      await reader.cancel();
      return undefined;
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let o = 0;
  for (const c of chunks) { out.set(c, o); o += c.length; }
  return out;
}

/** `Authorization: Bearer <base64url capability>`; `undefined` for a missing or malformed header. */
export function bearer(request: Request): Uint8Array | undefined {
  const h = request.headers.get("authorization");
  if (h === null || !h.startsWith("Bearer ")) return undefined;
  try {
    const cap = fromBase64Url(h.slice(7).trim());
    return cap.length >= 16 && cap.length <= 96 ? cap : undefined;
  } catch {
    return undefined;
  }
}
