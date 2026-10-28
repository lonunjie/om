// Where the app's messaging provider (a Home, home-api.md) lives. Two sources, in this order:
//   1. a URL the user chose (stored in this browser; the Settings screen writes it with `saveRelayUrl`), then
//   2. the deployment default baked in at build time (`VITE_OM_RELAY_URL`, part of the signed bundle manifest).
// With neither, the app runs unconnected and says so ("Not connected to a provider"); nothing is ever guessed or
// hard-coded, and no vendor name appears here. A URL is validated before use (https only; plain http only for a loopback
// host in a build that opted in, and such a build refuses to be a production build, see vite.config.ts).
import { createHttpProvider, createIndexedDbSecretStore, parseProviderUrl, type HttpProvider, type ProviderConfig } from "@om/provider-http";

const DB = "om-app-config";
const STORE = "config";
const KEY = "provider-url";

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

async function readStored(idb: IDBFactory): Promise<string | undefined> {
  const db = await open(idb);
  try {
    return await new Promise<string | undefined>((resolve, reject) => {
      const q = db.transaction(STORE, "readonly").objectStore(STORE).get(KEY);
      q.onsuccess = () => { resolve(typeof q.result === "string" ? q.result : undefined); };
      q.onerror = () => { reject(new Error("config read failed")); };
    });
  } finally { db.close(); }
}

/** Stores the user's Home URL (`undefined` clears it). Returns false, storing nothing, if the URL is not acceptable. */
export async function saveRelayUrl(env: ProviderEnv, url: string | undefined): Promise<boolean> {
  const parsed = url === undefined ? undefined : parseProviderUrl(url, { allowLoopbackHttp: env.allowLoopbackHttp === true });
  if (url !== undefined && parsed === undefined) return false;
  const db = await open(env.indexedDB);
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      if (parsed === undefined) tx.objectStore(STORE).delete(KEY); else tx.objectStore(STORE).put(parsed.baseUrl, KEY);
      tx.oncomplete = () => { resolve(); };
      tx.onerror = () => { reject(new Error("config write failed")); };
      tx.onabort = () => { reject(new Error("config write aborted")); };
    });
  } finally { db.close(); }
  return true;
}

/** The configured provider, or `null` when none is configured (or the stored choice is unusable). */
export async function resolveProviderConfig(env: ProviderEnv): Promise<ProviderConfig | null> {
  const opts = { allowLoopbackHttp: env.allowLoopbackHttp === true };
  let stored: string | undefined;
  try { stored = await readStored(env.indexedDB); } catch { stored = undefined; }
  const chosen = stored !== undefined ? parseProviderUrl(stored, opts) : undefined;
  if (chosen) return chosen;
  const built = env.buildUrl !== undefined && env.buildUrl !== "" ? parseProviderUrl(env.buildUrl, opts) : undefined;
  return built ?? null;
}

export async function createConfiguredProvider(env: ProviderEnv): Promise<HttpProvider | null> {
  const config = await resolveProviderConfig(env);
  if (config === null) return null;
  return createHttpProvider({ config, secrets: createIndexedDbSecretStore(env.indexedDB), ...(env.fetch === undefined ? {} : { fetch: env.fetch }) });
}
