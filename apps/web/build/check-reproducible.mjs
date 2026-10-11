// Builds the bundle twice into clean output dirs and requires byte-identical bundle manifests.
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const tmp = mkdtempSync(join(tmpdir(), "om-repro-"));
const env = { ...process.env, OM_GIT_COMMIT: process.env.OM_GIT_COMMIT ?? "repro-check" };
const manifests = [];
try {
  for (const n of [1, 2]) {
    const out = join(tmp, `out${n}`);
    const r = spawnSync("pnpm", ["exec", "vite", "build", "--outDir", out, "--emptyOutDir"], { cwd: root, env, stdio: ["ignore", "ignore", "inherit"] });
    if (r.status !== 0) throw new Error(`build ${n} failed`);
    manifests.push(readFileSync(join(out, "bundle-manifest.json")));
  }
  if (!manifests[0].equals(manifests[1])) throw new Error("NOT REPRODUCIBLE: bundle-manifest.json differs between clean builds");
  console.log(`reproducible: 2 clean builds produced identical bundle-manifest.json (${manifests[0].length} bytes)`);
} catch (e) {
  console.error(String(e instanceof Error ? e.message : e));
  process.exitCode = 1;
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
