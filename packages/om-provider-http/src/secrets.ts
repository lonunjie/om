// SPDX-License-Identifier: MPL-2.0
// The adapter's one secret: 32 random bytes per installation. Every provider capability the adapter must keep to itself
// (receive, admin, key-publish, descriptor-publish) is an HMAC of this secret over the public identifier it protects, so
// nothing else needs storing and a peer who knows a public identifier (a mailbox, a device id) still cannot derive it.
// Losing the secret only loses the right to drain/administer queues created with it, never a message already stored in Core.

export interface SecretStore {
  get(name: string): Promise<Uint8Array | undefined>;
  put(name: string, value: Uint8Array): Promise<void>;
}

export function createMemorySecretStore(): SecretStore {
  const m = new Map<string, Uint8Array>();
  return {
    get: (n) => Promise.resolve(m.get(n)?.slice()),
    put: (n, v) => { m.set(n, v.slice()); return Promise.resolve(); },
  };
}

const DB = "om-provider-secrets";
const STORE = "secrets";

function openDb(idb: IDBFactory): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = idb.open(DB, 1);
    r.onupgradeneeded = () => { r.result.createObjectStore(STORE); };
    r.onsuccess = () => { resolve(r.result); };
    r.onerror = () => { reject(new Error("provider secret store unavailable")); };
  });
}

/** A tiny dedicated IndexedDB store (localStorage is banned by the lint policy). Same-origin script can read it, as it can every other client store. */
export function createIndexedDbSecretStore(idb: IDBFactory): SecretStore {
  return {
    async get(name) {
      const db = await openDb(idb);
      try {
        return await new Promise<Uint8Array | undefined>((resolve, reject) => {
          const q = db.transaction(STORE, "readonly").objectStore(STORE).get(name);
          q.onsuccess = () => { resolve(q.result instanceof Uint8Array ? q.result : undefined); };
          q.onerror = () => { reject(new Error("provider secret read failed")); };
        });
      } finally { db.close(); }
    },
    async put(name, value) {
      const db = await openDb(idb);
      try {
        await new Promise<void>((resolve, reject) => {
          const tx = db.transaction(STORE, "readwrite");
          tx.objectStore(STORE).put(value, name);
          tx.oncomplete = () => { resolve(); };
          tx.onerror = () => { reject(new Error("provider secret write failed")); };
          tx.onabort = () => { reject(new Error("provider secret write aborted")); };
        });
      } finally { db.close(); }
    },
  };
}
