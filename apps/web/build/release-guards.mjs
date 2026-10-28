// Checks a built site folder must pass BEFORE it is signed (audit 2026-10-28, H1): no harness or test-only provider
// anywhere in the files, and no test-only marker in the wasm assets (tools/core_guard.py --wasm).
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { join } from "node:path";
import { findHarness } from "./check-no-harness.mjs";

const wasmFiles = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? wasmFiles(join(dir, e.name)) : e.name.endsWith(".wasm") ? [join(dir, e.name)] : []));

/** Returns a list of problems (empty when the site is clean). `repoRoot` locates tools/core_guard.py. */
export function siteProblems(site, repoRoot, env) {
  const problems = findHarness(site).map((h) => `harness/test provider in site: ${h}`);
  const wasms = wasmFiles(site);
  if (wasms.length === 0) problems.push("no om_wasm.wasm asset in the site: the real Core is missing");
  else {
    const g = spawnSync("python3", ["tools/core_guard.py", "--wasm", ...wasms], { cwd: repoRoot, env, encoding: "utf8" });
    if (g.status !== 0) problems.push(`core guard: ${(g.stderr || g.stdout || `exit ${g.status}`).trim()}`);
  }
  return problems;
}
