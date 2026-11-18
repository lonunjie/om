// SPDX-License-Identifier: MPL-2.0
// Optional push wake (om.relay.push.v1, ADR-0210). A Home that offers it lets one device register one browser push endpoint per
// mailbox; when an item arrives for a mailbox nobody is listening on, the Home sends that endpoint an EMPTY push (a timing signal
// only: no body, no content, no id) signed with a key only this Home holds. The encoding is JSON (an implementation choice of the
// reference Home, like the usage read); the capability that authorises it travels in the Authorization header, never in a URL.
import { sha256, toHex } from "./bytes";

export const PUSH_FEATURE = "om.relay.push.v1";
export const PUSH_PATHS = { key: "/v1/push/key", subscription: "/v1/push/subscription" } as const;

/** Bounds a Home applies and advertises under `limits` (the numbers are the reference Home's; another may differ). */
export const PUSH_LIMITS = {
  /** Devices that may be registered for one mailbox. */
  perQueue: 4,
  /** Registrations in the whole Home. */
  perHome: 64,
  /** A registration lapses after this long unless the device registers again (the client does at each start). */
  lifetimeSeconds: 30 * 24 * 3600,
  /** At most one push per registration in this window; arrivals inside it are covered by the first one's notice. */
  minIntervalSeconds: 30,
  /** At most this many pushes per registration per UTC day. */
  perDay: 100,
  /** How long the push service may hold an undelivered wake (the TTL header). */
  ttlSeconds: 3600,
} as const;

/** Where a Home will agree to send. Fixed to the push services of the major browsers; the owner can add exact origins (OM_PUSH_HOSTS). */
const SERVICE_SUFFIXES = ["fcm.googleapis.com", "push.services.mozilla.com", "push.apple.com", "notify.windows.com"] as const;
const MAX_ENDPOINT = 512;

export type PushEndpointCheck = { readonly ok: true } | { readonly ok: false };

/**
 * An endpoint is the one address a Home will send to, so it is the one place a registration could turn a Home into something that
 * calls arbitrary addresses. It must be https on the default port, with no credentials or fragment, a name (not an address), and
 * belong to a known push service, or exactly match an origin or host the owner listed (`extra`: `https://host`, `http://127.0.0.1:4545`, or a bare host).
 */
export function checkPushEndpoint(endpoint: string, extra: readonly string[] = []): PushEndpointCheck {
  const no: PushEndpointCheck = { ok: false };
  if (endpoint.length === 0 || endpoint.length > MAX_ENDPOINT) return no;
  let u: URL;
  try { u = new URL(endpoint); } catch { return no; }
  if (u.username !== "" || u.password !== "" || u.hash !== "" || endpoint.includes("#")) return no;
  for (const e of extra) {
    const x = e.trim();
    if (x === "") continue;
    if (x.includes("://") ? x === u.origin : (u.protocol === "https:" && u.port === "" && u.hostname === x.toLowerCase())) return { ok: true };
  }
  if (u.protocol !== "https:" || u.port !== "") return no;
  const host = u.hostname.toLowerCase();
  if (host.startsWith("[") || /^[0-9.]+$/.test(host)) return no;
  return SERVICE_SUFFIXES.some((s) => host === s || host.endsWith(`.${s}`)) ? { ok: true } : no;
}

/** The body of a registration or its removal: exactly `{"endpoint": "<https address>"}`. */
export function parsePushRequest(text: string): string | undefined {
  let v: unknown;
  try { v = JSON.parse(text); } catch { return undefined; }
  if (typeof v !== "object" || v === null || Array.isArray(v)) return undefined;
  const keys = Object.keys(v);
  const e = (v as Record<string, unknown>)["endpoint"];
  return keys.length === 1 && typeof e === "string" && e.length > 0 ? e : undefined;
}

/** What identifies one registration at a Home and in its answers: 16 bytes of the endpoint's hash, so the Home never has to echo the address. */
export async function pushSubscriptionId(endpoint: string): Promise<string> {
  return toHex((await sha256(new TextEncoder().encode(endpoint))).slice(0, 16));
}
