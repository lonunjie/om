// Checks a built site folder must pass BEFORE it is signed (audit 2026-10-28, H1): no harness or test-only provider
// anywhere in the files, and no test-only marker in the wasm assets (tools/core_guard.py --wasm).
import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { findHarness } from "./check-no-harness.mjs";

const wasmFiles = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? wasmFiles(join(dir, e.name)) : e.name.endsWith(".wasm") ? [join(dir, e.name)] : []));

/** Returns a list of problems (empty when the site is clean). `repoRoot` locates tools/core_guard.py. */
export function siteProblems(site, repoRoot, env) {
  const problems = findHarness(site).map((h) => `harness/test provider in site: ${h}`);
  const wasms = wasmFiles(site);
  const names = wasms.map((w) => w.split("/").pop() ?? "");
  const manifestFiles = (() => { try { return JSON.parse(readFileSync(join(site, "bundle-manifest.json"), "utf8")).files ?? []; } catch { return []; } })();
  const core = names.filter((n) => /^om_wasm-[A-Za-z0-9_-]+\.wasm$/.test(n));
  const groups = names.filter((n) => /^om_wasm_groups-[A-Za-z0-9_-]+\.wasm$/.test(n));
  if (core.length !== 1) problems.push("the site must hold exactly one om_wasm.wasm asset (the real Core)");
  if (groups.length !== 1) problems.push("the site must hold exactly one om_wasm_groups.wasm asset (the groups module, ADR-0187)");
  else if (manifestFiles.find((f) => f.path === `assets/${groups[0]}`)?.lazy !== true) problems.push("the groups module is not listed as a lazy file in bundle-manifest.json");
  if (wasms.length === 0) problems.push("no wasm asset in the site: the real Core is missing");
  else {
    const g = spawnSync("python3", ["tools/core_guard.py", "--wasm", ...wasms], { cwd: repoRoot, env, encoding: "utf8" });
    if (g.status !== 0) problems.push(`core guard: ${(g.stderr || g.stdout || `exit ${g.status}`).trim()}`);
  }
  return problems;
}
