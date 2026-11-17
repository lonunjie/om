// Service-worker shell logic with injected storage, so it is testable in Node and never owns crypto or relay traffic.
// The worker is a cache/deployment mechanism only (CLIENTHOST_UPDATE_LIFECYCLE §5): the origin stays the trust anchor,
// and every correctness path works with no worker running.
import { commitFloor, evaluateManifest, type FloorStore } from "../update/floor";
import { sha256Hex } from "../update/hash";
import { MANIFEST_PATH, parseManifest } from "../update/manifest";
import type { TrustRoot } from "../update/signature";
import { verifyLoadedBundle } from "../update/verify";

export interface ShellCache {
  put(path: string, res: Response): Promise<void>;
  get(path: string): Promise<Response | undefined>;
  paths(): Promise<string[]>;
}
export interface ShellStorage {
  open(name: string): Promise<ShellCache>;
  delete(name: string): Promise<void>;
  names(): Promise<string[]>;
  /** Origin-wide exclusion across worker instances for staging, switching and cleanup. */
  exclusive<T>(job: () => Promise<T>): Promise<T>;
}
export interface KV {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
}
export interface ShellDeps {
  readonly storage: ShellStorage;
  readonly kv: KV;
  /** Network fetch of a bundle path, bypassing HTTP caches. */
  readonly fetchFn: (path: string) => Promise<Response>;
  readonly trustRoot: TrustRoot;
}
export interface StageResult {
  readonly version: string;
  readonly cacheName: string;
  readonly unsignedDev: boolean;
}

export const CACHE_PREFIX = "om-shell-";

/** A navigation may not be answered with a redirected response (the browser shows "site can't be reached"), and the host
 *  redirects `/index.html` to `/`. Rebuild such a response with the same body and headers so it is served as a plain one. */
const plain = async (res: Response): Promise<Response> =>
  res.redirected ? new Response(await res.blob(), { status: res.status, statusText: res.statusText, headers: res.headers }) : res;
/** Workers older than this served the host's redirected shell, which browsers refuse ("site can't be reached"), so a page stuck
 *  behind one never runs the code that would let a newer worker take over. No recorded version means older still. */
const FIRST_WORKING_SHELL = [0, 0, 2] as const;
export const servesBrokenShell = (version: string | null): boolean => {
  if (version === null) return true;
  const m = /^(\d+)\.(\d+)\.(\d+)/.exec(version);
  if (m === null) return false; // dev and e2e builds carry free-form versions
  for (let i = 0; i < 3; i++) {
    const have = Number(m[i + 1]);
    const need = FIRST_WORKING_SHELL[i] ?? 0;
    if (have !== need) return have < need;
  }
  return false;
};
const FLOOR_KEY = "floor";
const DELEGATION_FLOOR_KEY = "delegation-floor";
const PQ_FLOOR_KEY = "pq-required-floor";

export const createShell = (deps: ShellDeps) => {
  const { storage, kv, fetchFn, trustRoot } = deps;
  const floor: FloorStore = {
    get: () => kv.get(FLOOR_KEY),
    set: (v) => kv.set(FLOOR_KEY, v),
    getDelegation: async () => {
      const v = await kv.get(DELEGATION_FLOOR_KEY);
      const n = v === null ? Number.NaN : Number(v);
      // A stored value that is not a positive integer is treated as corrupt: refuse everything rather than reset the floor.
      return v === null ? null : Number.isSafeInteger(n) && n >= 1 ? n : Number.MAX_SAFE_INTEGER;
    },
    setDelegation: (v) => kv.set(DELEGATION_FLOOR_KEY, String(v)),
    // ADR-0182: any stored value counts as "required" (a corrupt value must not switch the requirement off); only "1" is ever written.
    getPqRequired: async () => (await kv.get(PQ_FLOOR_KEY)) !== null,
    setPqRequired: () => kv.set(PQ_FLOOR_KEY, "1"),
  };

  /** Download, verify and stage a new bundle into its own cache. Throws (leaving the live shell untouched) on any failure. */
  const stage = (): Promise<StageResult> => storage.exclusive(async () => {
    const res = await fetchFn(MANIFEST_PATH);
    if (!res.ok) throw new Error("manifest unavailable");
    const text = await res.text();
    const manifest = parseManifest(text);
    const accepted = await evaluateManifest(manifest, floor, trustRoot);
    if (!accepted.ok) throw new Error(`manifest refused: ${accepted.reason}`);
    const cacheName = CACHE_PREFIX + (await sha256Hex(new TextEncoder().encode(text))).slice(0, 16) + "-" + crypto.randomUUID();
    const cache = await storage.open(cacheName);
    try {
      for (const f of manifest.files) {
        if (f.lazy === true) continue; // loaded on demand and verified by the page (D65): the install stays the size of the base bundle
        const r = await fetchFn(f.path);
        if (!r.ok) throw new Error(`fetch failed: ${f.path}`);
        await cache.put(f.path, await plain(r));
      }
      const verdict = await verifyLoadedBundle(manifest, async (p) => (await cache.get(p))?.clone(), () => cache.paths());
      if (!verdict.ok) throw new Error(`bundle verification failed: ${verdict.reason} ${verdict.path}`);
      // Seal all metadata before publishing the cache; readers never see a partially staged build.
      await kv.set("entry:" + cacheName, manifest.entry);
      await commitFloor(floor, accepted, manifest.version);
      await kv.set("staged", cacheName);
      return { version: manifest.version, cacheName, unsignedDev: accepted.unsignedDev };
    } catch (e) {
      if (await kv.get("active") !== cacheName) {
        await storage.delete(cacheName);
        await kv.delete("entry:" + cacheName);
      }
      throw e;
    }
  });

  /** Promote the staged cache and drop every other shell cache. Called only when the worker takes over. */
  const activate = (): Promise<void> => storage.exclusive(async () => {
    const staged = await kv.get("staged");
    if (staged === null || await kv.get("entry:" + staged) === null) return;
    await kv.set("active", staged);
    for (const n of await storage.names()) if (n.startsWith(CACHE_PREFIX) && n !== staged) {
      await storage.delete(n);
      await kv.delete("entry:" + n);
    }
  });

  /** Cached answer for same-origin GETs of precached files; `undefined` means "go to the network". */
  const respond = async (pathname: string, isNavigation: boolean): Promise<Response | undefined> => {
    const path = pathname.replace(/^\//, "");
    if (path === MANIFEST_PATH) return undefined;
    // Activation may retire the cache after this reader obtains the old pointer. Retry once against the new build.
    for (let attempt = 0; attempt < 2; attempt++) {
      const activeName = await kv.get("active");
      if (activeName === null) return undefined;
      const cache = await storage.open(activeName);
      const hit = path === "" ? undefined : await cache.get(path);
      if (hit) return plain(hit);
      if (isNavigation) {
        const entry = await cache.get((await kv.get("entry:" + activeName)) ?? "index.html");
        if (entry) return plain(entry);
      }
      if (await kv.get("active") === activeName) return undefined;
    }
    return undefined;
  };

  /** True when a worker is already serving a shell that browsers refuse: the new worker may then take over at once, since nothing
   *  that page shows can be lost. Never true for a first install, and never for a working predecessor (those wait for idle). */
  const replacesBrokenShell = async (): Promise<boolean> => (await kv.get("active")) !== null && servesBrokenShell(await kv.get("worker-version"));

  return { stage, activate, respond, replacesBrokenShell };
};
