#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Builds the signed OM relay release as a ready-to-publish deploy template (the content of the per-user template repository).
// Design: PROVISIONING_AND_BOOTSTRAP.md section 10 / R-261 (a template deploys a signed, digest-pinned release behind a verifier, never a branch head).
// Usage: node build.mjs (--sign-from-vault | --sign-with-key-file <key>) [--out <dir>] [--delegation <file>] [--key-id <id>] [--builder-id <uri>] [--allow-dirty]
//                       [--append-digest-log [file]]   after the release verifies, append its digests to the single-operator signed log (default deploy/release-log/relay-digests.jsonl)
//                       [--test-roots-file <file>]       tests only (with --sign-with-key-file): JSON {roots,threshold[,pq]} replacing the pinned root
//        node build.mjs --compare <template dir>          rebuilds the Worker from this checkout and compares it with <dir>/relay/worker.js
// Signing is the web release's: the same pinned 2-of-3 offline root, the same root-signed delegation (apps/web/release-delegation.json), the same
// delegated online key (read in memory from the agent vault, never written, logged or put in argv). Output: <out>/om-relay-<version>/.
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanBuildEnv, pollutedKeys } from "../../../apps/web/build/clean-env.mjs";
import { BUILD_TYPE, DEFAULT_BUILDER_ID, DEFAULT_REPO, provenanceLine, signProvenance } from "../../../apps/web/build/provenance.mjs";
import { appendEntry } from "../../../apps/web/build/digest-log.mjs";
import { RELEASE_KEY_ID, pemFromSecretText, pemFromVault, signManifest } from "../../../apps/web/build/sign.mjs";
import { PINNED_PQ_ROOT, PINNED_ROOTS, PINNED_ROOT_THRESHOLD, PROVENANCE_FILE, SLSA_PREDICATE_TYPE, STATEMENT_TYPE } from "../../../apps/web/build/verify-release.mjs";
import { LOCK_FILE, MANIFEST_FILE, MANIFEST_FORMAT, SUMS_FILE, WORKER_FILE, verifyRelay } from "./verify-relay.mjs";

const here = fileURLToPath(new URL(".", import.meta.url));
const homeRoot = resolve(here, "..");
const repoRoot = resolve(homeRoot, "../..");
const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const opt = (n) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
const fail = (m) => { console.error(`relay release: ${m}`); process.exit(1); };
const sha = (b) => createHash("sha256").update(b).digest("hex");

// A release never inherits switches from the caller's environment (same rule as the web release, audit H1).
const polluted = pollutedKeys(process.env);
if (polluted.length > 0) fail(`refusing to build: environment variable(s) set that could change what is built: ${polluted.join(", ")}. Unset them.`);
const buildEnv = { ...cleanBuildEnv(process.env), CI: "1", WRANGLER_SEND_METRICS: "false" };

/** The Worker exactly as `wrangler deploy` would upload it (dry run: no account, token or network). Deterministic. */
const bundleWorker = () => {
  const out = mkdtempSync(join(tmpdir(), "om-relay-bundle-"));
  try {
    const r = spawnSync("pnpm", ["exec", "wrangler", "deploy", "--dry-run", "--config", "wrangler.jsonc", "--outdir", out], { cwd: homeRoot, env: { ...buildEnv, WRANGLER_LOG_PATH: join(out, "logs") }, encoding: "utf8" });
    if (r.status !== 0) throw new Error(`wrangler dry run failed (exit ${r.status}): ${(r.stderr || r.stdout || "").trim().split("\n").slice(-3).join(" | ")}`);
    return readFileSync(join(out, "index.js"));
  } finally { rmSync(out, { recursive: true, force: true }); }
};

const compare = opt("--compare");
if (compare !== undefined) {
  let want;
  try { want = readFileSync(join(resolve(compare), ...WORKER_FILE.split("/"))); } catch { fail(`cannot read ${WORKER_FILE} in ${compare}`); }
  const built = bundleWorker();
  const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).stdout.trim();
  const same = sha(built) === sha(want);
  console.log(`${same ? "MATCH" : "DIFFERENT"}: rebuilt ${sha(built)} (checkout ${commit}), template ${sha(want)}`);
  process.exit(same ? 0 : 1);
}

const fromVault = flag("--sign-from-vault");
const keyFile = opt("--sign-with-key-file");
if (fromVault === (keyFile !== undefined)) fail("pick exactly one of --sign-from-vault and --sign-with-key-file <key>; an unsigned relay release does not exist");
const testRootsFile = opt("--test-roots-file");
if (testRootsFile !== undefined && keyFile === undefined) fail("--test-roots-file is only valid together with --sign-with-key-file (tests)");
const testRoots = testRootsFile === undefined ? undefined : JSON.parse(readFileSync(testRootsFile, "utf8"));
const trust = testRoots ?? { roots: PINNED_ROOTS, threshold: PINNED_ROOT_THRESHOLD, pq: PINNED_PQ_ROOT };
trust.pq = trust.pq ?? null;
if (Object.keys(trust.roots).length === 0) fail("no offline root key is pinned (deploy/KEY-CEREMONY.md)");
const delegationFile = resolve(opt("--delegation") ?? join(repoRoot, "apps/web/release-delegation.json"));
const outBase = resolve(opt("--out") ?? join(homeRoot, "dist/relay-template"));

const git = (...a) => spawnSync("git", a, { cwd: repoRoot, encoding: "utf8" }).stdout.trim();
const commit = git("rev-parse", "HEAD");
if (!flag("--allow-dirty")) {
  const dirty = git("status", "--porcelain", "--", "infra/cloudflare-home", "packages/om-home-wire", "apps/web/build", "pnpm-lock.yaml");
  if (dirty !== "") fail(`refusing to sign: uncommitted changes in what this release is built from (the provenance names commit ${commit.slice(0, 12)}):\n${dirty}`);
}
const version = JSON.parse(readFileSync(join(homeRoot, "package.json"), "utf8")).version;

let pem;
try { pem = fromVault ? pemFromVault() : pemFromSecretText(readFileSync(keyFile, "utf8")); } catch (e) { fail(`reading the signing key failed: ${e.message}`); }
let delegation;
try { delegation = JSON.parse(readFileSync(delegationFile, "utf8")); } catch { fail(`delegation missing or unreadable: ${delegationFile}`); }

const dir = join(outBase, `om-relay-${version}`);
rmSync(dir, { recursive: true, force: true });
mkdirSync(join(dir, "relay"), { recursive: true });
cpSync(join(here, "template"), dir, { recursive: true });
const pkgPath = join(dir, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
writeFileSync(pkgPath, JSON.stringify({ ...pkg, version }, null, 2) + "\n");
writeFileSync(join(dir, WORKER_FILE), bundleWorker());
copyFileSync(join(here, "verify-relay.mjs"), join(dir, "verify.mjs")); // ships under the name the template scripts call
copyFileSync(join(here, "verify-chain.mjs"), join(dir, "verify-chain.mjs")); // follows the symlink: a byte copy of apps/web/build/verify-release.mjs

const readJson = (p) => JSON.parse(readFileSync(join(dir, p), "utf8"));
const { name: _wn, $schema: _ws, ...wranglerConfig } = readJson("wrangler.json");
const { name: _pn, version: _pv, description: _pd, private: _pp, cloudflare: _pc, ...packageConfig } = readJson("package.json");
const files = ["relay/worker.js", "verify.mjs", "verify-chain.mjs", ".dev.vars.example", ".node-version", "pnpm-lock.yaml"].sort()
  .map((path) => { const b = readFileSync(join(dir, path)); return { path, sha256: sha(b), bytes: b.byteLength }; });
const unsigned = { format: MANIFEST_FORMAT, version, worker: WORKER_FILE, files, config: { wrangler: wranglerConfig, package: packageConfig }, createdFrom: { gitCommit: commit }, delegation, signature: null };
let manifest;
try {
  const s = signManifest(unsigned, pem, opt("--key-id") ?? RELEASE_KEY_ID, trust);
  manifest = { ...unsigned, signature: { alg: s.alg, keyId: s.keyId, sig: s.sig } };
} catch (e) { fail(`signing failed: ${e.message}`); }
const manifestText = JSON.stringify(manifest, null, 2) + "\n";
writeFileSync(join(dir, MANIFEST_FILE), manifestText);
writeFileSync(join(dir, SUMS_FILE), [...files.map((f) => `${f.sha256}  ${f.path}`), `${sha(Buffer.from(manifestText))}  ${MANIFEST_FILE}`].sort((a, b) => (a.slice(66) < b.slice(66) ? -1 : 1)).join("\n") + "\n");
writeFileSync(join(dir, LOCK_FILE), JSON.stringify({ version, manifestSha256: sha(Buffer.from(manifestText)), minVersion: version }, null, 2) + "\n");

try {
  // The web release's statement, for this release's files (provenance.mjs buildProvenance is fixed to the web release's file names).
  const lockfile = join(repoRoot, "pnpm-lock.yaml");
  const st = {
    _type: STATEMENT_TYPE,
    subject: [MANIFEST_FILE, SUMS_FILE].map((name) => ({ name, digest: { sha256: sha(readFileSync(join(dir, name))) } })),
    predicateType: SLSA_PREDICATE_TYPE,
    predicate: {
      buildDefinition: {
        buildType: BUILD_TYPE,
        externalParameters: { repo: DEFAULT_REPO, commit, version, artifact: "om-relay" },
        resolvedDependencies: [{ uri: `git+${DEFAULT_REPO}@${commit}`, digest: { gitCommit: commit } }, { uri: `${DEFAULT_REPO}/blob/${commit}/pnpm-lock.yaml`, name: "pnpm-lock.yaml", digest: { sha256: sha(readFileSync(lockfile)) } }],
      },
      runDetails: { builder: { id: opt("--builder-id") ?? DEFAULT_BUILDER_ID }, metadata: { finishedOn: new Date().toISOString() } },
    },
  };
  writeFileSync(join(dir, PROVENANCE_FILE), provenanceLine(signProvenance(st, pem, manifest.signature.keyId, manifest.delegation, trust)));
} catch (e) { fail(`provenance failed: ${e.message}`); }

const v = verifyRelay(dir, { roots: trust.roots, threshold: trust.threshold, pq: trust.pq });
if (!v.ok) fail(`the release does not verify:\n  ${v.errors.join("\n  ")}`);
if (flag("--append-digest-log")) {
  // The relay has its own log file: its versions (0.2.0) are not the web release's (0.0.x), and the log refuses a duplicate version.
  const next = args[args.indexOf("--append-digest-log") + 1];
  const logFile = resolve(next && !next.startsWith("--") ? next : join(repoRoot, "deploy/release-log/relay-digests.jsonl"));
  try { appendEntry(logFile, { version, commit, manifestSha256: sha(readFileSync(join(dir, MANIFEST_FILE))), sha256sumsSha256: sha(readFileSync(join(dir, SUMS_FILE))) }, pem, manifest.signature.keyId, manifest.delegation, trust); } catch (e) { fail(`digest log append failed: ${e.message}`); }
  console.log(`relay release: digests appended to ${logFile} (single-operator log; commit it)`);
}
console.log(`relay release: signed om-relay-${version} (commit ${commit.slice(0, 12)}, worker sha256 ${v.info.workerSha256})\n  folder ${dir}`);
