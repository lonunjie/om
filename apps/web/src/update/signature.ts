import { signingPayload, type BundleManifest } from "./manifest";

/** One pinned verification key. `alg` is part of the pin: an algorithm tag alone never grants authority (ADR-0149). */
export interface SignatureVerifier {
  readonly alg: string;
  verify(payload: Uint8Array, sig: Uint8Array): Promise<boolean>;
}
/** Caller-injected trust root. No key management lives here. */
export interface TrustRoot {
  readonly verifiers: Readonly<Record<string, SignatureVerifier>>;
  /** Only dev builds of the shell set this; production trust roots leave it false. */
  readonly allowUnsignedDev?: boolean;
}
export type SignatureResult =
  | { readonly ok: true; readonly unsignedDev: boolean }
  | { readonly ok: false; readonly reason: "unsigned" | "unknown_key" | "alg_mismatch" | "bad_signature" };

const fromBase64 = (s: string): Uint8Array | null => {
  try {
    return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
  } catch {
    return null;
  }
};

/** Fails closed: an empty signature slot is accepted only for a `dev` manifest under a trust root that allows it. */
export const verifyBundleSignature = async (manifest: BundleManifest, trustRoot: TrustRoot): Promise<SignatureResult> => {
  const s = manifest.signature;
  if (s === null) {
    return manifest.dev === true && trustRoot.allowUnsignedDev === true ? { ok: true, unsignedDev: true } : { ok: false, reason: "unsigned" };
  }
  const v = Object.hasOwn(trustRoot.verifiers, s.keyId) ? trustRoot.verifiers[s.keyId] : undefined;
  if (!v) return { ok: false, reason: "unknown_key" };
  if (v.alg !== s.alg) return { ok: false, reason: "alg_mismatch" };
  const sig = fromBase64(s.sig);
  if (!sig) return { ok: false, reason: "bad_signature" };
  try {
    return (await v.verify(signingPayload(manifest), sig)) ? { ok: true, unsignedDev: false } : { ok: false, reason: "bad_signature" };
  } catch {
    return { ok: false, reason: "bad_signature" };
  }
};

/** WebCrypto Ed25519 verifier for a raw 32-byte public key (available in Node 24 and current browsers). */
export const ed25519Verifier = (rawPublicKey: Uint8Array): SignatureVerifier => ({
  alg: "ed25519",
  verify: async (payload, sig) => {
    const key = await crypto.subtle.importKey("raw", rawPublicKey as BufferSource, { name: "Ed25519" }, false, ["verify"]);
    return crypto.subtle.verify({ name: "Ed25519" }, key, sig as BufferSource, payload as BufferSource);
  },
});
