// SPDX-License-Identifier: Apache-2.0
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const targets = process.argv.slice(2);
if (targets.length === 0) targets.push("worker", "sqlite");
if (targets.some((target) => !["worker", "sqlite", "sqlite-direct"].includes(target))) {
  process.stderr.write("Usage: pnpm conformance:provider [worker|sqlite|sqlite-direct]\n");
  process.exit(2);
}

// Identical public API assertions for each target. The harness alone supplies lifecycle and transport.
const suites = ["smoke", "relay", "admission", "usage", "services", "key-rotation", "blobs", "recovery", "wake"]
  .map((suite) => `infra/cloudflare-home/test/${suite}.test.ts`);
for (const target of targets) {
  process.stdout.write(`\nProvider conformance: ${target}\n`);
  const result = spawnSync("pnpm", ["exec", "vitest", "run", ...suites], {
    cwd: fileURLToPath(new URL("../../", import.meta.url)),
    stdio: "inherit",
    env: { ...process.env, OM_PROVIDER_TEST_TARGET: target },
  });
  if (result.error) process.stderr.write(`${result.error.message}\n`);
  if (result.status !== 0) process.exitCode = 1;
}
