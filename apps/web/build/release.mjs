#!/usr/bin/env node
// Builds the production web bundle and emits a versioned, self-contained release folder + zip (see deploy/README.md).
// Usage: node build/release.mjs [--dev] [--signature <file.json>] [--out <dir>] [--no-zip]
//   --dev        build with OM_BUNDLE_DEV=1: unsigned dev bundle, folder/zip are named "-dev". Never for production.
//   --signature  JSON {alg,keyId,sig} produced externally over signingPayload(manifest); injected into the manifest.
// Without --dev the release REFUSES to be produced unless a signature is supplied (production keys do not exist yet).
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = resolve(webRoot, "../..");
const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const opt = (n) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
const fail = (m) => { console.error(`release: ${m}`); process.exit(1); };

const dev = flag("--dev");
const sigFile = opt("--signature");
const outBase = resolve(opt("--out") ?? join(webRoot, "release"));
if (!dev && !sigFile) {
  fail("refusing to produce a production release: the bundle would be UNSIGNED. Production signing keys do not exist yet.\n" +
    "  Pass --signature <file.json> ({alg,keyId,sig} over the manifest signing payload) once keys exist,\n" +
    "  or pass --dev for an explicitly unsigned DEV release (never deploy a dev release to real users).");
}
if (dev && sigFile) fail("--dev and --signature are mutually exclusive");

const walk = (d) => readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(d, e.name)) : [join(d, e.name)]));
const sha = (b) => createHash("sha256").update(b).digest("hex");
const tmp = mkdtempSync(join(tmpdir(), "om-release-"));
try {
  const site = join(tmp, "site");
  const b = spawnSync("pnpm", ["exec", "vite", "build", "--outDir", site, "--emptyOutDir"], {
    cwd: webRoot, stdio: "inherit", env: { ...process.env, ...(dev ? { OM_BUNDLE_DEV: "1" } : {}) },
  });
  if (b.status !== 0) fail("vite build failed");

  const manifestPath = join(site, "bundle-manifest.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (sigFile) {
    const s = JSON.parse(readFileSync(sigFile, "utf8"));
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
    "1. node verify.mjs            # checks SHA256SUMS and every file in site/bundle-manifest.json\n" +
    "2. Upload site/ to any static host (see deploy/ in https://github.com/lonunjie/om).\n" +
    "Serve site/ at the ORIGIN ROOT; site/_headers carries the required CSP (translate it for non-Cloudflare hosts).\n");
  const sums = walk(dir).map((p) => relative(dir, p).split(sep).join("/")).filter((p) => p !== "SHA256SUMS").sort()
    .map((p) => `${sha(readFileSync(join(dir, p)))}  ${p}`);
  writeFileSync(join(dir, "SHA256SUMS"), sums.join("\n") + "\n");

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
