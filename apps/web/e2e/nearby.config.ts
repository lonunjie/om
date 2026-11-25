// Runs only the dev-harness build (port 4174), for specs that need no other server: `playwright test -c e2e/nearby.config.ts`.
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  testMatch: "nearby.spec.ts",
  timeout: 90_000,
  expect: { timeout: 30_000 },
  workers: 1,
  reporter: [["list"]],
  outputDir: "../test-results",
  use: { baseURL: "http://localhost:4174" },
  webServer: [{ command: "pnpm exec vite preview --outDir dist-harness --port 4174 --strictPort", cwd: "..", url: "http://localhost:4174", reuseExistingServer: true }],
});
