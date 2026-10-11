import { readFileSync } from "node:fs";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import { bundleManifestPlugin } from "./build/plugin.ts";
import { SECURITY_HEADERS, headersFile } from "./csp.ts";

const emitHeaders = (): Plugin => ({
  name: "om-emit-headers",
  generateBundle() {
    this.emitFile({ type: "asset", fileName: "_headers", source: headersFile() });
  },
});

const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string };
const dev = process.env["OM_BUNDLE_DEV"] === "1";

export default defineConfig({
  plugins: [react(), emitHeaders(), bundleManifestPlugin({ version, dev })],
  define: { __OM_DEV_BUNDLE__: JSON.stringify(dev) },
  build: {
    target: "es2023",
    sourcemap: false,
    modulePreload: { polyfill: false },
    assetsInlineLimit: 0,
    // The service worker is a second entry with a stable, unhashed URL (/sw.js).
    rollupOptions: {
      input: { index: "index.html", sw: "src/sw/sw.ts" },
      output: { entryFileNames: (chunk) => (chunk.name === "sw" ? "sw.js" : "assets/[name]-[hash].js") },
    },
  },
  preview: { headers: { ...SECURITY_HEADERS } },
});
