#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// OM relay release verifier. Ships in every relay template as verify.mjs (this file, copied by build.mjs), next to verify-chain.mjs (a byte copy of apps/web/build/verify-release.mjs:
// the same pinned offline root, delegation and Ed25519/ML-DSA-65 checks the web release uses). Dependency-free; Node 24 (ML-DSA in node:crypto).
// PROVISIONING_AND_BOOTSTRAP.md section 10 / R-261: nothing is deployed unless the Worker in relay/ is a signed release pinned by digest.
// Usage: node verify.mjs [dir] [--min-version x.y.z]        Exit 0 only when everything below holds, else 1 (the deploy script stops).
// Checks: release.lock pins the manifest digest and the version (and an owner-set floor); the manifest is signed (pinned root -> delegation -> release
// key -> signature); every listed file matches its hash and size and relay/ holds nothing else; SHA256SUMS and provenance.intoto.jsonl agree;
// wrangler.json and package.json carry nothing but the signed configuration (only the Worker name and the prompt text may differ); no other config
// file that could change what is built or deployed is present; an OM_ADMISSION in the build environment, if visible, is well formed.
// Limit (said plainly): this file is part of the template, so it cannot vouch for itself. Run the copy from a trusted checkout of lonunjie/om
// (infra/cloudflare-home/release/verify-relay.mjs next to verify-chain.mjs) against your template to check the template's own copy: README "Check it yourself".
import { createHash, createPublicKey, verify } from "node:crypto";
import { existsSync, readdirSync, readFileSync, realpathSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { DSSE_PAYLOAD_TYPE, PINNED_PQ_ROOT, PINNED_ROOTS, PINNED_ROOT_THRESHOLD, PROVENANCE_FILE, SLSA_PREDICATE_TYPE, STATEMENT_TYPE, canonicalJson, dssePae, resolveReleaseKey, verifyDelegation, verifyManifestSignature } from "./verify-chain.mjs";

export const MANIFEST_FORMAT = "om.relay-release/1";
export const MANIFEST_FILE = "relay-manifest.json";
export const LOCK_FILE = "release.lock";
export const SUMS_FILE = "SHA256SUMS";
export const WORKER_FILE = "relay/worker.js";

const sha = (b) => createHash("sha256").update(b).digest("hex");
const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
const HEX64 = /^[0-9a-f]{64}$/;
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const safePath = (p) => typeof p === "string" && p.length > 0 && !p.startsWith("/") && !p.includes("\\") && !p.split("/").some((s) => s === "" || s === "." || s === "..");
/** -1, 0, 1; null when either is not strict x.y.z (callers fail closed). */
export const compareVersions = (a, b) => {
  const ma = SEMVER.exec(a), mb = SEMVER.exec(b);
  if (!ma || !mb) return null;
  for (let i = 1; i <= 3; i++) { const d = Number(ma[i]) - Number(mb[i]); if (d !== 0) return d < 0 ? -1 : 1; }
  return 0;
};
const walk = (d) => readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(d, e.name)) : [join(d, e.name)]));
const readJson = (p) => { try { return JSON.parse(readFileSync(p, "utf8")); } catch { return undefined; } };

// Files that could change what is built or deployed without being part of the signed configuration. Defence in depth, not a complete list.
const FOREIGN_CONFIG = [/^wrangler\.(toml|jsonc)$/, /^\.npmrc$/, /^\.yarnrc(\.yml)?$/, /^\.pnpmfile\.c?js$/, /^pnpm-workspace\.yaml$/, /^bunfig\.toml$/, /^\.env(\..*)?$/, /^\.dev\.vars(\..*)?$/, /^\.wrangler$/];
const FOREIGN_ALLOWED = new Set([".dev.vars.example"]);
/** The Worker name and the Cloudflare prompt text are the only things a person may change. */
const withoutOwnerFields = (wrangler) => { const { name: _n, $schema: _s, ...rest } = wrangler; return rest; };
const withoutOwnerPackageFields = (pkg) => { const { name: _n, version: _v, description: _d, private: _p, cloudflare: _c, ...rest } = pkg; return rest; };
const differingKeys = (a, b) => [...new Set([...Object.keys(a), ...Object.keys(b)])].filter((k) => canonicalJson(a[k]) !== canonicalJson(b[k])).sort();

const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");
const edVerify = (hex, payload, sigB64) => {
  try { return verify(null, payload, createPublicKey({ key: Buffer.concat([SPKI_ED25519, Buffer.from(hex, "hex")]), format: "der", type: "spki" }), Buffer.from(sigB64, "base64")); } catch { return false; }
};

/**
 * Verifies `<dir>/provenance.intoto.jsonl` (in-toto Statement v1, SLSA v1 predicate, DSSE envelope): signed by a release key the pinned root authorises, under the
 * delegation the manifest embeds, with subjects relay-manifest.json and SHA256SUMS matching the files here. The same checks as verifyProvenance in verify-chain.mjs,
 * which is fixed to the web release's file names. Returns an array of errors.
 */
const provenanceErrors = (dir, manifest, { roots, threshold, pq, now }) => {
  const errors = [];
  const bad = (m) => { errors.push(m); return errors; };
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
  if (manifest.delegation === undefined || canonicalJson(manifest.delegation) !== canonicalJson(env.delegation ?? null)) return bad("delegation differs from the one embedded in the manifest");
  const d = verifyDelegation(env.delegation, { keys: roots, threshold, pq, pqRequired: pq !== null }, { now });
  if (!d.ok) return bad(`delegation: ${d.reason}`);
  const k = resolveReleaseKey(d.delegation, sig.keyid, "ed25519", now);
  if (!k.ok) return bad(`release key: ${k.reason}`);
  if (manifest.signature?.keyId !== sig.keyid) return bad("signed by a different key than the manifest");
  if (!edVerify(k.pub, dssePae(env.payloadType, body), sig.sig)) return bad("bad_signature");
  let st;
  try { st = JSON.parse(body.toString("utf8")); } catch { return bad("payload is not JSON"); }
  if (!isRecord(st) || st._type !== STATEMENT_TYPE || st.predicateType !== SLSA_PREDICATE_TYPE || !Array.isArray(st.subject) || st.subject.length === 0) return bad("not an in-toto v1 SLSA provenance statement");
  const names = new Set();
  for (const s of st.subject) {
    const want = isRecord(s) && isRecord(s.digest) ? s.digest.sha256 : undefined;
    if (!isRecord(s) || typeof s.name !== "string" || typeof want !== "string" || !safePath(s.name)) { errors.push("bad subject entry"); continue; }
    names.add(s.name);
    try { if (sha(readFileSync(join(dir, s.name))) !== want) errors.push(`subject digest mismatch ${s.name}`); } catch { errors.push(`subject missing ${s.name}`); }
  }
  for (const need of [MANIFEST_FILE, SUMS_FILE]) if (!names.has(need)) errors.push(`subject ${need} not covered`);
  const ep = st.predicate?.buildDefinition?.externalParameters;
  if (!isRecord(ep) || ep.version !== manifest.version || ep.commit !== manifest.createdFrom.gitCommit || ep.artifact !== "om-relay") errors.push("externalParameters differ from the manifest (version, commit or artifact)");
  return errors;
};

/** Strict structural check of the untrusted manifest; returns an error string or null. */
const manifestProblem = (m) => {
  if (!isRecord(m) || m.format !== MANIFEST_FORMAT) return "bad format";
  if (typeof m.version !== "string" || !SEMVER.test(m.version)) return "bad version";
  if (!Array.isArray(m.files) || m.files.length === 0) return "bad files";
  const seen = new Set();
  for (const f of m.files) {
    if (!isRecord(f) || !safePath(f.path) || typeof f.sha256 !== "string" || !HEX64.test(f.sha256) || !Number.isSafeInteger(f.bytes) || f.bytes < 0) return "bad file entry";
    if (f.path === MANIFEST_FILE || f.path === SUMS_FILE || seen.has(f.path)) return `duplicate or reserved path ${f.path}`;
    seen.add(f.path);
  }
  if (m.worker !== WORKER_FILE || !seen.has(WORKER_FILE)) return "worker is not relay/worker.js or not listed";
  if (!isRecord(m.config) || !isRecord(m.config.wrangler) || !isRecord(m.config.package)) return "bad config";
  if (!isRecord(m.createdFrom) || typeof m.createdFrom.gitCommit !== "string") return "bad createdFrom";
  if (m.delegation !== undefined && !isRecord(m.delegation)) return "bad delegation";
  return null;
};

/** Returns { ok, errors, info }. Pure over the filesystem (and `env`); exported for tests. */
export const verifyRelay = (dir, { roots = PINNED_ROOTS, threshold = PINNED_ROOT_THRESHOLD, pq = roots === PINNED_ROOTS ? PINNED_PQ_ROOT : null, now = Date.now(), minVersion = null, env = process.env } = {}) => {
  const errors = [];
  const info = {};
  const bad = (m) => { errors.push(m); };
  const manifestPath = join(dir, MANIFEST_FILE);
  let text;
  try { text = readFileSync(manifestPath); } catch { return { ok: false, errors: [`${MANIFEST_FILE}: unreadable`], info }; }
  let manifest;
  try { manifest = JSON.parse(text.toString("utf8")); } catch { return { ok: false, errors: [`${MANIFEST_FILE}: not JSON`], info }; }
  const mp = manifestProblem(manifest);
  if (mp) return { ok: false, errors: [`${MANIFEST_FILE}: ${mp}`], info };
  info.version = manifest.version;
  info.commit = manifest.createdFrom.gitCommit;

  // 1. the owner's pin: this exact manifest, at or above the owner's floor
  const lock = readJson(join(dir, LOCK_FILE));
  if (!isRecord(lock) || typeof lock.manifestSha256 !== "string" || !HEX64.test(lock.manifestSha256) || lock.version !== manifest.version) bad(`${LOCK_FILE}: missing or malformed, or its version differs from the manifest`);
  else if (lock.manifestSha256 !== sha(text)) bad(`${LOCK_FILE}: the pinned digest does not match ${MANIFEST_FILE} (a different release than the one pinned)`);
  for (const [what, floor] of [["release.lock minVersion", isRecord(lock) ? lock.minVersion : undefined], ["--min-version", minVersion ?? undefined]]) {
    if (floor === undefined) continue;
    const c = typeof floor === "string" ? compareVersions(manifest.version, floor) : null;
    if (c === null) bad(`${what}: not a strict x.y.z version`);
    else if (c < 0) bad(`${what}: release ${manifest.version} is below the floor ${floor} (rollback refused)`);
  }

  // 2. signature chain: pinned root -> delegation -> release key -> manifest signature. ML-DSA-65 co-signature required whenever a PQ root is pinned.
  const sig = verifyManifestSignature(manifest, { keys: roots, threshold, pq, pqRequired: pq !== null }, { now });
  info.signatureValid = sig.ok;
  if (sig.ok) { info.delegationVersion = sig.delegationVersion; info.keyId = manifest.signature.keyId; } else bad(`signature: ${sig.reason}`);

  // 3. every listed file, and nothing else under relay/
  const listed = new Set();
  for (const f of manifest.files) {
    listed.add(f.path);
    try {
      const b = readFileSync(join(dir, ...f.path.split("/")));
      if (sha(b) !== f.sha256) bad(`hash mismatch ${f.path}`);
      if (b.byteLength !== f.bytes) bad(`size mismatch ${f.path}`);
      if (f.path === WORKER_FILE) info.workerSha256 = sha(b);
    } catch { bad(`missing ${f.path}`); }
  }
  if (existsSync(join(dir, "relay"))) for (const abs of walk(join(dir, "relay"))) { const rel = relative(dir, abs).split(sep).join("/"); if (!listed.has(rel)) bad(`unlisted file ${rel}`); }

  // 4. SHA256SUMS lists exactly the manifest's files and the manifest
  const want = new Map([...manifest.files.map((f) => [f.path, f.sha256]), [MANIFEST_FILE, sha(text)]]);
  try {
    const got = new Map();
    for (const line of readFileSync(join(dir, SUMS_FILE), "utf8").split("\n").filter(Boolean)) {
      const m = /^([0-9a-f]{64}) [ *](.+)$/.exec(line);
      if (!m) { bad(`${SUMS_FILE}: malformed line`); continue; }
      got.set(m[2], m[1]);
    }
    for (const [p, h] of want) if (got.get(p) !== h) bad(`${SUMS_FILE}: ${got.has(p) ? "digest differs for" : "does not list"} ${p}`);
    for (const p of got.keys()) if (!want.has(p)) bad(`${SUMS_FILE}: lists unexpected ${p}`);
  } catch { bad(`${SUMS_FILE}: unreadable`); }

  // 5. provenance (mandatory): signed by the same delegated key, subjects = manifest and SHA256SUMS
  const provErrors = provenanceErrors(dir, manifest, { roots, threshold, pq, now });
  info.provenance = provErrors.length === 0;
  for (const e of provErrors) if (!(sig.ok === false && /^(delegation|release key): /.test(e))) bad(`provenance: ${e}`);

  // 6. configuration: only the signed one, up to the owner's fields
  const wrangler = readJson(join(dir, "wrangler.json"));
  if (!isRecord(wrangler)) bad("wrangler.json: missing or not JSON");
  else {
    const d = differingKeys(withoutOwnerFields(wrangler), manifest.config.wrangler);
    if (d.length > 0) bad(`wrangler.json differs from the signed release at: ${d.join(", ")}`);
    if (typeof wrangler.name !== "string" || !/^[a-z0-9][a-z0-9-]{0,62}$/.test(wrangler.name)) bad("wrangler.json: name is not a valid Worker name");
  }
  const pkg = readJson(join(dir, "package.json"));
  if (!isRecord(pkg)) bad("package.json: missing or not JSON");
  else {
    const d = differingKeys(withoutOwnerPackageFields(pkg), manifest.config.package);
    if (d.length > 0) bad(`package.json differs from the signed release at: ${d.join(", ")}`);
  }
  for (const e of readdirSync(dir)) if (FOREIGN_CONFIG.some((re) => re.test(e)) && !FOREIGN_ALLOWED.has(e)) bad(`${e}: a file that could change the build or the deployment is present`);

  // 7. the admission value, where this environment shows it (Cloudflare may keep a secret from the build: then the running relay is the check)
  const adm = env.OM_ADMISSION;
  if (typeof adm === "string" && adm.trim() !== "" && !adm.split(",").every((h) => HEX64.test(h.trim().toLowerCase()))) bad("OM_ADMISSION is not a list of 64-character hexadecimal values (a malformed value would leave your delivery service unusable: it refuses to create anything)");
  info.admissionSeen = typeof adm === "string" && adm.trim() !== "";
  return { ok: errors.length === 0, errors, info };
};

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  const args = process.argv.slice(2);
  const mv = args.indexOf("--min-version");
  const dir = resolve(args.find((a, i) => !a.startsWith("--") && i !== mv + 1) ?? dirname(fileURLToPath(import.meta.url)));
  const r = verifyRelay(dir, { minVersion: mv >= 0 ? args[mv + 1] : null });
  console.log("OM relay verification");
  for (const e of r.errors) console.error("FAIL:", e);
  if (r.errors.some((e) => e.startsWith("signature: pq_") || e.startsWith("provenance: delegation: pq_"))) console.error("HINT: the post-quantum co-signature check needs Node 24 or newer (ML-DSA in node:crypto). This template asks for it in .node-version.");
  const i = r.info;
  console.log(`${r.ok ? "OK" : "FAILED"}: relay ${i.version ?? "?"}${r.ok ? ` signed (key ${i.keyId}, delegation v${i.delegationVersion}), commit ${i.commit}, worker sha256 ${i.workerSha256}` : ": nothing will be deployed"}`);
  if (r.ok && !i.admissionSeen) console.log("NOTE: the setup value (OM_ADMISSION) is not visible to this step. After deploying, check that your relay's /.well-known/om-provider-capabilities lists om.admission.v1; if it does not, the setup value did not take: your relay then refuses to create anything (it fails closed) until you correct it.");
  process.exit(r.ok ? 0 : 1);
}
