#!/usr/bin/env node
// Standalone, dependency-free verifier for an OM web release folder. Shipped inside every release as verify.mjs.
// Usage: node verify.mjs [releaseDir] [--require-signed]
// Checks: SHA256SUMS against every file, every bundle-manifest.json entry (hash + size), no unlisted files in site/.
// With a signature present it verifies the chain pinned ROOT key -> root-signed delegation (unexpired, release key listed,
// in window, not revoked) -> Ed25519 manifest signature. The delegation version floor lives in the browser (service worker).
// --require-signed additionally fails on unsigned/dev bundles. The root pin must equal src/update/release-key.ts (release.test.ts checks).
import { createHash, createPublicKey, verify } from "node:crypto";
import { readdirSync, readFileSync, realpathSync, statSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const sha = (b) => createHash("sha256").update(b).digest("hex");
const walk = (d) => readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(d, e.name)) : [join(d, e.name)]));
// Pinned OFFLINE ROOT key(s) (audit B2). TEMPORARY PLACEHOLDER: unset until the operator key ceremony (deploy/KEY-CEREMONY.md).
// While unset, no signed release verifies (fail closed) and production release builds refuse. Must equal ROOT_KEYS /
// ROOT_THRESHOLD in src/update/release-key.ts (release.test.ts checks). Release keys are NOT pinned: they come from the
// root-signed delegation inside the manifest.
// Operator decision 2026-10-28: 2-of-3 roots (two Ledgers, one software key). A pinned value is a bare 64-hex public key
// (frame "raw") or { pub, frame }; the frame is pinned per key and a signature cannot choose it.
export const PINNED_ROOTS = {};
export const PINNED_ROOT_THRESHOLD = 2;
const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
export const canonicalJson = (v) => {
  if (Array.isArray(v)) return `[${v.map(canonicalJson).join(",")}]`;
  if (isRecord(v)) return `{${Object.keys(v).filter((k) => v[k] !== undefined).sort().map((k) => `${JSON.stringify(k)}:${canonicalJson(v[k])}`).join(",")}}`;
  return JSON.stringify(v);
};
const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");
const ID = /^[A-Za-z0-9._-]{1,64}$/;
const HEX64 = /^[0-9a-f]{64}$/;
const TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
export const DELEGATION_FORMAT = "om.release-delegation/1";
const timeMs = (s) => (typeof s === "string" && TIME.test(s) ? Date.parse(s) : Number.NaN);
export const FRAME_RAW = "raw";
export const FRAME_SOLANA_OFFCHAIN_V0 = "solana-offchain-v0";
export const KNOWN_FRAMES = [FRAME_RAW, FRAME_SOLANA_OFFCHAIN_V0];
const edVerify = (hex, payload, sigB64) => {
  try {
    const key = createPublicKey({ key: Buffer.concat([SPKI_ED25519, Buffer.from(hex, "hex")]), format: "der", type: "spki" });
    return verify(null, payload, key, Buffer.from(sigB64, "base64"));
  } catch { return false; }
};

/** Bytes a raw-framed root signature covers. Mirror of delegationPayload in src/update/delegation.ts. */
export const delegationPayload = (d) => Buffer.from(canonicalJson({ ...d, signatures: null }), "utf8");

// Ledger Solana app off-chain message, header v0, format 0 (restricted ASCII). Mirror of frameDelegationPayload; see
// deploy/KEY-CEREMONY.md "Ledger framing". The app signs the WHOLE envelope (PureEdDSA).
const SOLANA_DOMAIN = Buffer.concat([Buffer.from([0xff]), Buffer.from("solana offchain", "ascii")]);
const SOLANA_APP_DOMAIN = Buffer.concat([Buffer.from("om.release-delegation/1", "ascii"), Buffer.alloc(9)]);
/** The short ASCII text a Ledger shows and signs for a delegation. */
export const ledgerText = (d, payloadSha256Hex) => `OM release delegation | version ${d.version} | expires ${d.expires}${d.pqRequired === true ? " | pq required" : ""} | sha256 ${payloadSha256Hex}`;
/** The exact bytes a root key with `frame` signs for delegation `d`; null for an unknown frame. */
export const frameDelegationPayload = (frame, d, signerPubHex) => {
  const payload = delegationPayload(d);
  if (frame === FRAME_RAW) return payload;
  if (frame !== FRAME_SOLANA_OFFCHAIN_V0) return null;
  const text = Buffer.from(ledgerText(d, sha(payload)), "ascii");
  const len = Buffer.alloc(2);
  len.writeUInt16LE(text.length);
  return Buffer.concat([SOLANA_DOMAIN, Buffer.from([0]), SOLANA_APP_DOMAIN, Buffer.from([0, 1]), Buffer.from(signerPubHex, "hex"), len, text]);
};
/** Pinned descriptor as { pub, frame }; null when malformed or the frame is unknown (fail closed). */
export const describeRoot = (v) => {
  const d = typeof v === "string" ? { pub: v, frame: FRAME_RAW } : v;
  if (!isRecord(d) || typeof d.pub !== "string" || typeof d.frame !== "string") return null;
  return HEX64.test(d.pub) && KNOWN_FRAMES.includes(d.frame) ? { pub: d.pub, frame: d.frame } : null;
};

/** Strict structural check; returns the delegation or null. Mirror of parseDelegation. */
export const parseDelegation = (v) => {
  if (!isRecord(v) || v.format !== DELEGATION_FORMAT) return null;
  const { version, expires, keys, revoked, signatures } = v;
  if (!Number.isSafeInteger(version) || version < 1) return null;
  if (Number.isNaN(timeMs(expires))) return null;
  if (!Array.isArray(keys) || !Array.isArray(revoked) || !Array.isArray(signatures) || signatures.length > 16) return null;
  const ids = new Set();
  for (const k of keys) {
    if (!isRecord(k) || typeof k.keyId !== "string" || !ID.test(k.keyId) || ids.has(k.keyId)) return null;
    if (typeof k.alg !== "string" || typeof k.pub !== "string" || !HEX64.test(k.pub)) return null;
    if (Number.isNaN(timeMs(k.notBefore)) || Number.isNaN(timeMs(k.notAfter))) return null;
    ids.add(k.keyId);
  }
  for (const r of revoked) if (typeof r !== "string" || !ID.test(r)) return null;
  for (const x of signatures) {
    if (!isRecord(x) || typeof x.keyId !== "string" || typeof x.alg !== "string" || typeof x.sig !== "string") return null;
    if (x.frame !== undefined && (typeof x.frame !== "string" || x.frame.length > 64)) return null;
  }
  if (v.pqRequired !== undefined && v.pqRequired !== true) return null;
  return v;
};

// ---- ADR-0182: ML-DSA-65 (FIPS 204) PQ root co-signature. Mirror of src/update/pq.ts, but on node:crypto (OpenSSL), an
// implementation independent of @noble/post-quantum that the client uses, so the shared fixtures cross-check the two. ----
export const PQ_ALG = "ml-dsa-65";
export const PQ_FRAME = "om-pq-v1";
export const PQ_DOMAIN = "om.release-delegation.pq/1";
export const PQ_PUBLIC_KEY_BYTES = 1952;
export const PQ_SIGNATURE_BYTES = 3309;
const SPKI_MLDSA65 = Buffer.from("308207b2300b0609608648016503040312038207a100", "hex");
const HEX_PQ_PUB = new RegExp(`^[0-9a-f]{${PQ_PUBLIC_KEY_BYTES * 2}}$`);
/** Pinned PQ root as { keyId, alg, pub, frame }; null when malformed or when the algorithm or frame is unknown (fail closed). */
export const describePq = (v) => {
  if (!isRecord(v) || typeof v.keyId !== "string" || !ID.test(v.keyId) || typeof v.alg !== "string" || typeof v.pub !== "string" || typeof v.frame !== "string") return null;
  return v.alg === PQ_ALG && v.frame === PQ_FRAME && HEX_PQ_PUB.test(v.pub) ? { keyId: v.keyId, alg: v.alg, pub: v.pub, frame: v.frame } : null;
};
/** "om.release-delegation.pq/1" 00 alg 00 | SHA-256(raw PQ public key) | payload. Pure ML-DSA, empty context string. */
export const pqSigningBytes = (pin, payload) => Buffer.concat([Buffer.from(`${PQ_DOMAIN}\0${pin.alg}\0`, "ascii"), createHash("sha256").update(Buffer.from(pin.pub, "hex")).digest(), payload]);
export const pqVerify = (pin, payload, sigB64) => {
  try {
    const sig = Buffer.from(sigB64, "base64");
    if (sig.length !== PQ_SIGNATURE_BYTES) return false;
    const key = createPublicKey({ key: Buffer.concat([SPKI_MLDSA65, Buffer.from(pin.pub, "hex")]), format: "der", type: "spki" });
    return verify(null, pqSigningBytes(pin, payload), key, sig);
  } catch { return false; } // includes a runtime without ML-DSA in node:crypto: nothing verifies (fail closed)
};

/**
 * root -> delegation. Same order and reason strings as verifyDelegation in src/update/delegation.ts:
 * root_unset, malformed_delegation, root_threshold, delegation_expired, delegation_rollback, then ADR-0182: pq_root_unset
 * (a pinned PQ descriptor is unusable, checked right after root_unset; or PQ is required and none is pinned), pq_missing, pq_invalid.
 * `pin` is { keys: {keyId: hex | {pub, frame}}, threshold, pq?: {keyId, alg, pub, frame} | null, pqRequired?: boolean }.
 * `pqFloor` is the browser-persisted "a PQ-required delegation was accepted" flag.
 */
export const verifyDelegation = (raw, pin, { now = Date.now(), floor = null, pqFloor = false } = {}) => {
  const ids = Object.keys(pin.keys);
  const pubs = new Set();
  let usable = ids.length > 0 && Number.isSafeInteger(pin.threshold) && pin.threshold >= 1 && pin.threshold <= ids.length;
  for (const i of usable ? ids : []) {
    const r = describeRoot(pin.keys[i]);
    if (!r || pubs.has(r.pub)) { usable = false; break; }
    pubs.add(r.pub);
  }
  if (!usable) return { ok: false, reason: "root_unset" };
  const pq = pin.pq === undefined || pin.pq === null ? null : describePq(pin.pq);
  if (pin.pq !== undefined && pin.pq !== null && pq === null) return { ok: false, reason: "pq_root_unset" };
  const d = parseDelegation(raw);
  if (!d) return { ok: false, reason: "malformed_delegation" };
  const good = new Set();
  for (const s of d.signatures) {
    const root = Object.hasOwn(pin.keys, s.keyId) ? describeRoot(pin.keys[s.keyId]) : null;
    if (root === null || s.alg !== "ed25519" || good.has(s.keyId) || (s.frame ?? FRAME_RAW) !== root.frame) continue;
    const framed = frameDelegationPayload(root.frame, d, root.pub);
    if (framed && edVerify(root.pub, framed, s.sig)) good.add(s.keyId);
  }
  if (good.size < pin.threshold) return { ok: false, reason: "root_threshold" };
  if (now >= timeMs(d.expires)) return { ok: false, reason: "delegation_expired" };
  if (floor !== null && d.version < floor) return { ok: false, reason: "delegation_rollback" };
  // ADR-0182: required once the build, the persisted floor or this root-signed delegation says so; a present PQ signature must verify.
  const pqRequired = pin.pqRequired === true || pqFloor === true || d.pqRequired === true;
  if (pq === null) {
    if (pqRequired) return { ok: false, reason: "pq_root_unset" };
  } else {
    const entries = d.signatures.filter((s) => s.keyId === pq.keyId);
    if (entries.length === 0) {
      if (pqRequired) return { ok: false, reason: "pq_missing" };
    } else if (!entries.some((s) => s.alg === pq.alg && s.frame === pq.frame && pqVerify(pq, delegationPayload(d), s.sig))) return { ok: false, reason: "pq_invalid" };
  }
  return pqRequired ? { ok: true, delegation: d, pqRequired: true } : { ok: true, delegation: d };
};

/** delegation -> release key. Mirror of resolveReleaseKey: revoked_key, unlisted_key, alg_mismatch, key_not_yet_valid, key_expired. */
export const resolveReleaseKey = (d, keyId, alg, now = Date.now()) => {
  if (d.revoked.includes(keyId)) return { ok: false, reason: "revoked_key" };
  const k = d.keys.find((x) => x.keyId === keyId);
  if (!k) return { ok: false, reason: "unlisted_key" };
  if (k.alg !== "ed25519" || alg !== k.alg) return { ok: false, reason: "alg_mismatch" };
  if (now < timeMs(k.notBefore)) return { ok: false, reason: "key_not_yet_valid" };
  if (now > timeMs(k.notAfter)) return { ok: false, reason: "key_expired" };
  return { ok: true, pub: k.pub };
};

/**
 * Full chain for a manifest: pinned root -> delegation -> release key -> manifest signature. Returns { ok:true,
 * delegationVersion } or { ok:false, reason } with the same reasons as verifyBundleSignature in src/update/signature.ts.
 */
export const verifyManifestSignature = (manifest, pin, { now = Date.now(), floor = null, pqFloor = false } = {}) => {
  const s = manifest.signature;
  if (s === null || s === undefined) return { ok: false, reason: "unsigned" };
  if (!isRecord(s) || typeof s.alg !== "string" || typeof s.keyId !== "string" || typeof s.sig !== "string") return { ok: false, reason: "bad_signature" };
  if (manifest.delegation === undefined) return { ok: false, reason: "no_delegation" };
  const d = verifyDelegation(manifest.delegation, pin, { now, floor, pqFloor });
  if (!d.ok) return d;
  const k = resolveReleaseKey(d.delegation, s.keyId, s.alg, now);
  if (!k.ok) return k;
  const payload = Buffer.from(canonicalJson({ ...manifest, signature: null }), "utf8");
  return edVerify(k.pub, payload, s.sig) ? { ok: true, delegationVersion: d.delegation.version, ...(d.pqRequired ? { pqRequired: true } : {}) } : { ok: false, reason: "bad_signature" };
};
// Pinned PQ root (ADR-0182): null until the operator pins the ML-DSA-65 root key (deploy/KEY-CEREMONY-PQ.md). Must equal PQ_ROOT /
// PQ_REQUIRED_BY_BUILD in src/update/pq-root.ts (pq-cosign.test.ts checks). Kept apart from PINNED_ROOTS so the two pins are committed independently.
export const PINNED_PQ_ROOT = null;
export const PINNED_PQ_REQUIRED = false;
const UNSERVED = new Set(["_headers"]);

/** Returns { ok, errors, info }. Pure over the filesystem; exported for tests. */
export const verifyRelease = (dir, { requireSigned = false, roots = PINNED_ROOTS, threshold = PINNED_ROOT_THRESHOLD, now = Date.now(), delegationFloor = null, pq = roots === PINNED_ROOTS ? PINNED_PQ_ROOT : null, pqRequired = roots === PINNED_ROOTS ? PINNED_PQ_REQUIRED : false, pqFloor = false } = {}) => {
  const errors = [];
  const info = {};
  const site = join(dir, "site");
  // 1. SHA256SUMS
  try {
    for (const line of readFileSync(join(dir, "SHA256SUMS"), "utf8").split("\n").filter(Boolean)) {
      const m = /^([0-9a-f]{64}) [ *](.+)$/.exec(line);
      if (!m) { errors.push(`SHA256SUMS: malformed line: ${line}`); continue; }
      const [, want, rel] = m;
      if (rel.split("/").includes("..") || rel.startsWith("/")) { errors.push(`SHA256SUMS: unsafe path ${rel}`); continue; }
      try { if (sha(readFileSync(join(dir, rel))) !== want) errors.push(`SHA256SUMS: hash mismatch ${rel}`); } catch { errors.push(`SHA256SUMS: missing ${rel}`); }
    }
  } catch { errors.push("SHA256SUMS: unreadable"); }
  // 2. bundle manifest vs. site/
  let manifest;
  try { manifest = JSON.parse(readFileSync(join(site, "bundle-manifest.json"), "utf8")); } catch { errors.push("site/bundle-manifest.json: unreadable"); }
  if (manifest) {
    const listed = new Set();
    for (const f of manifest.files ?? []) {
      listed.add(f.path);
      if (typeof f.path !== "string" || f.path.startsWith("/") || f.path.split("/").some((s) => s === ".." || s === "." || s === "")) { errors.push(`manifest: unsafe path ${f.path}`); continue; }
      try {
        const b = readFileSync(join(site, f.path));
        if (sha(b) !== f.sha256) errors.push(`manifest: hash mismatch ${f.path}`);
        if (b.byteLength !== f.bytes) errors.push(`manifest: size mismatch ${f.path}`);
      } catch { errors.push(`manifest: missing ${f.path}`); }
    }
    for (const abs of walk(site)) {
      const rel = relative(site, abs).split(sep).join("/");
      if (rel !== "bundle-manifest.json" && !UNSERVED.has(rel) && !listed.has(rel)) errors.push(`site: unlisted file ${rel}`);
    }
    info.version = manifest.version;
    info.dev = manifest.dev === true;
    info.signed = manifest.signature !== null && manifest.signature !== undefined;
    info.keyId = manifest.signature?.keyId;
    if (info.signed) {
      const r = verifyManifestSignature(manifest, { keys: roots, threshold, pq, pqRequired }, { now, floor: delegationFloor, pqFloor });
      info.signatureValid = r.ok;
      if (r.ok) info.delegationVersion = r.delegationVersion;
      else errors.push(`signature: ${r.reason}`);
    }
    if (requireSigned && (!info.signed || info.dev)) errors.push("--require-signed: bundle is " + (info.dev ? "a dev build" : "unsigned"));
  }
  return { ok: errors.length === 0, errors, info };
};

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  const args = process.argv.slice(2);
  const dir = resolve(args.find((a) => !a.startsWith("--")) ?? dirname(fileURLToPath(import.meta.url)));
  try { statSync(dir); } catch { console.error(`no such directory: ${dir}`); process.exit(2); }
  const r = verifyRelease(dir, { requireSigned: args.includes("--require-signed") });
  for (const e of r.errors) console.error("FAIL:", e);
  const i = r.info;
  if (i.signed && !i.signatureValid && Object.keys(PINNED_ROOTS).length === 0) console.error("HINT: no offline root key is pinned in this verifier yet (key ceremony pending: deploy/KEY-CEREMONY.md), so no signed release can verify.");
  if (r.errors.some((e) => e.startsWith("signature: pq_"))) console.error("HINT: the delegation needs the ML-DSA-65 post-quantum co-signature of the pinned PQ root (ADR-0182); this check needs Node 24 with ML-DSA support in node:crypto (tested on 24.14).");
  console.log(`${r.ok ? "OK" : "FAILED"}: version ${i.version ?? "?"}, ${i.dev ? "DEV build (unsigned, not for production)" : i.signed ? `signed (key ${i.keyId}, delegation v${i.delegationVersion}; verified root -> delegation -> release key -> manifest)` : "UNSIGNED"}`);
  process.exit(r.ok ? 0 : 1);
}
