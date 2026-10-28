// SPDX-License-Identifier: MPL-2.0
// Provider configuration: where the Home is. Never a vendor name; any conforming Home at an https URL works.

export interface ProviderConfig {
  /** Base URL of the Home, without a trailing slash. */
  readonly baseUrl: string;
}

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

export type ProviderUrlProblem = "malformed" | "not_https" | "credentials" | "query_or_fragment";

/**
 * Validates a user- or build-supplied Home URL. https only; plain http is accepted only for a loopback host and only when
 * the caller opts in (local development and the local end-to-end test). No credentials, query or fragment: a bearer
 * capability must never ride in a URL (home-api.md section 3.1), so a URL that could carry one is refused outright.
 * The reason is for the Settings screen; nothing else may branch on it.
 */
export function checkProviderUrl(input: string, opts: { readonly allowLoopbackHttp?: boolean } = {}): { readonly ok: true; readonly config: ProviderConfig } | { readonly ok: false; readonly reason: ProviderUrlProblem } {
  let u: URL;
  try { u = new URL(input.trim()); } catch { return { ok: false, reason: "malformed" }; }
  if (u.protocol !== "https:" && u.protocol !== "http:") return { ok: false, reason: "malformed" };
  if (u.username !== "" || u.password !== "") return { ok: false, reason: "credentials" };
  if (u.search !== "" || u.hash !== "") return { ok: false, reason: "query_or_fragment" };
  const loopbackHttp = u.protocol === "http:" && opts.allowLoopbackHttp === true && LOOPBACK.has(u.hostname);
  if (u.protocol !== "https:" && !loopbackHttp) return { ok: false, reason: "not_https" };
  if (u.hostname === "") return { ok: false, reason: "malformed" };
  return { ok: true, config: { baseUrl: (u.origin + u.pathname).replace(/\/+$/, "") } };
}

export function parseProviderUrl(input: string, opts: { readonly allowLoopbackHttp?: boolean } = {}): ProviderConfig | undefined {
  const r = checkProviderUrl(input, opts);
  return r.ok ? r.config : undefined;
}
