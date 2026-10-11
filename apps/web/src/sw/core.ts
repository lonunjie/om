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
}
export interface KV {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
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
const FLOOR_KEY = "floor";

export const createShell = (deps: ShellDeps) => {
  const { storage, kv, fetchFn, trustRoot } = deps;
  const floor: FloorStore = { get: () => kv.get(FLOOR_KEY), set: (v) => kv.set(FLOOR_KEY, v) };
  let activeName: string | null | undefined;

  /** Download, verify and stage a new bundle into its own cache. Throws (leaving the live shell untouched) on any failure. */
  const stage = async (): Promise<StageResult> => {
    const res = await fetchFn(MANIFEST_PATH);
    if (!res.ok) throw new Error("manifest unavailable");
    const text = await res.text();
    const manifest = parseManifest(text);
    const accepted = await evaluateManifest(manifest, floor, trustRoot);
    if (!accepted.ok) throw new Error(`manifest refused: ${accepted.reason}`);
    const cacheName = CACHE_PREFIX + (await sha256Hex(new TextEncoder().encode(text))).slice(0, 16);
    await storage.delete(cacheName);
    const cache = await storage.open(cacheName);
    try {
      for (const f of manifest.files) {
        const r = await fetchFn(f.path);
        if (!r.ok) throw new Error(`fetch failed: ${f.path}`);
        await cache.put(f.path, r);
      }
      const verdict = await verifyLoadedBundle(manifest, async (p) => (await cache.get(p))?.clone(), () => cache.paths());
      if (!verdict.ok) throw new Error(`bundle verification failed: ${verdict.reason} ${verdict.path}`);
    } catch (e) {
      await storage.delete(cacheName);
      throw e;
    }
    await kv.set("staged", cacheName);
    await kv.set("entry:" + cacheName, manifest.entry);
    await commitFloor(floor, accepted, manifest.version);
    return { version: manifest.version, cacheName, unsignedDev: accepted.unsignedDev };
  };

  /** Promote the staged cache and drop every other shell cache. Called only when the worker takes over. */
  const activate = async (): Promise<void> => {
    const staged = await kv.get("staged");
    if (staged === null) return;
    await kv.set("active", staged);
    activeName = staged;
    for (const n of await storage.names()) if (n.startsWith(CACHE_PREFIX) && n !== staged) await storage.delete(n);
  };

  /** Cached answer for same-origin GETs of precached files; `undefined` means "go to the network". */
  const respond = async (pathname: string, isNavigation: boolean): Promise<Response | undefined> => {
    activeName ??= await kv.get("active");
    if (activeName === null) return undefined;
    const path = pathname.replace(/^\//, "");
    if (path === MANIFEST_PATH) return undefined;
    const cache = await storage.open(activeName);
    const hit = path === "" ? undefined : await cache.get(path);
    if (hit) return hit;
    if (isNavigation) return cache.get((await kv.get("entry:" + activeName)) ?? "index.html");
    return undefined;
  };

  return { stage, activate, respond };
};
