// CycloneDX SBOM of the web workspace's production dependencies, via `pnpm sbom` (pnpm >= 12; no third-party tool).
// Usage: node build/sbom.mjs <out.json>     or import { writeSbom }.
// The SBOM lives OUTSIDE the bundle (never in site/), so it cannot affect `pnpm repro`. It is made deterministic: the random serialNumber and
// the generation timestamp are dropped and components are sorted, so the same lockfile yields the same bytes and SHA256SUMS stays reproducible.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(fileURLToPath(new URL("../../..", import.meta.url)));

/** Pure: removes the per-run fields from a parsed CycloneDX document. */
export const normalizeSbom = (doc) => {
  const { serialNumber: _s, ...rest } = doc;
  if (rest.metadata && typeof rest.metadata === "object") {
    const { timestamp: _t, ...meta } = rest.metadata;
    rest.metadata = meta;
  }
  // pnpm emits components in a run-dependent order: sort by bom-ref/ref (and each dependency's dependsOn) for stable bytes.
  const byKey = (k) => (a, b) => (a[k] < b[k] ? -1 : a[k] > b[k] ? 1 : 0);
  if (Array.isArray(rest.components)) rest.components = [...rest.components].sort(byKey("bom-ref"));
  if (Array.isArray(rest.dependencies)) rest.dependencies = rest.dependencies.map((d) => (Array.isArray(d.dependsOn) ? { ...d, dependsOn: [...d.dependsOn].sort() } : d)).sort(byKey("ref"));
  return rest;
};

/** Runs pnpm sbom for @om/web (production deps, from the lockfile) and writes the normalised document to `outFile`. Returns the component count. */
export const writeSbom = (outFile, { cwd = repoRoot, env = process.env } = {}) => {
  const tmp = mkdtempSync(join(tmpdir(), "om-sbom-"));
  try {
    const raw = join(tmp, "sbom.json");
    const r = spawnSync("pnpm", ["--filter", "@om/web", "sbom", "--sbom-format", "cyclonedx", "--prod", "--lockfile-only", "--out", raw], { cwd, env, encoding: "utf8" });
    if (r.status !== 0) throw new Error(`pnpm sbom failed (exit ${r.status}): ${(r.stderr || r.stdout || "").trim().split("\n").slice(-3).join(" | ")}`);
    const doc = normalizeSbom(JSON.parse(readFileSync(raw, "utf8")));
    if (doc.bomFormat !== "CycloneDX") throw new Error("pnpm sbom did not produce a CycloneDX document");
    writeFileSync(outFile, JSON.stringify(doc, null, 2) + "\n");
    return (doc.components ?? []).length;
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) { console.error("usage: node build/sbom.mjs <out.json>"); process.exit(2); }
  try { console.log(`sbom: ${writeSbom(resolve(process.argv[2]))} components -> ${process.argv[2]}`); }
  catch (e) { console.error(`sbom: ${e.message}`); process.exit(1); }
}
