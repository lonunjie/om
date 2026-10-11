import type { TrustRoot } from "../update/signature";

declare const __OM_DEV_BUNDLE__: boolean;

// No release keys exist yet (no key management in the web app): production fails closed on every manifest.
// Dev bundles (OM_BUNDLE_DEV=1 at build) additionally accept an unsigned dev manifest.
export const TRUST_ROOT: TrustRoot = { verifiers: {}, allowUnsignedDev: __OM_DEV_BUNDLE__ };
