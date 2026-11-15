// Reproducibility check. (1) Both Core modules (the base om_wasm.wasm and the groups module om_wasm_groups.wasm, ADR-0187) are built twice from
// cold, separate cargo target directories and their SHA-256 digests must be identical, and equal to the modules the web build bundles (skip with
// OM_REPRO_SKIP_WASM=1 when only the bundle is of interest; a cold build of both modules takes minutes). (2) The bundle is built twice into clean
// output dirs and the bundle manifests (which list both modules' digests) must be byte-identical.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const tmp = mkdtempSync(join(tmpdir(), "om-repro-"));
const env = { ...process.env, OM_GIT_COMMIT: process.env.OM_GIT_COMMIT ?? "repro-check" };
const manifests = [];
const wasmDigest = (f) => createHash("sha256").update(readFileSync(f)).digest("hex");
try {
  if (process.env.OM_REPRO_SKIP_WASM !== "1") {
    const built = [];
    for (const n of [1, 2]) {
      const dir = join(tmp, `wasm${n}`);
      const e = { ...env, OM_WASM_TARGET_DIR: join(dir, "target"), OM_WASM_OUT_DIR: join(dir, "out") };
      for (const script of ["build:wasm", "build:wasm:groups"]) {
        const r = spawnSync("pnpm", ["--filter", "@om/core-wasm", script], { cwd: root, env: e, stdio: ["ignore", "ignore", "inherit"] });
        if (r.status !== 0) throw new Error(`wasm build ${n} (${script}) failed`);
      }
      built.push({ core: wasmDigest(join(dir, "out/om_wasm.wasm")), groups: wasmDigest(join(dir, "out/om_wasm_groups.wasm")) });
    }
    for (const k of ["core", "groups"]) if (built[0][k] !== built[1][k]) throw new Error(`NOT REPRODUCIBLE: the ${k} module differs between two cold builds`);
    const shipped = { core: wasmDigest(join(root, "../../packages/om-core-wasm/dist/om_wasm.wasm")), groups: wasmDigest(join(root, "../../packages/om-core-wasm/dist/om_wasm_groups.wasm")) };
    for (const k of ["core", "groups"]) if (built[0][k] !== shipped[k]) throw new Error(`the ${k} module the web build bundles (packages/om-core-wasm/dist) is not what a cold build produces: rebuild it (pnpm --filter @om/core-wasm build:wasm${k === "groups" ? ":groups" : ""})`);
    console.log(`reproducible: both Core modules built twice from cold target dirs are identical (core ${built[0].core.slice(0, 12)}, groups ${built[0].groups.slice(0, 12)}) and equal the bundled ones`);
  }
  for (const n of [1, 2]) {
    const out = join(tmp, `out${n}`);
    const r = spawnSync("pnpm", ["exec", "vite", "build", "--outDir", out, "--emptyOutDir"], { cwd: root, env, stdio: ["ignore", "ignore", "inherit"] });
    if (r.status !== 0) throw new Error(`build ${n} failed`);
    manifests.push(readFileSync(join(out, "bundle-manifest.json")));
  }
  if (!manifests[0].equals(manifests[1])) throw new Error("NOT REPRODUCIBLE: bundle-manifest.json differs between clean builds");
  console.log(`reproducible: 2 clean builds produced identical bundle-manifest.json (${manifests[0].length} bytes, listing both Core modules)`);
} catch (e) {
  console.error(String(e instanceof Error ? e.message : e));
  process.exitCode = 1;
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
