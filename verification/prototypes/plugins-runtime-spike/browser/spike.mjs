// SPDX-License-Identifier: MPL-2.0
// V-010 feasibility spike (non-production): core-Wasm plugin limits in a Worker on Chromium, Firefox and WebKit.
// Usage: node spike.mjs <path-to-hostile.wasm>
import { chromium, firefox, webkit } from "playwright";
import { readFileSync } from "node:fs";

const wasm = readFileSync(process.argv[2]);
const page = `<!doctype html><meta charset=utf-8><body><script>
const workerSrc = \`
self.onmessage = async (e) => {
  const { bytes, mode } = e.data;
  const t0 = performance.now();
  try {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 64 });
    const mod = await WebAssembly.compile(bytes);
    const imports = { env: { memory } };
    const inst = await WebAssembly.instantiate(mod, imports);
    const t1 = performance.now();
    if (mode === "ok") self.postMessage({ ok: inst.exports.ok(), inst_ms: t1 - t0 });
    else if (mode === "grow") self.postMessage({ grow: inst.exports.grow(), pages: memory.buffer.byteLength / 65536, inst_ms: t1 - t0 });
    else if (mode === "spin") { self.postMessage({ started: true }); inst.exports.spin(); self.postMessage({ returned: true }); }
    else if (mode === "ambient") self.postMessage({ fetch: typeof fetch, xhr: typeof XMLHttpRequest, ws: typeof WebSocket, idb: typeof indexedDB });
    else if (mode === "stripped") {
      for (const k of ["fetch","XMLHttpRequest","WebSocket","EventSource","indexedDB","caches","importScripts","BroadcastChannel","SharedWorker","Worker","WebTransport","navigator"]) { try { delete self[k]; Object.defineProperty(self, k, { value: undefined, configurable: false, writable: false }); } catch (_) {} }
      let reach = {};
      for (const k of ["fetch","XMLHttpRequest","WebSocket","indexedDB","importScripts"]) reach[k] = typeof self[k];
      let viaProto = typeof Object.getPrototypeOf(self).fetch;
      self.postMessage({ reach, viaProto });
    }
  } catch (err) { self.postMessage({ error: String(err).slice(0, 120) }); }
};\`;
window.run = (bytes, mode, watchdogMs) => new Promise((resolve) => {
  const url = URL.createObjectURL(new Blob([workerSrc], { type: "text/javascript" }));
  const w = new Worker(url);
  const t0 = performance.now();
  const msgs = [];
  const done = (extra) => { w.terminate(); resolve({ msgs, wall_ms: Math.round(performance.now() - t0), ...extra }); };
  w.onmessage = (e) => { msgs.push(e.data); if (!e.data.started) done({}); };
  w.onerror = (e) => done({ worker_error: String(e.message).slice(0, 80) });
  w.postMessage({ bytes, mode });
  setTimeout(() => done({ killed_by_watchdog: true }), watchdogMs);
});
</script>`;

for (const [name, type] of [["chromium", chromium], ["firefox", firefox], ["webkit", webkit]]) {
  const out = { engine: name };
  try {
    const browser = await type.launch();
    const ctx = await browser.newContext();
    const p = await ctx.newPage();
    await p.setContent(page);
    out.version = browser.version();
    out.component_api = await p.evaluate(() => typeof WebAssembly.Component);
    out.ok = await p.evaluate((b) => window.run(new Uint8Array(b), "ok", 5000), [...wasm]);
    out.grow = await p.evaluate((b) => window.run(new Uint8Array(b), "grow", 5000), [...wasm]);
    out.spin = await p.evaluate((b) => window.run(new Uint8Array(b), "spin", 800), [...wasm]);
    out.after_spin_page_alive = await p.evaluate(() => 1 + 1);
    out.ambient = await p.evaluate((b) => window.run(new Uint8Array(b), "ambient", 3000), [...wasm]);
    out.stripped = await p.evaluate((b) => window.run(new Uint8Array(b), "stripped", 3000), [...wasm]);
    await browser.close();
  } catch (e) {
    out.error = String(e).slice(0, 200);
  }
  console.log(JSON.stringify(out));
}
