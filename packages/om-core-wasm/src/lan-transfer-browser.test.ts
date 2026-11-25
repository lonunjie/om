// SPDX-License-Identifier: MPL-2.0
// local_delivery: a multi-chunk object moves between two real Cores over a real WebRTC DataChannel in each browser engine
// (`@om/web-platform/lan`), with the channel set up through the sealed signalling envelope, the transfer driven by the
// `lan-driver` state machines and confirmed only by the authenticated receipt. Two pages of one context on one machine; it says
// nothing about real phones, Wi-Fi, client isolation or iOS permissions. Runs only where the engines are installed and
// `OM_LAN_BROWSERS=1` (emeth4, Playwright 1.63.0). WebKit on Linux does not establish and is recorded as such.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { chromium, firefox, webkit } from "@playwright/test";
import type { Page } from "@playwright/test";
import ts from "typescript";
import { createLanReceiver, createLanSender } from "./lan-driver";
import { fromHex, toHex } from "./types";
import { SCOPE_WASM, connectPair } from "./test-support-lan-signal";

const enabled = process.env.OM_LAN_BROWSERS === "1" && existsSync(SCOPE_WASM);
const adapter = readFileSync(new URL("../../om-web-platform/src/lan/channel.ts", import.meta.url), "utf8");
const js = ts.transpileModule(adapter, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
const boot = `(async (js) => {
  const mod = await import(URL.createObjectURL(new Blob([js], { type: "text/javascript" })));
  window.__lan = { mod, peer: mod.createLanPeer(mod.browserLanEnvironment()), ch: null };
})`;
interface Ch { send(b: Uint8Array): void; onMessage(cb: (b: Uint8Array) => void): void }
interface Lan { peer: { offer(): Promise<string>; accept(s: string): Promise<string>; finish(s: string): Promise<void>; opened(): Promise<Ch> }; ch: Ch | null }
type W = { __lan: Lan; __toNode: (hex: string) => void };

const TOTAL = 70_000;
const payload = Uint8Array.from({ length: TOTAL }, (_, i) => (i * 31 + 7) & 0xff);
const digest = createHash("sha256").update(payload).digest("hex");
const tag = "5c".repeat(32);

async function attach(page: Page, onFrame: (b: Uint8Array) => void): Promise<void> {
  await page.exposeFunction("__toNode", (hex: string) => { onFrame(fromHex(hex)); });
  await page.evaluate(async () => {
    const L = (window as unknown as W).__lan;
    const ch = await L.peer.opened();
    L.ch = ch;
    ch.onMessage((b) => { (window as unknown as W).__toNode(Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("")); });
  });
}
const sendVia = (page: Page, bytes: Uint8Array): void => {
  void page.evaluate((hex) => { (window as unknown as W).__lan.ch?.send(Uint8Array.from(hex.match(/../g) ?? [], (h) => parseInt(h, 16))); }, toHex(bytes));
};

describe.skipIf(!enabled)("object transfer over a real DataChannel", () => {
  for (const [name, type] of [["chromium", chromium], ["firefox", firefox], ["webkit", webkit]] as const) {
    it(`${name}: a 70,000-byte object arrives in 16 KiB frames and is confirmed only by the receipt`, async () => {
      const { alice, bob, aConv, bConv } = await connectPair();
      const b = await type.launch();
      try {
        const ctx = await b.newContext();
        const [a, c] = [await ctx.newPage(), await ctx.newPage()];
        for (const p of [a, c]) {
          await p.route("http://localhost/", (rt) => rt.fulfill({ contentType: "text/html", body: "<!doctype html><title>lan</title>" }));
          await p.goto("http://localhost/");
          await p.evaluate(`(${boot})(${JSON.stringify(js)})`);
        }
        const sid = "cd".repeat(16);
        const offer = await a.evaluate(() => (window as unknown as W).__lan.peer.offer());
        const so = alice.engines.lanSignal.seal(alice.ctx, aConv, { kind: "offer", sessionId: sid, sdp: offer });
        if ("refused" in so) throw new Error(so.refused);
        const op = bob.engines.lanSignal.open(bob.ctx, bConv, so.envelope);
        if ("refused" in op) throw new Error(op.refused);
        const answer = await c.evaluate((s) => (window as unknown as W).__lan.peer.accept(s), op.sdp);
        const sa = bob.engines.lanSignal.seal(bob.ctx, bConv, { kind: "answer", sessionId: op.sessionId, sdp: answer });
        if ("refused" in sa) throw new Error(sa.refused);
        const oa = alice.engines.lanSignal.open(alice.ctx, aConv, sa.envelope);
        if ("refused" in oa) throw new Error(oa.refused);
        await a.evaluate((s) => (window as unknown as W).__lan.peer.finish(s), oa.sdp);
        const up = await Promise.all([
          a.evaluate(() => (window as unknown as W).__lan.peer.opened().then(() => true, () => false)),
          c.evaluate(() => (window as unknown as W).__lan.peer.opened().then(() => true, () => false)),
        ]);
        if (!up[0] || !up[1]) { expect(name).toBe("webkit"); return; }

        const transferId = "07".repeat(16);
        const stored = new Uint8Array(TOTAL);
        const receiver = createLanReceiver({
          core: bob.core, engines: bob.engines, channel: { send: (m) => { sendVia(c, m); } }, handle: "t", storedOffset: 0,
          write: (o, bytes) => { stored.set(bytes, o); },
          digestMatches: (d) => createHash("sha256").update(stored).digest("hex") === d, receiptTag: () => tag,
        });
        const sender = createLanSender({
          core: alice.core, engines: alice.engines, channel: { send: (m) => { sendVia(a, m); } }, handle: "t", transferId, blobId: "aa".repeat(32), digest,
          objectKind: 8, total: TOTAL, read: (o, l) => payload.subarray(o, o + l), verifyReceipt: (t) => t === tag,
        });
        await attach(a, (bytes) => { sender.onFrame(bytes); });
        await attach(c, (bytes) => { receiver.onFrame(bytes); });
        sender.start();
        for (let i = 0; i < 400 && !sender.confirmed; i++) await new Promise((r) => setTimeout(r, 50));
        expect(receiver.complete).toBe(true);
        expect(toHex(stored)).toBe(toHex(payload));
        expect(sender.confirmed).toBe(true);
      } finally { await b.close(); }
    }, 180_000);
  }
});
