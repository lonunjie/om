// The web build bundles the real Core (packages/om-core-wasm/dist/om_wasm.wasm). Build it when it is missing, so
// `pnpm build` works from a clean checkout with the pinned Rust toolchain (nothing is downloaded or installed).
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const wasm = fileURLToPath(new URL("../../../packages/om-core-wasm/dist/om_wasm.wasm", import.meta.url));
if (!existsSync(wasm)) {
  console.log("ensure-core-wasm: building om_wasm.wasm");
  execFileSync("pnpm", ["--filter", "@om/core-wasm", "build:wasm"], { stdio: "inherit" });
}
if (!existsSync(wasm)) {
  console.error("ensure-core-wasm: om_wasm.wasm is missing after the build");
  process.exit(1);
}
