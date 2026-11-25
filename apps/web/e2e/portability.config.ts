// Runs only the dev-harness build (port 4174), for specs that need no other server: `playwright test -c e2e/portability.config.ts`.
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "portability.spec.ts",
  timeout: 90_000,
  expect: { timeout: 30_000 },
  workers: 1,
  reporter: [["list"]],
  outputDir: "../test-results",
  use: { baseURL: "http://localhost:4174" },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "firefox", use: { ...devices["Desktop Firefox"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] } },
  ],
  webServer: [{ command: "pnpm exec vite preview --outDir dist-harness --port 4174 --strictPort", cwd: "..", url: "http://localhost:4174", reuseExistingServer: true }],
});
