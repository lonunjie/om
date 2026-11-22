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
const PROD_APP = "http://localhost:4181";
// Opt-in (OM_LIVE_RELAY=1): the production build against the LIVE reference Home. Never part of the hermetic run.
const live = process.env["OM_LIVE_RELAY"] === "1";
const cameraArgs = ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--disable-features=WebRtcHideLocalIpsWithMdns", "--allow-loopback-in-peer-connection", `--use-file-for-fake-video-capture=${writeQrY4m()}`];

// Firefox gets fake capture devices, no permission prompt and loopback ICE candidates from preferences (the same three things the Chromium flags above give). WebKit has no equivalent switches; calls.spec.ts says what it can and cannot check there.
const firefoxMediaPrefs = {
  "media.navigator.streams.fake": true,
  "media.navigator.permission.disabled": true,
  "media.peerconnection.ice.obfuscate_host_addresses": false,
  "media.peerconnection.ice.loopback": true,
};
const OTHER_ENGINE_IGNORE = /(chromium|live-relay|live-invite)\.spec\.ts/;

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
    // The real Core, with the groups module fetched on demand (ADR-0187) and the real group carriage over the local reference Home; a dev-marked bundle only because the local Home needs the loopback flags.
    { command: "pnpm exec vite preview --outDir dist-groups-e2e --port 4180 --strictPort", cwd: "..", url: GROUPS_APP, reuseExistingServer: true, env: { OM_E2E_CONNECT_SRC: `${RELAY} ${LOCKED_RELAY}` } },
    // The plain production build (`pnpm build`: no test flag, no dev marking): starts on the verified base Core module and never fetches the groups module at first load.
    { command: "pnpm exec vite preview --outDir dist --port 4181 --strictPort", cwd: "..", url: PROD_APP, reuseExistingServer: true },
    // The real Core in the production entry, connected to the local reference Home (the Worker bundle in workerd).
    { command: "node ../../infra/cloudflare-home/test/serve.ts 8788", cwd: "..", url: `${RELAY}/healthz`, reuseExistingServer: true, timeout: 180_000 },
    { command: "pnpm exec vite preview --outDir dist-relay-e2e --port 4177 --strictPort", cwd: "..", url: RELAY_APP, reuseExistingServer: true, env: { OM_E2E_CONNECT_SRC: RELAY } },
    // A build with no relay of its own (as shipped) but allowed to reach the local reference Home: the first-run delivery-service setup page.
    // ... and a locked Home (it holds only the hash of the owner's admission secret), which is where that setup page connects.
    { command: "node ../../infra/cloudflare-home/test/serve.ts 8789", cwd: "..", url: `${LOCKED_RELAY}/healthz`, reuseExistingServer: true, timeout: 180_000, env: { OM_ADMISSION: E2E_ADMISSION_HASH } },
    // ... and a second locked Home, which is where the backup delivery service is added (apps/web/e2e/backup-service.spec.ts).
    { command: "node ../../infra/cloudflare-home/test/serve.ts 8790", cwd: "..", url: `${LOCKED_RELAY_2}/healthz`, reuseExistingServer: true, timeout: 180_000, env: { OM_ADMISSION: E2E_ADMISSION_HASH_2 } },
    { command: "pnpm exec vite preview --outDir dist-setup-e2e --port 4179 --strictPort", cwd: "..", url: SETUP_APP, reuseExistingServer: true, env: { OM_E2E_CONNECT_SRC: `${LOCKED_RELAY} ${LOCKED_RELAY_2} http://home.localhost:8789 ${RELAY}` } },
    ...(live ? [{ command: "pnpm exec vite preview --outDir dist-live-relay --port 4178 --strictPort", cwd: "..", url: LIVE_APP, reuseExistingServer: true, env: { ...(/^http:\/\/(127\.0\.0\.1|localhost)(:\d{2,5})?$/.test(process.env["OM_LIVE_RELAY_URL"] ?? "") ? { OM_E2E_CONNECT_SRC: process.env["OM_LIVE_RELAY_URL"] ?? "" } : {}) } }] : []),
  ],
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], launchOptions: { args: cameraArgs } }, testIgnore: /(cross|live-relay|live-invite)\.spec\.ts/ },
    // Firefox and WebKit run every hermetic spec except the Chromium-only ones (the fake camera device and the Chromium service-worker checks are Chromium features); ADR-0172 puts macOS Safari and Firefox in the first-release matrix.
    // Firefox gets one retry: in calls.spec.ts, about one run in eight, the ICE restart after the emulated offline blip does not finish inside the 20 s limit (the call then fails honestly). A retried pass is reported as "flaky", never hidden.
    { name: "firefox", use: { ...devices["Desktop Firefox"], launchOptions: { firefoxUserPrefs: firefoxMediaPrefs } }, testIgnore: OTHER_ENGINE_IGNORE, retries: 1 },
    // WebKit gets one retry (Firefox one, for a call-recovery reason above): on a loaded macOS host it intermittently times out a test that passes alone (lock, the long three-person Rooms run,
    // peer-recovery confirm); every such test passed in isolation and on Chromium and Firefox. A retried pass is reported as "flaky", never hidden.
    { name: "webkit", use: { ...devices["Desktop Safari"] }, testIgnore: OTHER_ENGINE_IGNORE, retries: 1 },
    ...(live ? [{ name: "live-chromium", use: { ...devices["Desktop Chrome"] }, testMatch: /live-(relay|invite)\.spec\.ts/, timeout: 240_000 }] : []),
  ],
});

