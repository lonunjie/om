#!/usr/bin/env node
// Standalone, dependency-free verifier for an OM web release folder. Shipped inside every release as verify.mjs.
// Usage: node verify.mjs [releaseDir] [--require-signed]
// Checks: SHA256SUMS against every file, every bundle-manifest.json entry (hash + size), no unlisted files in site/.
// It does NOT verify the manifest signature (that needs the pinned trust root inside the client); it reports whether one exists.
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, realpathSync, statSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const sha = (b) => createHash("sha256").update(b).digest("hex");
const walk = (d) => readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(d, e.name)) : [join(d, e.name)]));
const UNSERVED = new Set(["_headers"]);

/** Returns { ok, errors, info }. Pure over the filesystem; exported for tests. */
export const verifyRelease = (dir, { requireSigned = false } = {}) => {
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
  console.log(`${r.ok ? "OK" : "FAILED"}: version ${i.version ?? "?"}, ${i.dev ? "DEV build (unsigned, not for production)" : i.signed ? `signed (key ${i.keyId}; signature is verified by the client's pinned trust root, not here)` : "UNSIGNED"}`);
  process.exit(r.ok ? 0 : 1);
}
