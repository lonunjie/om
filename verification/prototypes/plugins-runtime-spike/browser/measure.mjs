// SPDX-License-Identifier: MPL-2.0
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { callCases, makeFixture, root, runNative, transpileFixture } from "./fixture.mjs";
import { startServer } from "./server.mjs";
import { apis, evaluate } from "./verdict.mjs";

const engineNames = ["chromium", "firefox", "webkit"];
const results = join(root, "results");
const samples = 5;
const [option, ...extra] = process.argv.slice(2);
if (extra.length || (option && !["--native-only", "--no-browser"].includes(option))) {
  throw new Error("Usage: pnpm measure [--native-only | --no-browser]");
}
const date = new Date().toISOString();
const manifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
const toolPins = { pnpm: manifest.packageManager, ...manifest.devDependencies };
const redact = (message) => String(message).replaceAll(root, "<spike>");

await mkdir(results, { recursive: true });

async function record(name, value, preserve = false) {
  const file = join(results, `${name}.json`);
  const json = JSON.stringify(value, null, 2) + "\n";
  if (preserve) {
    await writeFile(file, json, { flag: "wx" }).catch((error) => { if (error.code !== "EEXIST") throw error; });
    return;
  }
  const temp = `${file}.tmp`;
  await writeFile(temp, json);
  await rename(temp, file);
}

function withDeadline(promise, timeoutMs) {
  const controller = new AbortController();
  const timeout = delay(timeoutMs, undefined, { signal: controller.signal }).then(() => {
    throw new Error(`Page did not complete within ${timeoutMs} ms`);
  });
  return Promise.race([promise, timeout]).finally(() => controller.abort());
}

function probeUrls(origin, engine, phase) {
  const path = `${origin}/probe/${engine}/${phase}`;
  return { worker: Object.fromEntries(apis.map((api) => [api, `${path}/worker/${api}`])), frameBeacon: `${path}/frame/sendBeacon` };
}

async function measureEngine(name, browserType, server, native, transpile) {
  const { clientOrigin, pluginOrigin, sinkOrigin } = server;
  const out = { date, engine: name, playwright: toolPins.playwright, component_sha256: transpile.component.sha256,
    origins: { client: clientOrigin, plugin: pluginOrigin, sink: sinkOrigin },
    policies: { control: server.policy(false), confined: server.policy(true) }, console: [], errors: [] };
  let browser;
  try {
    browser = await browserType.launch({ timeout: 15_000 });
    out.version = browser.version();
    const context = await browser.newContext();
    const page = await context.newPage();
    page.on("console", (message) => {
      if (["error", "warning"].includes(message.type())) out.console.push({ type: message.type(), text: redact(message.text()), url: message.location().url });
    });
    page.on("pageerror", (error) => out.errors.push(redact(error.message)));
    const url = new URL(clientOrigin);
    url.search = new URLSearchParams({ plugin: pluginOrigin, sink: sinkOrigin });
    await page.goto(url.href, { timeout: 8_000 });
    for (const phase of ["control", "confined"]) {
      try {
        out[phase] = await withDeadline(page.evaluate((request) => window.measurePwa(request), {
          phase, cases: callCases, urls: probeUrls(sinkOrigin, name, phase), samples,
        }), 90_000);
      } catch (error) {
        out[phase] = { error: redact(error.message) };
        // A page evaluation with a stuck frame must not overlap the next phase.
        if (phase === "control") await page.reload({ timeout: 8_000 });
      }
    }
    await delay(500); // Leave time for queued beacon delivery before comparing the sink receipts.
  } catch (error) {
    out.error = redact(error.message);
  } finally {
    await browser?.close().catch((error) => { out.close_error = redact(error.message); });
  }
  out.traffic = server.traffic.filter(({ path }) => path.startsWith(`/probe/${name}/`));
  out.checks = evaluate(out, native, samples);
  out.status = out.error || out.close_error ? "error" : out.checks.pass ? "passed" : "failed-checks";
  return out;
}

const preflight = { date, node: process.version, platform: process.platform, arch: process.arch, pins: toolPins,
  pnpm_used: process.env.npm_config_user_agent?.match(/\bpnpm\/([^ ]+)/)?.[1] ?? null };
let transpiled;
let native;
try {
  preflight.input = await makeFixture();
  if (option === "--native-only") {
    preflight.transpile = { status: "not-run", reason: "native-only mode" };
  } else {
    try {
      transpiled = await transpileFixture(toolPins["@bytecodealliance/jco"]);
      preflight.transpile = { status: "measured", ...transpiled.report };
    } catch (error) {
      preflight.transpile = { status: "error", error: redact(error.message) };
    }
  }
  native = runNative(Boolean(transpiled));
  preflight.native = native;
} catch (error) {
  preflight.error = redact(error.message);
}
await record("preflight", preflight);
console.log(`Native ${native?.pass ? "passed" : "failed"}. Transpile ${preflight.transpile?.status ?? "not-run"}. See results/preflight.json.`);

let skip = option ? "Browser execution excluded by the selected mode." : null;
if (!skip && (!native || !transpiled)) skip = "A build prerequisite failed. See preflight.json.";
if (skip) {
  for (const engine of engineNames) await record(engine, { engine, playwright: toolPins.playwright, status: "not-run", reason: skip }, true);
  process.exitCode = native?.pass && (option === "--native-only" || transpiled) ? 0 : 1;
} else {
  let server;
  try {
    const playwright = await import("playwright");
    const installed = await import("playwright/package.json", { with: { type: "json" } });
    if (installed.default.version !== toolPins.playwright) throw new Error(`Expected Playwright ${toolPins.playwright}, got ${installed.default.version}`);
    server = await startServer(transpiled.assets);
    for (const engine of engineNames) {
      const out = await measureEngine(engine, playwright[engine], server, native, transpiled.report);
      await record(engine, out);
      console.log(`${engine} ${out.status}. See results/${engine}.json.`);
      if (out.status !== "passed") process.exitCode = 1;
    }
  } catch (error) {
    process.exitCode = 1;
    const reason = redact(error.message);
    for (const engine of engineNames) await record(engine, { engine, status: "not-run", reason }, true);
    console.error(reason);
  } finally {
    await server?.close();
  }
}
