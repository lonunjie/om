// SPDX-License-Identifier: MPL-2.0
// local_delivery slice 4 end to end: the real `@om/web-platform/lan` adapter in each browser engine, with the offer and answer
// sealed and opened by two real Cores (a pair seed from a real accepted relationship) instead of being passed in clear. Two
// pages of one context on one machine; it says nothing about real phones, Wi-Fi, client isolation or iOS permissions.
// Runs only where the engines are installed and `OM_LAN_BROWSERS=1` (emeth4, Playwright 1.63.0). An engine that cannot
// establish is recorded as such (WebKit on Linux did not in the earlier spike), not failed: the capability row says so.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { chromium, firefox, webkit } from "@playwright/test";
import ts from "typescript";
import { SCOPE_WASM, connectPair } from "./lan-signal-support";

const enabled = process.env.OM_LAN_BROWSERS === "1" && existsSync(SCOPE_WASM);
const adapter = readFileSync(new URL("../../om-web-platform/src/lan/channel.ts", import.meta.url), "utf8");
const js = ts.transpileModule(adapter, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
const FRAMES = 64;
const boot = `(async (js) => {
  const mod = await import(URL.createObjectURL(new Blob([js], { type: "text/javascript" })));
  window.__lan = { mod, got: 0, order: true, last: -1 };
})`;

interface Channel { send(b: Uint8Array): void; canSend(): boolean; onDrain(cb: () => void): void; onMessage(cb: (b: Uint8Array) => void): void }
interface Peer { offer(): Promise<string>; accept(s: string): Promise<string>; finish(s: string): Promise<void>; opened(): Promise<Channel> }
interface Lan { mod: { createLanPeer(e: unknown): Peer; browserLanEnvironment(): unknown; lanChannelCapability(e: unknown): string }; peer: Peer; got: number; order: boolean; last: number; drains: number }
type LanWindow = { __lan: Lan };

describe.skipIf(!enabled)("sealed signalling in real browser engines", () => {
  for (const [name, type] of [["chromium", chromium], ["firefox", firefox], ["webkit", webkit]] as const) {
    it(`${name}: sealed offer and answer, then frames in order where the engine establishes`, async () => {
      const { alice, bob, aConv, bConv } = await connectPair();
      const b = await type.launch();
      try {
        const ctx = await b.newContext();
        const [a, c] = [await ctx.newPage(), await ctx.newPage()];
        for (const p of [a, c]) {
          await p.route("http://localhost/", (rt) => rt.fulfill({ contentType: "text/html", body: "<!doctype html><title>lan</title>" }));
          await p.goto("http://localhost/");
          await p.evaluate(`(${boot})(${JSON.stringify(js)})`);
          await p.evaluate(() => { const L = (window as unknown as LanWindow).__lan; L.peer = L.mod.createLanPeer(L.mod.browserLanEnvironment()); L.drains = 0; });
        }
        const capability = await a.evaluate(() => { const L = (window as unknown as LanWindow).__lan; return L.mod.lanChannelCapability(L.mod.browserLanEnvironment()); });
        expect(capability).toBe("available");
        const sessionId = "ab".repeat(16);
        const offer = await a.evaluate(() => (window as unknown as LanWindow).__lan.peer.offer());
        const sealedOffer = alice.engines.lanSignal.seal(alice.ctx, aConv, { kind: "offer", sessionId, sdp: offer });
        if ("refused" in sealedOffer) throw new Error(sealedOffer.refused);
        expect(sealedOffer.envelope).not.toContain(Buffer.from("v=0").toString("hex"));
        const opened = bob.engines.lanSignal.open(bob.ctx, bConv, sealedOffer.envelope);
        if ("refused" in opened) throw new Error(opened.refused);
        expect(opened.sdp).toBe(offer);
        const answer = await c.evaluate((s) => (window as unknown as LanWindow).__lan.peer.accept(s), opened.sdp);
        const sealedAnswer = bob.engines.lanSignal.seal(bob.ctx, bConv, { kind: "answer", sessionId: opened.sessionId, sdp: answer });
        if ("refused" in sealedAnswer) throw new Error(sealedAnswer.refused);
        const back = alice.engines.lanSignal.open(alice.ctx, aConv, sealedAnswer.envelope);
        if ("refused" in back) throw new Error(back.refused);
        expect(back.sdp).toBe(answer);
        await a.evaluate((s) => (window as unknown as LanWindow).__lan.peer.finish(s), back.sdp);
        const established = await Promise.all([
          a.evaluate(() => (window as unknown as LanWindow).__lan.peer.opened().then(() => true, () => false)),
          c.evaluate(() => (window as unknown as LanWindow).__lan.peer.opened().then((ch) => {
            const L = (window as unknown as LanWindow).__lan;
            ch.onMessage((v) => { const n = (v[0]! << 8) | v[1]!; if (n !== L.last + 1) L.order = false; L.last = n; L.got++; });
            return true;
          }, () => false)),
        ]);
        if (!established[0] || !established[1]) {
          expect(name).toBe("webkit"); // only the engine the spike already found unable to establish may fail to
          return;
        }
        await a.evaluate(async (n) => {
          const L = (window as unknown as LanWindow).__lan;
          const ch = await L.peer.opened();
          ch.onDrain(() => { L.drains++; });
          for (let i = 0; i < n; i++) {
            while (!ch.canSend()) { const prev = L.drains; await new Promise<void>((res) => { const t = setInterval(() => { if (L.drains > prev) { clearInterval(t); res(); } }, 5); }); }
            const f = new Uint8Array(16384); f[0] = i >> 8; f[1] = i & 255; ch.send(f);
          }
        }, FRAMES);
        const got = await c.evaluate(async (n) => { const L = (window as unknown as LanWindow).__lan; for (let i = 0; i < 200 && L.got < n; i++) await new Promise((r) => setTimeout(r, 50)); return { got: L.got, order: L.order }; }, FRAMES);
        expect(got).toEqual({ got: FRAMES, order: true });
      } finally { await b.close(); }
    }, 180_000);
  }
});
