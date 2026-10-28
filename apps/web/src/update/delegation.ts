// Release-key delegation (audit B2, PR-005, ADR-0149): an OFFLINE root key (held only by the operator) signs a small
// canonical `delegation` document that lists the ONLINE release key(s) allowed to sign bundle manifests, with validity
// windows and a revocation list. The client pins only the root key(s). node build/verify-release.mjs implements the same
// rules (shared-fixture test: build/delegation.test.ts); keep the two identical, including the failure reason strings.
import { canonicalJson } from "./manifest";
import { ed25519Verifier, fromBase64, fromHex } from "./ed25519";

export const DELEGATION_FORMAT = "om.release-delegation/1";
export const MAX_ROOT_SIGNATURES = 16;

export interface DelegatedKey {
  readonly keyId: string;
  readonly alg: string;
  /** Raw 32-byte Ed25519 public key, lowercase hex. */
  readonly pub: string;
  /** UTC `YYYY-MM-DDTHH:MM:SSZ`. */
  readonly notBefore: string;
  readonly notAfter: string;
}
export interface RootSignature {
  readonly keyId: string;
  readonly alg: string;
  readonly sig: string;
}
export interface Delegation {
  readonly format: typeof DELEGATION_FORMAT;
  /** Monotonic integer >= 1. Clients persist the highest accepted value and refuse a lower one. */
  readonly version: number;
  /** UTC `YYYY-MM-DDTHH:MM:SSZ`; an expired delegation is refused (fail closed). */
  readonly expires: string;
  readonly keys: readonly DelegatedKey[];
  readonly revoked: readonly string[];
  /** Root signatures over `delegationPayload(d)`; not part of the signed bytes. */
  readonly signatures: readonly RootSignature[];
}
/** Pinned root keys (keyId -> raw public key hex) and how many distinct ones must sign. */
export interface RootPin {
  readonly keys: Readonly<Record<string, string>>;
  readonly threshold: number;
}
export type DelegationFailure = "root_unset" | "malformed_delegation" | "root_threshold" | "delegation_expired" | "delegation_rollback";
export type KeyFailure = "revoked_key" | "unlisted_key" | "alg_mismatch" | "key_not_yet_valid" | "key_expired";
export type DelegationResult = { readonly ok: true; readonly delegation: Delegation } | { readonly ok: false; readonly reason: DelegationFailure };
export type KeyResult = { readonly ok: true; readonly pub: string } | { readonly ok: false; readonly reason: KeyFailure };

const ID = /^[A-Za-z0-9._-]{1,64}$/;
const HEX64 = /^[0-9a-f]{64}$/;
const TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const timeMs = (s: unknown): number => (typeof s === "string" && TIME.test(s) ? Date.parse(s) : Number.NaN);

/** Bytes a root signature covers: the whole delegation with the signature slot nulled. */
export const delegationPayload = (d: Delegation): Uint8Array => new TextEncoder().encode(canonicalJson({ ...d, signatures: null }));

/** Strict structural check of untrusted delegation JSON; null when anything is off. */
export const parseDelegation = (v: unknown): Delegation | null => {
  if (!isRecord(v) || v["format"] !== DELEGATION_FORMAT) return null;
  const { version, expires, keys, revoked, signatures } = v;
  if (typeof version !== "number" || !Number.isSafeInteger(version) || version < 1) return null;
  if (Number.isNaN(timeMs(expires))) return null;
  if (!Array.isArray(keys) || !Array.isArray(revoked) || !Array.isArray(signatures) || signatures.length > MAX_ROOT_SIGNATURES) return null;
  const ids = new Set<string>();
  for (const k of keys as unknown[]) {
    if (!isRecord(k) || typeof k["keyId"] !== "string" || !ID.test(k["keyId"]) || ids.has(k["keyId"])) return null;
    if (typeof k["alg"] !== "string" || typeof k["pub"] !== "string" || !HEX64.test(k["pub"])) return null;
    if (Number.isNaN(timeMs(k["notBefore"])) || Number.isNaN(timeMs(k["notAfter"]))) return null;
    ids.add(k["keyId"]);
  }
  for (const r of revoked as unknown[]) if (typeof r !== "string" || !ID.test(r)) return null;
  for (const s of signatures as unknown[]) {
    if (!isRecord(s) || typeof s["keyId"] !== "string" || typeof s["alg"] !== "string" || typeof s["sig"] !== "string") return null;
  }
  return v as unknown as Delegation;
};

const pinUsable = (pin: RootPin): boolean => {
  const ids = Object.keys(pin.keys);
  return ids.length > 0 && Number.isSafeInteger(pin.threshold) && pin.threshold >= 1 && pin.threshold <= ids.length && ids.every((i) => HEX64.test(pin.keys[i] ?? ""));
};

/**
 * root -> delegation. Order of checks (identical in verify-release.mjs): pin usable, structure, root threshold,
 * expiry, version floor. Thresholds count distinct pinned root keyIds with a valid Ed25519 signature; duplicates and
 * unknown keyIds add nothing (ADR-0149).
 */
export const verifyDelegation = async (raw: unknown, pin: RootPin, ctx: { now: number; floor: number | null }): Promise<DelegationResult> => {
  if (!pinUsable(pin)) return { ok: false, reason: "root_unset" };
  const d = parseDelegation(raw);
  if (!d) return { ok: false, reason: "malformed_delegation" };
  const payload = delegationPayload(d);
  const good = new Set<string>();
  for (const s of d.signatures) {
    const hex = Object.hasOwn(pin.keys, s.keyId) ? pin.keys[s.keyId] : undefined;
    if (hex === undefined || s.alg !== "ed25519" || good.has(s.keyId)) continue;
    const sig = fromBase64(s.sig);
    if (!sig) continue;
    try {
      if (await ed25519Verifier(fromHex(hex)).verify(payload, sig)) good.add(s.keyId);
    } catch {
      /* an unverifiable signature counts for nothing */
    }
  }
  if (good.size < pin.threshold) return { ok: false, reason: "root_threshold" };
  if (ctx.now >= timeMs(d.expires)) return { ok: false, reason: "delegation_expired" };
  if (ctx.floor !== null && d.version < ctx.floor) return { ok: false, reason: "delegation_rollback" };
  return { ok: true, delegation: d };
};

/** delegation -> release key: listed, not revoked, right algorithm, inside its validity window. */
export const resolveReleaseKey = (d: Delegation, keyId: string, alg: string, now: number): KeyResult => {
  if (d.revoked.includes(keyId)) return { ok: false, reason: "revoked_key" };
  const k = d.keys.find((x) => x.keyId === keyId);
  if (!k) return { ok: false, reason: "unlisted_key" };
  if (k.alg !== "ed25519" || alg !== k.alg) return { ok: false, reason: "alg_mismatch" };
  if (now < timeMs(k.notBefore)) return { ok: false, reason: "key_not_yet_valid" };
  if (now > timeMs(k.notAfter)) return { ok: false, reason: "key_expired" };
  return { ok: true, pub: k.pub };
};

/** The whole chain in one call: root -> delegation -> release key. Returns the key plus the delegation version to persist. */
export const verifyChain = async (
  raw: unknown,
  pin: RootPin,
  ctx: { now: number; floor: number | null },
  keyId: string,
  alg: string,
): Promise<{ readonly ok: true; readonly pub: string; readonly delegationVersion: number } | { readonly ok: false; readonly reason: DelegationFailure | KeyFailure }> => {
  const d = await verifyDelegation(raw, pin, ctx);
  if (!d.ok) return d;
  const k = resolveReleaseKey(d.delegation, keyId, alg, ctx.now);
  return k.ok ? { ok: true, pub: k.pub, delegationVersion: d.delegation.version } : k;
};
