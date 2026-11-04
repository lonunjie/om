// SPDX-License-Identifier: AGPL-3.0-or-later
// The adapter's use of the wake channel (om.relay.wake.v1) against the real Home: an open socket replaces the timer, and any
// failure or absence of the channel leaves ordinary polling exactly as it was.
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { toHex } from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl, type HttpProvider } from "@om/provider-http";
import { startHome, type Home } from "./harness";
import { sealed } from "./kit";

const rand = (n: number): Uint8Array => crypto.getRandomValues(new Uint8Array(n));
const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
const envelope = (): string => toHex(sealed());

let home: Home;
let off: Home;
beforeAll(async () => { home = await startHome(); off = await startHome({ vars: { OM_WAKE: "0" } }); }, 120_000);
afterAll(async () => { await home.dispose(); await off.dispose(); });

const urlOf = (i: RequestInfo | URL): string => (typeof i === "string" ? i : i instanceof URL ? i.href : i.url);

interface Counted { readonly p: HttpProvider; readonly drains: () => number; readonly requests: () => number }
function mk(h: Home, extra: Partial<Parameters<typeof createHttpProvider>[0]> = {}, secrets = createMemorySecretStore()): Counted {
  const config = parseProviderUrl(h.url, { allowLoopbackHttp: true });
  if (!config) throw new Error("url");
  let drains = 0;
  let total = 0;
  const p = createHttpProvider({
    config, secrets, timeoutMs: 10_000, ...extra,
    fetch: (i, init) => {
      total++;
      if (urlOf(i).endsWith("/v1/relay/drain")) drains++;
      return fetch(i, init);
    },
  });
  return { p, drains: () => drains, requests: () => total };
}

async function until(f: () => boolean | Promise<boolean>, ms = 8000): Promise<void> {
  const t0 = Date.now();
  while (!(await f())) {
    if (Date.now() - t0 > ms) throw new Error("condition not met");
    await sleep(25);
  }
}

/** Polls (as the runtime's timer would) until the provider holds a live socket for the mailbox. */
async function goLive(r: Counted, mailbox: string, after = 0): Promise<void> {
  r.p.setWakeActive(true);
  await until(async () => { await r.p.poll(mailbox, after); return r.p.wakeStats().live === 1; });
}

describe("a live wake channel replaces the timer", () => {
  it("polls ask the Home nothing while a socket vouches the queue is quiet; a deposit wakes the client and the next poll collects it", async () => {
    const recipient = mk(home);
    const sender = mk(home);
    const mailbox = toHex(rand(16));
    await recipient.p.poll(mailbox, 0); // claims the queue
    await goLive(recipient, mailbox);
    const before = recipient.requests();
    for (let i = 0; i < 30; i++) expect(await recipient.p.poll(mailbox, 0)).toEqual([]);
    expect(recipient.requests() - before).toBe(0);

    let woken = 0;
    recipient.p.onWake(() => { woken++; });
    expect(await sender.p.perform({ call: "deposit", mailbox, bytes: envelope() })).toEqual({ result: "ok" });
    await until(() => woken === 1);
    const got = await recipient.p.poll(mailbox, 0);
    expect(got).toHaveLength(1);
    // having collected it, it is quiet again (with the cursor Core would hold)
    const seq = got[0]?.seq ?? 0;
    const mid = recipient.requests();
    for (let i = 0; i < 10; i++) expect(await recipient.p.poll(mailbox, seq)).toEqual([]);
    expect(recipient.requests() - mid).toBe(0);
    expect(woken).toBe(1);
    recipient.p.dispose();
  });

  it("a second deposit wakes again, and mail that arrived before the socket opened is found by the catch-up", async () => {
    const recipient = mk(home);
    const sender = mk(home);
    const mailbox = toHex(rand(16));
    await recipient.p.poll(mailbox, 0);
    expect(await sender.p.perform({ call: "deposit", mailbox, bytes: envelope() })).toEqual({ result: "ok" });
    let woken = 0;
    recipient.p.onWake(() => { woken++; });
    recipient.p.setWakeActive(true);
    // The first poll drains normally (no socket yet) and finds it; polls that follow are quiet once the socket is up.
    const first = await recipient.p.poll(mailbox, 0);
    expect(first).toHaveLength(1);
    await until(async () => { await recipient.p.poll(mailbox, first[0]?.seq ?? 0); return recipient.p.wakeStats().live === 1; });
    expect(await sender.p.perform({ call: "deposit", mailbox, bytes: envelope() })).toEqual({ result: "ok" });
    await until(() => woken >= 1);
    const next = await recipient.p.poll(mailbox, first[0]?.seq ?? 0);
    expect(next).toHaveLength(1);
    recipient.p.dispose();
  });

  it("with a socket open, several mailboxes each cost one connection and no polling", async () => {
    const recipient = mk(home);
    const boxes = [toHex(rand(16)), toHex(rand(16)), toHex(rand(16))];
    for (const m of boxes) await recipient.p.poll(m, 0);
    recipient.p.setWakeActive(true);
    await until(async () => { for (const m of boxes) await recipient.p.poll(m, 0); return recipient.p.wakeStats().live === 3; });
    const before = recipient.requests();
    for (let i = 0; i < 10; i++) for (const m of boxes) await recipient.p.poll(m, 0);
    expect(recipient.requests() - before).toBe(0);
    recipient.p.dispose();
  });
});

describe("the channel is an optimisation, never a dependency", () => {
  it("a Home that does not advertise it is polled exactly as before", async () => {
    const recipient = mk(off);
    const mailbox = toHex(rand(16));
    recipient.p.setWakeActive(true);
    await recipient.p.poll(mailbox, 0);
    await sleep(300);
    const before = recipient.drains();
    for (let i = 0; i < 5; i++) await recipient.p.poll(mailbox, 0);
    expect(recipient.drains() - before).toBe(5);
    expect(recipient.p.wakeStats().connects).toBe(0);
  });

  it("with the app hidden (inactive) there is no socket and every poll drains; turning it on again reconnects", async () => {
    const recipient = mk(home);
    const mailbox = toHex(rand(16));
    await recipient.p.poll(mailbox, 0);
    await goLive(recipient, mailbox);
    recipient.p.setWakeActive(false);
    expect(recipient.p.wakeStats().sockets).toBe(0);
    const before = recipient.drains();
    for (let i = 0; i < 4; i++) await recipient.p.poll(mailbox, 0);
    expect(recipient.drains() - before).toBe(4);
    await goLive(recipient, mailbox);
    recipient.p.dispose();
  });

  it("webSocket: false keeps the provider on polling", async () => {
    const recipient = mk(home, { webSocket: false });
    const mailbox = toHex(rand(16));
    recipient.p.setWakeActive(true);
    for (let i = 0; i < 3; i++) await recipient.p.poll(mailbox, 0);
    expect(recipient.drains()).toBe(3);
    expect(recipient.p.wakeStats().connects).toBe(0);
  });

  it("a socket that cannot open leaves polling in place (bad address, nothing listening)", async () => {
    const recipient = mk(home, { webSocket: (_u, protocols) => new WebSocket("ws://127.0.0.1:1/v1/relay/wake", protocols), wake: { connectMs: 300 } });
    const mailbox = toHex(rand(16));
    recipient.p.setWakeActive(true);
    for (let i = 0; i < 3; i++) { await recipient.p.poll(mailbox, 0); await sleep(150); }
    expect(recipient.drains()).toBe(3);
    expect(recipient.p.wakeStats().live).toBe(0);
    recipient.p.dispose();
  });

  it("a live socket still lets one real drain through now and then (a lost wake costs at most the safety interval)", async () => {
    const recipient = mk(home, { wake: { safetyMs: 400 } });
    const mailbox = toHex(rand(16));
    await recipient.p.poll(mailbox, 0);
    await goLive(recipient, mailbox);
    await sleep(500);
    const before = recipient.drains();
    await recipient.p.poll(mailbox, 0);
    expect(recipient.drains() - before).toBe(1);
    const mid = recipient.drains();
    await recipient.p.poll(mailbox, 0); // the drain re-verified the socket
    expect(recipient.drains() - mid).toBe(0);
    recipient.p.dispose();
  });

  it("a mailbox this install does not own is not served by a socket (it is reported lost by polling, as before)", async () => {
    const owner = mk(home);
    const intruder = mk(home);
    const mailbox = toHex(rand(16));
    await owner.p.poll(mailbox, 0);
    intruder.p.setWakeActive(true);
    for (let i = 0; i < 6; i++) { await intruder.p.poll(mailbox, 0); await sleep(100); }
    expect(intruder.p.wakeStats().live).toBe(0);
    expect(intruder.p.lostMailboxes()).toContain(mailbox);
    owner.p.dispose(); intruder.p.dispose();
  });
});
