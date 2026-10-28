// Opt-in build for e2e/live-relay.spec.ts: the production entry (real Core, no test flags) compiled with the live reference
// Home as its build default (VITE_OM_RELAY_URL). Not part of `pnpm e2e` or CI; `pnpm e2e:live` runs it.
// The bundle is dev-signed (OM_BUNDLE_DEV) like the other e2e bundles; it is never a release.
import { execFileSync } from "node:child_process";

// The first test Home (docs/reports/2026-10-28-relay-status.md, "Deployed"). Override with OM_LIVE_RELAY_URL.
const HOME = process.env.OM_LIVE_RELAY_URL ?? "https://om-home-lonunjie.majal.workers.dev";
execFileSync("node", ["build/ensure-core-wasm.mjs"], { stdio: "inherit" });
execFileSync("pnpm", ["exec", "vite", "build", "--outDir", "dist-live-relay", "--emptyOutDir"], {
  stdio: "inherit",
  env: { ...process.env, VITE_OM_RELAY_URL: HOME, OM_BUNDLE_DEV: "1" },
});
