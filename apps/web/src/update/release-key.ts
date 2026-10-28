import type { TrustRoot } from "./signature";
import type { RootKeyDescriptor, RootPin } from "./delegation";

// ============================================================================================================
// TEMPORARY PLACEHOLDER (audit B2). The OFFLINE ROOT public key has not been generated yet (operator key
// ceremony pending: deploy/KEY-CEREMONY.md). Until ROOT_KEYS is filled in:
//   - production release builds FAIL (vite.config.ts under OM_RELEASE_BUILD, build/release.mjs, verify.mjs),
//   - any signed bundle is refused by the client (fail closed: "root_unset"),
//   - dev bundles (OM_BUNDLE_DEV=1, unsigned) are unaffected.
// Operator decision 2026-10-28: the root is 2-of-3 Ed25519 signers (Ledger A, Ledger B, one software key kept only in the
// operator's personal vault). After the ceremony set the three public descriptors printed by `root-ceremony.mjs`, e.g.
//   { "root-0123456789abcdef": { pub: "<64 hex>", frame: "solana-offchain-v0" },   // a Ledger
//     "root-fedcba9876543210": { pub: "<64 hex>", frame: "solana-offchain-v0" },   // the other Ledger
//     "root-00112233aabbccdd": "<64 hex>" }                                         // the software key ("raw")
// and set the SAME value in build/verify-release.mjs (PINNED_ROOTS); release.test.ts checks both agree.
// The client pins ONLY root keys. The online release key arrives in a root-signed delegation inside every manifest.
// ============================================================================================================
export const ROOT_KEYS: Readonly<Record<string, RootKeyDescriptor>> = {
  "root-1ad451cf9d829f8c": {
    "pub": "79f4816d521646a1a05f4044aee630b59f711a9b0c6111c9fbdfd106152b5a2a",
    "frame": "solana-offchain-v0"
  },
  "root-424a711d6c98e116": {
    "pub": "f3576e4b061d5bf87e879d1ee980ab5a9b7ca5685057347e7120429bba257b99",
    "frame": "solana-offchain-v0"
  },
  "root-547e93244cf6a195": "dd0206e04683a13c1c2bf0fa7b01abd0060561f029de21c57d767bdb8c9f7729"
};
/** Distinct root signatures required on a delegation: 2 of the 3 pinned roots (operator decision 2026-10-28; deploy/KEY-CEREMONY.md). */
export const ROOT_THRESHOLD = 2;
/** Exactly this many distinct roots are pinned once the ceremony is done (release.test.ts checks it). */
export const ROOT_KEY_COUNT = 3;

export const rootPin = (keys: Readonly<Record<string, RootKeyDescriptor>> = ROOT_KEYS, threshold = ROOT_THRESHOLD): RootPin => ({ keys, threshold });
export const isRootPinned = (): boolean => Object.keys(ROOT_KEYS).length > 0;

/** Production trust root; `allowUnsignedDev` is true solely for dev bundles. */
export const makeTrustRoot = (allowUnsignedDev: boolean): TrustRoot => ({ roots: rootPin(), allowUnsignedDev });
