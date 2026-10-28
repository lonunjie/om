// The list of identities (profiles) created in this browser, as opaque ids only. It is not secret and not the message
// database, so it lives in a tiny dedicated IndexedDB store (localStorage is banned by the lint policy). Labels and
// everything else come from Core after the profile's vault is unlocked.
import { asProfileId, type ProfileId } from "@om/platform-api";

const DB = "om-app-registry";
const STORE = "profiles";

function open(idb: IDBFactory): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = idb.open(DB, 1);
    r.onupgradeneeded = () => { r.result.createObjectStore(STORE); };
    r.onsuccess = () => { resolve(r.result); };
    r.onerror = () => { reject(new Error("registry unavailable")); };
  });
}

const done = (tx: IDBTransaction): Promise<void> =>
  new Promise((resolve, reject) => {
    tx.oncomplete = () => { resolve(); };
    tx.onerror = () => { reject(new Error("registry write failed")); };
    tx.onabort = () => { reject(new Error("registry write aborted")); };
  });

export interface ProfileRegistry {
  list(): Promise<readonly ProfileId[]>;
  add(profileId: ProfileId): Promise<void>;
}

export function createProfileRegistry(idb: IDBFactory): ProfileRegistry {
  return {
    async list() {
      const db = await open(idb);
      try {
        const tx = db.transaction(STORE, "readonly");
        const keys = await new Promise<IDBValidKey[]>((resolve, reject) => {
          const q = tx.objectStore(STORE).getAllKeys();
          q.onsuccess = () => { resolve(q.result); };
          q.onerror = () => { reject(new Error("registry read failed")); };
        });
        return keys.filter((k): k is string => typeof k === "string").sort().map(asProfileId);
      } finally {
        db.close();
      }
    },
    async add(profileId) {
      const db = await open(idb);
      try {
        const tx = db.transaction(STORE, "readwrite");
        tx.objectStore(STORE).put(Date.now(), profileId);
        await done(tx);
      } finally {
        db.close();
      }
    },
  };
}
