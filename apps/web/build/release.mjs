#!/usr/bin/env node
// Builds the production web bundle and emits a versioned, self-contained release folder + zip (see deploy/README.md).
// Usage: node build/release.mjs [--dev] [--signature <file.json>] [--out <dir>] [--no-zip]
//   --dev        build with OM_BUNDLE_DEV=1: unsigned dev bundle, folder/zip are named "-dev". Never for production.
//   --signature  JSON {alg,keyId,sig} produced externally over signingPayload(manifest); injected into the manifest.
//   --sign-from-vault     sign with the production key read in-memory from the agent vault (key id om-release-2026-10-28).
//   --sign-with-key-file  sign with a key file: PKCS8 PEM, one-line PEM or base64 DER, or vault-item JSON (tests only: pass a throwaway key; also --key-id <id>, default the production id).
//   --delegation <file>   root-signed release-key delegation to embed in the manifest (default apps/web/release-delegation.json).
//                         Signing REFUSES unless it is signed by the pinned offline root, unexpired, and lists the signing key (audit B2).
//   --test-roots-file <file>  tests only (with --sign-with-key-file or --signature): JSON {roots:{id:hex},threshold[,pq]} replacing the pinned root (and PQ root).
//   --builder-id <uri>    builder identity recorded in the provenance (default: the operator workstation id in provenance.mjs).
//   --append-digest-log [file]  after a signed release verifies, append its digests to the single-operator signed log (default deploy/release-log/digests.jsonl).
// Signed (non-dev) releases also get provenance.intoto.jsonl (in-toto/SLSA statement in a DSSE envelope signed by the same delegated key; not listed in SHA256SUMS
// because it covers SHA256SUMS) and sbom.cdx.json (CycloneDX, listed in SHA256SUMS). With an external --signature there is no key here, so provenance is skipped (said so).
// Without --dev the release REFUSES to be produced unless it is signed (--signature, --sign-from-vault or --sign-with-key-file).
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { appendEntry, DEFAULT_LOG } from "./digest-log.mjs";
import { buildProvenance, provenanceLine, signProvenance } from "./provenance.mjs";
import { writeSbom } from "./sbom.mjs";
import { RELEASE_KEY_ID, pemFromSecretText, pemFromVault, signManifest } from "./sign.mjs";
import { cleanBuildEnv, pollutedKeys } from "./clean-env.mjs";
import { siteProblems } from "./release-guards.mjs";
import { PINNED_PQ_ROOT, PINNED_ROOTS, PINNED_ROOT_THRESHOLD, PROVENANCE_FILE, verifyRelease } from "./verify-release.mjs";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = resolve(webRoot, "../..");
const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const opt = (n) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
const fail = (m) => { console.error(`release: ${m}`); process.exit(1); };

const dev = flag("--dev");
const sigFile = opt("--signature");
const fromVault = flag("--sign-from-vault");
const keyFile = opt("--sign-with-key-file");
const signing = fromVault || keyFile !== undefined;
const outBase = resolve(opt("--out") ?? join(webRoot, "release"));
if (!dev && !sigFile && !signing) {
  fail("refusing to produce a production release: the bundle would be UNSIGNED.\n" +
    "  Pass --sign-from-vault, or --signature <file.json> ({alg,keyId,sig} over the manifest signing payload),\n" +
    "  or pass --dev for an explicitly unsigned DEV release (never deploy a dev release to real users).");
}
const testRootsFile = opt("--test-roots-file");
if (testRootsFile !== undefined && keyFile === undefined && sigFile === undefined) fail("--test-roots-file is only valid together with --sign-with-key-file or --signature (tests)");
// B2: the client pins only the offline ROOT key. Until the operator has run the key ceremony and set it, no production
// release can be built (dev bundles are unaffected). A throwaway-key test release brings its own root.
const testRoots = testRootsFile === undefined ? undefined : JSON.parse(readFileSync(testRootsFile, "utf8"));
const trust = testRoots ?? { roots: PINNED_ROOTS, threshold: PINNED_ROOT_THRESHOLD, pq: PINNED_PQ_ROOT }; // ADR-0182: testRoots may carry `pq` too
if (!dev && Object.keys(trust.roots).length === 0) {
  fail("refusing to produce a production release: no offline ROOT key is pinned yet (key ceremony pending).\n" +
    "  Follow deploy/KEY-CEREMONY.md, then set ROOT_KEYS in apps/web/src/update/release-key.ts and PINNED_ROOTS in apps/web/build/verify-release.mjs.");
}
const delegationFile = resolve(opt("--delegation") ?? join(webRoot, "release-delegation.json"));
if (dev && (sigFile || signing)) fail("--dev and signing flags are mutually exclusive");
if ([sigFile !== undefined, fromVault, keyFile !== undefined].filter(Boolean).length > 1) fail("pick exactly one of --signature, --sign-from-vault, --sign-with-key-file");

// H1: a release never inherits bundle-selecting variables (VITE_OM_DEV_HARNESS, VITE_OM_E2E_CORE, OM_BUNDLE_DEV, ...).
// Refuse loudly, and build with an allowlisted copy of the environment regardless.
const polluted = pollutedKeys(process.env);
if (polluted.length > 0) {
  fail(`refusing to build: bundle-selecting environment variable(s) set: ${polluted.join(", ")}.\n` +
    "  They can turn a harness or test-loopback build into a signed release. Unset them (use --dev for a dev bundle).");
}
// OM_RELEASE_BUILD makes vite itself refuse to build while the root key is unset (a throwaway-root test release is exempt).
const buildEnv = cleanBuildEnv(process.env, { dev, release: !dev && testRoots === undefined });

const manifestPath_of = (dir) => join(dir, "site", "bundle-manifest.json");
const walk = (d) => readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(d, e.name)) : [join(d, e.name)]));
const sha = (b) => createHash("sha256").update(b).digest("hex");
const tmp = mkdtempSync(join(tmpdir(), "om-release-"));
try {
  const site = join(tmp, "site");
  // Never reuse a stale dist/om_wasm.wasm: rebuild it now, under the om-core/production compile guard (R-072).
  const w = spawnSync("pnpm", ["--filter", "@om/core-wasm", "build:wasm"], { cwd: repoRoot, stdio: "inherit", env: buildEnv });
  if (w.status !== 0) fail("core wasm build failed");
  const b = spawnSync("pnpm", ["exec", "vite", "build", "--outDir", site, "--emptyOutDir"], { cwd: webRoot, stdio: "inherit", env: buildEnv });
  if (b.status !== 0) fail("vite build failed");
  // Nothing is signed until the built site carries no harness, test provider or test-only wasm marker.
  const problems = siteProblems(site, repoRoot, buildEnv);
  if (problems.length > 0) fail(`the built site failed the pre-signing checks:\n  ${problems.join("\n  ")}`);

  const manifestPath = join(site, "bundle-manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  let signerPem; // kept in memory only, to sign the provenance statement with the same key
  if (sigFile || signing) {
    // The delegation is part of the signed payload: embed it first, and refuse early if the chain cannot verify.
    let delegation;
    try { delegation = JSON.parse(readFileSync(delegationFile, "utf8")); }
    catch { fail(`delegation missing or unreadable: ${delegationFile}\n  Run the key ceremony (deploy/KEY-CEREMONY.md) and commit the root-signed delegation there, or pass --delegation <file>.`); }
    manifest.delegation = delegation;
    let s;
    if (sigFile) s = JSON.parse(readFileSync(sigFile, "utf8"));
    else {
      try { signerPem = fromVault ? pemFromVault() : pemFromSecretText(readFileSync(keyFile, "utf8")); s = signManifest(manifest, signerPem, opt("--key-id") ?? RELEASE_KEY_ID, { roots: trust.roots, threshold: trust.threshold, pq: trust.pq ?? null }); }
      catch (e) { fail(`signing failed: ${e.message}`); }
    }
    if (typeof s.alg !== "string" || typeof s.keyId !== "string" || typeof s.sig !== "string") fail("bad signature file");
    manifest.signature = { alg: s.alg, keyId: s.keyId, sig: s.sig };
    writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
  }
  if (!dev && (manifest.signature === null || manifest.dev === true)) fail("production release must be signed and not a dev bundle");

  const name = `om-web-${manifest.version}${dev ? "-dev" : ""}`;
  const dir = join(outBase, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  cpSync(site, join(dir, "site"), { recursive: true });
  copyFileSync(join(webRoot, "build/verify-release.mjs"), join(dir, "verify.mjs"));
  for (const f of ["wrangler.toml", "wrangler.workers.toml"]) {
    const src = join(repoRoot, "deploy/cloudflare-pages", f);
    if (existsSync(src)) copyFileSync(src, join(dir, f));
  }
  writeFileSync(join(dir, "README.txt"),
    `OM web release ${manifest.version}${dev ? " (DEV BUILD: UNSIGNED, NOT FOR PRODUCTION)" : ""}\n\n` +
    `1. node verify.mjs${dev ? "           " : (signerPem === undefined ? " --require-signed" : " --require-signed --require-provenance")}   # checks SHA256SUMS, every file in site/bundle-manifest.json${dev ? "" : " and the release signature" + (signerPem === undefined ? "" : ", provenance.intoto.jsonl")}\n` +
    "2. Upload site/ to any static host (see deploy/ in https://github.com/lonunjie/om).\n" +
    "Serve site/ at the ORIGIN ROOT; site/_headers carries the required CSP (translate it for non-Cloudflare hosts).\n");
  if (!dev) {
    try { console.log(`release: sbom.cdx.json (${writeSbom(join(dir, "sbom.cdx.json"))} components)`); }
    catch (e) { fail(`SBOM failed: ${e.message}`); }
  }
  // SHA256SUMS lists everything except itself and provenance.intoto.jsonl (provenance covers SHA256SUMS, so listing it would be circular).
  const sums = walk(dir).map((p) => relative(dir, p).split(sep).join("/")).filter((p) => p !== "SHA256SUMS" && p !== PROVENANCE_FILE).sort()
    .map((p) => `${sha(readFileSync(join(dir, p)))}  ${p}`);
  writeFileSync(join(dir, "SHA256SUMS"), sums.join("\n") + "\n");

  // L3: a "signed" release is only reported once its whole chain verifies (pinned root -> delegation -> release key ->
  // manifest signature); an externally supplied --signature is not trusted on sight.
  if (!dev) {
    const v = verifyRelease(dir, { requireSigned: true, roots: trust.roots, threshold: trust.threshold, pq: trust.pq ?? null, pqRequired: (trust.pq ?? null) !== null });
    if (!v.ok) fail(`the release does not verify (--require-signed):\n  ${v.errors.join("\n  ")}`);
    if (signerPem === undefined) console.log("release: provenance skipped (external --signature: the release key is not available here)");
    else {
      // The same commit vite stamps into the bundle: HEAD of this checkout (OM_* variables are refused above, so no override exists).
      const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).stdout.trim();
      const keyId = manifest.signature.keyId;
      const ptrust = { roots: trust.roots, threshold: trust.threshold, pq: trust.pq ?? null };
      try {
        const st = buildProvenance(dir, { commit, finishedOn: new Date().toISOString(), builderId: opt("--builder-id"), repoRoot });
        writeFileSync(join(dir, PROVENANCE_FILE), provenanceLine(signProvenance(st, signerPem, keyId, manifest.delegation, ptrust)));
      } catch (e) { fail(`provenance failed: ${e.message}`); }
      const pv = verifyRelease(dir, { requireSigned: true, requireProvenance: true, roots: trust.roots, threshold: trust.threshold, pq: trust.pq ?? null, pqRequired: (trust.pq ?? null) !== null });
      if (!pv.ok) fail(`the release does not verify with provenance:\n  ${pv.errors.join("\n  ")}`);
      console.log(`release: provenance verified (${PROVENANCE_FILE}, commit ${commit.slice(0, 12)})`);
      if (flag("--append-digest-log")) {
        const next = args[args.indexOf("--append-digest-log") + 1];
        const logFile = resolve(next && !next.startsWith("--") ? next : DEFAULT_LOG);
        try {
          appendEntry(logFile, { version: manifest.version, commit, manifestSha256: sha(readFileSync(manifestPath_of(dir))), sha256sumsSha256: sha(readFileSync(join(dir, "SHA256SUMS"))) }, signerPem, keyId, manifest.delegation, ptrust);
        } catch (e) { fail(`digest log append failed: ${e.message}`); }
        console.log(`release: digests appended to ${logFile} (single-operator log; commit it)`);
      }
    }
  }

  const out = { dir, zip: null };
  if (!flag("--no-zip")) {
    const zip = join(outBase, `${name}.zip`);
    rmSync(zip, { force: true });
    const z = spawnSync("zip", ["-qrX", zip, name], { cwd: outBase });
    if (z.status !== 0) fail("zip failed (is `zip` installed? use --no-zip to skip)");
    writeFileSync(`${zip}.sha256`, `${sha(readFileSync(zip))}  ${name}.zip\n`);
    out.zip = zip;
  }
  console.log(`release: ${dev ? "DEV (unsigned) " : "signed "}${name}\n  folder ${out.dir}\n  zip    ${out.zip ?? "(skipped)"}`);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
