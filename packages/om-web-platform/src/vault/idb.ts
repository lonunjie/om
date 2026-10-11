// One IndexedDB database per profile: `om-vault-<profileId>` (profileId validated by the caller).
// Connections are opened per operation and closed after, and close on `versionchange`, so deletion is never blocked by us.
export const DB_VERSION = 1;
export const STORE_WRAPPED = "wrapped";
export const STORE_DEVICE_KEY = "deviceKey";
export const STORE_KV = "kv";
export const STORES = [STORE_WRAPPED, STORE_DEVICE_KEY, STORE_KV] as const;
export const WRAPPED_ID = "pvk";
export const DEVICE_KEY_ID = "wrap";

export const dbName = (profileId: string): string => `om-vault-${profileId}`;

export function request<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => { resolve(r.result); };
    r.onerror = () => { reject(r.error ?? new Error("idb_request_failed")); };
  });
}

/** Resolves on commit; rejects on error or abort (the transaction is all-or-nothing). */
export function committed(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => { resolve(); };
    tx.onerror = () => { reject(tx.error ?? new Error("idb_tx_failed")); };
    tx.onabort = () => { reject(tx.error ?? new Error("idb_tx_aborted")); };
  });
}

const guard = (db: IDBDatabase): IDBDatabase => {
  db.onversionchange = () => { db.close(); };
  return db;
};

const hasStores = (db: IDBDatabase): boolean => db.version === DB_VERSION && STORES.every((s) => db.objectStoreNames.contains(s));

export type Existing =
  | { readonly kind: "open"; readonly db: IDBDatabase }
  | { readonly kind: "absent" }
  | { readonly kind: "unusable" } // wrong version or layout: corrupt
  | { readonly kind: "error" };

/** Opens a vault database only if it already exists; never creates one (the upgrade is aborted). */
export function openExisting(idb: IDBFactory, name: string): Promise<Existing> {
  return new Promise((resolve) => {
    let creating = false;
    let r: IDBOpenDBRequest;
    try {
      r = idb.open(name);
    } catch {
      resolve({ kind: "error" });
      return;
    }
    r.onupgradeneeded = () => { creating = true; r.transaction?.abort(); };
    r.onsuccess = () => {
      const db = r.result;
      if (creating || !hasStores(db)) { db.close(); resolve(creating ? { kind: "absent" } : { kind: "unusable" }); return; }
      resolve({ kind: "open", db: guard(db) });
    };
    r.onerror = (e) => { e.preventDefault(); resolve(creating ? { kind: "absent" } : { kind: "error" }); };
  });
}

/** Opens or creates the vault database with its layout. `undefined` if it exists with another layout. */
export function openOrCreate(idb: IDBFactory, name: string): Promise<IDBDatabase | undefined> {
  return new Promise((resolve, reject) => {
    const r = idb.open(name, DB_VERSION);
    r.onupgradeneeded = (e) => {
      if (e.oldVersion !== 0) return;
      for (const s of STORES) r.result.createObjectStore(s);
    };
    r.onsuccess = () => {
      const db = r.result;
      if (hasStores(db)) { resolve(guard(db)); return; }
      db.close();
      resolve(undefined);
    };
    r.onerror = (e) => { e.preventDefault(); reject(r.error ?? new Error("idb_open_failed")); };
  });
}

/** `blocked`: another connection kept the database open past `blockedWait`; the delete may still complete later. */
export function deleteDb(idb: IDBFactory, name: string, blockedWait: () => Promise<void>): Promise<"deleted" | "error" | "blocked"> {
  return new Promise((resolve) => {
    const r = idb.deleteDatabase(name);
    r.onsuccess = () => { resolve("deleted"); };
    r.onerror = (e) => { e.preventDefault(); resolve("error"); };
    r.onblocked = () => { void blockedWait().then(() => { resolve("blocked"); }); };
  });
}

export interface VaultRecords {
  readonly wrapped: unknown;
  readonly deviceKey: unknown;
  readonly kvCount: number;
}

export async function readRecords(db: IDBDatabase): Promise<VaultRecords> {
  const tx = db.transaction([...STORES], "readonly");
  const [wrapped, deviceKey, kvCount] = await Promise.all([
    request<unknown>(tx.objectStore(STORE_WRAPPED).get(WRAPPED_ID)),
    request<unknown>(tx.objectStore(STORE_DEVICE_KEY).get(DEVICE_KEY_ID)),
    request(tx.objectStore(STORE_KV).count()),
  ]);
  return { wrapped, deviceKey, kvCount };
}
