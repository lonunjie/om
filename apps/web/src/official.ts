// Decides whether the page is a copy of OM that someone else deployed. Decided from the page's own origin against the
// pinned official one; nothing is fetched, and the signed manifest does not depend on it.
const OFFICIAL_HOST = "om.lonunjie.org";
const OFFICIAL_PAGES_PROJECT = "om-lonunjie.pages.dev";
const LOCAL_HOSTS: ReadonlySet<string> = new Set(["localhost", "127.0.0.1", "[::1]"]);

/** True for a deployment that is neither the official site, a preview of the official Pages project, nor a local run. */
export function isCopyOrigin(origin: string): boolean {
  let host: string;
  try { host = new URL(origin).hostname.toLowerCase(); } catch { return false; }
  if (host === "") return false;
  if (host === OFFICIAL_HOST || host === OFFICIAL_PAGES_PROJECT || host.endsWith(`.${OFFICIAL_PAGES_PROJECT}`)) return false;
  if (LOCAL_HOSTS.has(host) || host.endsWith(".localhost")) return false;
  return true;
}
