// Release signing helpers (Node only). The payload MUST equal src/update/manifest.ts signingPayload() byte-for-byte;
// release.test.ts cross-checks both implementations. The private key is held in memory only: never written, logged or put in argv.
import { createPrivateKey, sign } from "node:crypto";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";

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
/** Returns {alg,keyId,sig} (sig base64) over the manifest's signing payload. */
export const signManifest = (manifest, pem, keyId = RELEASE_KEY_ID) => {
  const key = createPrivateKey(pem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("signing key is not ed25519");
  return { alg: "ed25519", keyId, sig: sign(null, signingPayload(manifest), key).toString("base64") };
};
