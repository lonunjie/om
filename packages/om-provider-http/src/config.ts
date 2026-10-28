// SPDX-License-Identifier: MPL-2.0
// Provider configuration: where the Home is. Never a vendor name; any conforming Home at an https URL works.

export interface ProviderConfig {
  /** Base URL of the Home, without a trailing slash. */
  readonly baseUrl: string;
}

const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

/**
 * Validates a user- or build-supplied Home URL. https only; plain http is accepted only for a loopback host and only when
 * the caller opts in (local development and the local end-to-end test). No credentials, query or fragment: a bearer
 * capability must never ride in a URL (home-api.md section 3.1), so a URL that could carry one is refused outright.
 */
export function parseProviderUrl(input: string, opts: { readonly allowLoopbackHttp?: boolean } = {}): ProviderConfig | undefined {
  let u: URL;
  try { u = new URL(input.trim()); } catch { return undefined; }
  if (u.username !== "" || u.password !== "" || u.search !== "" || u.hash !== "") return undefined;
  const secure = u.protocol === "https:";
  const loopbackHttp = u.protocol === "http:" && opts.allowLoopbackHttp === true && LOOPBACK.has(u.hostname);
  if (!secure && !loopbackHttp) return undefined;
  if (u.hostname === "") return undefined;
  return { baseUrl: (u.origin + u.pathname).replace(/\/+$/, "") };
}
