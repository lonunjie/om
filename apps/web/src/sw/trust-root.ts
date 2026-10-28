import { makeTrustRoot } from "../update/release-key";

declare const __OM_DEV_BUNDLE__: boolean;

// Pins ONLY the offline root key (see ../update/release-key.ts); the online release key arrives in a root-signed delegation. Dev bundles (OM_BUNDLE_DEV=1 at build) additionally accept an unsigned dev manifest.
export const TRUST_ROOT = makeTrustRoot(__OM_DEV_BUNDLE__);
