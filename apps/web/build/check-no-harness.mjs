// Fails when the dev-only Core harness leaked into a production bundle (`dist/`).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// The dev harness and every test-only Core provider are forbidden in a production bundle. The real Core
// (@om/core-wasm, om_wasm.wasm) is deliberately NOT listed: it is what production ships.
export const FORBIDDEN = [
  "OM-DEV-HARNESS-NOT-FOR-PRODUCTION", "core-harness",
  "OM-TEST-LOOPBACK-NOT-FOR-PRODUCTION", "core-wasm/test-support", "OM-V001-INSECURE-TEST-RNG", "InsecureTestRng",
];

/** Returns "relative/path: needle" for every forbidden string found in any file (names included) under `dir`. */
export function findHarness(dir, base = dir) {
  const hits = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const rel = p.slice(base.length + 1);
    if (statSync(p).isDirectory()) { hits.push(...findHarness(p, base)); continue; }
    const text = rel + "\n" + readFileSync(p, "latin1");
    for (const needle of FORBIDDEN) if (text.includes(needle)) hits.push(`${rel}: ${needle}`);
  }
  return hits;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ?? fileURLToPath(new URL("../dist", import.meta.url));
  let hits;
  try { hits = findHarness(dir); } catch (e) { console.error(`check-no-harness: cannot read ${dir}: ${String(e.message ?? e)}`); process.exit(1); }
  if (hits.length > 0) { console.error(`check-no-harness: harness found in ${dir}\n${hits.join("\n")}`); process.exit(1); }
  console.log(`check-no-harness: ok (${dir})`);
}
