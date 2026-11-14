// End-to-end checks in real browsers (Chromium, Firefox, WebKit). Uses the browsers already in the shared
// Playwright cache (@playwright/test is pinned to the version that matches them), so nothing is downloaded.
// Servers: the dev-harness build (4174) and a dev-signed production bundle with the service worker (4175).
import { defineConfig, devices } from "@playwright/test";
import { E2E_ADMISSION_HASH, E2E_ADMISSION_HASH_2, LOCKED_RELAY, LOCKED_RELAY_2 } from "./admission-fixture.ts";
import { writeQrY4m } from "./qr-video.ts";

const HARNESS = "http://localhost:4174";
const BUNDLE = "http://localhost:4175";
const CORE = "http://localhost:4176";
const GROUPS_APP = "http://localhost:4180";
const RELAY_APP = "http://localhost:4177";
const RELAY = "http://127.0.0.1:8788";
const LIVE_APP = "http://localhost:4178";
const SETUP_APP = "http://localhost:4179";
// Opt-in (OM_LIVE_RELAY=1): the production build against the LIVE reference Home. Never part of the hermetic run.
const live = process.env["OM_LIVE_RELAY"] === "1";
const cameraArgs = ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", `--use-file-for-fake-video-capture=${writeQrY4m()}`];

export default defineConfig({
  testDir: ".",
  testMatch: "*.spec.ts",
  timeout: 90_000,
  expect: { timeout: 30_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  outputDir: "../test-results",
  use: { baseURL: HARNESS, trace: "retain-on-failure" },
  webServer: [
    { command: "pnpm exec vite preview --outDir dist-harness --port 4174 --strictPort", cwd: "..", url: HARNESS, reuseExistingServer: true },
    { command: "pnpm exec vite preview --outDir dist-e2e --port 4175 --strictPort", cwd: "..", url: BUNDLE, reuseExistingServer: true },
    // The real Core (om_wasm.wasm) with the in-page TEST loopback provider; never a production build.
    { command: "pnpm exec vite preview --outDir dist-core-e2e --port 4176 --strictPort", cwd: "..", url: CORE, reuseExistingServer: true },
    // The real Core WITH the groups engine module (VITE_OM_GROUPS=1, ADR-0187 accepted) and an in-page group carriage; never a production build.
    { command: "pnpm exec vite preview --outDir dist-groups-e2e --port 4180 --strictPort", cwd: "..", url: GROUPS_APP, reuseExistingServer: true },
    // The real Core in the production entry, connected to the local reference Home (the Worker bundle in workerd).
    { command: "node ../../infra/cloudflare-home/test/serve.ts 8788", cwd: "..", url: `${RELAY}/healthz`, reuseExistingServer: true },
    { command: "pnpm exec vite preview --outDir dist-relay-e2e --port 4177 --strictPort", cwd: "..", url: RELAY_APP, reuseExistingServer: true, env: { OM_E2E_CONNECT_SRC: RELAY } },
    // A build with no relay of its own (as shipped) but allowed to reach the local reference Home: the first-run delivery-service setup page.
    // ... and a locked Home (it holds only the hash of the owner's admission secret), which is where that setup page connects.
    { command: "node ../../infra/cloudflare-home/test/serve.ts 8789", cwd: "..", url: `${LOCKED_RELAY}/healthz`, reuseExistingServer: true, env: { OM_ADMISSION: E2E_ADMISSION_HASH } },
    // ... and a second locked Home, which is where the backup delivery service is added (apps/web/e2e/backup-service.spec.ts).
    { command: "node ../../infra/cloudflare-home/test/serve.ts 8790", cwd: "..", url: `${LOCKED_RELAY_2}/healthz`, reuseExistingServer: true, env: { OM_ADMISSION: E2E_ADMISSION_HASH_2 } },
    { command: "pnpm exec vite preview --outDir dist-setup-e2e --port 4179 --strictPort", cwd: "..", url: SETUP_APP, reuseExistingServer: true, env: { OM_E2E_CONNECT_SRC: `${LOCKED_RELAY} ${LOCKED_RELAY_2}` } },
    ...(live ? [{ command: "pnpm exec vite preview --outDir dist-live-relay --port 4178 --strictPort", cwd: "..", url: LIVE_APP, reuseExistingServer: true, env: { ...(/^http:\/\/(127\.0\.0\.1|localhost)(:\d{2,5})?$/.test(process.env["OM_LIVE_RELAY_URL"] ?? "") ? { OM_E2E_CONNECT_SRC: process.env["OM_LIVE_RELAY_URL"] ?? "" } : {}) } }] : []),
  ],
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], launchOptions: { args: cameraArgs } }, testIgnore: /(cross|live-relay|live-invite)\.spec\.ts/ },
    { name: "firefox", use: { ...devices["Desktop Firefox"] }, testMatch: /(cross|core|relay|invite-link|navigation|backup|backup-service|device-link|omnibar|attachments|search|lock|relay-setup|landing)\.spec\.ts/ },
    { name: "webkit", use: { ...devices["Desktop Safari"] }, testMatch: /(cross|core|relay|invite-link|navigation|backup|backup-service|device-link|omnibar|attachments|search|lock|relay-setup|landing)\.spec\.ts/ },
    ...(live ? [{ name: "live-chromium", use: { ...devices["Desktop Chrome"] }, testMatch: /live-(relay|invite)\.spec\.ts/, timeout: 240_000 }] : []),
  ],
});

