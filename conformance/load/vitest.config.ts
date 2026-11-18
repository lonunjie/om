import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

// Not part of `pnpm test`: the load harness is run on purpose (`pnpm load:home`).
export default defineConfig({
  resolve: { alias: { "@om/home-wire": fileURLToPath(new URL("../../packages/om-home-wire/src/index.ts", import.meta.url)) } },
  test: { include: ["conformance/load/*.load.ts"], environment: "node", testTimeout: 3_600_000, hookTimeout: 600_000, fileParallelism: false },
});
