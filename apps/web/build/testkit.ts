// Test-only helpers for the release-key chain (root -> delegation -> release key -> manifest). Uses the real ceremony
// code (root-ceremony.mjs) with THROWAWAY keys generated per test run; nothing here is a real key.
import { createPrivateKey, generateKeyPairSync, sign } from "node:crypto";
import { addRootSignature, buildDelegation, describeKey, type RootKey } from "./root-ceremony.mjs";
import { signingPayload } from "./sign.mjs";
import type { BundleManifest } from "../src/update/manifest";

export const NOW = Date.parse("2026-11-01T00:00:00Z");
export const DAY = 86_400_000;
export interface Root extends RootKey { pem: string }
export interface Online { pem: string; pub: string }

const pair = () => {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  return { pem: privateKey.export({ format: "pem", type: "pkcs8" }), pub: publicKey.export({ format: "der", type: "spki" }).subarray(-32).toString("hex") };
};
export const mkRoot = (): Root => {
  const p = pair();
  return { pem: p.pem, key: createPrivateKey(p.pem), ...describeKey(p.pub) };
};
export const mkOnline = (): Online => pair();
export const pinOf = (roots: Root[], threshold = 1) => ({ keys: Object.fromEntries(roots.map((r) => [r.id, r.pub])), threshold });
const iso = (ms: number) => new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");

export interface DelegateOpts {
  roots: Root[]; online: Online; keyId?: string; version?: number; now?: number; expiresInDays?: number;
  notBefore?: string; notAfter?: string; prev?: unknown;
  /** Edit the unsigned delegation before the roots sign it (to build deliberately odd chains). */
  mutate?: (d: Record<string, unknown>) => Record<string, unknown>;
}
/** A root-signed delegation listing `online` as `keyId` (default "online-1"). */
export const delegate = (o: DelegateOpts): Record<string, unknown> => {
  const now = o.now ?? NOW;
  let d: ReturnType<typeof buildDelegation> = buildDelegation({
    prev: o.prev, version: o.prev === undefined ? o.version ?? 1 : undefined, now, expires: iso(now + (o.expiresInDays ?? 90) * DAY),
    addKey: { keyId: o.keyId ?? "online-1", pub: o.online.pub, notBefore: o.notBefore ?? iso(now - DAY), notAfter: o.notAfter ?? iso(now + 90 * DAY) },
  });
  if (o.mutate) d = o.mutate(d as unknown as Record<string, unknown>) as unknown as typeof d;
  for (const r of o.roots) d = addRootSignature(d, r);
  return d as unknown as Record<string, unknown>;
};
/** Manifest with the delegation embedded and signed by `online` (no validation: tests build bad chains on purpose). */
export const signWith = (m: BundleManifest, delegation: unknown, online: Online, keyId = "online-1"): BundleManifest => {
  const withD = { ...m, delegation } as BundleManifest;
  const sig = sign(null, signingPayload(withD), createPrivateKey(online.pem)).toString("base64");
  return { ...withD, signature: { alg: "ed25519", keyId, sig } };
};
