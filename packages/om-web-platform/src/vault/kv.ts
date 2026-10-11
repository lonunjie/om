// EncryptedKv: profile-scoped store in the profile's vault database. Keys are not secret; values are sealed
// (purpose `local-db`) with the key inside the plaintext, so a value moved to another key fails closed.
import type { DeviceVault, EncryptedKv, KvGetResult, KvListResult, KvResult, ProfileId, VaultError, VaultErrorReason } from "@om/platform-api";
import type { Bytes } from "./env";
import { concat, isValidProfileId, utf8 } from "./crypto";
import { STORE_KV, committed, dbName, openExisting, request } from "./idb";

const MAX_KEY_LENGTH = 512;
const fail = <R extends VaultErrorReason>(reason: R): VaultError<R> => ({ ok: false, reason });
const validKey = (k: unknown): k is string => typeof k === "string" && k.length > 0 && k.length <= MAX_KEY_LENGTH;
const decoder = new TextDecoder("utf-8", { fatal: true });

function frame(key: string, value: Uint8Array): Bytes {
  const k = utf8(key);
  const len = new Uint8Array(4);
  new DataView(len.buffer).setUint32(0, k.length);
  return concat(len, k, value);
}

function unframe(key: string, b: Uint8Array): Bytes | undefined {
  if (b.length < 4) return undefined;
  const n = new DataView(b.buffer, b.byteOffset, b.byteLength).getUint32(0);
  if (b.length < 4 + n) return undefined;
  try {
    if (decoder.decode(b.subarray(4, 4 + n)) !== key) return undefined;
  } catch {
    return undefined;
  }
  return b.slice(4 + n);
}

export function createEncryptedKv(vault: DeviceVault, env: { readonly indexedDB: IDBFactory }, profileId: ProfileId): EncryptedKv {
  async function withStore<T>(mode: IDBTransactionMode, run: (s: IDBObjectStore) => IDBRequest<T>): Promise<{ ok: true; value: T } | VaultError<VaultErrorReason>> {
    if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
    const ex = await openExisting(env.indexedDB, dbName(profileId));
    if (ex.kind !== "open") return fail(ex.kind === "absent" ? "not_found" : ex.kind === "unusable" ? "corrupt" : "storage_failed");
    try {
      const tx = ex.db.transaction(STORE_KV, mode);
      const done = committed(tx);
      const value = await request(run(tx.objectStore(STORE_KV)));
      await done;
      return { ok: true, value };
    } catch {
      return fail("storage_failed");
    } finally {
      ex.db.close();
    }
  }

  return {
    async put(key, value): Promise<KvResult> {
      if (!validKey(key)) return fail("invalid_key");
      const sealed = await vault.seal(profileId, "local-db", frame(key, value));
      if (!sealed.ok) return sealed;
      const r = await withStore("readwrite", (s) => s.put(sealed.sealed, key));
      return r.ok ? { ok: true } : r;
    },

    async get(key): Promise<KvGetResult> {
      if (!validKey(key)) return fail("invalid_key");
      const r = await withStore<unknown>("readonly", (s) => s.get(key));
      if (!r.ok) return r;
      if (r.value === undefined) return { ok: true, value: undefined };
      if (!(r.value instanceof Uint8Array)) return fail("malformed");
      const opened = await vault.open(profileId, "local-db", r.value);
      if (!opened.ok) return opened;
      const value = unframe(key, opened.bytes);
      opened.bytes.fill(0);
      return value === undefined ? fail("integrity_failure") : { ok: true, value };
    },

    async delete(key): Promise<KvResult> {
      if (!validKey(key)) return fail("invalid_key");
      const r = await withStore("readwrite", (s) => s.delete(key));
      return r.ok ? { ok: true } : r;
    },

    async list(): Promise<KvListResult> {
      const r = await withStore("readonly", (s) => s.getAllKeys());
      return r.ok ? { ok: true, keys: r.value.filter((k): k is string => typeof k === "string") } : r;
    },
  };
}
