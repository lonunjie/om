// SPDX-License-Identifier: MPL-2.0
// One HTTP exchange with a Home, classified for Core's outbox: a definite typed answer, or a network failure whose
// outcome is unknown. Bearer capabilities travel only in the Authorization header or the CBOR body; nothing here puts one
// in a URL, sends a cookie, follows a redirect or leaks a referrer (ADR-0141).
import { ADMISSION_HEADER, decodeHomeError, toBase64Url, type HomeErrorBody } from "@om/home-wire";

export type Exchange =
  | { readonly kind: "ok"; readonly status: number; readonly body: Uint8Array }
  | { readonly kind: "home"; readonly error: HomeErrorBody }
  /** The Home answered, but not with a typed error (a CDN or gateway page): the operation's outcome is unknown. */
  | { readonly kind: "gateway"; readonly status: number }
  /** No answer: refused, reset, timed out or offline. */
  | { readonly kind: "network" };

export interface HttpOptions {
  readonly fetch: typeof fetch;
  readonly timeoutMs: number;
  /** The owner's admission secret for THIS Home (feature om.admission.v1); sent only on the operations that create resources. */
  readonly admission?: Uint8Array;
}

/** Operations that create or replace the owner's own resources; nothing a peer does needs the admission secret. */
const needsAdmission = (url: string, method: string): boolean => {
  if (method === "GET") return false;
  const path = new URL(url).pathname;
  return path === "/v1/relay/queues" || (path === "/v1/blobs" && method === "POST") || path === "/v1/intro/tickets" || path.startsWith("/v1/discovery/descriptors/") || path.startsWith("/v1/keys/bundles/");
};

export async function exchange(
  opts: HttpOptions,
  url: string,
  init: { readonly method: "GET" | "POST" | "PUT" | "DELETE"; readonly body?: Uint8Array; readonly bearer?: Uint8Array; /** Extra transport headers (never a capability). */ readonly headers?: Readonly<Record<string, string>> },
): Promise<Exchange> {
  const headers: Record<string, string> = { ...init.headers };
  if (init.body !== undefined) headers["content-type"] = "application/cbor";
  if (init.bearer !== undefined) headers["authorization"] = `Bearer ${toBase64Url(init.bearer)}`;
  if (opts.admission !== undefined && needsAdmission(url, init.method)) headers[ADMISSION_HEADER] = toBase64Url(opts.admission);
  let res: Response;
  try {
    res = await opts.fetch(url, {
      method: init.method, headers,
      ...(init.body === undefined ? {} : { body: init.body as Uint8Array<ArrayBuffer> }),
      credentials: "omit", cache: "no-store", redirect: "error", referrerPolicy: "no-referrer", mode: "cors",
      signal: AbortSignal.timeout(opts.timeoutMs),
    });
  } catch {
    return { kind: "network" };
  }
  let body: Uint8Array;
  try { body = new Uint8Array(await res.arrayBuffer()); } catch { return { kind: "network" }; }
  if (res.ok) return { kind: "ok", status: res.status, body };
  if ((res.headers.get("content-type") ?? "").startsWith("application/cbor")) {
    try { return { kind: "home", error: decodeHomeError(body) }; } catch { /* fall through: a malformed error body is not a typed answer */ }
  }
  return { kind: "gateway", status: res.status };
}
