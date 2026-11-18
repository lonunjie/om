// SPDX-License-Identifier: AGPL-3.0-or-later
import { build } from "esbuild";

await build({
  absWorkingDir: import.meta.dirname,
  entryPoints: ["src/server.ts"],
  outfile: process.argv[2] ?? "dist/home.mjs",
  bundle: true,
  format: "esm",
  platform: "node",
  target: "node24",
  alias: {
    "cloudflare:workers": "./src/runtime.ts",
    "@om/home-wire": "../../packages/om-home-wire/src/index.ts",
  },
  inject: ["./src/wake.ts"],
  // ws is bundled, including its JavaScript implementations of masking and UTF-8 validation.
  define: { "process.env.WS_NO_BUFFER_UTIL": '"1"', "process.env.WS_NO_UTF_8_VALIDATE": '"1"' },
  banner: { js: 'import { createRequire } from "node:module"; const require = createRequire(import.meta.url);' },
});
