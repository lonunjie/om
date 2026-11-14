import { readFileSync } from "node:fs";
import react from "@vitejs/plugin-react";
import { build as viteBuild, defineConfig, loadEnv, type Plugin, type UserConfig } from "vite";
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

// The service worker is registered as a CLASSIC script at a stable URL (/sw.js), so it must be one self-contained file. Built as a
// second entry it shared a chunk with the app (both use @noble/hashes) and a classic worker cannot import it: registration failed
// silently and no production build had a service worker. It is therefore bundled on its own (IIFE, nothing shared) and emitted
// as /sw.js before the bundle manifest is computed.
/** The worker carries the release version: browsers only reinstall a service worker whose bytes changed, and the install is what stages a new bundle. */
const serviceWorker = (bundleVersion: string): Plugin => ({
  name: "om-service-worker",
  async generateBundle() {
    const out = await viteBuild({
      configFile: false,
      logLevel: "silent",
      define: { __OM_DEV_BUNDLE__: JSON.stringify(dev), __OM_BUNDLE_VERSION__: JSON.stringify(bundleVersion) },
      build: {
        write: false,
        target: "es2023",
        sourcemap: false,
        minify: true,
        lib: { entry: new URL("./src/sw/sw.ts", import.meta.url).pathname, formats: ["iife"], name: "OmServiceWorker", fileName: () => "sw.js" },
      },
    });
    const outputs = (Array.isArray(out) ? out : [out]).flatMap((o) => ("output" in o ? o.output : []));
    const chunk = outputs.find((o) => o.type === "chunk");
    if (chunk === undefined) throw new Error("service worker build produced no chunk");
    if (/^\s*(import|export)\b|\bimport\s*\(|\bimportScripts\b/m.test(chunk.code)) throw new Error("service worker bundle must be self-contained (no import/export/importScripts)");
    this.emitFile({ type: "asset", fileName: "sw.js", source: chunk.code });
  },
});

const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as { version: string };
const dev = process.env["OM_BUNDLE_DEV"] === "1";

// Local end-to-end only: the preview server may additionally allow loopback http origins (and their ws origins, for the wake channel) for the local reference Homes
// (one, or several separated by spaces: a person with a backup delivery service has two). Never applied to the emitted `_headers`, and anything that is not a loopback origin is ignored.
function localRelayCsp(): Record<string, string> {
  const origins = (process.env["OM_E2E_CONNECT_SRC"] ?? "").split(/\s+/).filter((o) => /^http:\/\/(127\.0\.0\.1|localhost)(:\d{2,5})?$/.test(o));
  if (origins.length === 0) return {};
  const extra = origins.flatMap((o) => [o, o.replace(/^http/, "ws")]).join(" ");
  return { "Content-Security-Policy": cspHeader().replace("connect-src 'self' https: wss:", `connect-src 'self' https: wss: ${extra}`) };
}

const config: UserConfig = {
  plugins: [react(), landingPlugin(), emitHeaders(), serviceWorker(version), bundleManifestPlugin({ version, dev })],
  define: { __OM_DEV_BUNDLE__: JSON.stringify(dev), __OM_VERSION__: JSON.stringify(version) },
  build: {
    target: "es2023",
    sourcemap: false,
    modulePreload: { polyfill: false },
    assetsInlineLimit: 0,
  },
  preview: { headers: { ...SECURITY_HEADERS, ...localRelayCsp() } },
};

// R-072 / audit H1: the bundle content is chosen by these flags (src/main.tsx). A production-mode build that sets one
// without the explicit dev-bundle switch would ship a harness or the in-page TEST loopback provider as if it were
// the product, so it does not build at all. VITE_OM_RELAY_URL is on the list since ADR-0186: a signed build never carries
// a relay; every person sets up their own in onboarding.
// VITE_OM_GROUPS is on the list: ADR-0187 is accepted, but no product carriage for group wires exists yet (groups plan Q2), so no signed build ships the groups module.
const TEST_FLAGS = ["VITE_OM_DEV_HARNESS", "VITE_OM_E2E_CORE", "VITE_OM_ALLOW_LOOPBACK_RELAY", "VITE_OM_RELAY_URL", "VITE_OM_GROUPS"] as const;

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
