#!/usr/bin/env node
// Standalone, dependency-free verifier for an OM web release folder. Shipped inside every release as verify.mjs.
// Usage: node verify.mjs [releaseDir] [--require-signed] [--require-provenance]
// Checks: SHA256SUMS against every file, every bundle-manifest.json entry (hash + size), no unlisted files in site/.
// With a signature present it verifies the chain pinned ROOT key -> root-signed delegation (unexpired, release key listed,
// in window, not revoked) -> Ed25519 manifest signature. The delegation version floor lives in the browser (service worker).
// provenance.intoto.jsonl (when present) is verified too: DSSE signature by the delegated key, subjects = bundle-manifest.json and SHA256SUMS. --require-provenance makes it mandatory
// (--require-signed does not, so releases made before provenance existed still verify).
// --require-signed additionally fails on unsigned/dev bundles. The root pin must equal src/update/release-key.ts (release.test.ts checks).
import { createHash, createPublicKey, verify } from "node:crypto";
import { existsSync, readdirSync, readFileSync, realpathSync, statSync } from "node:fs";
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
export const PINNED_ROOTS = {
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
export const PINNED_PQ_ROOT = {
  "keyId": "pq-root-b44d71873569f5f4",
  "alg": "ml-dsa-65",
  "pub": "63578eff00b9b5f35e4b50287791b0872de3b1d23a699b7fc73ca250ef8ba26d3b0298ec9acde0258b134afff3e904fddef51bf0eb24aa0b19871b88364e27cb1d3099c015fb106812b38acf058cb711149a01a09f0d48a78cbc1e8814044afe473d145f48633f8b81fcf3184039459def75fa64a5af125121bb08bc8231069ac5a314b2b9bb82cd401711ad30c87d88d02b56072b17e0004329f91179d3ac5e734504129fbf6cb8c0a43a01b0b8e89090d19972ea5c8093c85737f8f83e41b523944f33c1782de5b8f9311cb3210ff009298f2657b0749abb1637903c35e89ed57ed1a1c04d6b72ae9c44b0bc4afd21c42712aaf2a45d1e67326c75c3457e781c366e6a20fc0b0c016fec4d06857b5378edf5ffa328e6880191fa750273face2fc6026c07e553c345fe9f1fd4af08e6a9d79f6041ee515c4d0f2d0495da48889930ecd373c2c1c8cd8d4f56bec8b42288179f5f64025f7caf53c9b127ec5faf4d7ddfea66df1a177ea488babfb1a8eee4eec2dec5f52d7e9c9ee7b67142a2945bce9b0c01764927cfc56e7ccec42749f1ad8cae3d986a2e4ab24047ee084b87095721526001065400b86e0f90c0d5d036a32643a01907d99bb286d8ac07d8d85c1134492f2951b53cd352d7cfe35f52dfa2ea1673a4c3fdf9bdd3a93dd06809c578c4ef1e44fd87259f29dc4669709878c9f634f0ce2b43040a60ae8c5c411028c0af45e77299551ff91fa03a031a12d7ddc7b38628e36c10e061f990b910ef3ca0392312e1cd8aad2cc922b2637b60afcacae6b02dd0e4c248f0a4b16dc22cc467a8213de0a697b748b21bad2edc126d9a69e8f832d8f40b1fb33a642371194ca0a5fb923af1a0a364b802a35984b263ac5e8fad975d6470723393d083481628f6f9f67fc061c54e447f4235c5ecf66ca7f2e96063c119a5beb1e2c810a1449b5b35c9e44b1e72e08cfc94ef04d16601e4e7ae6040ed56f7cc84239ab073d74e83cf9ccbce7104c96f44e8fced67a54f16daebf5b4b5ef90f08e7bc801eb1e14183460337fcee4beacff11457c82c0adc01e50ff1d25fbb64807eb11dc01398b3bfa94059cccf7bcf087b10a83f63f0f19affba47fb98f8781318006036ee565ac6b4a1fe8b9a9623e2782baeb4c891537bfc534cedf410620240db91b094fe7842bab66237eba94bffd48d1ef41236eb3bce85fa42c419d9af21c86cf55f9dc838450d71bf6e06c84b66382e1dd28dc6a88827db3b4e914140bd3f75b6e0accc9e5d29d2b4f06e73012b15f8d65db6b8a5adafd5186ec7f83063fa8d8d45536a1011f5c6359da8efdb57ad37fd2744ba4108694a79fbb0caefa1c4d5bc7b7bf830cc6eed80ab409e4183996f09ceb23d56890b30c510d45104e2f267c7788815c21648e9f0d6db9a4c1850d8abbc26d57cdbeb855fd563da96b06e0c9ddaeabf8077ae603f7fcc28dbd4b4b6c6bb9d40ff028c34b76268c56d28812abcee698f9e9e50f57af35cf93743a5568026c4960b08196c092808fb599a8818bafa0bbf95e298940e98e7e541ee157de980a7fee1975496b195a20440e1cf9af34318e5fd151a0651b982b667b6847513ab8f723fbd1989cac0da7e8e1448353b0331ced4e2096cd72ec16236ffd3adee8383d79eefc1cb5231e5c119773f8bbef024420c02c7126d53f101875e020db41a991d3f6cebe6d4881ae6dde707734d29d10e398aac3b5e493eb8e57fe262d0e507c2b206214fb6af1f58fed79e0d74f7083148aa1fb492931f80d95eba4320b9f14666668ab0aacd33e0c0d9d6ac5daf5d12b86e1448a6e5d9d1090a66d84f22fc09d231dc828b71d26d95f25831862bb5ccdeb227d05a419d1a523bdd370142b6e47eda536b8838ae277ce79bd49e4c31c00fc6f607eabf8dab3051322e109ceccc8a8fccb29e1e66751a0e29d77c49fcaa37b9d51e6dc5245fa5120c45bf5fb84ab981e232b2c00cda83d018b1891be09986d882a27cf51f1124d7398746fc3f94595f33c6512390e894349dae96809717628474c26d8781d48419433d3759bafa0256166df101327692d67868ffd37779f868b2ec86b5241fc3c5204053431c9f21b22b24e7e9f1f85b61f969a4a4a62b9f7afb3a0a7767ac05e6bc84d8869496f8834808c979e663dbabfe73f251206bef18a7e8ff401b9f8307462a054dace553c06cfd2ee21fe8111d61e14ab0ce9027f005e1a35ae500437c01ddd07c1d0504f836e79d957f7bbf1f3979be655fafdefe9b5fc82972d3c341cd60c079086a210f2f4a73dc70a545e0123fb2866400bec59430339f57395d1fd4c2111ff1fb0fd6922a8c0f2134d9d0c7ecadefc0ebaaf677f955bd20a3793e3dbc61e53120582c7292f04d23b4510f539fe67a4b488401dc303658bfded564bf207a05cfb18f839d143d5be1dbb32328f72d9e9e2ed7deb794aa72295e7750411dcac23d4b4ce007c9af5f2f9c2c44b7f6961585f5108a9ead8e36f5b9f5429ef73f58692437a5165008811b577f89f7bfc5d6c5576aa389fefa98a8e09c97d71933fd808c7e52d845f3e01f5a307dbd25145a5ba6f3f46106206ae604ef5c4f347260de0cd0ddacc6a03c4eca756f278e11f579731d40bf5972db25bef0a90c5c34a8a887c9c40371e3a514e86671ef9c7b0a077b44b0af2e3f193b3f29f0200b8299b41d822b12851e776b5d001f5807a7ca793cbedb90f6c22d8e6817a7ea6bd3d4bd760814904f26aa6be5f1727616f652c",
  "frame": "om-pq-v1"
};
export const PINNED_PQ_REQUIRED = false;
// ---- Release provenance (in-toto Statement v1 in a DSSE envelope, signed by the delegated release key; see provenance.mjs) ----
export const PROVENANCE_FILE = "provenance.intoto.jsonl";
export const DSSE_PAYLOAD_TYPE = "application/vnd.in-toto+json";
export const STATEMENT_TYPE = "https://in-toto.io/Statement/v1";
export const SLSA_PREDICATE_TYPE = "https://slsa.dev/provenance/v1";
/** DSSE v1 pre-authentication encoding. */
export const dssePae = (type, body) => Buffer.concat([Buffer.from(`DSSEv1 ${Buffer.byteLength(type)} ${type} ${body.length} `, "utf8"), body]);
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const edVerifyPae = (hex, payload, sigB64) => {
  try { return verify(null, payload, createPublicKey({ key: Buffer.concat([ED25519_SPKI_PREFIX, Buffer.from(hex, "hex")]), format: "der", type: "spki" }), Buffer.from(sigB64, "base64")); } catch { return false; }
};
/**
 * Verifies `<dir>/provenance.intoto.jsonl`: DSSE signature by a release key that the pinned root authorises (the delegation carried by the
 * envelope must be the one embedded in the manifest), statement/predicate types, and that every subject digest matches the file in `dir`.
 * Returns { ok, errors, info }. A missing file is reported as one error ("missing"); callers decide whether that is acceptable.
 */
export const verifyProvenance = (dir, { roots = PINNED_ROOTS, threshold = PINNED_ROOT_THRESHOLD, now = Date.now(), delegationFloor = null, pq = roots === PINNED_ROOTS ? PINNED_PQ_ROOT : null, pqRequired = roots === PINNED_ROOTS ? PINNED_PQ_REQUIRED : false, pqFloor = false } = {}) => {
  const errors = [];
  const info = {};
  const bad = (m) => { errors.push(m); return { ok: false, errors, info }; };
  let text;
  try { text = readFileSync(join(dir, PROVENANCE_FILE), "utf8"); } catch { return bad("missing"); }
  const lines = text.split("\n").filter(Boolean);
  if (lines.length !== 1) return bad(`expected exactly one envelope line, found ${lines.length}`);
  let env;
  try { env = JSON.parse(lines[0]); } catch { return bad("envelope is not JSON"); }
  if (!isRecord(env) || env.payloadType !== DSSE_PAYLOAD_TYPE || typeof env.payload !== "string" || !Array.isArray(env.signatures) || env.signatures.length !== 1) return bad("not a DSSE envelope of the expected shape");
  const sig = env.signatures[0];
  if (!isRecord(sig) || typeof sig.keyid !== "string" || typeof sig.sig !== "string") return bad("bad envelope signature entry");
  const body = Buffer.from(env.payload, "base64");
  // chain: pinned root -> delegation (must equal the manifest's) -> release key -> envelope signature
  let manifest;
  try { manifest = JSON.parse(readFileSync(join(dir, "site", "bundle-manifest.json"), "utf8")); } catch { return bad("site/bundle-manifest.json unreadable"); }
  if (manifest.delegation === undefined || canonicalJson(manifest.delegation) !== canonicalJson(env.delegation ?? null)) return bad("delegation differs from the one embedded in the manifest");
  const d = verifyDelegation(env.delegation, { keys: roots, threshold, pq, pqRequired }, { now, floor: delegationFloor, pqFloor });
  if (!d.ok) return bad(`delegation: ${d.reason}`);
  const k = resolveReleaseKey(d.delegation, sig.keyid, "ed25519", now);
  if (!k.ok) return bad(`release key: ${k.reason}`);
  if (manifest.signature?.keyId !== sig.keyid) return bad("signed by a different key than the manifest");
  if (!edVerifyPae(k.pub, dssePae(env.payloadType, body), sig.sig)) return bad("bad_signature");
  let st;
  try { st = JSON.parse(body.toString("utf8")); } catch { return bad("payload is not JSON"); }
  if (!isRecord(st) || st._type !== STATEMENT_TYPE || st.predicateType !== SLSA_PREDICATE_TYPE || !Array.isArray(st.subject) || st.subject.length === 0) return bad("not an in-toto v1 SLSA provenance statement");
  const names = new Set();
  for (const s of st.subject) {
    const want = isRecord(s) && isRecord(s.digest) ? s.digest.sha256 : undefined;
    if (!isRecord(s) || typeof s.name !== "string" || typeof want !== "string" || s.name.startsWith("/") || s.name.split("/").some((x) => x === ".." || x === "." || x === "")) { errors.push("bad subject entry"); continue; }
    names.add(s.name);
    try { if (sha(readFileSync(join(dir, s.name))) !== want) errors.push(`subject digest mismatch ${s.name}`); } catch { errors.push(`subject missing ${s.name}`); }
  }
  for (const need of ["site/bundle-manifest.json", "SHA256SUMS"]) if (!names.has(need)) errors.push(`subject ${need} not covered`);
  const ep = st.predicate?.buildDefinition?.externalParameters;
  if (!isRecord(ep) || ep.version !== manifest.version) errors.push("externalParameters.version differs from the manifest version");
  info.commit = isRecord(ep) ? ep.commit : undefined;
  info.keyId = sig.keyid;
  info.builder = st.predicate?.runDetails?.builder?.id;
  return { ok: errors.length === 0, errors, info };
};
const UNSERVED = new Set(["_headers"]);

/** Returns { ok, errors, info }. Pure over the filesystem; exported for tests. */
export const verifyRelease = (dir, { requireSigned = false, roots = PINNED_ROOTS, threshold = PINNED_ROOT_THRESHOLD, now = Date.now(), delegationFloor = null, pq = roots === PINNED_ROOTS ? PINNED_PQ_ROOT : null, pqRequired = roots === PINNED_ROOTS ? PINNED_PQ_REQUIRED : false, pqFloor = false, requireProvenance = false } = {}) => {
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
  // 3. provenance (optional: releases made before B4 have none; --require-provenance makes it mandatory)
  if (existsSync(join(dir, PROVENANCE_FILE)) || requireProvenance) {
    const p = verifyProvenance(dir, { roots, threshold, now, delegationFloor, pq, pqRequired, pqFloor });
    info.provenance = p.ok ? { ok: true, ...p.info } : { ok: false };
    // When the manifest's own chain already failed, the same delegation/key failure would only repeat in the provenance check.
    for (const e of p.errors) if (!(info.signatureValid === false && /^(delegation|release key): /.test(e))) errors.push(`provenance: ${e}`);
  }
  return { ok: errors.length === 0, errors, info };
};

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  const args = process.argv.slice(2);
  const dir = resolve(args.find((a) => !a.startsWith("--")) ?? dirname(fileURLToPath(import.meta.url)));
  try { statSync(dir); } catch { console.error(`no such directory: ${dir}`); process.exit(2); }
  const r = verifyRelease(dir, { requireSigned: args.includes("--require-signed"), requireProvenance: args.includes("--require-provenance") });
  for (const e of r.errors) console.error("FAIL:", e);
  const i = r.info;
  if (i.signed && !i.signatureValid && Object.keys(PINNED_ROOTS).length === 0) console.error("HINT: no offline root key is pinned in this verifier yet (key ceremony pending: deploy/KEY-CEREMONY.md), so no signed release can verify.");
  if (r.errors.some((e) => e.startsWith("signature: pq_"))) console.error("HINT: the delegation needs the ML-DSA-65 post-quantum co-signature of the pinned PQ root (ADR-0182); this check needs Node 24 with ML-DSA support in node:crypto (tested on 24.14).");
  if (i.provenance?.ok) console.log(`provenance: verified (commit ${i.provenance.commit}, builder ${i.provenance.builder})`);
  console.log(`${r.ok ? "OK" : "FAILED"}: version ${i.version ?? "?"}, ${i.dev ? "DEV build (unsigned, not for production)" : i.signed ? `signed (key ${i.keyId}, delegation v${i.delegationVersion}; verified root -> delegation -> release key -> manifest)` : "UNSIGNED"}`);
  process.exit(r.ok ? 0 : 1);
}
