// Where the app's messaging provider (a Home, home-api.md) lives. Two sources, in this order:
//   1. a URL the user chose (stored in this browser; the Settings screen writes it with `saveRelayUrl`), then
//   2. a build-time URL (`VITE_OM_RELAY_URL`), which only dev/test bundles may set: a signed production build refuses it (ADR-0186,
//      vite.config.ts), because nobody relies on a relay of ours.
// With neither, the app runs unconnected and says so ("Not connected to a provider"); nothing is ever guessed or
// hard-coded, and no vendor name appears here. A URL is validated before use (https only; plain http only for a loopback
// host in a build that opted in, and such a build refuses to be a production build, see vite.config.ts).
import { createHomesProvider, createHttpProvider, createIndexedDbSecretStore, parseProviderUrl, readAdmission, type HttpProvider, type ProviderConfig } from "@om/provider-http";
import type { RouteOffer } from "@om/core-wasm";

const DB = "om-app-config";
const STORE = "config";
/** The single address of an install before backups existed (a string). Still written, mirroring the main service, so an older build reads a working config. */
const KEY = "provider-url";
/** The person's delivery services in order, as `StoredHome[]`. Wins over `KEY` when present; written together with it in one transaction. */
const HOMES_KEY = "homes-v1";
/** A bound on the stored list; nobody needs more, and a corrupt store cannot grow it. */
export const MAX_HOMES = 8;

export interface ProviderEnv {
  readonly indexedDB: IDBFactory;
  /** `import.meta.env.VITE_OM_RELAY_URL`. */
  readonly buildUrl?: string | undefined;
  /** True only in the local end-to-end build. */
  readonly allowLoopbackHttp?: boolean;
  readonly fetch?: typeof fetch;
}

function open(idb: IDBFactory): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = idb.open(DB, 1);
    r.onupgradeneeded = () => { r.result.createObjectStore(STORE); };
    r.onsuccess = () => { resolve(r.result); };
    r.onerror = () => { reject(new Error("config unavailable")); };
  });
}

/** One entry of the stored list: an address and whether the person is moving away from it (never offered for new routes, still collected from). */
export interface StoredHome { readonly url: string; readonly draining?: boolean }

const usable = (url: string, env: ProviderEnv): ProviderConfig | undefined => parseProviderUrl(url, { allowLoopbackHttp: env.allowLoopbackHttp === true });

/**
 * What the store says, with the old single address migrated: `homes-v1` wins; otherwise a lone `provider-url` is the one main service.
 * Entries that fail today's address rules are dropped, duplicates collapse, the main (non-leaving) services come first in stored order,
 * and at least one service is never leaving (a store that says otherwise has its first entry kept as the main one).
 */
export function decodeHomes(homes: unknown, legacy: unknown, env: ProviderEnv): readonly StoredHome[] {
  const raw: StoredHome[] = [];
  if (Array.isArray(homes)) {
    for (const h of homes as readonly unknown[]) {
      if (typeof h !== "object" || h === null) continue;
      const { url, draining } = h as { url?: unknown; draining?: unknown };
      if (typeof url === "string") raw.push({ url, draining: draining === true });
    }
  } else if (typeof legacy === "string") raw.push({ url: legacy });
  const seen = new Set<string>();
  const list: { url: string; draining: boolean }[] = [];
  for (const h of raw) {
    const c = usable(h.url, env);
    if (c === undefined || seen.has(c.baseUrl) || list.length >= MAX_HOMES) continue;
    seen.add(c.baseUrl);
    list.push({ url: c.baseUrl, draining: h.draining === true });
  }
  if (list.length > 0 && list.every((h) => h.draining)) list[0] = { url: (list[0] as { url: string }).url, draining: false };
  return [...list.filter((h) => !h.draining), ...list.filter((h) => h.draining)].map((h) => (h.draining ? h : { url: h.url }));
}

/**
 * Read, change and write the stored list in ONE transaction, so two tabs cannot interleave. `change` gets the decoded list (`undefined`: nothing
 * stored) and returns the list to store (`undefined` clears everything), or `"keep"` to leave the store alone.
 */
async function updateHomes<T>(env: ProviderEnv, change: (stored: readonly StoredHome[] | undefined) => { readonly next: readonly StoredHome[] | undefined | "keep"; readonly result: T }): Promise<T> {
  const db = await open(env.indexedDB);
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      const store = tx.objectStore(STORE);
      let outcome: { readonly result: T } | undefined;
      const fail = (): void => { reject(new Error("config write failed")); };
      tx.onerror = fail;
      tx.onabort = fail;
      tx.oncomplete = () => { if (outcome === undefined) fail(); else resolve(outcome.result); };
      const a = store.get(HOMES_KEY);
      a.onsuccess = () => {
        const b = store.get(KEY);
        b.onsuccess = () => {
          const decoded = decodeHomes(a.result, b.result, env);
          const { next, result } = change(decoded.length === 0 ? undefined : decoded);
          outcome = { result };
          if (next === "keep") return;
          if (next === undefined || next.length === 0) { store.delete(HOMES_KEY); store.delete(KEY); return; }
          const clean = decodeHomes(next, undefined, env);
          store.put(clean.map((h) => ({ url: h.url, ...(h.draining === true ? { draining: true } : {}) })), HOMES_KEY);
          store.put((clean[0] as StoredHome).url, KEY);
        };
      };
    });
  } finally { db.close(); }
}

async function readStored(env: ProviderEnv): Promise<readonly StoredHome[] | undefined> {
  const db = await open(env.indexedDB);
  try {
    return await new Promise<readonly StoredHome[] | undefined>((resolve, reject) => {
      const store = db.transaction(STORE, "readonly").objectStore(STORE);
      const a = store.get(HOMES_KEY);
      const b = store.get(KEY);
      b.onsuccess = () => { const d = decodeHomes(a.result, b.result, env); resolve(d.length === 0 ? undefined : d); };
      a.onerror = b.onerror = () => { reject(new Error("config read failed")); };
    });
  } finally { db.close(); }
}

/**
 * Stores the user's one delivery service, replacing any others (`undefined` clears them all). Returns false, storing nothing, if the URL is
 * not acceptable. Several services are changed with `addBackupHome` and `markHomeLeaving`, never by this.
 */
export async function saveRelayUrl(env: ProviderEnv, url: string | undefined): Promise<boolean> {
  const parsed = url === undefined ? undefined : usable(url, env);
  if (url !== undefined && parsed === undefined) return false;
  await updateHomes(env, () => ({ next: parsed === undefined ? undefined : [{ url: parsed.baseUrl }], result: null }));
  return true;
}

/**
 * Adds a service after the ones already stored (the build default counts as the first when nothing is stored yet, so adding a backup never
 * silently drops the service in use). `already`: this address is in the list, leaving or not; nothing changed.
 */
export async function addBackupHome(env: ProviderEnv, url: string): Promise<"added" | "already" | "full" | "failed"> {
  const parsed = usable(url, env);
  if (parsed === undefined) return "failed";
  const built = env.buildUrl !== undefined && env.buildUrl !== "" ? usable(env.buildUrl, env) : undefined;
  try {
    return await updateHomes(env, (stored) => {
      const list: readonly StoredHome[] = stored ?? (built === undefined ? [] : [{ url: built.baseUrl }]);
      if (list.length === 0) return { next: "keep", result: "failed" as const }; // a backup needs a service to back up
      if (list.some((h) => h.url === parsed.baseUrl)) return { next: "keep", result: "already" as const };
      if (list.length >= MAX_HOMES) return { next: "keep", result: "full" as const };
      return { next: [...list, { url: parsed.baseUrl }], result: "added" as const };
    });
  } catch { return "failed"; }
}

/**
 * Records that the person is moving away from this service: it stays in the list (mail may still arrive there) but is never offered for new
 * routes again. Refused (`false`, nothing changed) for an address not in the list, or when it is the only service not being left.
 * Marking one already leaving succeeds and changes nothing.
 */
export async function markHomeLeaving(env: ProviderEnv, url: string): Promise<boolean> {
  const parsed = usable(url, env);
  if (parsed === undefined) return false;
  try {
    return await updateHomes(env, (stored) => {
      const at = stored?.findIndex((h) => h.url === parsed.baseUrl) ?? -1;
      if (stored === undefined || at < 0) return { next: "keep", result: false };
      if (stored[at]?.draining === true) return { next: "keep", result: true };
      if (stored.filter((h) => h.draining !== true).length < 2) return { next: "keep", result: false };
      return { next: stored.map((h, i) => (i === at ? { url: h.url, draining: true } : h)), result: true };
    });
  } catch { return false; }
}

/** Where the Home in use came from: the user's own choice, or the default baked into this build. */
export interface ProviderChoice {
  readonly config: ProviderConfig;
  readonly source: "chosen" | "default";
}

/** One delivery service of the person; `leaving` ones are still collected from but never offered for new routes. */
export interface HomeEntry { readonly config: ProviderConfig; readonly leaving: boolean }

/** Every configured delivery service: the main one first, then backups, then the ones being left. Never empty. */
export interface HomeSet {
  readonly source: "chosen" | "default";
  readonly homes: readonly HomeEntry[];
}

/** True when this build carries a usable default Home (so "use the default" is a real option). */
export function hasBuildDefault(env: ProviderEnv): boolean {
  return env.buildUrl !== undefined && env.buildUrl !== "" && usable(env.buildUrl, env) !== undefined;
}

/** The configured services and where they came from, or `null` when none is configured (or the stored choice is unusable). */
export async function resolveHomeSet(env: ProviderEnv): Promise<HomeSet | null> {
  let stored: readonly StoredHome[] | undefined;
  try { stored = await readStored(env); } catch { stored = undefined; }
  if (stored !== undefined) {
    const homes = stored.flatMap((h) => { const config = usable(h.url, env); return config === undefined ? [] : [{ config, leaving: h.draining === true }]; });
    if (homes.length > 0) return { source: "chosen", homes };
  }
  const built = env.buildUrl !== undefined && env.buildUrl !== "" ? usable(env.buildUrl, env) : undefined;
  return built ? { source: "default", homes: [{ config: built, leaving: false }] } : null;
}

/** The main delivery service and where it came from, or `null` when none is configured. */
export async function resolveProviderChoice(env: ProviderEnv): Promise<ProviderChoice | null> {
  const set = await resolveHomeSet(env);
  const main = set?.homes[0];
  return set === null || main === undefined ? null : { config: main.config, source: set.source };
}

/** The main provider config, or `null` when none is configured (or the stored choice is unusable). */
export async function resolveProviderConfig(env: ProviderEnv): Promise<ProviderConfig | null> {
  return (await resolveProviderChoice(env))?.config ?? null;
}

/**
 * The wake channel (a socket per mailbox that says "mail is waiting", so nothing asks on a timer) exists only while the page
 * is on screen: it opens when the page becomes visible and closes when it is hidden, where the slow poll takes over.
 */
export function createHttpProviderFor(env: ProviderEnv, config: ProviderConfig, admission?: Uint8Array): HttpProvider {
  const provider = createHttpProvider({ config, secrets: createIndexedDbSecretStore(env.indexedDB), ...(env.allowLoopbackHttp === true ? { allowLoopbackHttp: true } : {}), ...(env.fetch === undefined ? {} : { fetch: env.fetch }), ...(admission === undefined ? {} : { admission }) });
  if (typeof document === "undefined") return provider;
  const sync = (): void => { provider.setWakeActive(document.visibilityState === "visible"); };
  document.addEventListener("visibilitychange", sync);
  sync();
  return { ...provider, dispose: () => { document.removeEventListener("visibilitychange", sync); provider.dispose(); } };
}

/** The owner's admission secret for exactly this Home, if this device set it up (om.admission.v1); a Home chosen any other way gets none. */
export function readAdmissionFor(env: ProviderEnv, config: ProviderConfig): Promise<Uint8Array | undefined> {
  return readAdmission(createIndexedDbSecretStore(env.indexedDB), config.baseUrl).catch(() => undefined);
}

/** The provider Core runs against, plus a way to stop offering one service at once (before the host has reconnected). */
export interface HomesProvider extends HttpProvider {
  /** Stops offering this service for new routes right now; it is still polled, acknowledged and deposited to. Returns how to undo it. */
  leave(baseUrl: string): () => void;
}

/**
 * Every configured service behind one provider (`createHomesProvider`): the first not-leaving service is the active one (key material, invites,
 * introductions), new routes are spread across the not-leaving services, and leaving ones are only collected from. One service and nothing
 * leaving is exactly that service's provider.
 */
export async function createHomesProviderFor(env: ProviderEnv, set: HomeSet): Promise<HomesProvider> {
  const made = await Promise.all(set.homes.map(async (h) => ({ leaving: h.leaving, provider: createHttpProviderFor(env, h.config, await readAdmissionFor(env, h.config)) })));
  const offered = made.filter((m) => !m.leaving).map((m) => m.provider);
  const leavingNow = made.filter((m) => m.leaving).map((m) => m.provider);
  const inner = createHomesProvider(offered, leavingNow);
  const leaving = new Set<string>();
  return {
    ...inner,
    leave(baseUrl) { leaving.add(baseUrl); return () => { leaving.delete(baseUrl); }; },
    // A service being left must not be offered again between the moment Core drops its routes and the next connection, or Core adds them back.
    allocateRoutes(count: number, held?: readonly string[]): Promise<readonly RouteOffer[]> {
      const live = offered.filter((h) => !leaving.has(h.baseUrl));
      const first = live[0];
      return first === undefined ? Promise.resolve([]) : createHomesProvider(live).allocateRoutes(count, held);
    },
  };
}

export async function createConfiguredProvider(env: ProviderEnv): Promise<HomesProvider | null> {
  const set = await resolveHomeSet(env);
  return set === null ? null : createHomesProviderFor(env, set);
}
