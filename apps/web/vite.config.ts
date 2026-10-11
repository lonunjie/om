import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import { SECURITY_HEADERS, headersFile } from "./csp.ts";

const emitHeaders = (): Plugin => ({
  name: "om-emit-headers",
  generateBundle() {
    this.emitFile({ type: "asset", fileName: "_headers", source: headersFile() });
  },
});

export default defineConfig({
  plugins: [react(), emitHeaders()],
  build: { target: "es2023", sourcemap: false, modulePreload: { polyfill: false }, assetsInlineLimit: 0 },
  preview: { headers: { ...SECURITY_HEADERS } },
});
