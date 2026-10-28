import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { include: ["packages/**/*.test.{ts,tsx}", "apps/**/*.test.{ts,tsx}", "infra/**/*.test.ts", "tools/web/**/*.test.ts"], environment: "node", setupFiles: ["packages/om-ui/src/test-setup.ts"], testTimeout: 20000, hookTimeout: 20000 },
});
