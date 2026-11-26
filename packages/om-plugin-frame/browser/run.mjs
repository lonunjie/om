// SPDX-License-Identifier: MPL-2.0
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { root } from "../build.mjs";
import { startServer } from "./server.mjs";

const names = ["chromium", "firefox", "webkit"];
const fixtures = [
  "opaque",
  "wrong_origin",
  "foreign",
  "forged_ready",
  "forged_port",
  "oversized",
  "tainted",
  "send",
  "ui",
  "host_state",
  "spin",
  "memory",
  "unknown",
  "ungranted",
  "bilateral",
];
const registry = {
  malformed_request: 1,
  unknown_capability: 2,
  capability_not_granted: 3,
  grant_expired: 4,
  tainted_egress_denied: 5,
  user_confirmation_required: 6,
  bilateral_consent_required: 7,
  origin_channel_mismatch: 8,
  resource_budget_exceeded: 9,
  host_policy_denied: 10,
  release_not_authorized: 11,
  invalid_ui_tree: 12,
};
const success = {
  fetch: "completed",
  websocket: "opened",
  xhr: "completed",
  importScripts: "executed",
  sendBeacon: "queued",
};
const describe = (error) => String(error?.stack ?? error).replaceAll(root, "<plugin-frame>/");

function refused(verdict, name) {
  return verdict?.refused === name && verdict.code === registry[name];
}

function checkFixture(name, data, server) {
  const events = data.observations ?? [];
  const bootstrap = events.filter(({ type }) => type === "bootstrap");
  const calls = events.filter(({ type }) => type === "call");
  const last = events.at(-1);
  const call = calls.at(-1);
  const stopped = last?.type === "stop";
  const done = last?.type === "done";
  const rejected = (origin, sameWindow) =>
    bootstrap.some((event) =>
      event.origin === origin && event.sameWindow === sameWindow && refused(event.verdict, "origin_channel_mismatch")
    );
  const noTransfer = !calls.length && !bootstrap.some(({ verdict }) => verdict.done) && stopped &&
    last.reason === "deadline";
  const denial = (kind, error) => done && call?.kind === kind && refused(call.verdict, error);

  if (
    !Object.entries(registry).every(([key, code]) => data.registry?.errorCodes[key] === code) ||
    Object.keys(data.registry.errorCodes).length !== 12
  ) return false;
  switch (name) {
    case "opaque":
      return data.defaultDisabled && data.sandbox === "allow-scripts" && noTransfer &&
        (bootstrap.length === 0 || rejected("null", true));
    case "wrong_origin":
      return noTransfer && rejected(server.sink, true);
    case "foreign":
      return noTransfer && rejected(server.sink, false);
    case "forged_ready":
      return noTransfer && rejected(server.plugin, false);
    case "forged_port":
      return data.forged?.sent && data.forged.replies.length === 0;
    case "oversized":
      return calls.length === 1 && denial("malformed", "malformed_request") && data.prompts.length === 0;
    case "tainted":
    case "bilateral":
      return calls[0]?.kind === "read" && calls[0].verdict.done === true &&
        denial("egress", name === "tainted" ? "tainted_egress_denied" : "bilateral_consent_required");
    case "send":
      return denial("send", "user_confirmation_required") && data.prompts.length === 1 &&
        data.prompts[0].destination === "synthetic-room" &&
        data.prompts[0].content === "Synthetic exact outgoing content";
    case "ui":
      return denial("ui", "invalid_ui_tree");
    case "host_state":
      return denial("host_state", "host_policy_denied");
    case "spin":
      return call?.kind === "host_state" && refused(call.verdict, "host_policy_denied") &&
        stopped && last.reason === "watchdog" && refused(last.verdict, "resource_budget_exceeded");
    case "memory":
      return calls[0]?.memoryBytes === 65_536 && calls[0].verdict.done === true &&
        call?.memoryBytes === 131_072 && denial("account", "resource_budget_exceeded");
    case "unknown":
      return calls.length === 1 && denial("read", "unknown_capability");
    case "ungranted":
      return denial("read", "capability_not_granted");
    default:
      return false;
  }
}

function checkNetwork(control, confined, traffic, scope) {
  if (!control || !confined) return "not-measured";
  const api = confined.api;
  const seen = ({ url }) => traffic.some(({ path }) => path === new URL(url).pathname);
  if (seen(confined)) return "escaped";
  if (control.outcome === "unavailable" && confined.outcome === "unavailable") return "api-unavailable";
  if (!seen(control) || control.outcome !== success[api]) return "control-failed";
  if (confined.outcome === success[api] && api !== "sendBeacon") return "unexpected-success";
  const url = new URL(api === "websocket" ? confined.url.replace(/^http/, "ws") : confined.url);
  const directive = api === "importScripts" ? "script-src" : "connect-src";
  const csp = scope.some(({ disposition, effectiveDirective, blockedURI }) =>
    disposition === "enforce" && effectiveDirective.startsWith(directive) &&
    [url.href, url.origin, `${url.origin}/`].includes(blockedURI)
  );
  const failed = ["threw", "error-event", "rejected", "closed"].includes(confined.outcome);
  return csp || failed ? "blocked" : "inconclusive";
}

async function networkPhase(page, server, engine, mode) {
  const run = `${server.sink}/probe/${engine}/${mode}`;
  const urls = Object.fromEntries(Object.keys(success).map((api) => [api, `${run}/worker/${api}`]));
  const target = `${server.plugin}/network?mode=${mode}`;
  const seenRequests = [];
  page.on("requestfailed", (r) => seenRequests.push(`FAIL ${r.url()} ${r.failure()?.errorText}`));
  page.on("response", (r) => seenRequests.push(`${r.status()} ${r.url()}`));
  await page.evaluate(({ target, plugin }) => {
    const frame = document.createElement("iframe");
    frame.sandbox.add("allow-scripts", "allow-same-origin");
    frame.allow = `cross-origin-isolated ${plugin}`;
    frame.referrerPolicy = "no-referrer";
    frame.src = target;
    document.body.append(frame);
  }, { target, plugin: server.plugin });
  // Poll for the child frame: Firefox reports an empty URL for this sandboxed frame, so match it as the only child.
  let frame;
  for (let i = 0; i < 100 && !frame; i += 1) {
    frame = page.frames().find((candidate) => candidate !== page.mainFrame());
    if (!frame) await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!frame) throw new Error(`Network probe frame did not load; frames seen: ${page.frames().map((f) => f.url()).join(" | ")}; requests: ${seenRequests.join(" ; ")}`);
  let framed = true;
  try {
    await frame.waitForFunction(() => typeof globalThis.networkProbe === "function", null, { timeout: 4_000 });
  } catch {
    // Playwright on Firefox exposes no script context for this sandboxed frame. Measure the same document, with the same
    // headers, as a top-level page instead, and record that the frame context was not used.
    framed = false;
    await page.goto(target, { timeout: 8_000 });
    frame = page.mainFrame();
    await frame.waitForFunction(() => typeof globalThis.networkProbe === "function", null, { timeout: 6_000 });
  }
  const measured = await frame.evaluate(async ({ mode, urls, beacon }) => {
    const violations = globalThis.observeCsp();
    const worker = new Worker(`/network-worker.js?mode=${mode}`);
    try {
      const workerResult = new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("Network Worker timed out")), 5_000);
        worker.onmessage = (event) => {
          clearTimeout(timer);
          resolve(event.data);
        };
        worker.onerror = (event) => {
          clearTimeout(timer);
          reject(new Error(event.message));
        };
      });
      worker.postMessage(urls);
      const [network, frameBeacon] = await Promise.all([
        workerResult,
        globalThis.networkProbe("sendBeacon", beacon),
      ]);
      await new Promise((resolve) => setTimeout(resolve, 250));
      return { network, frameBeacon, violations, crossOriginIsolated, secureContext: isSecureContext };
    } finally {
      worker.terminate();
    }
  }, { mode, urls, beacon: `${run}/frame/sendBeacon` });
  return { ...measured, framed };
}

function within(promise, ms) {
  const timeout = new AbortController();
  return Promise.race([
    promise,
    delay(ms, undefined, { signal: timeout.signal }).then(() => {
      throw new Error(`Timed out after ${ms} ms`);
    }),
  ]).finally(() => timeout.abort());
}

async function runEngine(name, browserType, server, wasmSha256, playwright) {
  const out = {
    engine: name,
    playwright,
    wasmSha256,
    timestamp: new Date().toISOString(),
    origins: { app: server.client, plugin: server.plugin, sink: server.sink },
    headers: { confined: server.policy, control: server.controlPolicy },
    console: [],
    errors: [],
    fixtures: {},
    network: {},
  };
  let browser;

  async function measure(label, run) {
    const page = await browser.newPage();
    page.on("console", (message) => {
      if (["error", "warning"].includes(message.type())) {
        out.console.push({ label, type: message.type(), text: describe(message.text()), location: message.location() });
      }
    });
    page.on("pageerror", (error) => out.errors.push({ label, error: describe(error) }));
    try {
      await page.goto(server.client, { timeout: 8_000 });
      return await within(run(page), 15_000);
    } catch (error) {
      return { error: describe(error) };
    } finally {
      await page.close();
    }
  }

  try {
    browser = await browserType.launch({ timeout: 15_000 });
    out.version = browser.version();
    for (const name of fixtures) {
      const data = await measure(name, (page) => page.evaluate((name) => globalThis.runPluginFixture(name), name));
      out.fixtures[name] = { pass: data.error === undefined && checkFixture(name, data, server), data };
    }
    for (const mode of ["control", "confined"]) {
      out.network[mode] = await measure(mode, (page) => networkPhase(page, server, name, mode));
    }
    await delay(500);
    out.traffic = server.traffic.filter(({ path }) => path.startsWith(`/probe/${name}/`));
    const { control, confined } = out.network;
    out.network.checks = Object.fromEntries(
      Object.keys(success).map((api) => [
        api,
        checkNetwork(
          control.network?.results.find((result) => result.api === api),
          confined.network?.results.find((result) => result.api === api),
          out.traffic,
          confined.network?.violations ?? [],
        ),
      ]),
    );
    out.network.checks.frameSendBeacon = checkNetwork(
      control.frameBeacon,
      confined.frameBeacon,
      out.traffic,
      confined.violations ?? [],
    );
    // Firefox sends a Referer on the control Worker's importScripts request even under Referrer-Policy: no-referrer
    // (an engine behaviour on a request to the local sink, not a plugin request). It is recorded, not hidden.
    const isImport = ({ path }) => path.endsWith("/worker/importScripts");
    out.network.importScriptsReferrerPresent = out.traffic.filter(isImport).some(({ referrerPresent }) => referrerPresent);
    out.network.noReferrer = out.traffic.length > 0 &&
      out.traffic.filter((entry) => !isImport(entry)).every(({ referrerPresent }) => !referrerPresent);
    const networkPass = Object.entries(out.network.checks).every(([api, status]) =>
      status === "blocked" || (api === "sendBeacon" && status === "api-unavailable")
    );
    out.status = networkPass && out.network.noReferrer && Object.values(out.fixtures).every(({ pass }) => pass) &&
        out.errors.length === 0
      ? "passed"
      : "failed";
  } catch (error) {
    out.status = "error";
    out.error = describe(error);
  } finally {
    try {
      await browser?.close();
    } catch (error) {
      out.status = "error";
      out.closeError = describe(error);
    }
  }
  return out;
}

const results = join(root, "results");
const testPin = JSON.parse(await readFile(join(root, "package.json"), "utf8")).devDependencies["@playwright/test"];
await mkdir(results, { recursive: true });

async function save(report) {
  const path = join(results, `${report.engine}.json`);
  await writeFile(`${path}.tmp`, `${JSON.stringify(report, null, 2)}\n`);
  await rename(`${path}.tmp`, path);
  console.log(`${report.engine} ${report.status} (results/${report.engine}.json)`);
  if (report.status !== "passed") process.exitCode = 1;
}

let server;
const written = new Set();
try {
  const pinned = await import("@playwright/test/package.json", { with: { type: "json" } });
  if (pinned.default.version !== testPin) {
    throw new Error(`Expected Playwright ${testPin}, got ${pinned.default.version}`);
  }
  const browsers = await import("@playwright/test");
  const output = join(root, "dist", "browser-core");
  execFileSync("pnpm", ["--filter", "@om/core-wasm", "build:wasm:scope"], {
    cwd: join(root, "../.."),
    env: { ...process.env, OM_WASM_OUT_DIR: output },
    stdio: "inherit",
  });
  const wasm = await readFile(join(output, "om_wasm_scope.wasm"));
  const wasmSha256 = createHash("sha256").update(wasm).digest("hex");
  server = await startServer(wasm);
  for (const name of names) {
    await save(await runEngine(name, browsers[name], server, wasmSha256, testPin));
    written.add(name);
  }
} catch (error) {
  for (const engine of names.filter((name) => !written.has(name))) {
    await save({ engine, playwright: testPin, status: "not-run", error: describe(error) });
  }
  console.error(describe(error));
} finally {
  await server?.close();
}
