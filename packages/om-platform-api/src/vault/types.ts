// DeviceVault (DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md §4-5, ADR-0172). Shell-agnostic: no DOM types.
// The vault never hands its ProfileVaultKey or any subkey to callers; results carry machine reason codes only.
import type { ProfileId } from "../core/ids";

export const VAULT_PURPOSES = ["local-db", "search-index", "draft-store", "attachment-cache", "diagnostics-seal"] as const;
export type VaultPurpose = (typeof VAULT_PURPOSES)[number];
export const isVaultPurpose = (v: unknown): v is VaultPurpose =>
  typeof v === "string" && (VAULT_PURPOSES as readonly string[]).includes(v);

/** `evicted` is reported only when the caller says the profile is known (`{ known: true }`). */
export type VaultStatus = "absent" | "locked" | "unlocked" | "evicted" | "corrupt";

/** Method 1 = WebAuthn PRF, method 2 = non-extractable WebCrypto key. Method 3 (passphrase) is a layer, never a sole wrapper. */
export type WrappingMethod = "webauthn-prf" | "device-key";
export type WrappingPreference = "auto" | "device-key";

/** Why the method was chosen; selection is by test, never by capability flag (R-391). */
export type WrappingReason =
  | "prf_verified"
  | "device_key_requested"
  | "webauthn_absent"
  | "prf_not_enabled"
  | "prf_no_output"
  | "webauthn_create_failed"
  | "webauthn_get_failed";

export type CredentialBackup = "device_bound" | "synced" | "unknown";

export interface VaultWrappingInfo {
  readonly method: WrappingMethod;
  readonly reason: WrappingReason;
  /** Method 1 only, from the authenticator-data BE flag; `unknown` is treated as synced. */
  readonly credentialBackup?: CredentialBackup;
  /** ADR-0129 rule 15: true when a synced passkey's PRF wraps the vault. The UI must state the property. */
  readonly reducedAtRestProperty: boolean;
}

/** Method 3 slot. No reviewed memory-hard KDF (Argon2id) is available yet, so every request is refused. */
export interface PassphraseFactor {
  readonly kind: "passphrase";
  readonly secret: Uint8Array;
}

export interface CreateVaultOptions {
  readonly wrapping?: WrappingPreference;
  readonly additionalFactor?: PassphraseFactor;
  /** Shown by the passkey provider; supplied by the caller (copy stays out of the vault). */
  readonly passkeyUser?: { readonly name: string; readonly displayName: string };
}

export type RotateOptions = Omit<CreateVaultOptions, "additionalFactor">;

/** Which step needed the user, so the UI can ask for a fresh gesture for exactly that prompt. */
export type VaultStage = "enroll_create" | "enroll_verify" | "unlock" | "rotate_unwrap";

export type VaultErrorReason =
  | "invalid_profile_id"
  | "invalid_purpose"
  | "invalid_key"
  | "not_found"
  | "corrupt"
  | "locked"
  | "already_exists"
  | "conflict"
  | "needs_user_gesture"
  | "ceremony_failed"
  | "prf_unavailable"
  | "unwrap_failed"
  | "unsupported_factor"
  | "unsupported_platform"
  | "malformed"
  | "unsupported_version"
  | "wrong_purpose"
  | "integrity_failure"
  | "crypto_failed"
  | "storage_failed"
  | "delete_unverified";

export interface VaultError<R extends VaultErrorReason> {
  readonly ok: false;
  readonly reason: R;
  readonly stage?: VaultStage;
}

type Ceremony = "needs_user_gesture" | "ceremony_failed" | "prf_unavailable";

export type CreateVaultResult =
  | { readonly ok: true; readonly wrapping: VaultWrappingInfo }
  | VaultError<"invalid_profile_id" | "already_exists" | "corrupt" | "unsupported_factor" | "unsupported_platform"
      | "needs_user_gesture" | "crypto_failed" | "storage_failed">;

export type UnlockResult =
  | { readonly ok: true }
  | VaultError<"invalid_profile_id" | "not_found" | "corrupt" | "conflict" | "unwrap_failed" | "crypto_failed" | "storage_failed"
      | Ceremony>;

export type SealResult =
  | { readonly ok: true; readonly sealed: Uint8Array }
  | VaultError<"invalid_profile_id" | "invalid_purpose" | "locked" | "crypto_failed">;

export type OpenResult =
  | { readonly ok: true; readonly bytes: Uint8Array }
  | VaultError<"invalid_profile_id" | "invalid_purpose" | "locked" | "malformed" | "unsupported_version" | "wrong_purpose"
      | "integrity_failure">;

export type DeleteVaultResult =
  | { readonly ok: true }
  | VaultError<"invalid_profile_id" | "storage_failed" | "delete_unverified">;

export type RotateResult =
  | { readonly ok: true; readonly wrapping: VaultWrappingInfo }
  | VaultError<"invalid_profile_id" | "not_found" | "corrupt" | "conflict" | "unwrap_failed" | "crypto_failed"
      | "storage_failed" | Ceremony>;

export type WrappingInfoResult =
  | { readonly ok: true; readonly wrapping: VaultWrappingInfo }
  | VaultError<"invalid_profile_id" | "not_found" | "corrupt" | "storage_failed">;

/** Presence hints only. They never select a wrapping method (R-391). */
export interface VaultCapabilities {
  readonly webauthnPresent: boolean;
  readonly deviceKeyPresent: boolean;
  readonly passphraseFactor: "unavailable_no_reviewed_kdf";
  /** The vault never relies on `storage.persist()` (ADR-0172). */
  readonly reliesOnPersistentStorage: false;
}

export interface DeviceVault {
  capabilities(): VaultCapabilities;
  status(profileId: ProfileId, opts?: { readonly known?: boolean }): Promise<VaultStatus>;
  wrapping(profileId: ProfileId): Promise<WrappingInfoResult>;
  /** Fresh random PVK and fresh wrapper; never depends on another origin's credentials (R-392). Refuses existing data. */
  createProfileVault(profileId: ProfileId, opts?: CreateVaultOptions): Promise<CreateVaultResult>;
  unlock(profileId: ProfileId): Promise<UnlockResult>;
  lock(profileId: ProfileId): void;
  seal(profileId: ProfileId, purpose: VaultPurpose, bytes: Uint8Array): Promise<SealResult>;
  open(profileId: ProfileId, purpose: VaultPurpose, sealed: Uint8Array): Promise<OpenResult>;
  deleteProfileVault(profileId: ProfileId): Promise<DeleteVaultResult>;
  /** Re-wraps the same PVK under a fresh wrapper; sealed data is not re-encrypted. */
  rotateWrapping(profileId: ProfileId, opts?: RotateOptions): Promise<RotateResult>;
}

export type KvResult = { readonly ok: true } | VaultError<VaultErrorReason>;
export type KvGetResult = { readonly ok: true; readonly value: Uint8Array | undefined } | VaultError<VaultErrorReason>;
export type KvListResult = { readonly ok: true; readonly keys: readonly string[] } | VaultError<VaultErrorReason>;

/** Profile-scoped key/value store. Keys are not secret; every value is sealed with purpose `local-db`. */
export interface EncryptedKv {
  put(key: string, value: Uint8Array): Promise<KvResult>;
  get(key: string): Promise<KvGetResult>;
  delete(key: string): Promise<KvResult>;
  list(): Promise<KvListResult>;
}
