// WebDeviceVault (DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md §4-5, 2026-10-01 resolution; ADR-0172 R-391/R-392).
import type {
  CreateVaultOptions, CreateVaultResult, CredentialBackup, DeleteVaultResult, DeviceVault, OpenResult, ProfileId,
  PassphraseFactor, RotateOptions, RotateResult, SealResult, UnlockOptions, UnlockResult, VaultCapabilities, VaultError, VaultErrorReason,
  VaultPurpose, VaultStage, VaultStatus, VaultWrappingInfo, WrappingInfoResult, WrappingMethod, WrappingPreference, WrappingReason,
} from "@om/platform-api";
import { isVaultPurpose } from "@om/platform-api";
import type { Bytes, VaultEnv } from "./env";
import {
  PASS_INNER_BYTES, PASS_KDF_V1, PASS_SALT_BYTES, PRF_OUTPUT_BYTES, PRF_SALT_BYTES, PVK_BYTES, SEAL_FORMAT, WRAP_FORMAT_PASSPHRASE,
  argon2idRaw, deriveSubkey, equalBytes, importPvk, isValidProfileId, newDeviceKey, normalizePassphrase, openBytes,
  openPassphraseInner, parseSealed, passphraseInner, passphraseKek, prfInput, prfKek, randomBytes, sealBytes, unwrapPvk, wrapPvk,
} from "./crypto";
import type { Argon2Cost, PassphraseHeader, WrappedPvk } from "./crypto";
import {
  DEVICE_KEY_ID, STORES, STORE_DEVICE_KEY, STORE_KV, STORE_WRAPPED, WRAPPED_ID, committed, dbName, deleteDb,
  openExisting, openOrCreate, readRecords,
} from "./idb";
import { base64url, createVaultCredential, evaluateVaultPrf } from "./webauthn";

const DELETE_WAIT_MS = 5000;
const DEFAULT_PASSKEY_USER = { name: "om-vault", displayName: "OM" };
const MAX_PASSPHRASE_TARGET_MS = 10_000;

/**
 * Argon2id cost policy for the passphrase layer (KDF version 1).
 * - `standard` is what a new record gets: 64 MiB, 3 passes, 1 lane = the RFC 9106 §4 second recommended option's
 *   memory and passes, with p = 1 because pure-JS Argon2 runs on one thread (more lanes would cost us time without
 *   costing an attacker anything). Measured ~1 s in Node on Apple silicon; expect several seconds on low-end phones.
 * - `floor` is the least a stored record may claim (OWASP's Argon2id minimum, 19 MiB / 2 passes); a record below it,
 *   or above `max`, is `corrupt`, so a tampered header can neither downgrade the cost nor force a huge allocation.
 * - Calibration (`passphraseTargetMs`) only ever raises `t` above `standard`, up to `max`.
 */
export interface KdfPolicy { readonly floor: Argon2Cost; readonly standard: Argon2Cost; readonly max: Argon2Cost }
export const PRODUCTION_KDF_POLICY: KdfPolicy = Object.freeze({
  floor: Object.freeze({ m: 19_456, t: 2, p: 1 }),
  standard: Object.freeze({ m: 65_536, t: 3, p: 1 }),
  max: Object.freeze({ m: 262_144, t: 16, p: 1 }),
});

interface PrfRecord {
  readonly credentialId: Bytes;
  readonly salt: Bytes;
  readonly rpId: string;
  readonly backup: CredentialBackup;
}

/** Stored beside the data; the PVK only ever appears here wrapped. Format 2 nests the passphrase layer inside. */
interface WrappedRecord extends WrappedPvk {
  readonly format: typeof SEAL_FORMAT | typeof WRAP_FORMAT_PASSPHRASE;
  readonly profileId: string;
  readonly method: WrappingMethod;
  readonly reason: WrappingReason;
  readonly generation: number;
  readonly prf?: PrfRecord;
  readonly passphrase?: PassphraseHeader;
}

/** What a wrapper build or rotation writes; the outer (method) key is chosen first, the payload layered under it. */
type Layered = Pick<WrappedRecord, "format" | "nonce" | "ciphertext" | "passphrase">;
type Layer = (kek: CryptoKey, method: WrappingMethod) => Promise<Layered>;

const REASONS: readonly WrappingReason[] = [
  "prf_verified", "device_key_requested", "webauthn_absent", "prf_not_enabled", "prf_no_output", "webauthn_create_failed",
  "webauthn_get_failed",
];
const BACKUPS: readonly CredentialBackup[] = ["device_bound", "synced", "unknown"];

const asRecord = (v: unknown): Record<string, unknown> | undefined =>
  typeof v === "object" && v !== null ? (v as Record<string, unknown>) : undefined;
const bytesOf = (v: unknown, len?: number): Bytes | undefined =>
  v instanceof Uint8Array && (len === undefined || v.length === len) ? new Uint8Array(v) : undefined;

const inRange = (v: unknown, lo: number, hi: number): v is number => typeof v === "number" && Number.isSafeInteger(v) && v >= lo && v <= hi;

/** Only KDF version 1 with a cost inside [floor, max] (per field) is accepted. */
function parsePassphrase(v: unknown, policy: KdfPolicy): PassphraseHeader | undefined {
  const r = asRecord(v);
  const salt = bytesOf(r?.["salt"], PASS_SALT_BYTES);
  const { floor, max } = policy;
  if (r?.["kdf"] !== PASS_KDF_V1 || salt === undefined || !inRange(r["m"], floor.m, max.m) || !inRange(r["t"], floor.t, max.t)
    || !inRange(r["p"], floor.p, max.p) || r["m"] < 8 * r["p"]) return undefined;
  return { kdf: PASS_KDF_V1, m: r["m"], t: r["t"], p: r["p"], salt };
}

function parseRecord(v: unknown, profileId: string, policy: KdfPolicy): WrappedRecord | undefined {
  const r = asRecord(v);
  if (r === undefined || r["profileId"] !== profileId) return undefined;
  const format = r["format"];
  let passphrase: PassphraseHeader | undefined;
  if (format === WRAP_FORMAT_PASSPHRASE) {
    passphrase = parsePassphrase(r["passphrase"], policy);
    if (passphrase === undefined) return undefined;
  } else if (format !== SEAL_FORMAT || r["passphrase"] !== undefined) return undefined;
  const method = r["method"];
  const reason = r["reason"];
  const generation = r["generation"];
  const nonce = bytesOf(r["nonce"], 12);
  const ciphertext = bytesOf(r["ciphertext"], (passphrase === undefined ? PVK_BYTES : PASS_INNER_BYTES) + 16);
  if ((method !== "webauthn-prf" && method !== "device-key") || !REASONS.includes(reason as WrappingReason)
    || typeof generation !== "number" || !Number.isSafeInteger(generation) || nonce === undefined || ciphertext === undefined) return undefined;
  const base = {
    format, profileId, method, reason: reason as WrappingReason, generation, nonce, ciphertext, ...(passphrase === undefined ? {} : { passphrase }),
  } as const;
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

const infoOf = (rec: WrappedRecord): VaultWrappingInfo => {
  const passphraseProtected = rec.passphrase !== undefined;
  return rec.prf === undefined
    ? { method: rec.method, reason: rec.reason, reducedAtRestProperty: false, passphraseProtected }
    : { method: rec.method, reason: rec.reason, credentialBackup: rec.prf.backup, reducedAtRestProperty: rec.prf.backup !== "device_bound", passphraseProtected };
};

/** Normalised passphrase bytes; always zeroes the caller's buffer. `invalid` = empty or not UTF-8. */
type Taken = { readonly k: "none" | "invalid" | "unsupported" } | { readonly k: "ok"; readonly pw: Bytes };
function takePassphrase(f: PassphraseFactor | undefined): Taken {
  if (f === undefined) return { k: "none" };
  // Callers may be untyped JS: check the shape at runtime.
  const r = f as { readonly kind?: unknown; readonly secret?: unknown };
  if (r.kind !== "passphrase") return { k: "unsupported" };
  if (!(r.secret instanceof Uint8Array)) return { k: "invalid" };
  const pw = normalizePassphrase(r.secret);
  return pw === undefined ? { k: "invalid" } : { k: "ok", pw };
}
const wipe = (t: Taken): void => { if (t.k === "ok") t.pw.fill(0); };

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
  return createVaultWithKdfPolicy(env, PRODUCTION_KDF_POLICY);
}

/**
 * Internal: lets tests run Argon2id with tiny costs. Not exported from the package entry point, so production code
 * can only reach `createWebDeviceVault` and `PRODUCTION_KDF_POLICY`.
 */
export function createVaultWithKdfPolicy(env: VaultEnv, policy: KdfPolicy): DeviceVault {
  const c = env.crypto;
  const parse = (v: unknown, profileId: string): WrappedRecord | undefined => parseRecord(v, profileId, policy);
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
      const rec = parse(r.wrapped, profileId);
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
  async function buildWrapper(profileId: ProfileId, layer: Layer, pref: WrappingPreference, user: CreateVaultOptions["passkeyUser"]): Promise<Built> {
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
            const w = await layer(kek, "webauthn-prf");
            return { ok: true, deviceKey: undefined, rec: {
              ...w, profileId, method: "webauthn-prf", reason: "prf_verified", prf: { credentialId, salt, rpId: env.rpId, backup },
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
      const w = await layer(key, "device-key");
      return { ok: true, deviceKey: key, rec: { ...w, profileId, method: "device-key", reason } };
    } catch {
      return fail("crypto_failed");
    }
  }

  /** No passphrase: the raw PVK directly under the method key (format 1). */
  const plainLayer = (profileId: ProfileId, raw: Bytes): Layer => async (kek, method) =>
    ({ format: SEAL_FORMAT, ...(await wrapPvk(c, kek, raw, profileId, method)) });

  /** An existing passphrase inner layer re-wrapped under a new method key (rotation keeps the passphrase). */
  const keptPassphraseLayer = (profileId: ProfileId, inner: Bytes, h: PassphraseHeader): Layer => async (kek, method) =>
    ({ format: WRAP_FORMAT_PASSPHRASE, passphrase: h, ...(await wrapPvk(c, kek, inner, profileId, method, h)) });

  /**
   * New passphrase layer. Runs after the WebAuthn ceremonies (inside `buildWrapper`), so the slow derivation never
   * spends the user activation a ceremony needs.
   */
  const newPassphraseLayer = (profileId: ProfileId, raw: Bytes, pw: Bytes, targetMs: number | undefined): Layer => async (kek, method) => {
    const h: PassphraseHeader = { kdf: PASS_KDF_V1, ...(await chooseCost(targetMs)), salt: randomBytes(c, PASS_SALT_BYTES) };
    const inner = await passphraseInner(c, await passphraseKek(c, pw, h), raw, profileId, h);
    try { return await keptPassphraseLayer(profileId, inner, h)(kek, method); } finally { inner.fill(0); }
  };

  /** `standard`, or more passes when the caller's time budget allows; never below `standard`, never above `max`. */
  async function chooseCost(targetMs: number | undefined): Promise<Argon2Cost> {
    const std = policy.standard;
    if (targetMs === undefined || !Number.isFinite(targetMs) || targetMs <= 0) return std;
    const probe = randomBytes(c, PASS_SALT_BYTES);
    const start = env.now();
    (await argon2idRaw(probe, probe, { ...std, t: 1 })).fill(0);
    const perPass = env.now() - start;
    if (!(perPass > 0)) return std;
    const t = Math.ceil(Math.min(targetMs, MAX_PASSPHRASE_TARGET_MS) / perPass);
    return { ...std, t: Math.min(policy.max.t, Math.max(std.t, t)) };
  }

  /** Raw PVK (format 1) or the passphrase inner layer (format 2) through the stored method wrapper (caller zeroes it). */
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
    const raw = await unwrapPvk(c, kek, rec, rec.profileId, rec.method, rec.passphrase);
    return raw === undefined ? fail("unwrap_failed") : { ok: true, raw };
  }

  async function install(profileId: ProfileId, raw: Bytes): Promise<void> {
    sessions.set(profileId, { base: await importPvk(c, raw), subkeys: new Map() });
  }

  async function signalUnknown(prf: PrfRecord | undefined): Promise<void> {
    if (prf === undefined || env.signalUnknownCredential === undefined) return;
    try { await env.signalUnknownCredential({ rpId: prf.rpId, credentialId: base64url(prf.credentialId) }); } catch { /* best effort */ }
  }

  async function doUnlock(profileId: ProfileId, pass: Taken): Promise<UnlockResult> {
    try {
      const startEpoch = epoch(profileId);
      const l = await load(profileId);
      if (l.kind !== "ok") return fail(loadFailure(l.kind));
      const h = l.rec.passphrase;
      // Checked before any ceremony, so a missing passphrase never costs the user a passkey prompt.
      if (h !== undefined && pass.k !== "ok") return fail("passphrase_required");
      const u = await unwrapStored(l.rec, l.deviceKey, "unlock");
      if (!u.ok) return u;
      let raw: Bytes | undefined = u.raw;
      if (h !== undefined && pass.k === "ok") {
        // The outer layer authenticated the inner bytes, so an inner failure means the passphrase is wrong.
        // This is a local check on a local copy: nothing rate-limits guesses against a copied profile.
        try {
          raw = await openPassphraseInner(c, await passphraseKek(c, pass.pw, h), u.raw, profileId, h);
        } catch {
          return fail("crypto_failed");
        } finally {
          u.raw.fill(0);
        }
        if (raw === undefined) return fail("wrong_passphrase");
      }
      if (epoch(profileId) !== startEpoch) { raw.fill(0); return fail("conflict"); } // locked or deleted meanwhile: lock wins
      try { await install(profileId, raw); } catch { return fail("crypto_failed"); }
      return { ok: true };
    } finally {
      wipe(pass);
    }
  }

  async function create(profileId: ProfileId, opts: CreateVaultOptions | undefined, pass: Taken): Promise<CreateVaultResult> {
    const l = await load(profileId);
    if (l.kind === "ok") return fail("already_exists");
    if (l.kind === "corrupt") return fail("corrupt"); // never re-created over existing data
    if (l.kind === "error") return fail("storage_failed");
    const startEpoch = epoch(profileId);
    const raw = randomBytes(c, PVK_BYTES);
    try {
      const layer = pass.k === "ok" ? newPassphraseLayer(profileId, raw, pass.pw, opts?.passphraseTargetMs) : plainLayer(profileId, raw);
      const built = await buildWrapper(profileId, layer, opts?.wrapping ?? "auto", opts?.passkeyUser);
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
        passphraseFactor: "available",
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
      const pass = takePassphrase(opts?.additionalFactor);
      try {
        if (!isValidProfileId(profileId)) return fail("invalid_profile_id");
        if (pass.k === "unsupported") return fail("unsupported_factor");
        if (pass.k === "invalid") return fail("passphrase_required");
        return await create(profileId, opts, pass);
      } finally {
        wipe(pass);
      }
    },

    unlock(profileId, opts?: UnlockOptions): Promise<UnlockResult> {
      const pass = takePassphrase(opts?.passphrase);
      if (!isValidProfileId(profileId)) { wipe(pass); return Promise.resolve(fail("invalid_profile_id")); }
      if (sessions.has(profileId)) { wipe(pass); return Promise.resolve({ ok: true }); }
      let p = unlocking.get(profileId);
      if (p === undefined) {
        p = doUnlock(profileId, pass).finally(() => { unlocking.delete(profileId); });
        unlocking.set(profileId, p);
      } else wipe(pass); // joins the in-flight unlock; its own passphrase is not used
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
        // A passphrase layer is kept byte-for-byte (same salt, cost and inner ciphertext): only the method wrapper
        // changes, so rotation needs no passphrase and never sees the PVK of a passphrase-protected vault.
        const h = l.rec.passphrase;
        const layer = h === undefined ? plainLayer(profileId, u.raw) : keptPassphraseLayer(profileId, u.raw, h);
        const built = await buildWrapper(profileId, layer, opts?.wrapping ?? "auto", opts?.passkeyUser);
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
            const now = parse(cur.result, profileId);
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
