#!/usr/bin/env node
// Third-party rebuild/compare (deploy/REBUILD-AND-COMPARE.md): checks that a locally REBUILT site/ has exactly the file list and sha256 digests
// that a published release's signed bundle-manifest.json lists. Only content is compared; signatures (and the manifest's delegation/signature
// fields) cannot be rebuilt. Usage: node compare-rebuild.mjs <published-release-dir-or-site> <rebuilt-site-dir>
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const sha = (b) => createHash("sha256").update(b).digest("hex");
const siteOf = (d) => (existsSync(join(d, "site", "bundle-manifest.json")) ? join(d, "site") : d);

/** Returns { ok, errors }. Compares manifest.files (path, sha256, bytes) and the rebuilt bytes; version must match too. */
export const compareRebuild = (published, rebuilt) => {
  const errors = [];
  const pub = JSON.parse(readFileSync(join(siteOf(published), "bundle-manifest.json"), "utf8"));
  const site = siteOf(rebuilt);
  const mine = JSON.parse(readFileSync(join(site, "bundle-manifest.json"), "utf8"));
  if (pub.version !== mine.version) errors.push(`version differs: published ${pub.version}, rebuilt ${mine.version}`);
  const want = new Map((pub.files ?? []).map((f) => [f.path, f]));
  const have = new Map((mine.files ?? []).map((f) => [f.path, f]));
  for (const [p, f] of want) {
    const g = have.get(p);
    if (!g) { errors.push(`missing from rebuild: ${p}`); continue; }
    if (g.sha256 !== f.sha256 || g.bytes !== f.bytes) errors.push(`differs: ${p}`);
    try { if (sha(readFileSync(join(site, p))) !== f.sha256) errors.push(`rebuilt bytes differ from published digest: ${p}`); } catch { errors.push(`rebuilt file unreadable: ${p}`); }
  }
  for (const p of have.keys()) if (!want.has(p)) errors.push(`extra in rebuild: ${p}`);
  return { ok: errors.length === 0, errors };
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [a, b] = process.argv.slice(2);
  if (!a || !b) { console.error("usage: node compare-rebuild.mjs <published-release-dir> <rebuilt-site-dir>"); process.exit(2); }
  const r = compareRebuild(resolve(a), resolve(b));
  for (const e of r.errors) console.error("DIFF:", e);
  console.log(r.ok ? "MATCH: every file in the published manifest was reproduced byte for byte" : "MISMATCH");
  process.exit(r.ok ? 0 : 1);
}
