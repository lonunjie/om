import type { TrustRoot } from "./signature";
import type { RootPin } from "./delegation";

// ============================================================================================================
// TEMPORARY PLACEHOLDER (audit B2). The OFFLINE ROOT public key has not been generated yet (operator key
// ceremony pending: deploy/KEY-CEREMONY.md). Until ROOT_KEYS is filled in:
//   - production release builds FAIL (vite.config.ts under OM_RELEASE_BUILD, build/release.mjs, verify.mjs),
//   - any signed bundle is refused by the client (fail closed: "root_unset"),
//   - dev bundles (OM_BUNDLE_DEV=1, unsigned) are unaffected.
// After the ceremony set exactly the public output of `root-ceremony.mjs gen-root`, e.g.
//   { "root-0123456789abcdef": "<64 hex chars>" }
// and set the SAME value in build/verify-release.mjs (PINNED_ROOTS); release.test.ts checks both agree.
// The client pins ONLY root keys. The online release key arrives in a root-signed delegation inside every manifest.
// ============================================================================================================
export const ROOT_KEYS: Readonly<Record<string, string>> = {};
/** Distinct root signatures required on a delegation. Single root today: 1. A 2-of-N upgrade is a pin change (see KEY-CEREMONY.md). */
export const ROOT_THRESHOLD = 1;

export const rootPin = (keys: Readonly<Record<string, string>> = ROOT_KEYS, threshold = ROOT_THRESHOLD): RootPin => ({ keys, threshold });
export const isRootPinned = (): boolean => Object.keys(ROOT_KEYS).length > 0;

/** Production trust root; `allowUnsignedDev` is true solely for dev bundles. */
export const makeTrustRoot = (allowUnsignedDev: boolean): TrustRoot => ({ roots: rootPin(), allowUnsignedDev });
