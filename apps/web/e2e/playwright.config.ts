// End-to-end checks in real browsers (Chromium, Firefox, WebKit). Uses the browsers already in the shared
// Playwright cache (@playwright/test is pinned to the version that matches them), so nothing is downloaded.
// Servers: the dev-harness build (4174) and a dev-signed production bundle with the service worker (4175).
import { defineConfig, devices } from "@playwright/test";
import { writeQrY4m } from "./qr-video.ts";

const HARNESS = "http://localhost:4174";
const BUNDLE = "http://localhost:4175";
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
  ],
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"], launchOptions: { args: cameraArgs } }, testIgnore: /cross\.spec\.ts/ },
    { name: "firefox", use: { ...devices["Desktop Firefox"] }, testMatch: /cross\.spec\.ts/ },
    { name: "webkit", use: { ...devices["Desktop Safari"] }, testMatch: /cross\.spec\.ts/ },
  ],
});

