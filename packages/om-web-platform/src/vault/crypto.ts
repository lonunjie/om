// WebDeviceVault primitives: WebCrypto (HKDF-SHA-256, AES-256-GCM, SHA-256) plus Argon2id from the audited
// @noble/hashes (pure JS; WebCrypto has no memory-hard KDF). No hand-rolled crypto.
import { argon2idAsync } from "@noble/hashes/argon2.js";
import type { VaultPurpose, WrappingMethod } from "@om/platform-api";
import { VAULT_PURPOSES } from "@om/platform-api";
import type { Bytes } from "./env";

// Implementation-private local labels (DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION §4-5): not registered, never on the wire.
const SEAL_AAD_LABEL = "om:v1:vault-seal";
const WRAP_AAD_LABEL = "om:v1:pvk-wrap";
/** Vault PRF label; distinct from every RecoveryRoot salt/label so one PRF output never serves both (ADR-0129). */
const PRF_LABEL = "om:v1:vault-prf";
const PRF_KEK_LABEL = "om:v1:vault-prf-kek";
const PASS_WRAP_AAD_LABEL = "om:v1:pvk-wrap-passphrase";
const PASS_KEK_LABEL = "om:v1:vault-passphrase-kek";

export const PVK_BYTES = 32;
export const PRF_SALT_BYTES = 32;
export const PRF_OUTPUT_BYTES = 32;
const NONCE_BYTES = 12;
const TAG_BYTES = 16;
export const SEAL_FORMAT = 1;
/** Wrapped-PVK record format with the method 3 passphrase layer. Format 1 records (no passphrase) are unchanged. */
export const WRAP_FORMAT_PASSPHRASE = 2;
const MAGIC = [0x4f, 0x4d, 0x56, 0x53]; // "OMVS"
const HEADER_BYTES = MAGIC.length + 2; // magic, format, purpose code

/** ProfileId is opaque; it is validated before it ever names a database or enters AAD. */
const PROFILE_ID = /^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/;
export const isValidProfileId = (p: string): boolean => PROFILE_ID.test(p);

const encoder = new TextEncoder();
export const utf8 = (s: string): Bytes => encoder.encode(s);

export function concat(...parts: readonly (Uint8Array | readonly number[])[]): Bytes {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) { out.set(p, at); at += p.length; }
  return out;
}

export const randomBytes = (c: Crypto, n: number): Bytes => c.getRandomValues(new Uint8Array(n));
const zeroSalt = (): Bytes => new Uint8Array(32);

/**
 * Imports the PVK as a non-extractable HKDF base key, then zeroes our raw copy. JavaScript cannot guarantee
 * zeroization (the engine or WebCrypto may hold other copies); this only bounds the lifetime of the buffer we own.
 */
export async function importPvk(c: Crypto, raw: Bytes): Promise<CryptoKey> {
  try {
    return await c.subtle.importKey("raw", raw, "HKDF", false, ["deriveKey"]);
  } finally {
    raw.fill(0);
  }
}

export const deriveSubkey = (c: Crypto, base: CryptoKey, purpose: VaultPurpose): Promise<CryptoKey> =>
  c.subtle.deriveKey(
    { name: "HKDF", hash: "SHA-256", salt: zeroSalt(), info: utf8(`om:v1:${purpose}`) },
    base, { name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"],
  );

const sealAad = (header: Uint8Array, profileId: string): Bytes => concat(utf8(SEAL_AAD_LABEL), [0], header, utf8(profileId));

/** Versioned, self-describing blob: magic | format | purpose | 96-bit random nonce | ciphertext+tag. */
export async function sealBytes(c: Crypto, key: CryptoKey, profileId: string, purpose: VaultPurpose, plaintext: Bytes): Promise<Bytes> {
  const header = new Uint8Array([...MAGIC, SEAL_FORMAT, VAULT_PURPOSES.indexOf(purpose) + 1]);
  const nonce = randomBytes(c, NONCE_BYTES);
  const ct = await c.subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: sealAad(header, profileId), tagLength: 128 }, key, plaintext);
  return concat(header, nonce, new Uint8Array(ct));
}

export type ParsedSeal =
  | { readonly ok: true; readonly purpose: VaultPurpose; readonly header: Bytes; readonly nonce: Bytes; readonly ct: Bytes }
  | { readonly ok: false; readonly reason: "malformed" | "unsupported_version" };

export function parseSealed(blob: Uint8Array): ParsedSeal {
  if (blob.length < HEADER_BYTES + NONCE_BYTES + TAG_BYTES || MAGIC.some((b, i) => blob[i] !== b)) return { ok: false, reason: "malformed" };
  if (blob[MAGIC.length] !== SEAL_FORMAT) return { ok: false, reason: "unsupported_version" };
  const purpose = VAULT_PURPOSES[(blob[MAGIC.length + 1] ?? 0) - 1];
  if (purpose === undefined) return { ok: false, reason: "malformed" };
  return {
    ok: true,
    purpose,
    header: blob.slice(0, HEADER_BYTES),
    nonce: blob.slice(HEADER_BYTES, HEADER_BYTES + NONCE_BYTES),
    ct: blob.slice(HEADER_BYTES + NONCE_BYTES),
  };
}

/** `undefined` on any authentication failure (tamper, wrong key, wrong profile/purpose in AAD). */
export async function openBytes(c: Crypto, key: CryptoKey, profileId: string, s: Extract<ParsedSeal, { ok: true }>): Promise<Bytes | undefined> {
  try {
    return new Uint8Array(await c.subtle.decrypt({ name: "AES-GCM", iv: s.nonce, additionalData: sealAad(s.header, profileId), tagLength: 128 }, key, s.ct));
  } catch {
    return undefined;
  }
}

export interface WrappedPvk { readonly nonce: Bytes; readonly ciphertext: Bytes }

// ---- Method 3: passphrase layer (Argon2id, RFC 9106) ----------------------------------------------------------------

/** Argon2id cost: `m` memory in KiB, `t` passes, `p` lanes. */
export interface Argon2Cost { readonly m: number; readonly t: number; readonly p: number }

/** KDF version 1 = Argon2id v0x13, 32-byte output, NFKC-normalised UTF-8 passphrase, 16-byte random salt. */
export const PASS_KDF_V1 = 1;
export const PASS_SALT_BYTES = 16;
/** Stored in the wrapped record and authenticated in both layers' AAD, so the cost cannot be downgraded. */
export interface PassphraseHeader extends Argon2Cost { readonly kdf: typeof PASS_KDF_V1; readonly salt: Bytes }

/** Inner layer: nonce | AES-GCM(passphrase KEK, PVK). The outer (method 1/2) layer encrypts these bytes. */
export const PASS_INNER_BYTES = NONCE_BYTES + PVK_BYTES + TAG_BYTES;

/** kdf(1) | m(u32be) | t(u32be) | p(1) | saltLen(1) | salt: fixed layout, so the AAD encoding is unambiguous. */
export function passphraseHeaderBytes(h: PassphraseHeader): Bytes {
  const b = new Uint8Array(11);
  const dv = new DataView(b.buffer);
  dv.setUint8(0, h.kdf);
  dv.setUint32(1, h.m);
  dv.setUint32(5, h.t);
  dv.setUint8(9, h.p);
  dv.setUint8(10, h.salt.length);
  return concat(b, h.salt);
}

const fatalUtf8 = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

/**
 * Passphrase bytes as hashed: UTF-8 -> Unicode NFKC -> UTF-8. NFKC (one of the two forms NIST SP 800-63B §5.1.1.2
 * allows; RFC 8265 OpaqueString uses NFC) is chosen so the same visible passphrase typed through different
 * keyboards/IMEs (composed vs decomposed accents, full-width digits, no-break spaces) derives the same key; the
 * entropy lost to compatibility folding is negligible. Fixed by KDF version 1; changing it needs a new version.
 * Zeroes `secret`. `undefined` when the input is empty or not UTF-8. The intermediate JS string cannot be erased.
 */
export function normalizePassphrase(secret: Uint8Array): Bytes | undefined {
  try {
    const s = fatalUtf8.decode(secret).normalize("NFKC");
    return s.length === 0 ? undefined : utf8(s);
  } catch {
    return undefined;
  } finally {
    secret.fill(0);
  }
}

/** Raw Argon2id output (caller zeroes it). Async API with `asyncTick`, so a long derivation yields to the event loop. */
export async function argon2idRaw(pw: Bytes, salt: Bytes, cost: Argon2Cost): Promise<Bytes> {
  const out = await argon2idAsync(pw, salt, { m: cost.m, t: cost.t, p: cost.p, dkLen: 32, asyncTick: 10 });
  return new Uint8Array(out);
}

/** Passphrase KEK = HKDF-SHA-256(Argon2id(pw), salt, label || header). Does not zero `pw`. */
export async function passphraseKek(c: Crypto, pw: Bytes, h: PassphraseHeader): Promise<CryptoKey> {
  const out = await argon2idRaw(pw, h.salt, h);
  try {
    const base = await c.subtle.importKey("raw", out, "HKDF", false, ["deriveKey"]);
    return await c.subtle.deriveKey({ name: "HKDF", hash: "SHA-256", salt: h.salt, info: concat(utf8(PASS_KEK_LABEL), [0], passphraseHeaderBytes(h)) },
      base, { name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
  } finally {
    out.fill(0);
  }
}

const passInnerAad = (profileId: string, h: PassphraseHeader): Bytes =>
  concat(utf8(PASS_WRAP_AAD_LABEL), [0, WRAP_FORMAT_PASSPHRASE, 0], passphraseHeaderBytes(h), utf8(profileId));

/** Inner layer bytes (caller zeroes them). */
export async function passphraseInner(c: Crypto, kek: CryptoKey, raw: Bytes, profileId: string, h: PassphraseHeader): Promise<Bytes> {
  const nonce = randomBytes(c, NONCE_BYTES);
  const ct = await c.subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: passInnerAad(profileId, h), tagLength: 128 }, kek, raw);
  return concat(nonce, new Uint8Array(ct));
}

/** Raw PVK (caller zeroes it), or `undefined` when the passphrase KEK does not authenticate the inner layer. */
export async function openPassphraseInner(c: Crypto, kek: CryptoKey, inner: Bytes, profileId: string, h: PassphraseHeader): Promise<Bytes | undefined> {
  if (inner.length !== PASS_INNER_BYTES) return undefined;
  try {
    const raw = new Uint8Array(await c.subtle.decrypt({ name: "AES-GCM", iv: inner.subarray(0, NONCE_BYTES), additionalData: passInnerAad(profileId, h), tagLength: 128 },
      kek, inner.subarray(NONCE_BYTES)));
    if (raw.length === PVK_BYTES) return raw;
    raw.fill(0);
    return undefined;
  } catch {
    return undefined;
  }
}

// ---- Outer layer (method 1/2) -------------------------------------------------------------------------------------

/** Format 1 AAD is unchanged; format 2 adds the passphrase header, so neither layer's parameters can be swapped. */
const wrapAad = (profileId: string, method: WrappingMethod, pass?: PassphraseHeader): Bytes => pass === undefined
  ? concat(utf8(WRAP_AAD_LABEL), [0, SEAL_FORMAT, 0], utf8(method), [0], utf8(profileId))
  : concat(utf8(WRAP_AAD_LABEL), [0, WRAP_FORMAT_PASSPHRASE, 0], utf8(method), [0], passphraseHeaderBytes(pass), utf8(profileId));

/** Wraps the raw PVK (format 1) or the passphrase inner layer (format 2, `pass` given) under the method 1/2 KEK. */
export async function wrapPvk(c: Crypto, kek: CryptoKey, payload: Bytes, profileId: string, method: WrappingMethod, pass?: PassphraseHeader): Promise<WrappedPvk> {
  const nonce = randomBytes(c, NONCE_BYTES);
  const ct = await c.subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: wrapAad(profileId, method, pass), tagLength: 128 }, kek, payload);
  return { nonce, ciphertext: new Uint8Array(ct) };
}

/**
 * Raw PVK (format 1) or the passphrase inner layer (format 2); caller must zero it. `undefined` when the wrapper
 * does not authenticate.
 */
export async function unwrapPvk(c: Crypto, kek: CryptoKey, w: WrappedPvk, profileId: string, method: WrappingMethod, pass?: PassphraseHeader): Promise<Bytes | undefined> {
  try {
    const raw = new Uint8Array(await c.subtle.decrypt({ name: "AES-GCM", iv: w.nonce, additionalData: wrapAad(profileId, method, pass), tagLength: 128 }, kek, w.ciphertext));
    if (raw.length === (pass === undefined ? PVK_BYTES : PASS_INNER_BYTES)) return raw;
    raw.fill(0);
    return undefined;
  } catch {
    return undefined;
  }
}

/** PRF eval input for the vault: SHA-256(label || 0x00 || per-profile random salt). */
export const prfInput = async (c: Crypto, salt: Bytes): Promise<Bytes> =>
  new Uint8Array(await c.subtle.digest("SHA-256", concat(utf8(PRF_LABEL), [0], salt)));

/** Wrapping key from a PRF output; zeroes the output buffer. */
export async function prfKek(c: Crypto, prfOutput: Bytes, salt: Bytes): Promise<CryptoKey> {
  try {
    const base = await c.subtle.importKey("raw", prfOutput, "HKDF", false, ["deriveKey"]);
    return await c.subtle.deriveKey({ name: "HKDF", hash: "SHA-256", salt, info: utf8(PRF_KEK_LABEL) }, base,
      { name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
  } finally {
    prfOutput.fill(0);
  }
}

/** Method 2 wrapper: generated with `extractable: false`; persisted only as a structured-clone CryptoKey. */
export const newDeviceKey = (c: Crypto): Promise<CryptoKey> =>
  c.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);

export const equalBytes = (a: Uint8Array, b: Uint8Array): boolean => a.length === b.length && a.every((v, i) => v === b[i]);
