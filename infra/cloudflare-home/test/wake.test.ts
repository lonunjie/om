// SPDX-License-Identifier: AGPL-3.0-or-later
// Feature om.relay.wake.v1 (PC-W01..): the optional wake channel of a queue. Raw WebSockets against the real Worker bundle,
// independent of the reference adapter. The channel carries no content and no capability in its URL.
import { request } from "node:http";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  FEATURES, WAKE_CLOSE, WAKE_SUBPROTOCOL, decodeWakeEvent, encodeWakeAuth, toHex, wakeLabel, type WakeEvent,
} from "@om/home-wire";
import { startHome, type Home } from "./harness";
import { newCaps, rand, relay, sealed, type Caps } from "./kit";

let home: Home;
let off: Home;
beforeAll(async () => {
  home = await startHome();
  off = await startHome({ vars: { OM_WAKE: "0" } });
}, 120_000);
afterAll(async () => { await home.dispose(); await off.dispose(); });

async function queue(): Promise<Caps> {
  const c = newCaps();
  expect((await relay.create(home, c)).ok).toBe(true);
  return c;
}

/** A raw client socket: frames arrive in order; `closed` resolves with the close code. */
class Wake {
  readonly frames: Uint8Array[] = [];
  readonly texts: string[] = [];
  readonly closed: Promise<number>;
  private waiters: (() => void)[] = [];
  readonly ws: WebSocket;
  opened: Promise<void>;
  constructor(readonly url: string, protocols: string[]) {
    this.ws = new WebSocket(url, protocols);
    this.ws.binaryType = "arraybuffer";
    this.opened = new Promise((resolve, reject) => { this.ws.addEventListener("open", () => { resolve(); }); this.ws.addEventListener("error", () => { reject(new Error("socket error")); }); });
    this.opened.catch(() => undefined);
    this.closed = new Promise((resolve) => { this.ws.addEventListener("close", (e) => { resolve(e.code); }); });
    this.ws.addEventListener("message", (e) => {
      if (typeof e.data === "string") this.texts.push(e.data); else this.frames.push(new Uint8Array(e.data as ArrayBuffer));
      for (const w of this.waiters.splice(0)) w();
    });
  }
  async next(timeoutMs = 5000): Promise<WakeEvent> {
    const t0 = Date.now();
    while (this.frames.length === 0) {
      if (Date.now() - t0 > timeoutMs) throw new Error("no frame");
      await new Promise<void>((r) => { this.waiters.push(r); setTimeout(r, 50); });
    }
    return decodeWakeEvent(this.frames.shift() as Uint8Array);
  }
  async quiet(ms = 400): Promise<boolean> { await new Promise((r) => setTimeout(r, ms)); return this.frames.length === 0; }
  send(b: Uint8Array | string): void { this.ws.send(b as string); }
}

/** A plain HTTP request that may carry `Upgrade` (the fetch API forbids that header). Resolves with the status and headers of the answer. */
function raw(h: Home, method: string, headers: Record<string, string>): Promise<{ status: number; headers: Record<string, string | string[] | undefined> }> {
  return new Promise((resolve, reject) => {
    const req = request(h.url + "/v1/relay/wake", { method, headers }, (res) => { res.resume(); resolve({ status: res.statusCode ?? 0, headers: res.headers }); });
    req.on("upgrade", (res, socket) => { socket.destroy(); resolve({ status: res.statusCode ?? 0, headers: res.headers }); });
    req.on("error", reject);
    req.end();
  });
}
const handshake = { connection: "Upgrade", upgrade: "websocket", "sec-websocket-version": "13", "sec-websocket-key": "dGhlIHNhbXBsZSBub25jZQ==" };

const wsUrl = (h: Home): string => h.url.replace(/^http/, "ws") + "/v1/relay/wake";
const connect = (c: Caps, h: Home = home): Wake => new Wake(wsUrl(h), [WAKE_SUBPROTOCOL, wakeLabel(c.prefix)]);
async function authed(c: Caps): Promise<Wake> {
  const w = connect(c);
  await w.opened;
  w.send(encodeWakeAuth({ cap: c.receive }));
  expect((await w.next()).kind).toBe("ready");
  return w;
}

describe("PC-W01 advertisement", () => {
  it("is an advertised optional feature with its limits, and off when the deployment turns it off", async () => {
    const on = await (await home.fetch("/.well-known/om-provider-capabilities")).json() as { features: string[]; limits: Record<string, number> };
    expect(on.features).toContain(FEATURES.relayWake);
    expect(on.features).toContain(FEATURES.relayQueues);
    expect(on.limits["wake_sockets_per_queue"]).toBeGreaterThan(0);
    const no = await (await off.fetch("/.well-known/om-provider-capabilities")).json() as { features: string[]; limits: Record<string, number> };
    expect(no.features).not.toContain(FEATURES.relayWake);
    expect(no.limits["wake_sockets_per_queue"]).toBeUndefined();
    const r = await raw(off, "GET", { ...handshake, "sec-websocket-protocol": `${WAKE_SUBPROTOCOL}, ${wakeLabel(rand(16))}` });
    expect(r.status).toBe(404);
  });
});

describe("PC-W02 the upgrade", () => {
  it("refuses a request that is not a WebSocket upgrade, or has no or several routing labels, with a typed answer", async () => {
    const label = wakeLabel(rand(16));
    for (const headers of [
      {}, // not an upgrade
      { ...handshake }, // no subprotocols
      { ...handshake, "sec-websocket-protocol": label }, // no feature id
      { ...handshake, "sec-websocket-protocol": WAKE_SUBPROTOCOL }, // no label
      { ...handshake, "sec-websocket-protocol": `${WAKE_SUBPROTOCOL}, ${label}, ${wakeLabel(rand(16))}` }, // two labels
      { ...handshake, "sec-websocket-protocol": `${WAKE_SUBPROTOCOL}, om.q.nothex` },
    ]) {
      const r = await raw(home, "GET", headers);
      expect(r.status, JSON.stringify(headers)).toBe(400);
      expect(r.headers["content-type"]).toBe("application/cbor");
    }
  });

  it("the socket address is the bare path: no query, no fragment, and no part of any capability beyond the routing label", async () => {
    const c = await queue();
    const w = connect(c);
    await w.opened;
    const u = new URL(w.ws.url);
    expect(u.search).toBe("");
    expect(u.hash).toBe("");
    expect(u.pathname).toBe("/v1/relay/wake");
    expect(w.ws.url).not.toContain(toHex(c.receive));
    expect(w.ws.url).not.toContain(toHex(c.receive.slice(16)));
    expect(w.ws.protocol).toBe(WAKE_SUBPROTOCOL); // the Home echoes the feature id only, never the label
    w.ws.close();
  });
});

describe("PC-W03 authentication by the first frame", () => {
  it("accepts the receive capability and answers ready with the head sequence", async () => {
    const c = await queue();
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    const w = await authed(c).catch((e: unknown) => { throw e; });
    w.ws.close();
    const w2 = connect(c);
    await w2.opened;
    w2.send(encodeWakeAuth({ cap: c.receive }));
    expect(await w2.next()).toEqual({ kind: "ready", seq: 2 });
    w2.ws.close();
  });

  it("a forged capability, the deposit capability, the admin capability and a queue that does not exist all end the same way", async () => {
    const c = await queue();
    const ghost = newCaps(); // never created
    const cases: [Caps, Uint8Array][] = [[c, rand(48)], [c, c.deposit], [c, c.admin], [c, c.receive.slice(0, 20)], [ghost, ghost.receive]];
    const codes: number[] = [];
    for (const [q, cap] of cases) {
      const w = connect(q);
      await w.opened;
      w.send(encodeWakeAuth({ cap }));
      codes.push(await w.closed);
    }
    expect(new Set(codes)).toEqual(new Set([WAKE_CLOSE.unauthorized]));
  });

  it("anything but one binary auth frame first is a bad frame", async () => {
    const c = await queue();
    for (const first of ["hello", new Uint8Array(0), rand(40), new Uint8Array(1000), encodeWakeAuth({ cap: new Uint8Array(15) })] as const) {
      const w = connect(c);
      await w.opened;
      w.send(first);
      expect(await w.closed).toBe(WAKE_CLOSE.badFrame);
    }
    // after a good auth, any further frame ends the session
    const w = await authed(c);
    w.send(encodeWakeAuth({ cap: c.receive }));
    expect(await w.closed).toBe(WAKE_CLOSE.badFrame);
  });
});

describe("PC-W04 the wake", () => {
  it("a stored item wakes an authenticated socket with its sequence and nothing else; the item stays for the ordinary drain", async () => {
    const c = await queue();
    const w = await authed(c);
    const env = sealed();
    const dep = await relay.deposit(home, c.deposit, env);
    expect(dep.ok).toBe(true);
    const e = await w.next();
    expect(e).toEqual({ kind: "wake", seq: dep.ok ? (dep.value.seq ?? 0) : -1 });
    expect(w.frames).toHaveLength(0);
    const drained = await relay.drain(home, c.receive, 0);
    expect(drained.ok && drained.value.items.length).toBe(1);
    w.ws.close();
  });

  it("an exact retry of a stored deposit does not wake again; a rejected deposit does not wake", async () => {
    const c = await queue();
    const w = await authed(c);
    const env = sealed();
    const id = rand(16);
    expect((await relay.deposit(home, c.deposit, env, id)).ok).toBe(true);
    expect((await w.next()).kind).toBe("wake");
    expect((await relay.deposit(home, c.deposit, env, id)).ok).toBe(true); // idempotent retry
    expect((await relay.deposit(home, rand(48), sealed())).ok).toBe(false); // forged capability
    expect((await relay.deposit(home, c.deposit, rand(40))).ok).toBe(false); // not a sealed envelope
    expect(await w.quiet()).toBe(true);
    w.ws.close();
  });

  it("every authenticated socket of the queue is told; a socket that has not authenticated is not", async () => {
    const c = await queue();
    const a = await authed(c);
    const b = await authed(c);
    const pending = connect(c);
    await pending.opened;
    expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
    expect((await a.next()).kind).toBe("wake");
    expect((await b.next()).kind).toBe("wake");
    expect(await pending.quiet()).toBe(true);
    for (const s of [a, b, pending]) s.ws.close();
  });

  it("other queues are not told", async () => {
    const c1 = await queue();
    const c2 = await queue();
    const w1 = await authed(c1);
    const w2 = await authed(c2);
    expect((await relay.deposit(home, c1.deposit, sealed())).ok).toBe(true);
    expect((await w1.next()).kind).toBe("wake");
    expect(await w2.quiet()).toBe(true);
    w1.ws.close(); w2.ws.close();
  });

  it("wake sequences are increasing, so a client can tell it is behind", async () => {
    const c = await queue();
    const w = await authed(c);
    const seqs: number[] = [];
    for (let i = 0; i < 3; i++) {
      expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
      seqs.push((await w.next()).seq);
    }
    expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
    expect(new Set(seqs).size).toBe(3);
    w.ws.close();
  });

  it("rotation ends the session of the old capability; the new one authenticates", async () => {
    const c = await queue();
    const w = await authed(c);
    const r = await relay.rotate(home, c.receive);
    expect(r.ok).toBe(true);
    expect(await w.closed).toBe(WAKE_CLOSE.superseded);
    if (!r.ok) return;
    const w2 = connect({ ...c, receive: r.value.receiveCap });
    await w2.opened;
    w2.send(encodeWakeAuth({ cap: r.value.receiveCap }));
    expect((await w2.next()).kind).toBe("ready");
    w2.ws.close();
  });
});

describe("PC-W05 keepalive and bounds", () => {
  it("answers ping with pong without any capability", async () => {
    const c = await queue();
    const w = connect(c);
    await w.opened;
    w.send("ping");
    const t0 = Date.now();
    while (w.texts.length === 0 && Date.now() - t0 < 3000) await new Promise((r) => setTimeout(r, 25));
    expect(w.texts).toEqual(["pong"]);
    w.ws.close();
  });

  it("a queue holds a bounded number of sockets: a full queue admits its owner by dropping an idle socket, and refuses a newcomer when every socket is authenticated", async () => {
    const limit = (await (await home.fetch("/.well-known/om-provider-capabilities")).json() as { limits: Record<string, number> }).limits["wake_sockets_per_queue"] ?? 0;
    expect(limit).toBeGreaterThan(1);
    // Strangers hold every slot without authenticating; the owner still gets in.
    const c = await queue();
    const idle: Wake[] = [];
    for (let i = 0; i < limit; i++) { const w = connect(c); await w.opened; idle.push(w); }
    const owner = connect(c);
    await owner.opened;
    owner.send(encodeWakeAuth({ cap: c.receive }));
    expect((await owner.next()).kind).toBe("ready");
    for (const w of [...idle, owner]) w.ws.close();
    // Every slot authenticated: the next connection is refused with a typed rate limit.
    const d = await queue();
    const full: Wake[] = [];
    for (let i = 0; i < limit; i++) full.push(await authed(d));
    const refused = await raw(home, "GET", { ...handshake, "sec-websocket-protocol": `${WAKE_SUBPROTOCOL}, ${wakeLabel(d.prefix)}` });
    expect(refused.status).toBe(429);
    for (const w of full) w.ws.close();
  });
});

describe("PC-W06 what the channel does not do", () => {
  it("sends one small frame per stored item and nothing else: no item, no id, no capability", async () => {
    const c = await queue();
    const w = await authed(c);
    const dep = await relay.deposit(home, c.deposit, sealed());
    expect(dep.ok).toBe(true);
    const raw1 = await new Promise<Uint8Array>((resolve) => { const t = setInterval(() => { const f = w.frames.shift(); if (f) { clearInterval(t); resolve(f); } }, 10); });
    expect(raw1.length).toBeLessThan(16);
    expect(w.frames).toHaveLength(0);
    expect(w.texts).toHaveLength(0);
    w.ws.close();
  });
});
