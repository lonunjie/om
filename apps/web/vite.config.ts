import { readFileSync } from "node:fs";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv, type Plugin, type UserConfig } from "vite";
import { landingPlugin } from "./build/landing.ts";
import { bundleManifestPlugin } from "./build/plugin.ts";
import { SECURITY_HEADERS, cspHeader, headersFile } from "./csp.ts";
import { isRootPinned } from "./src/update/release-key.ts";

const emitHeaders = (): Plugin => ({
  name: "om-emit-headers",
  generateBundle() {
    this.emitFile({ type: "asset", fileName: "_headers", source: headersFile() });
  },
});

const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string };
const dev = process.env["OM_BUNDLE_DEV"] === "1";

// Local end-to-end only: the preview server may additionally allow one loopback http origin for the local reference Home.
// Never applied to the emitted `_headers`, and ignored for anything that is not a loopback origin.
function localRelayCsp(): Record<string, string> {
  const origin = process.env["OM_E2E_CONNECT_SRC"];
  if (origin === undefined || !/^http:\/\/(127\.0\.0\.1|localhost)(:\d{2,5})?$/.test(origin)) return {};
  return { "Content-Security-Policy": cspHeader().replace("connect-src 'self' https: wss:", `connect-src 'self' https: wss: ${origin}`) };
}

const config: UserConfig = {
  plugins: [react(), landingPlugin(), emitHeaders(), bundleManifestPlugin({ version, dev })],
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
  preview: { headers: { ...SECURITY_HEADERS, ...localRelayCsp() } },
};

// R-072 / audit H1: the bundle content is chosen by these flags (src/main.tsx). A production-mode build that sets one
// without the explicit dev-bundle switch would ship a harness or the in-page TEST loopback provider as if it were
// the product, so it does not build at all.
const TEST_FLAGS = ["VITE_OM_DEV_HARNESS", "VITE_OM_E2E_CORE", "VITE_OM_ALLOW_LOOPBACK_RELAY"] as const;

export default defineConfig(({ mode }) => {
  const env = { ...loadEnv(mode, process.cwd(), "VITE_OM_"), ...process.env };
  const set = TEST_FLAGS.filter((k) => (env[k] ?? "") !== "");
  if (mode === "production" && set.length > 0 && !dev) {
    throw new Error(
      `refusing a production build with ${set.join(", ")} set and OM_BUNDLE_DEV unset: that would ship a test build as the product (R-072)`,
    );
  }
  // Audit B2: a release build (build/release.mjs sets OM_RELEASE_BUILD) cannot exist until the offline root key is pinned
  // (src/update/release-key.ts). Plain production builds (`pnpm build`, checks) and dev bundles are unaffected.
  if (process.env["OM_RELEASE_BUILD"] === "1" && !dev && !isRootPinned()) {
    throw new Error("refusing a release build: the offline ROOT public key is not pinned yet (key ceremony pending, see deploy/KEY-CEREMONY.md; set ROOT_KEYS in src/update/release-key.ts)");
  }
  return config;
});
