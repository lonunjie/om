import { verifyChain, type DelegationFailure, type KeyFailure, type RootPin } from "./delegation";
import { ed25519Verifier, fromBase64, fromHex, type SignatureVerifier } from "./ed25519";
import { signingPayload, type BundleManifest } from "./manifest";

export { ed25519Verifier, type SignatureVerifier };

/** Caller-injected trust root: ONLY the offline root key(s). Release keys are never pinned, they arrive in the signed delegation. */
export interface TrustRoot {
  readonly roots: RootPin;
  /** Only dev builds of the shell set this; production trust roots leave it false. */
  readonly allowUnsignedDev?: boolean;
  /** Clock, epoch ms (injectable for tests). */
  readonly now?: () => number;
}
export type SignatureFailure = "unsigned" | "no_delegation" | DelegationFailure | KeyFailure | "bad_signature";
export type SignatureResult =
  | { readonly ok: true; readonly unsignedDev: boolean; /** The accepted delegation version to persist as the new floor (null for unsigned dev). */ readonly delegationVersion: number | null; /** Present (true) when a PQ co-signature was required and verified: persist the PQ floor (ADR-0182). */ readonly pqRequired?: true }
  | { readonly ok: false; readonly reason: SignatureFailure };

/**
 * Fails closed. Chain: pinned root -> signed delegation (unexpired, not rolled back below `delegationFloor`) ->
 * release key (listed, unrevoked, in window) -> manifest signature. An empty signature slot is accepted only for a
 * `dev` manifest under a trust root that allows it.
 */
export const verifyBundleSignature = async (
  manifest: BundleManifest,
  trustRoot: TrustRoot,
  delegationFloor: number | null = null,
  pqFloor = false,
): Promise<SignatureResult> => {
  const s = manifest.signature;
  if (s === null) {
    return manifest.dev === true && trustRoot.allowUnsignedDev === true ? { ok: true, unsignedDev: true, delegationVersion: null } : { ok: false, reason: "unsigned" };
  }
  if (manifest.delegation === undefined) return { ok: false, reason: "no_delegation" };
  const now = (trustRoot.now ?? Date.now)();
  const key = await verifyChain(manifest.delegation, trustRoot.roots, { now, floor: delegationFloor, pqFloor }, s.keyId, s.alg);
  if (!key.ok) return { ok: false, reason: key.reason };
  const sig = fromBase64(s.sig);
  if (!sig) return { ok: false, reason: "bad_signature" };
  try {
    const v: SignatureVerifier = ed25519Verifier(fromHex(key.pub));
    return (await v.verify(signingPayload(manifest), sig)) ? { ok: true, unsignedDev: false, delegationVersion: key.delegationVersion, ...(key.pqRequired ? { pqRequired: true as const } : {}) } : { ok: false, reason: "bad_signature" };
  } catch {
    return { ok: false, reason: "bad_signature" };
  }
};
