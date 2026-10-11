import type { KV } from "./core";

// Tiny IndexedDB key/value store for non-secret worker state (version floor, active cache name). Never holds keys.
const open = (): Promise<IDBDatabase> =>
  new Promise((resolve, reject) => {
    const req = indexedDB.open("om-sw", 1);
    req.onupgradeneeded = () => {
      req.result.createObjectStore("kv");
    };
    req.onsuccess = () => {
      resolve(req.result);
    };
    req.onerror = () => {
      reject(req.error ?? new Error("idb open failed"));
    };
  });

const run = async <T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> => {
  const db = await open();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction("kv", mode);
      const req = f(tx.objectStore("kv"));
      tx.oncomplete = () => {
        resolve(req.result);
      };
      tx.onerror = tx.onabort = () => {
        reject(tx.error ?? new Error("idb tx failed"));
      };
    });
  } finally {
    db.close();
  }
};

export const idbKv: KV = {
  get: async (key) => {
    const v = await run<unknown>("readonly", (s) => s.get(key));
    return typeof v === "string" ? v : null;
  },
  set: async (key, value) => {
    await run("readwrite", (s) => s.put(value, key));
  },
};
