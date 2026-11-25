// SPDX-License-Identifier: MPL-2.0
// local_delivery slice 4: runs the real `@om/web-platform/lan` adapter (packages/om-web-platform/src/lan/channel.ts) in each browser
// engine, two pages in one context on one machine, offer and answer passed by this script the way copy/paste or QR would.
// The pages are served as http://localhost (a secure context); on about:blank the adapter correctly reports `unavailable`.
// Records per engine: established, frames in order, the adapter's backpressure behaviour. It says nothing about real phones,
// a real Wi-Fi network, client isolation or iOS permissions.
// Run from the repo root on the build host: node verification/prototypes/lan-datachannel/adapter-e2e.mjs [--json out.json]
import { readFileSync, writeFileSync } from "node:fs";
import { chromium, firefox, webkit } from "@playwright/test";
import ts from "typescript";

const src = readFileSync(new URL("../../../packages/om-web-platform/src/lan/channel.ts", import.meta.url), "utf8");
const js = ts.transpileModule(src, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
const FRAMES = 128;

const PAGE = `(async (js) => {
  const mod = await import(URL.createObjectURL(new Blob([js], { type: "text/javascript" })));
  window.__lan = { mod, got: 0, order: true, last: -1, bytes: 0, drains: 0, maxBuffered: 0 };
})`;

const out = {};
for (const [name, type] of [["chromium", chromium], ["firefox", firefox], ["webkit", webkit]]) {
  const r = { engine: name };
  let b;
  try {
    b = await type.launch();
    const ctx = await b.newContext();
    const [a, c] = [await ctx.newPage(), await ctx.newPage()];
    for (const p of [a, c]) { await p.route("http://localhost/", (rt) => rt.fulfill({ contentType: "text/html", body: "<!doctype html><title>lan</title>" })); await p.goto("http://localhost/"); await p.evaluate(`(${PAGE})(${JSON.stringify(js)})`); }
    r.capability = await a.evaluate(() => window.__lan.mod.lanChannelCapability(window.__lan.mod.browserLanEnvironment()));
    await a.evaluate(() => { const m = window.__lan.mod; window.__lan.peer = m.createLanPeer(m.browserLanEnvironment()); });
    await c.evaluate(() => { const m = window.__lan.mod; window.__lan.peer = m.createLanPeer(m.browserLanEnvironment()); });
    const offer = await a.evaluate(() => window.__lan.peer.offer());
    const answer = await c.evaluate((s) => window.__lan.peer.accept(s), offer);
    await a.evaluate((s) => window.__lan.peer.finish(s), answer);
    await c.evaluate(async () => {
      const ch = await window.__lan.peer.opened();
      ch.onMessage((v) => { const L = window.__lan, n = (v[0] << 8) | v[1]; if (n !== L.last + 1) L.order = false; L.last = n; L.got++; L.bytes += v.byteLength; });
    });
    const sent = await a.evaluate(async (N) => {
      const L = window.__lan, ch = await L.peer.opened();
      ch.onDrain(() => { L.drains++; });
      let paused = 0;
      for (let i = 0; i < N; i++) {
        while (!ch.canSend()) { paused++; await new Promise((res) => { const prev = L.drains; const t = setInterval(() => { if (L.drains > prev) { clearInterval(t); res(); } }, 5); }); }
        const f = new Uint8Array(16384); f[0] = i >> 8; f[1] = i & 255; ch.send(f);
      }
      return { paused };
    }, FRAMES);
    const got = await c.evaluate(async (N) => { const L = window.__lan; for (let i = 0; i < 200 && L.got < N; i++) await new Promise((r) => setTimeout(r, 50)); return { got: L.got, bytes: L.bytes, order: L.order }; }, FRAMES);
    Object.assign(r, { established: true, frames: got.got, bytes: got.bytes, inOrder: got.order, pausedForBackpressure: sent.paused, sdpHasStun: /stun:|typ srflx|typ relay/.test(offer + answer) });
    r.pass = got.got === FRAMES && got.order && !r.sdpHasStun;
  } catch (e) { r.established = false; r.error = String(e).slice(0, 300); }
  finally { await b?.close(); }
  out[name] = r;
  console.log(JSON.stringify(r));
}
const i = process.argv.indexOf("--json");
if (i > 0) writeFileSync(process.argv[i + 1], JSON.stringify({ kind: "lan_adapter_e2e", frames: FRAMES, engines: out }, null, 1) + "\n");
