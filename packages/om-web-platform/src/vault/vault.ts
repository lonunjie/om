// WebDeviceVault (DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md §4-5, 2026-10-01 resolution; ADR-0172 R-391/R-392).
import type {
  CreateVaultOptions, CreateVaultResult, CredentialBackup, DeleteVaultResult, DeviceVault, OpenResult, ProfileId,
  RotateOptions, RotateResult, SealResult, UnlockResult, VaultCapabilities, VaultError, VaultErrorReason, VaultPurpose, VaultStage, VaultStatus,
  VaultWrappingInfo, WrappingInfoResult, WrappingMethod, WrappingPreference, WrappingReason,
} from "@om/platform-api";
import { isVaultPurpose } from "@om/platform-api";
import type { Bytes, VaultEnv } from "./env";
import {
  PRF_OUTPUT_BYTES, PRF_SALT_BYTES, PVK_BYTES, SEAL_FORMAT, deriveSubkey, equalBytes, importPvk, isValidProfileId,
  newDeviceKey, openBytes, parseSealed, prfInput, prfKek, randomBytes, sealBytes, unwrapPvk, wrapPvk,
} from "./crypto";
import type { WrappedPvk } from "./crypto";
import {
  DEVICE_KEY_ID, STORES, STORE_DEVICE_KEY, STORE_KV, STORE_WRAPPED, WRAPPED_ID, committed, dbName, deleteDb,
  openExisting, openOrCreate, readRecords,
} from "./idb";
import { base64url, createVaultCredential, evaluateVaultPrf } from "./webauthn";

const DELETE_WAIT_MS = 5000;
const DEFAULT_PASSKEY_USER = { name: "om-vault", displayName: "OM" };

interface PrfRecord {
  readonly credentialId: Bytes;
  readonly salt: Bytes;
  readonly rpId: string;
  readonly backup: CredentialBackup;
}

/** Stored beside the data; the PVK only ever appears here wrapped. */
interface WrappedRecord extends WrappedPvk {
  readonly format: typeof SEAL_FORMAT;
  readonly profileId: string;
  readonly method: WrappingMethod;
  readonly reason: WrappingReason;
  readonly generation: number;
  readonly prf?: PrfRecord;
}

const REASONS: readonly WrappingReason[] = [
  "prf_verified", "device_key_requested", "webauthn_absent", "prf_not_enabled", "prf_no_output", "webauthn_create_failed",
  "webauthn_get_failed",
];
const BACKUPS: readonly CredentialBackup[] = ["device_bound", "synced", "unknown"];

const asRecord = (v: unknown): Record<string, unknown> | undefined =>
  typeof v === "object" && v !== null ? (v as Record<string, unknown>) : undefined;
const bytesOf = (v: unknown, len?: number): Bytes | undefined =>
  v instanceof Uint8Array && (len === undefined || v.length === len) ? new Uint8Array(v) : undefined;

function parseRecord(v: unknown, profileId: string): WrappedRecord | undefined {
  const r = asRecord(v);
  if (r === undefined || r["format"] !== SEAL_FORMAT || r["profileId"] !== profileId) return undefined;
  const method = r["method"];
  const reason = r["reason"];
  const generation = r["generation"];
  const nonce = bytesOf(r["nonce"], 12);
  const ciphertext = bytesOf(r["ciphertext"], PVK_BYTES + 16);
  if ((method !== "webauthn-prf" && method !== "device-key") || !REASONS.includes(reason as WrappingReason)
    || typeof generation !== "number" || !Number.isSafeInteger(generation) || nonce === undefined || ciphertext === undefined) return undefined;
  const base = { format: SEAL_FORMAT, profileId, method, reason: reason as WrappingReason, generation, nonce, ciphertext } as const;
  if (method === "device-key") return r["prf"] === undefined && reason !== "prf_verified" ? base : undefined;
  const p = asRecord(r["prf"]);
  const credentialId = bytesOf(p?.["credentialId"]);
  const salt = bytesOf(p?.["salt"], PRF_SALT_BYTES);
  const rpId = p?.["rpId"];
  const backup = p?.["backup"];
  if (reason !== "prf_verified" || credentialId === undefined || credentialId.length === 0 || salt === undefined
    || typeof rpId !== "string" || !BACKUPS.includes(backup as CredentialBackup)) return undefined;
  return { ...base, prf: { credentialId, salt, rpId, backup: backup as CredentialBackup } };
}

const isDeviceKey = (k: unknown): k is CryptoKey =>
  typeof CryptoKey !== "undefined" && k instanceof CryptoKey && !k.extractable && k.algorithm.name === "AES-GCM"
  && k.usages.includes("decrypt");

const infoOf = (rec: WrappedRecord): VaultWrappingInfo =>
  rec.prf === undefined
    ? { method: rec.method, reason: rec.reason, reducedAtRestProperty: false }
    : { method: rec.method, reason: rec.reason, credentialBackup: rec.prf.backup, reducedAtRestProperty: rec.prf.backup !== "device_bound" };

type LoadFailure = "absent" | "empty" | "corrupt" | "error";
type Loaded =
  | { readonly kind: "ok"; readonly rec: WrappedRecord; readonly deviceKey: CryptoKey | undefined }
  | { readonly kind: LoadFailure };

const loadFailure = (k: LoadFailure): "not_found" | "corrupt" | "storage_failed" =>
  k === "corrupt" ? "corrupt" : k === "error" ? "storage_failed" : "not_found";

interface Session {
  readonly base: CryptoKey;
  readonly subkeys: Map<VaultPurpose, Promise<CryptoKey>>;
}

type Built =
  | { readonly ok: true; readonly rec: Omit<WrappedRecord, "generation">; readonly deviceKey: CryptoKey | undefined }
  | VaultError<"needs_user_gesture" | "crypto_failed">;

const fail = <R extends VaultErrorReason>(reason: R, stage?: VaultStage): VaultError<R> =>
  stage === undefined ? { ok: false, reason } : { ok: false, reason, stage };

export function createWebDeviceVault(env: VaultEnv): DeviceVault {
  const c = env.crypto;
  const sessions = new Map<ProfileId, Session>();
  const unlocking = new Map<ProfileId, Promise<UnlockResult>>();
  const epochs = new Map<ProfileId, number>();
  const epoch = (p: ProfileId): number => epochs.get(p) ?? 0;

  async function load(profileId: ProfileId): Promise<Loaded> {
    const ex = await openExisting(env.indexedDB, dbName(profileId));
    if (ex.kind === "absent") return { kind: "absent" };
    if (ex.kind === "unusable") return { kind: "corrupt" };
    if (ex.kind === "error") return { kind: "error" };
    try {
      const r = await readRecords(ex.db);
      if (r.wrapped === undefined) return r.deviceKey === undefined && r.kvCount === 0 ? { kind: "empty" } : { kind: "corrupt" };
      const rec = parseRecord(r.wrapped, profileId);
      if (rec === undefined) return { kind: "corrupt" };
      if (rec.method === "device-key") return isDeviceKey(r.deviceKey) ? { kind: "ok", rec, deviceKey: r.deviceKey } : { kind: "corrupt" };
      return r.deviceKey === undefined ? { kind: "ok", rec, deviceKey: undefined } : { kind: "corrupt" };
    } catch {
      return { kind: "error" };
    } finally {
      ex.db.close();
    }
  }

  /** Method selection by test (R-391): PRF only after `prf.enabled` at create AND a 32-byte output from a later get(). */
  async function buildWrapper(profileId: ProfileId, raw: Bytes, pref: WrappingPreference, user: CreateVaultOptions["passkeyUser"]): Promise<Built> {
    let reason: WrappingReason = "device_key_requested";
    if (pref === "auto") {
      const salt = randomBytes(c, PRF_SALT_BYTES);
      const input = await prfInput(c, salt);
      const created = await createVaultCredential(env, user ?? DEFAULT_PASSKEY_USER, input);
      if (!created.ok && created.reason === "needs_user_gesture") return fail("needs_user_gesture", "enroll_create");
      if (!created.ok) reason = created.reason === "unavailable" ? "webauthn_absent" : "webauthn_create_failed";
      else if (!created.value.prfEnabled) reason = "prf_not_enabled";
      else {
        const { credentialId, backup } = created.value;
        const ev = await evaluateVaultPrf(env, env.rpId, credentialId, input);
        if (!ev.ok && ev.reason === "needs_user_gesture") return fail("needs_user_gesture", "enroll_verify");
        if (!ev.ok) reason = "webauthn_get_failed";
        else if (ev.output?.length !== PRF_OUTPUT_BYTES) { ev.output?.fill(0); reason = "prf_no_output"; }
        else {
          try {
            const kek = await prfKek(c, ev.output, salt);
            const w = await wrapPvk(c, kek, raw, profileId, "webauthn-prf");
            return { ok: true, deviceKey: undefined, rec: {
              format: SEAL_FORMAT, profileId, method: "webauthn-prf", reason: "prf_verified", ...w,
              prf: { credentialId, salt, rpId: env.rpId, backup },
            } };
          } catch {
            return fail("crypto_failed");
          }
        }
      }
    }
    try {
      const key = await newDeviceKey(c);
      if (key.extractable) return fail("crypto_failed");
      const w = await wrapPvk(c, key, raw, profileId, "device-key");
      return { ok: true, deviceKey: key, rec: { format: SEAL_FORMAT, profileId, method: "device-key", reason, ...w } };
    } catch {
      return fail("crypto_failed");
    }
  }

  /** Raw PVK through the stored wrapper (caller zeroes it). */
  async function unwrapStored(
    rec: WrappedRecord, deviceKey: CryptoKey | undefined, stage: "unlock" | "rotate_unwrap",
  ): Promise<{ readonly ok: true; readonly raw: Bytes } | VaultError<"needs_user_gesture" | "ceremony_failed" | "prf_unavailable" | "unwrap_failed" | "crypto_failed">> {
    let kek: CryptoKey;
    if (rec.prf === undefined) {
      if (deviceKey === undefined) return fail("unwrap_failed");
      kek = deviceKey;
    } else {
      // A credential is bound to its RP ID; another origin's passkey is never used (R-392).
      if (rec.prf.rpId !== env.rpId) return fail("prf_unavailable", stage);
      const ev = await evaluateVaultPrf(env, rec.prf.rpId, rec.prf.credentialId, await prfInput(c, rec.prf.salt));
      if (!ev.ok) return fail(ev.reason === "needs_user_gesture" ? "needs_user_gesture" : ev.reason === "failed" ? "ceremony_failed" : "prf_unavailable", stage);
      if (ev.output?.length !== PRF_OUTPUT_BYTES) { ev.output?.fill(0); return fail("prf_unavailable", stage); }
      try { kek = await prfKek(c, ev.output, rec.prf.salt); } catch { return fail("crypto_failed"); }
    }
    const raw = await unwrapPvk(c, kek, rec, rec.profileId, rec.method);
    return raw === undefined ? fail("unwrap_failed") : { ok: true, raw };
  }

  async function install(profileId: ProfileId, raw: Bytes): Promise<void> {
    sessions.set(profileId, { base: await importPvk(c, raw), subkeys: new Map() });
  }

  async function signalUnknown(prf: PrfRecord | undefined): Promise<void> {
    if (prf === undefined || env.signalUnknownCredential === undefined) return;
    try { await env.signalUnknownCredential({ rpId: prf.rpId, credentialId: base64url(prf.credentialId) }); } catch { /* best effort */ }
  }

  async function doUnlock(profileId: ProfileId): Promise<UnlockResult> {
    const startEpoch = epoch(profileId);
    const l = await load(profileId);
    if (l.kind !== "ok") return fail(loadFailure(l.kind));
    const u = await unwrapStored(l.rec, l.deviceKey, "unlock");
    if (!u.ok) return u;
    if (epoch(profileId) !== startEpoch) { u.raw.fill(0); return fail("conflict"); } // locked or deleted meanwhile: lock wins
    try { await install(profileId, u.raw); } catch { return fail("crypto_failed"); }
    return { ok: true };
  }

  function subkey(s: Session, purpose: VaultPurpose): Promise<CryptoKey> {
    let k = s.subkeys.get(purpose);
    if (k === undefined) { k = deriveSubkey(c, s.base, purpose); s.subkeys.set(purpose, k); }
    return k;
  }

  const vault: DeviceVault = {
    capabilities(): VaultCapabilities {
      return {
        webauthnPresent: env.credentials !== undefined,
        deviceKeyPresent: true,
        passphraseFactor: "unavailable_no_reviewed_kdf",
        reliesOnPersistentStorage: false,
      };
    },

    async status(profileId, opts): Promise<VaultStatus> {
      if (!isValidProfileId(profileId)) return "corrupt";
      const l = await load(profileId);
      if (l.kind === "absent" || l.kind === "empty") {
        sessions.delete(profileId);
        return opts?.known === true ? "evicted" : "absent";
      }
      if (l.kind !== "ok") return "corrupt";
      return sessions.has(profileId) ? "unlocked" : "locked";
    },

    async wrapping(profileId): Promise<WrappingInfoResult> {
      if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
      const l = await load(profileId);
      return l.kind === "ok" ? { ok: true, wrapping: infoOf(l.rec) } : fail(loadFailure(l.kind));
    },

    async createProfileVault(profileId, opts): Promise<CreateVaultResult> {
      if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
      if (opts?.additionalFactor !== undefined) return fail("unsupported_factor"); // no reviewed Argon2id yet
      const l = await load(profileId);
      if (l.kind === "ok") return fail("already_exists");
      if (l.kind === "corrupt") return fail("corrupt"); // never re-created over existing data
      if (l.kind === "error") return fail("storage_failed");
      const startEpoch = epoch(profileId);
      const raw = randomBytes(c, PVK_BYTES);
      try {
        const built = await buildWrapper(profileId, raw, opts?.wrapping ?? "auto", opts?.passkeyUser);
        if (!built.ok) return built;
        const rec: WrappedRecord = { ...built.rec, generation: 1 };
        let db: IDBDatabase | undefined;
        try { db = await openOrCreate(env.indexedDB, dbName(profileId)); } catch { return fail("storage_failed"); }
        if (db === undefined) return fail("corrupt");
        let occupied: "already_exists" | "corrupt" | undefined;
        try {
          // One transaction: refuse if anything appeared meanwhile, else write record (and device key) together.
          const tx = db.transaction([...STORES], "readwrite");
          const done = committed(tx);
          const wrappedStore = tx.objectStore(STORE_WRAPPED);
          const keyStore = tx.objectStore(STORE_DEVICE_KEY);
          const kvCount = tx.objectStore(STORE_KV).count();
          const keyCount = keyStore.count();
          const recCount = wrappedStore.count();
          recCount.onsuccess = () => {
            if (recCount.result + keyCount.result + kvCount.result > 0) {
              occupied = recCount.result > 0 ? "already_exists" : "corrupt";
              tx.abort();
              return;
            }
            wrappedStore.add(rec, WRAPPED_ID);
            if (built.deviceKey !== undefined) keyStore.add(built.deviceKey, DEVICE_KEY_ID);
          };
          await done;
        } catch {
          return fail(occupied ?? "storage_failed");
        } finally {
          db.close();
        }
        // Stays unlocked after enrollment unless locked meanwhile; a failed import just leaves it locked.
        if (epoch(profileId) === startEpoch) { try { await install(profileId, raw); } catch { /* locked */ } }
        return { ok: true, wrapping: infoOf(rec) };
      } catch {
        return fail("crypto_failed");
      } finally {
        raw.fill(0);
      }
    },

    unlock(profileId): Promise<UnlockResult> {
      if (!isValidProfileId(profileId)) return Promise.resolve(fail("invalid_profile_id"));
      if (sessions.has(profileId)) return Promise.resolve({ ok: true });
      let p = unlocking.get(profileId);
      if (p === undefined) {
        p = doUnlock(profileId).finally(() => { unlocking.delete(profileId); });
        unlocking.set(profileId, p);
      }
      return p;
    },

    lock(profileId): void {
      // Drops the only references to the PVK base key and subkeys. JS cannot guarantee the engine erases them.
      epochs.set(profileId, epoch(profileId) + 1);
      sessions.delete(profileId);
    },

    async seal(profileId, purpose, bytes): Promise<SealResult> {
      if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
      if (!isVaultPurpose(purpose)) return fail("invalid_purpose");
      const s = sessions.get(profileId);
      if (s === undefined) return fail("locked");
      try {
        return { ok: true, sealed: await sealBytes(c, await subkey(s, purpose), profileId, purpose, new Uint8Array(bytes)) };
      } catch {
        return fail("crypto_failed");
      }
    },

    async open(profileId, purpose, sealed): Promise<OpenResult> {
      if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
      if (!isVaultPurpose(purpose)) return fail("invalid_purpose");
      const s = sessions.get(profileId);
      if (s === undefined) return fail("locked");
      const parsed = parseSealed(sealed);
      if (!parsed.ok) return fail(parsed.reason);
      if (parsed.purpose !== purpose) return fail("wrong_purpose");
      let bytes: Bytes | undefined;
      try { bytes = await openBytes(c, await subkey(s, purpose), profileId, parsed); } catch { bytes = undefined; }
      return bytes === undefined ? fail("integrity_failure") : { ok: true, bytes };
    },

    async deleteProfileVault(profileId): Promise<DeleteVaultResult> {
      if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
      vault.lock(profileId);
      const before = await load(profileId);
      const name = dbName(profileId);
      // Our connections close on versionchange; if another holder blocks, wait a bounded time, then report.
      const outcome = await deleteDb(env.indexedDB, name, () => env.sleep(DELETE_WAIT_MS));
      if (outcome === "error") return fail("storage_failed");
      if (outcome === "blocked" || (await openExisting(env.indexedDB, name)).kind !== "absent") return fail("delete_unverified");
      if (before.kind === "ok") await signalUnknown(before.rec.prf);
      return { ok: true };
    },

    async rotateWrapping(profileId, opts?: RotateOptions): Promise<RotateResult> {
      if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
      const l = await load(profileId);
      if (l.kind !== "ok") return fail(loadFailure(l.kind));
      const u = await unwrapStored(l.rec, l.deviceKey, "rotate_unwrap");
      if (!u.ok) return u;
      try {
        const built = await buildWrapper(profileId, u.raw, opts?.wrapping ?? "auto", opts?.passkeyUser);
        if (!built.ok) return built;
        const rec: WrappedRecord = { ...built.rec, generation: l.rec.generation + 1 };
        const ex = await openExisting(env.indexedDB, dbName(profileId));
        if (ex.kind !== "open") return fail(ex.kind === "absent" ? "not_found" : ex.kind === "unusable" ? "corrupt" : "storage_failed");
        const cas = { conflict: false };
        try {
          // Compare-and-swap in one transaction: the old wrapper (record + device key) is replaced atomically.
          const tx = ex.db.transaction([STORE_WRAPPED, STORE_DEVICE_KEY], "readwrite");
          const done = committed(tx);
          const wrappedStore = tx.objectStore(STORE_WRAPPED);
          const keyStore = tx.objectStore(STORE_DEVICE_KEY);
          const cur = wrappedStore.get(WRAPPED_ID);
          cur.onsuccess = () => {
            const now = parseRecord(cur.result, profileId);
            if (now === undefined || now.generation !== l.rec.generation || !equalBytes(now.nonce, l.rec.nonce)) {
              cas.conflict = true;
              tx.abort();
              return;
            }
            wrappedStore.put(rec, WRAPPED_ID);
            if (built.deviceKey === undefined) keyStore.delete(DEVICE_KEY_ID);
            else keyStore.put(built.deviceKey, DEVICE_KEY_ID);
          };
          await done;
        } catch {
          return fail(cas.conflict ? "conflict" : "storage_failed");
        } finally {
          ex.db.close();
        }
        if (l.rec.prf !== undefined && (rec.prf === undefined || !equalBytes(rec.prf.credentialId, l.rec.prf.credentialId))) {
          await signalUnknown(l.rec.prf);
        }
        return { ok: true, wrapping: infoOf(rec) };
      } catch {
        return fail("crypto_failed");
      } finally {
        u.raw.fill(0);
      }
    },
  };
  return vault;
}
