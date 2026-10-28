// PQ root ceremony library (ADR-0182, deploy/KEY-CEREMONY-PQ.md): the ML-DSA-65 (FIPS 204) co-signature on the release-root
// delegation. Used by root-ceremony.mjs (gen-pq-root, show-pq-root, pq-payload, pq-sign-delegation, pq-attach). Needs the
// workspace install (`pnpm install --frozen-lockfile` in the repo): @noble/post-quantum 0.7.1 generates the key and signs.
// Every signature is also verified with node:crypto (OpenSSL), a second, independent implementation, and with the
// production verifier (verify-release.mjs) before it is attached, so a defect in either library fails loudly here.
// The private key is the 32-byte FIPS 204 seed. It is never printed, logged, put in argv or in an error message, and it
// is written only as a Bitwarden secure-note item on stdout (gen-pq-root --to-stdin-for-vault) for the PERSONAL vault.
import { createHash, randomBytes } from "node:crypto";
import { ml_dsa65 } from "@noble/post-quantum/ml-dsa.js";
import { PQ_ALG, PQ_FRAME, PQ_PUBLIC_KEY_BYTES, PQ_SIGNATURE_BYTES, delegationPayload, describePq, parseDelegation, pqSigningBytes, pqVerify } from "./verify-release.mjs";

const sha256 = (b) => createHash("sha256").update(b).digest("hex");
export const pqFingerprint = (pubHex) => sha256(Buffer.from(pubHex, "hex"));
/** Deterministic id derived from the public key, so a signature entry cannot name the wrong key. */
export const pqKeyId = (pubHex) => `pq-root-${pqFingerprint(pubHex).slice(0, 16)}`;
const grouped = (hex) => hex.match(/.{1,8}/g).join(" ");
/** The PUBLIC descriptor that is pinned in pq-root.ts / verify-release.mjs. */
export const pqDescriptor = (pubHex) => ({ keyId: pqKeyId(pubHex), alg: PQ_ALG, pub: pubHex, frame: PQ_FRAME });
export const describePqKey = (pubHex) => ({ ...pqDescriptor(pubHex), fingerprint: grouped(pqFingerprint(pubHex)) });

const fromSeed = (seed) => {
  const k = ml_dsa65.keygen(seed);
  if (k.publicKey.length !== PQ_PUBLIC_KEY_BYTES) throw new Error("unexpected ML-DSA-65 public key size");
  return { seed, secretKey: k.secretKey, pub: Buffer.from(k.publicKey).toString("hex") };
};
/** A new PQ root key in memory. */
export const generatePqRoot = () => {
  const r = fromSeed(new Uint8Array(randomBytes(32)));
  return { ...r, ...describePqKey(r.pub) };
};

// The vault note is exactly this block (so `bw get notes <name>` is the key). `fingerprint` is public and only lets a
// restore detect a damaged seed; the seed alone is the key.
const BEGIN = "-----BEGIN OM PQ ROOT SEED-----";
const END = "-----END OM PQ ROOT SEED-----";
export const formatPqNote = (r) => `${BEGIN}\nalg: ${PQ_ALG}\nframe: ${PQ_FRAME}\nkey-id: ${r.keyId}\nfingerprint: sha256 ${pqFingerprint(r.pub)}\nseed: ${Buffer.from(r.seed).toString("hex")}\n${END}\n`;
/** Parses a vault note. Errors never include the content. */
export const loadPqRootNote = (text) => {
  const m = new RegExp(`${BEGIN}\\n([\\s\\S]*?)${END}`).exec(typeof text === "string" ? text : "");
  if (!m) throw new Error("input is not an OM PQ root seed note");
  const f = Object.fromEntries(m[1].split("\n").filter(Boolean).map((l) => { const i = l.indexOf(": "); return i < 0 ? ["", ""] : [l.slice(0, i), l.slice(i + 2)]; }));
  if (f.alg !== PQ_ALG || f.frame !== PQ_FRAME) throw new Error(`the PQ root note is for another algorithm or frame (this tool knows ${PQ_ALG} / ${PQ_FRAME})`);
  if (!/^[0-9a-f]{64}$/.test(f.seed ?? "")) throw new Error("the PQ root note has no valid 32-byte seed");
  const r = fromSeed(new Uint8Array(Buffer.from(f.seed, "hex")));
  const d = describePqKey(r.pub);
  if (f.fingerprint !== undefined && f.fingerprint !== `sha256 ${pqFingerprint(r.pub)}`) throw new Error("the PQ root note's fingerprint does not match its seed (damaged copy; restore from another backup)");
  if (f["key-id"] !== undefined && f["key-id"] !== d.keyId) throw new Error("the PQ root note's key id does not match its seed");
  return { ...r, ...d };
};
/** Reads the public descriptor file (JSON) written by --pub-out; the descriptor must be usable. */
export const checkDescriptor = (v) => {
  const d = describePq(v);
  if (!d) throw new Error("not a usable PQ root descriptor ({keyId, alg: ml-dsa-65, pub: 3904 hex, frame: om-pq-v1})");
  if (d.keyId !== pqKeyId(d.pub)) throw new Error("the descriptor's keyId is not derived from its public key");
  return d;
};

const attach = (d, entry) => ({ ...d, signatures: [...d.signatures.filter((s) => s.keyId !== entry.keyId), entry].sort((a, b) => (a.keyId < b.keyId ? -1 : 1)) });
const need = (delegation) => {
  const d = parseDelegation(delegation);
  if (!d) throw new Error("not a valid delegation document");
  return d;
};
/** The bytes a PQ signer signs for this delegation (what `pq-payload` writes for an external signer). */
export const pqPayload = (delegation, pin) => pqSigningBytes(pin, delegationPayload(need(delegation)));

/**
 * Signs the delegation with the PQ root and attaches the entry (replacing an older entry of the same key). Refuses unless the
 * signature verifies under BOTH node:crypto and the production verifier path, so nothing unverifiable is ever written.
 */
export const addPqSignature = (delegation, root) => {
  const d = need(delegation);
  const pin = pqDescriptor(root.pub);
  const msg = pqSigningBytes(pin, delegationPayload(d));
  const sig = Buffer.from(ml_dsa65.sign(msg, root.secretKey));
  if (sig.length !== PQ_SIGNATURE_BYTES) throw new Error("unexpected ML-DSA-65 signature size");
  const entry = { keyId: pin.keyId, alg: PQ_ALG, sig: sig.toString("base64"), frame: PQ_FRAME };
  if (!ml_dsa65.verify(sig, msg, Buffer.from(root.pub, "hex")) || !pqVerify(pin, delegationPayload(d), entry.sig)) {
    throw new Error("the fresh PQ signature does not verify under both ML-DSA implementations (noble and node:crypto); nothing was written (Node without ML-DSA in node:crypto? use Node 24.14 or later)");
  }
  return attach(d, entry);
};
/** Verifies an externally produced PQ signature (a future ML-DSA hardware signer) and returns the delegation with it attached, or throws. */
export const externalPqAttach = (delegation, pin, sig) => {
  const d = need(delegation);
  const b = /^[0-9a-fA-F]+$/.test(sig) && sig.length === PQ_SIGNATURE_BYTES * 2 ? Buffer.from(sig, "hex") : Buffer.from(String(sig), "base64");
  if (b.length !== PQ_SIGNATURE_BYTES) throw new Error(`--sig must be a ${PQ_SIGNATURE_BYTES}-byte ML-DSA-65 signature (hex or base64)`);
  const entry = { keyId: pin.keyId, alg: PQ_ALG, sig: b.toString("base64"), frame: PQ_FRAME };
  if (!pqVerify(pin, delegationPayload(d), entry.sig)) throw new Error(`the signature does not verify for this delegation and PQ root ${pin.keyId}; nothing was written`);
  return attach(d, entry);
};
