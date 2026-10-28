import { compareVersions, type BundleManifest } from "./manifest";
import { verifyBundleSignature, type TrustRoot } from "./signature";

/** Persisted monotonic floor: the highest bundle version ever accepted (IndexedDB in the SW, a fake in tests). */
export interface FloorStore {
  get(): Promise<string | null>;
  set(version: string): Promise<void>;
  /** Highest delegation version ever accepted (integer), separate from the bundle floor so a root-signed rollback of keys is refused too. */
  getDelegation(): Promise<number | null>;
  setDelegation(version: number): Promise<void>;
  /**
   * ADR-0182 PQ floor: true once a delegation requiring the ML-DSA-65 co-signature was accepted. It only ever rises: there is
   * no setter to clear it, so a root-signed (or replayed) Ed25519-only chain can never switch the requirement off again.
   */
  getPqRequired(): Promise<boolean>;
  setPqRequired(): Promise<void>;
}
export type AcceptResult =
  | { readonly ok: true; readonly unsignedDev: boolean; readonly rolledBack: boolean; readonly delegationVersion: number | null; readonly pqRequired?: true }
  | { readonly ok: false; readonly reason: string };

/**
 * Decide whether a manifest may be staged. Pure decision, no write: call `commitFloor` once the staged files verify.
 * A lower bundle version is refused unless the manifest carries a rollback marker for exactly (floor -> version)
 * AND its signature verified for real (an unsigned dev bundle can never roll back).
 */
export const evaluateManifest = async (manifest: BundleManifest, floor: FloorStore, trustRoot: TrustRoot): Promise<AcceptResult> => {
  const sig = await verifyBundleSignature(manifest, trustRoot, await floor.getDelegation(), await floor.getPqRequired());
  if (!sig.ok) return { ok: false, reason: sig.reason };
  const dv = sig.delegationVersion;
  const pqx = sig.pqRequired ? { pqRequired: true as const } : {};
  const current = await floor.get();
  if (current === null) return { ok: true, unsignedDev: sig.unsignedDev, rolledBack: false, delegationVersion: dv, ...pqx };
  const cmp = compareVersions(manifest.version, current);
  if (cmp === null) return { ok: false, reason: "bad_version" };
  if (cmp >= 0) return { ok: true, unsignedDev: sig.unsignedDev, rolledBack: false, delegationVersion: dv, ...pqx };
  const r = manifest.rollback;
  if (r && !sig.unsignedDev && r.from === current && r.to === manifest.version) return { ok: true, unsignedDev: false, rolledBack: true, delegationVersion: dv, ...pqx };
  return { ok: false, reason: "rollback_refused" };
};

/** Persist the floor after a verified stage. Never lowers it except through an accepted rollback. */
export const commitFloor = async (floor: FloorStore, accepted: Extract<AcceptResult, { ok: true }>, version: string): Promise<void> => {
  const current = await floor.get();
  if (accepted.rolledBack || current === null || (compareVersions(version, current) ?? -1) > 0) await floor.set(version);
  // The delegation floor only ever rises (a bundle rollback marker never lowers it).
  if (accepted.delegationVersion !== null) {
    const dCurrent = await floor.getDelegation();
    if (dCurrent === null || accepted.delegationVersion > dCurrent) await floor.setDelegation(accepted.delegationVersion);
  }
  // The PQ floor only ever rises, and only after the whole chain (including the PQ signature) verified.
  if (accepted.pqRequired === true && !(await floor.getPqRequired())) await floor.setPqRequired();
};
