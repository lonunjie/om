import { ed25519Verifier, type TrustRoot } from "./signature";

/** Pinned production release key (public part; lonunjie hq registry/signing.toml). build/verify-release.mjs pins the same value. */
export const RELEASE_KEY_ID = "om-release-2026-10-28";
export const RELEASE_PUBLIC_KEY_HEX = "fad8cae9155ea0b7943c0b3f141a20f85b35c88ca5eacfe576a9bf578f28ebeb";

const fromHex = (h: string): Uint8Array => Uint8Array.from(h.match(/../g) ?? [], (b) => parseInt(b, 16));

/** Only the production key is pinned (no dev signing key exists); `allowUnsignedDev` is true solely for dev bundles. */
export const makeTrustRoot = (allowUnsignedDev: boolean): TrustRoot => ({
  verifiers: { [RELEASE_KEY_ID]: ed25519Verifier(fromHex(RELEASE_PUBLIC_KEY_HEX)) },
  allowUnsignedDev,
});
