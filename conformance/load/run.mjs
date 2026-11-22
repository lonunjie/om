// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs the Home load and soak harness. Usage: node conformance/load/run.mjs [worker|sqlite ...] [--full] [--capacity] [--out DIR]
// --capacity runs the per-relay capacity scenarios (conformance/load/relay-capacity.load.ts) instead of the limits and soak ones.
// Results (one JSON file per target) go to DIR (default conformance/load/out, git-ignored); `pnpm load:home` is the short form.
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join, resolve } from "node:path";

const root = fileURLToPath(new URL("../../", import.meta.url));
const args = process.argv.slice(2);
const full = args.includes("--full");
const capacity = args.includes("--capacity");
const outIndex = args.indexOf("--out");
const outDir = resolve(root, outIndex >= 0 ? (args[outIndex + 1] ?? "") : "conformance/load/out");
const targets = args.filter((a, i) => !a.startsWith("--") && args[i - 1] !== "--out");
if (targets.length === 0) targets.push("worker", "sqlite");
if (targets.some((t) => !["worker", "sqlite"].includes(t))) {
  process.stderr.write("Usage: node conformance/load/run.mjs [worker|sqlite ...] [--full] [--capacity] [--out DIR]\n");
  process.exit(2);
}
mkdirSync(outDir, { recursive: true });
for (const target of targets) {
  const out = join(outDir, `${target}-${capacity ? "capacity-" : ""}${full ? "full" : "quick"}.json`);
  process.stdout.write(`\nHome ${capacity ? "capacity" : "load"}: ${target} (${full ? "full" : "quick"})\n`);
  const r = spawnSync("pnpm", ["exec", "vitest", "run", "-c", "conformance/load/vitest.config.ts", capacity ? "relay-capacity" : "load.load"], {
    cwd: root, stdio: "inherit",
    env: { ...process.env, OM_PROVIDER_TEST_TARGET: target, OM_LOAD_SCALE: full ? "full" : "quick", OM_LOAD_OUT: out },
  });
  if (r.status !== 0) process.exitCode = 1;
  try {
    const report = JSON.parse(readFileSync(out, "utf8"));
    process.stdout.write(`result: ${out}\nviolations: ${report.violations.length}\n`);
    for (const v of report.violations) process.stdout.write(`  - ${v}\n`);
  } catch { process.stdout.write("no result file written\n"); process.exitCode = 1; }
}
