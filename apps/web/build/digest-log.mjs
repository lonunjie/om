// Single-operator, append-only, signed release-digest log (PR-005, ADR-0183 section 3). One JSON line per release:
//   {version, commit, manifestSha256, sha256sumsSha256, time, prev, signature:{alg,keyId,delegation:{version,sha256},sig}}
// `prev` is the sha256 (hex) of the previous line's exact text (genesis = 64 zeros), so editing or dropping a line breaks every later link.
// `sig` is Ed25519 by the delegated release key over DOMAIN + canonical JSON of the entry without `sig`; the delegation is referenced by
// version and sha256 of its canonical JSON and stored under delegations/<sha256>.json next to the log (also embedded in each signed manifest).
// This is NOT a transparency log: one operator writes it, nobody independent monitors it, and the operator could rewrite it and re-sign it
// (see deploy/release-log/README.md). It only makes casual after-the-fact edits visible to someone holding an earlier copy.
import { createHash, createPrivateKey, createPublicKey, sign, verify } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync, appendFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalJson, requireDelegation } from "./sign.mjs";
import { PINNED_PQ_ROOT, PINNED_PQ_REQUIRED, PINNED_ROOTS, PINNED_ROOT_THRESHOLD, resolveReleaseKey, verifyDelegation } from "./verify-release.mjs";

export const GENESIS = "0".repeat(64);
export const LOG_DOMAIN = "om.release-log/1\n";
const repoRoot = resolve(fileURLToPath(new URL("../../..", import.meta.url)));
export const DEFAULT_LOG = join(repoRoot, "deploy/release-log/digests.jsonl");
const sha = (b) => createHash("sha256").update(b).digest("hex");
const HEX64 = /^[0-9a-f]{64}$/;
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const delegationsDir = (logFile) => join(dirname(logFile), "delegations");
const unsignedPart = (e) => ({ ...e, signature: { alg: e.signature.alg, keyId: e.signature.keyId, delegation: e.signature.delegation } });
const entryPayload = (e) => Buffer.from(LOG_DOMAIN + canonicalJson(unsignedPart(e)), "utf8");
const lineTexts = (file) => (existsSync(file) ? readFileSync(file, "utf8").split("\n").filter((l) => l !== "") : []);

/**
 * Appends one signed entry to `logFile` (created if absent) and stores the delegation beside it. Refuses (throws) unless sign.mjs requireDelegation
 * passes, or if `version` is already logged. Returns the entry. `time` defaults to now.
 */
export const appendEntry = (logFile, { version, commit, manifestSha256, sha256sumsSha256, time = new Date().toISOString() }, pem, keyId, delegation, trust = {}) => {
  if (typeof version !== "string" || version === "") throw new Error("digest-log: version required");
  if (typeof commit !== "string" || !/^[0-9a-f]{7,64}$/.test(commit)) throw new Error("digest-log: commit must be a hex git commit id");
  if (!HEX64.test(manifestSha256 ?? "") || !HEX64.test(sha256sumsSha256 ?? "")) throw new Error("digest-log: digests must be 64 hex chars");
  if (Number.isNaN(Date.parse(time))) throw new Error("digest-log: bad time");
  const key = createPrivateKey(pem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("signing key is not ed25519");
  const d = requireDelegation(delegation, pem, keyId, trust);
  const lines = lineTexts(logFile);
  const prior = lines.map((l) => JSON.parse(l));
  if (prior.some((e) => e.version === version)) throw new Error(`digest-log: version ${version} is already logged (the log is append-only)`);
  const delSha = sha(Buffer.from(canonicalJson(d), "utf8"));
  const entry = { version, commit, manifestSha256, sha256sumsSha256, time, prev: lines.length === 0 ? GENESIS : sha(Buffer.from(lines[lines.length - 1], "utf8")), signature: { alg: "ed25519", keyId, delegation: { version: d.version, sha256: delSha } } };
  entry.signature.sig = sign(null, entryPayload(entry), key).toString("base64");
  mkdirSync(delegationsDir(logFile), { recursive: true });
  const dp = join(delegationsDir(logFile), `${delSha}.json`);
  if (!existsSync(dp)) writeFileSync(dp, JSON.stringify(d, null, 2) + "\n");
  mkdirSync(dirname(logFile), { recursive: true });
  appendFileSync(logFile, (lines.length === 0 || readFileSync(logFile, "utf8").endsWith("\n") ? "" : "\n") + JSON.stringify(entry) + "\n");
  return entry;
};

/**
 * Verifies the whole log: line format, `prev` chain from genesis, unique versions, and each signature (pinned root -> stored delegation -> release key).
 * `delegations` (array) overrides the delegations/ directory. Returns { ok, errors, entries }.
 */
export const verifyLog = (logFile, { roots = PINNED_ROOTS, threshold = PINNED_ROOT_THRESHOLD, now = Date.now(), pq = roots === PINNED_ROOTS ? PINNED_PQ_ROOT : null, pqRequired = roots === PINNED_ROOTS ? PINNED_PQ_REQUIRED : false, delegations } = {}) => {
  const errors = [];
  const byHash = new Map();
  const pool = delegations ?? (existsSync(delegationsDir(logFile)) ? readdirSync(delegationsDir(logFile)).filter((f) => f.endsWith(".json")).map((f) => JSON.parse(readFileSync(join(delegationsDir(logFile), f), "utf8"))) : []);
  for (const d of pool) byHash.set(sha(Buffer.from(canonicalJson(d), "utf8")), d);
  const lines = lineTexts(logFile);
  const versions = new Set();
  let prev = GENESIS;
  const entries = [];
  lines.forEach((text, i) => {
    const at = `line ${i + 1}`;
    let e;
    try { e = JSON.parse(text); } catch { errors.push(`${at}: not JSON`); prev = sha(Buffer.from(text, "utf8")); return; }
    const s = e?.signature;
    const shaped = e && typeof e === "object" && typeof e.version === "string" && typeof e.commit === "string" && HEX64.test(e.manifestSha256 ?? "") && HEX64.test(e.sha256sumsSha256 ?? "") && typeof e.time === "string" && HEX64.test(e.prev ?? "")
      && s && s.alg === "ed25519" && typeof s.keyId === "string" && typeof s.sig === "string" && HEX64.test(s.delegation?.sha256 ?? "") && Number.isInteger(s.delegation?.version);
    if (!shaped) errors.push(`${at}: malformed entry`);
    else {
      if (e.prev !== prev) errors.push(`${at}: prev does not match the previous line (log edited, reordered or truncated)`);
      if (versions.has(e.version)) errors.push(`${at}: duplicate version ${e.version}`);
      versions.add(e.version);
      const del = byHash.get(s.delegation.sha256);
      if (!del) errors.push(`${at}: delegation ${s.delegation.sha256.slice(0, 12)} not found`);
      else {
        const dv = verifyDelegation(del, { keys: roots, threshold, pq, pqRequired }, { now });
        if (!dv.ok) errors.push(`${at}: delegation: ${dv.reason}`);
        else {
          // The key must have been valid when the entry was written (entry.time), so a later-expired key does not invalidate history.
          const k = resolveReleaseKey(dv.delegation, s.keyId, "ed25519", Date.parse(e.time));
          if (!k.ok) errors.push(`${at}: release key: ${k.reason}`);
          else {
            let ok = false;
            try { ok = verify(null, entryPayload(e), createPublicKey({ key: Buffer.concat([ED25519_SPKI_PREFIX, Buffer.from(k.pub, "hex")]), format: "der", type: "spki" }), Buffer.from(s.sig, "base64")); } catch { /* bad sig */ }
            if (!ok) errors.push(`${at}: bad_signature`);
          }
        }
      }
      entries.push(e);
    }
    prev = sha(Buffer.from(text, "utf8"));
  });
  return { ok: errors.length === 0, errors, entries };
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [cmd, file] = process.argv.slice(2);
  if (cmd !== "verify") { console.error("usage: node build/digest-log.mjs verify [digests.jsonl]  (appending happens inside release.mjs --append-digest-log)"); process.exit(2); }
  const r = verifyLog(resolve(file ?? DEFAULT_LOG));
  for (const e of r.errors) console.error("FAIL:", e);
  console.log(`${r.ok ? "OK" : "FAILED"}: ${r.entries.length} entries (single-operator log, not a transparency log)`);
  process.exit(r.ok ? 0 : 1);
}
