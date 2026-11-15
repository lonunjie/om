// The web build bundles both Core modules (packages/om-core-wasm/dist): the base om_wasm.wasm and the groups module om_wasm_groups.wasm
// (ADR-0187: a second signed asset, loaded on demand). Build whichever is missing, so `pnpm build` works from a clean checkout with the
// pinned Rust toolchain (nothing is downloaded or installed).
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

for (const [name, script] of [["om_wasm.wasm", "build:wasm"], ["om_wasm_groups.wasm", "build:wasm:groups"]]) {
  const wasm = fileURLToPath(new URL(`../../../packages/om-core-wasm/dist/${name}`, import.meta.url));
  if (!existsSync(wasm)) {
    console.log(`ensure-core-wasm: building ${name}`);
    execFileSync("pnpm", ["--filter", "@om/core-wasm", script], { stdio: "inherit" });
  }
  if (!existsSync(wasm)) {
    console.error(`ensure-core-wasm: ${name} is missing after the build`);
    process.exit(1);
  }
}
