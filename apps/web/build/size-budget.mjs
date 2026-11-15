// The ADR-0187 size budget: how much larger the groups module is than the base Core module, raw and gzipped. A breach means the fallback
// to option D (same engine, a separate module with an additive host import) is reconsidered, so it fails the check, never just warns.
import { readFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";

/** Sizes of one module: raw bytes and gzip (level 9). */
export const measure = (bytes) => ({ raw: bytes.byteLength, gzip: gzipSync(bytes, { level: 9 }).byteLength });

/** Limits from the budget file: baseline plus headroom. */
export const limits = (budget) => {
  const k = 1 + budget.headroomPercent / 100;
  return { deltaRaw: Math.floor(budget.baseline.deltaRawBytes * k), deltaGzip: Math.floor(budget.baseline.deltaGzipBytes * k) };
};

/** `{ ok, delta, limit, errors }` for measured base and groups sizes. */
export const checkBudget = (base, groups, budget) => {
  const delta = { raw: groups.raw - base.raw, gzip: groups.gzip - base.gzip };
  const limit = limits(budget);
  const errors = [];
  if (!(base.raw > 0) || !(groups.raw > base.raw)) errors.push("the groups module must be larger than the base Core (wrong or missing file?)");
  if (delta.raw > limit.deltaRaw) errors.push(`groups module is ${delta.raw} bytes larger than the base Core, over the budget of ${limit.deltaRaw} (ADR-0187: reconsider option D)`);
  if (delta.gzip > limit.deltaGzip) errors.push(`groups module is ${delta.gzip} bytes (gzip) larger than the base Core, over the budget of ${limit.deltaGzip} (ADR-0187: reconsider option D)`);
  return { ok: errors.length === 0, delta, limit, errors };
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dist = (n) => fileURLToPath(new URL(`../../../packages/om-core-wasm/dist/${n}`, import.meta.url));
  const [a, b] = process.argv.slice(2);
  const budget = JSON.parse(readFileSync(new URL("./size-budget.json", import.meta.url), "utf8"));
  const base = measure(readFileSync(a ?? dist("om_wasm.wasm")));
  const groups = measure(readFileSync(b ?? dist("om_wasm_groups.wasm")));
  const r = checkBudget(base, groups, budget);
  console.log(`size budget: base ${base.raw} raw / ${base.gzip} gzip, groups ${groups.raw} raw / ${groups.gzip} gzip, delta +${r.delta.raw} / +${r.delta.gzip} (limit +${r.limit.deltaRaw} / +${r.limit.deltaGzip})`);
  for (const e of r.errors) console.error(`size budget: ${e}`);
  process.exit(r.ok ? 0 : 1);
}
