import { makeTrustRoot } from "../update/release-key";

declare const __OM_DEV_BUNDLE__: boolean;

// Pins the production release key (see ../update/release-key.ts). Dev bundles (OM_BUNDLE_DEV=1 at build) additionally accept an unsigned dev manifest.
export const TRUST_ROOT = makeTrustRoot(__OM_DEV_BUNDLE__);
