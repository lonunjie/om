// Release signing helpers (Node only). B2: the online key signs only under a root-signed delegation (see requireDelegation). The payload MUST equal src/update/manifest.ts signingPayload() byte-for-byte;
// release.test.ts cross-checks both implementations. The private key is held in memory only: never written, logged or put in argv.
import { createPrivateKey, createPublicKey, sign } from "node:crypto";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";
import { PINNED_ROOTS, PINNED_ROOT_THRESHOLD, resolveReleaseKey, verifyDelegation } from "./verify-release.mjs";

export const RELEASE_KEY_ID = "om-release-2026-10-28";
export const VAULT_NOTE = "OM release signing key (ed25519) - lonunjie";

const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
/** Mirror of canonicalJson in src/update/manifest.ts. */
export const canonicalJson = (v) => {
  if (Array.isArray(v)) return `[${v.map(canonicalJson).join(",")}]`;
  if (isRecord(v)) {
    return `{${Object.keys(v).filter((k) => v[k] !== undefined).sort().map((k) => `${JSON.stringify(k)}:${canonicalJson(v[k])}`).join(",")}}`;
  }
  return JSON.stringify(v);
};
export const signingPayload = (m) => Buffer.from(canonicalJson({ ...m, signature: null }), "utf8");

export const extractPem = (text) => {
  const m = /-----BEGIN PRIVATE KEY-----[A-Za-z0-9+/=\s]+?-----END PRIVATE KEY-----/.exec(text);
  return m ? m[0] : null;
};
export const pemFromVault = () => {
  const bw = join(homedir(), "dig/bin/bw-agent");
  const r = spawnSync(bw, ["--profile", "lonunjie-agents", "get", "notes", VAULT_NOTE], { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`vault read failed (exit ${r.status})`); // never echo stdout/stderr: may carry secrets
  const pem = extractPem(r.stdout);
  if (!pem) throw new Error("vault note has no PKCS8 PEM block");
  return pem;
};
const rawPublicHex = (key) => createPublicKey(key).export({ format: "der", type: "spki" }).subarray(-32).toString("hex");

/**
 * Refuses (throws) unless the delegation is signed by the pinned root(s), unexpired, lists `keyId` as a valid,
 * unrevoked Ed25519 key inside its window, and that listed public key is the one `pem` belongs to.
 * Returns the delegation. Messages carry reason codes only, never key material.
 */
export const requireDelegation = (delegation, pem, keyId, { roots = PINNED_ROOTS, threshold = PINNED_ROOT_THRESHOLD, now = Date.now() } = {}) => {
  if (delegation === undefined || delegation === null) throw new Error("delegation missing: run the key ceremony (deploy/KEY-CEREMONY.md) and pass --delegation <file>");
  const d = verifyDelegation(delegation, { keys: roots, threshold }, { now });
  if (!d.ok) throw new Error(`delegation refused: ${d.reason}`);
  const k = resolveReleaseKey(d.delegation, keyId, "ed25519", now);
  if (!k.ok) throw new Error(`delegation does not authorise key ${keyId}: ${k.reason}`);
  if (rawPublicHex(createPrivateKey(pem)) !== k.pub) throw new Error(`the signing key does not match the public key the delegation lists for ${keyId}`);
  return d.delegation;
};

/** Returns {alg,keyId,sig} (sig base64) over the manifest's signing payload (which includes manifest.delegation). Refuses without a valid delegation (B2). */
export const signManifest = (manifest, pem, keyId = RELEASE_KEY_ID, trust = {}) => {
  const key = createPrivateKey(pem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("signing key is not ed25519");
  requireDelegation(manifest.delegation, pem, keyId, trust);
  return { alg: "ed25519", keyId, sig: sign(null, signingPayload(manifest), key).toString("base64") };
};
