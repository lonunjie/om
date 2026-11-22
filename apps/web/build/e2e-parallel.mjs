// Runs the three browser projects of the e2e suite at the same time instead of one after the other. The default `pnpm e2e` is unchanged; this is an
// additive way to run the same specs. Build the bundles first (`pnpm e2e:build`).
//
// How it stays safe: the specs use fixed ports and three shared local Homes, so the servers must be started exactly once and outlive every run.
// This script starts the `webServer` entries of e2e/playwright.config.ts itself, waits until each answers, runs one `playwright test` per project
// (they reuse the running servers: `reuseExistingServer`), and stops the servers at the end. Each run gets its own `--output` folder because
// Playwright empties its output folder when it starts. Every spec makes its own identities in its own browser context, so the engines share
// nothing but the Homes, where each person is a new random identity.
//
// Cost, measured on emeth4 (16 CPUs) on 2026-11-23: the sequential `pnpm e2e` took 48 minutes (232 passed, 21 skipped); this runner took 28
// minutes (chromium 83 passed, firefox 76, webkit 70) but three browsers plus the Homes at once is heavy, and the 90 s per-test limit then bites in
// the long real-Core specs (device-link, landing, peer-recovery-live timed out or needed their retry, none of which happens alone). So it is a quick
// first pass, not the gate: judge any failure by rerunning that spec alone with `pnpm exec playwright test -c e2e <spec> --project=<engine>`.
// Real sharding inside one engine (`--shard=i/n`) is not possible on one host: the ports and the three Homes are fixed, and an unprivileged network
// namespace (the way to give each shard its own copies) is refused on emeth4.
//
// Usage (from apps/web):  node build/e2e-parallel.mjs [playwright args, for example a spec name or -g pattern]
//   OM_E2E_PROJECTS=chromium,firefox  limits the projects (default: chromium,firefox,webkit)
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import config from "../e2e/playwright.config.ts";

const projects = (process.env["OM_E2E_PROJECTS"] ?? "chromium,firefox,webkit").split(",").map((p) => p.trim()).filter((p) => p !== "");
const extra = process.argv.slice(2);
const servers = (Array.isArray(config.webServer) ? config.webServer : config.webServer === undefined ? [] : [config.webServer]);

const up = async (url) => { try { return (await fetch(url, { signal: AbortSignal.timeout(2_000) })).status < 500; } catch { return false; } };
async function waitFor(url, deadlineMs) {
  for (const end = Date.now() + deadlineMs; Date.now() < end;) {
    if (await up(url)) return true;
    await new Promise((r) => setTimeout(r, 1_000));
  }
  return false;
}
const tag = (name, child) => {
  for (const stream of [child.stdout, child.stderr]) createInterface({ input: stream }).on("line", (line) => { console.log(`[${name}] ${line}`); });
};

const running = [];
const stopServers = () => { for (const s of running) s.kill("SIGTERM"); };
process.on("SIGINT", () => { stopServers(); process.exit(130); });

let code = 1;
try {
  for (const s of servers) {
    if (await up(s.url)) continue; // already running (reuseExistingServer)
    const child = spawn(s.command, { shell: true, cwd: s.cwd === undefined ? process.cwd() : new URL(`../e2e/${s.cwd}/`, import.meta.url), stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, ...(s.env ?? {}) } });
    tag(`server ${s.url}`, child);
    running.push(child);
  }
  for (const s of servers) {
    if (!(await waitFor(s.url, 300_000))) throw new Error(`the test server did not come up: ${s.url}`);
  }
  const runs = projects.map((project) => {
    const child = spawn("pnpm", ["exec", "playwright", "test", "-c", "e2e", `--project=${project}`, `--output=test-results-${project}`, ...extra], { stdio: ["ignore", "pipe", "pipe"], env: process.env });
    tag(project, child);
    return new Promise((resolve) => { child.on("close", (c) => { resolve(c ?? 1); }); });
  });
  const codes = await Promise.all(runs);
  console.log(`e2e-parallel: ${projects.map((p, i) => `${p}=${String(codes[i])}`).join(" ")}`);
  code = codes.every((c) => c === 0) ? 0 : 1;
} catch (e) {
  console.error(`e2e-parallel: ${e instanceof Error ? e.message : String(e)}`);
} finally {
  stopServers();
}
process.exit(code);
