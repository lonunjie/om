// SPDX-License-Identifier: Apache-2.0
import { createHash } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { homedir } from "node:os";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { makeFixture, root, transpileFixture } from "./fixture.mjs";
import { startServer } from "./server.mjs";

const engines = ["chromium", "firefox", "webkit"];
const scenarios = [
  { name: "one", pages: [128] },
  { name: "two", pages: [128, 128] },
  { name: "four", pages: [128, 128, 128, 128], attempt: [128, "instance_limit"], spin: true },
  { name: "aggregate", pages: [256, 256, 256], attempt: [128, "aggregate_memory"] },
  { name: "oversize", pages: [], attempt: [257, "instance_memory"] },
];
const results = join(root, "results");
const pageBytes = 65_536;
const watchdogMs = 500;
const deadlineMs = 8_000;
const pins = JSON.parse(await readFile(join(root, "package.json"), "utf8")).devDependencies;
const date = new Date().toISOString();
const redact = (message) => String(message).replaceAll(root, "<spike>").replaceAll(homedir(), "<home>");

// Shared by the frame and its embedder. The sampled heaps can overlap.
function heap() {
  const bytes = performance.memory?.usedJSHeapSize;
  return { source: "performance.memory.usedJSHeapSize", bytes: Number.isFinite(bytes) ? bytes : null,
    reason: Number.isFinite(bytes) ? null : "performance.memory.usedJSHeapSize is unavailable in this realm" };
}

function workerMain(pageBytes) {
  let plugin;
  self.onmessage = async ({ data }) => {
    try {
      if (data.type === "start") {
        const url = `/bundle/aggregate/${data.pages}`;
        const [{ instantiate }, { default: modules }] = await Promise.all([
          import(`${url}/fixture.js`), import(`${url}/fixture-bytes.js`),
        ]);
        const memories = new Set();
        plugin = await instantiate((name) => {
          if (typeof modules[name] !== "string") throw new Error(`Unexpected core module ${name}`);
          return WebAssembly.compile(Uint8Array.from(atob(modules[name]), (char) => char.charCodeAt(0)));
        }, Object.freeze({}), async (module, imports) => {
          const instance = await WebAssembly.instantiate(module, imports);
          if (instance.exports.memory instanceof WebAssembly.Memory) memories.add(instance.exports.memory);
          return instance;
        });
        plugin.init();
        if (memories.size !== 1) throw new Error(`Expected one fixture memory, observed ${memories.size}`);
        const [memory] = memories;
        self.postMessage({ type: "initialized", wasm_pages: memory.buffer.byteLength / pageBytes,
          cross_origin_isolated: crossOriginIsolated });
      } else if (data.type === "spin") {
        self.postMessage({ type: "spin-started" });
        plugin.onEvent(Uint8Array.of(255));
        self.postMessage({ type: "spin-returned" });
      } else if (data.type === "ping") {
        const reply = plugin.onEvent(Uint8Array.of(42));
        self.postMessage({ type: "pong", value: [...reply] });
      }
    } catch (error) {
      self.postMessage({ type: "worker-error", error: String(error) });
    }
  };
}

function frameMain(watchdogMs) {
  const client = new URL(location.href).searchParams.get("client");
  if (!client || parent === window) return;

  function bind(event) {
    if (!event.isTrusted || event.source !== parent || event.origin !== client ||
      event.data?.type !== "aggregate-bind" || event.ports.length !== 1) return;
    removeEventListener("message", bind);
    const { binding } = event.data;
    const [port] = event.ports;
    const worker = new Worker("/worker.js?phase=confined");
    let next = 1;
    let timer;

    function stop() {
      clearTimeout(timer);
      worker.terminate();
    }

    addEventListener("pagehide", () => { stop(); port.close(); }, { once: true });
    const send = (sequence, data) => port.postMessage({ ...data, binding, sequence });
    port.onmessage = (delivery) => {
      const request = delivery.data;
      if (delivery.target !== port || delivery.ports.length || request?.binding !== binding ||
        request.sequence !== next) return;
      next += 1;
      const spin = request.type === "spin";
      const start = performance.now();
      let started = false;

      function finish(data) {
        if (spin) stop();
        if (data.type === "watchdog-terminated") {
          const stopped = performance.now();
          data.call_to_terminate_ms = stopped - start;
          data.stopped_epoch_ms = performance.timeOrigin + stopped;
        }
        worker.onmessage = worker.onerror = worker.onmessageerror = null;
        send(request.sequence, data);
      }

      worker.onerror = (error) => finish({ type: "worker-error", error: error.message });
      worker.onmessageerror = () => finish({ type: "worker-error", error: "Worker message could not be decoded" });
      worker.onmessage = ({ data }) => {
        if (spin && data.type === "spin-started") {
          started = true;
          send(request.sequence, data);
        } else if (data.type === "initialized") {
          finish({ ...data, frame_heap: heap(), frame_cross_origin_isolated: crossOriginIsolated });
        } else {
          finish(data);
        }
      };
      if (spin) {
        timer = setTimeout(() => finish({ type: "watchdog-terminated", started, limit_ms: watchdogMs }), watchdogMs);
      }
      worker.postMessage({ type: request.type, pages: request.pages });
    };
    send(0, { type: "bound" });
  }

  addEventListener("message", bind);
  parent.postMessage({ type: "aggregate-ready" }, client);
}

async function measureScenario({ scenario, pluginOrigin, pageBytes, deadlineMs }) {
  const { reserveInstance, budgetSnapshot, BudgetError, heap } =
    await import(new URL("/bundle/aggregate-budget.js", location.href).href);
  const out = { heap_before: heap(), instances: [],
    heap_live: { bytes: null, reason: "Cohort did not reach startup sampling" }, error: null };
  const open = [];

  function wait(subscribe) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        unsubscribe();
        reject(new Error("Aggregate frame response deadline"));
      }, deadlineMs);
      const unsubscribe = subscribe((value) => {
        clearTimeout(timer);
        unsubscribe();
        resolve(value);
      });
    });
  }

  async function instantiate(pages) {
    const start = performance.now();
    const row = { requested_pages: pages, wasm_pages: null, first_response_ms: null };
    let release;
    try {
      release = reserveInstance(pages * pageBytes);
    } catch (error) {
      if (!(error instanceof BudgetError)) throw error;
      return { row: { ...row, status: "refused", reason: error.reason, stage: "before-frame",
        wasm_pages: 0, refusal_ms: performance.now() - start } };
    }

    const frame = document.createElement("iframe");
    const { port1, port2 } = new MessageChannel();
    const binding = crypto.randomUUID();
    let sequence = 0;
    const close = () => { port1.close(); port2.close(); frame.remove(); release(); };
    const endpoint = { close };
    open.push(endpoint);

    function receive(expected, progress) {
      return wait((finish) => {
        port1.onmessage = (event) => {
          const data = event.data;
          if (event.target !== port1 || event.ports.length || data?.binding !== binding ||
            data.sequence !== expected) return;
          const at = performance.now();
          if (progress && data.type === "spin-started") progress();
          else finish({ data, at });
        };
        return () => { port1.onmessage = null; };
      });
    }

    endpoint.command = (type, progress) => {
      sequence += 1;
      const response = receive(sequence, progress);
      port1.postMessage({ type, binding, sequence, pages });
      return response;
    };

    try {
      frame.setAttribute("sandbox", "allow-scripts allow-same-origin");
      frame.allow = `cross-origin-isolated ${pluginOrigin}`;
      const url = new URL("/frame", pluginOrigin);
      url.search = new URLSearchParams({ phase: "confined", client: location.origin });
      const ready = wait((finish) => {
        function bootstrap(event) {
          if (event.isTrusted && event.source === frame.contentWindow && event.origin === pluginOrigin &&
            !event.ports.length && event.data?.type === "aggregate-ready") finish();
        }
        addEventListener("message", bootstrap);
        return () => removeEventListener("message", bootstrap);
      });
      frame.src = url.href;
      document.body.append(frame);
      await ready;
      const bound = receive(0);
      frame.contentWindow.postMessage({ type: "aggregate-bind", binding }, pluginOrigin, [port2]);
      if ((await bound).data.type !== "bound") throw new Error("Frame binding failed");
      const { data, at } = await endpoint.command("start");
      if (data.type !== "initialized") throw new Error(data.error ?? data.type);
      return { endpoint, row: { ...row, ...data, status: "started",
        first_response_ms: at - start } };
    } catch (error) {
      close();
      return { row: { ...row, status: "error", reason: String(error) } };
    }
  }

  async function ping(endpoint) {
    const sent = performance.now();
    try {
      const { data, at } = await endpoint.command("ping");
      const pong = data.type === "pong" && data.value?.length === 1 && data.value[0] === 42;
      return { reply_ms: pong ? at - sent : null, reply_epoch_ms: pong ? performance.timeOrigin + at : null,
        reason: pong ? null : data.error ?? "Fixture did not echo the ping" };
    } catch (error) {
      return { reply_ms: null, reply_epoch_ms: null, reason: String(error) };
    }
  }

  try {
    const instances = await Promise.all(scenario.pages.map(instantiate));
    out.instances = instances.map(({ row }) => row);
    out.heap_live = heap();
    out.reservations_before_attempt = budgetSnapshot();
    if (scenario.attempt) {
      const [pages] = scenario.attempt;
      out.attempt = (await instantiate(pages)).row;
      out.reservations_after_attempt = budgetSnapshot();
    }
    if (scenario.spin && instances.every(({ endpoint }) => endpoint)) {
      const [loop, ...neighbours] = instances.map(({ endpoint }) => endpoint);
      const pingAll = () => Promise.all(neighbours.map(ping));
      const before = await pingAll();
      let during;
      const { data: stop } = await loop.command("spin", () => { during ??= pingAll(); });
      const overlap = during ? await during : [];
      loop.close();
      const after = await pingAll();
      out.watchdog = { stop, reservations_after_stop: budgetSnapshot(), neighbours: before.map((baseline, index) => {
        const overlapPing = overlap[index];
        return { before: baseline, during: overlapPing ?? { reply_ms: null, reason: "No loop-start signal" },
          replied_before_termination: Boolean(overlapPing?.reply_epoch_ms &&
            overlapPing.reply_epoch_ms < stop.stopped_epoch_ms), after: after[index] };
      }) };
    } else if (scenario.spin) {
      out.watchdog = { stop: null, reason: "The four fixture instances did not all start" };
    }
  } catch (error) {
    out.error = String(error);
  } finally {
    open.forEach(({ close }) => close());
    out.reservations_after_cleanup = budgetSnapshot();
  }
  return out;
}

function passed(scenario, out) {
  if (out.error || out.instances?.length !== scenario.pages.length ||
    !out.instances.every((row, index) => row.status === "started" && row.wasm_pages === scenario.pages[index])) return false;
  const { instances, memoryBytes } = out.reservations_before_attempt;
  if (instances !== scenario.pages.length || memoryBytes !== scenario.pages.reduce((a, b) => a + b, 0) * pageBytes ||
    out.reservations_after_cleanup.instances !== 0 || out.reservations_after_cleanup.memoryBytes !== 0) return false;
  if (scenario.attempt && (out.attempt?.status !== "refused" || out.attempt.stage !== "before-frame" ||
    out.attempt.reason !== scenario.attempt[1] ||
    out.reservations_after_attempt.instances !== instances || out.reservations_after_attempt.memoryBytes !== memoryBytes)) return false;
  if (!scenario.spin) return true;
  const { stop, neighbours, reservations_after_stop } = out.watchdog;
  return stop?.type === "watchdog-terminated" && stop.started &&
    reservations_after_stop.instances === instances - 1 && reservations_after_stop.memoryBytes === memoryBytes - scenario.pages[0] * pageBytes &&
    neighbours.every(({ before, replied_before_termination, after }) =>
      before.reply_ms !== null && replied_before_termination && after.reply_ms !== null);
}

async function measureEngine(engine, browserType, server, fixtures) {
  const { clientOrigin, pluginOrigin } = server;
  const out = { date, engine, playwright: pins.playwright, fixtures,
    origins: { client: clientOrigin, plugin: pluginOrigin }, policies: { confined: server.policy(true) },
    console: [], errors: [], scenarios: {} };
  let browser;
  try {
    browser = await browserType.launch({ timeout: 15_000 });
    out.version = browser.version();
    for (const scenario of scenarios) {
      const controller = new AbortController();
      const page = await browser.newPage();
      page.on("console", (message) => {
        if (["error", "warning"].includes(message.type())) out.console.push({ scenario: scenario.name,
          type: message.type(), text: redact(message.text()), url: message.location().url });
      });
      page.on("pageerror", (error) => out.errors.push({ scenario: scenario.name, error: redact(error.message) }));
      try {
        await page.goto(clientOrigin, { timeout: deadlineMs });
        const deadline = delay(4 * deadlineMs, undefined, { signal: controller.signal }).then(() => {
          throw new Error("Aggregate scenario deadline");
        });
        out.scenarios[scenario.name] = await Promise.race([deadline,
          page.evaluate(measureScenario, { scenario, pluginOrigin, pageBytes, deadlineMs })]);
      } catch (error) {
        out.scenarios[scenario.name] = { error: redact(error.message),
          heap_live: { bytes: null, reason: "Scenario did not complete" } };
      } finally {
        controller.abort();
        await page.close();
      }
    }
  } catch (error) {
    out.error = redact(error.message);
  } finally {
    await browser?.close().catch((error) => { out.close_error = redact(error.message); });
  }
  out.checks = Object.fromEntries(scenarios.map((scenario) => [
    scenario.name, passed(scenario, out.scenarios[scenario.name] ?? { error: "Scenario did not run" }),
  ]));
  out.status = out.error || out.close_error ? "error" :
    !out.errors.length && Object.values(out.checks).every(Boolean) ? "passed" : "failed-checks";
  return out;
}

async function record(engine, out) {
  const file = join(results, `aggregate-${engine}.json`);
  await writeFile(`${file}.tmp`, JSON.stringify(out, null, 2) + "\n");
  await rename(`${file}.tmp`, file);
  console.log(`${engine} ${out.status}. See results/aggregate-${engine}.json.`);
  if (out.status !== "passed") process.exitCode = 1;
}

async function prepare() {
  const playwright = await import("playwright");
  const installed = await import("playwright/package.json", { with: { type: "json" } });
  if (installed.default.version !== pins.playwright) throw new Error(`Expected Playwright ${pins.playwright}, got ${installed.default.version}`);
  const budget = await readFile(new URL("../../../../packages/om-plugin-frame/embedder/budget.ts", import.meta.url), "utf8");
  const scriptOverrides = {
    "frame.js": `${heap.toString()}\n(${frameMain.toString()})(${watchdogMs});`,
    "worker.js": `(${workerMain.toString()})(${pageBytes});`,
  };
  const assets = { "aggregate-budget.js": `${stripTypeScriptTypes(budget)}\nexport ${heap.toString()}\n` };
  const fixtures = {};
  const sizes = new Set(scenarios.flatMap(({ pages, attempt }) => attempt ? [...pages, attempt[0]] : pages));
  for (const fixedPages of sizes) {
    const input = await makeFixture({ fixedPages });
    const fixture = await transpileFixture(pins["@bytecodealliance/jco"], { fixedPages });
    fixtures[fixedPages] = { input, component_sha256: fixture.report.component.sha256 };
    for (const [name, content] of Object.entries(fixture.assets)) assets[`aggregate/${fixedPages}/${name}`] = content;
  }
  const budget_sha256 = createHash("sha256").update(budget).digest("hex");
  const server = await startServer(assets, scriptOverrides);
  return { playwright, server, fixtures, budget_sha256 };
}

await mkdir(results, { recursive: true });
let setup;
try {
  setup = await prepare();
} catch (error) {
  for (const engine of engines) await record(engine, { date, engine, playwright: pins.playwright,
    status: "not-run", reason: redact(error.message) });
}
if (setup) {
  const { playwright, server, fixtures, budget_sha256 } = setup;
  try {
    for (const engine of engines) {
      const out = await measureEngine(engine, playwright[engine], server, fixtures);
      await record(engine, { ...out, budget_sha256, node: process.version, platform: process.platform, arch: process.arch });
    }
  } finally {
    await server.close();
  }
}
