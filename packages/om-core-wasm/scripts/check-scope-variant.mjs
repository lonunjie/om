// The `scope-engines` engineering module (om_wasm_scope.wasm; never bundled, flag off in every release): built twice from cold, separate
// cargo target directories, the digests must match, and its size over the base Core module must stay inside size-budget-scope.json.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const measure = (bytes) => ({ raw: bytes.byteLength, gzip: gzipSync(bytes, { level: 9 }).byteLength });

const root = fileURLToPath(new URL("..", import.meta.url));
const tmp = mkdtempSync(join(tmpdir(), "om-scope-repro-"));
const digest = (f) => createHash("sha256").update(readFileSync(f)).digest("hex");
try {
  const out = [];
  for (const n of [1, 2]) {
    const dir = join(tmp, `wasm${n}`);
    const env = { ...process.env, OM_WASM_TARGET_DIR: join(dir, "target"), OM_WASM_OUT_DIR: join(dir, "out") };
    for (const script of ["build:wasm", "build:wasm:scope"]) {
      const r = spawnSync("pnpm", ["--filter", "@om/core-wasm", script], { cwd: root, env, stdio: ["ignore", "ignore", "inherit"] });
      if (r.status !== 0) throw new Error(`wasm build ${n} (${script}) failed`);
    }
    out.push({ file: join(dir, "out/om_wasm_scope.wasm"), base: join(dir, "out/om_wasm.wasm") });
  }
  if (digest(out[0].file) !== digest(out[1].file)) throw new Error("NOT REPRODUCIBLE: the scope-engines module differs between two cold builds");
  const base = measure(readFileSync(out[0].base));
  const scope = measure(readFileSync(out[0].file));
  const budget = JSON.parse(readFileSync(new URL("./size-budget-scope.json", import.meta.url), "utf8"));
  const delta = { raw: scope.raw - base.raw, gzip: scope.gzip - base.gzip };
  const k = 1 + budget.headroomPercent / 100;
  const limit = { raw: Math.floor(budget.baseline.deltaRawBytes * k), gzip: Math.floor(budget.baseline.deltaGzipBytes * k) };
  console.log(`scope-engines: reproducible (${digest(out[0].file).slice(0, 12)}), delta over base +${delta.raw} raw / +${delta.gzip} gzip (limit +${limit.raw} / +${limit.gzip})`);
  if (!(scope.raw > base.raw)) throw new Error("the scope-engines module must be larger than the base Core");
  if (delta.raw > limit.raw || delta.gzip > limit.gzip) throw new Error("scope-engines module is over its size budget");
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
