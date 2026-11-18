// SPDX-License-Identifier: AGPL-3.0-or-later
// Feature om.relay.push.v1 (PC-P01..; ADR-0210): the optional push wake, against the real Worker bundle. The push service is a
// local HTTP server that records exactly what the Home sends it, so the test can check that a wake is an EMPTY, signed request.
// Delivery through a browser vendor's real push service cannot run here (no vendor account, no browser); that part is checked by hand.
import { createServer, type IncomingMessage, type Server } from "node:http";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  ADMISSION_HEADER, FEATURES, PUSH_PATHS, WAKE_SUBPROTOCOL, admissionHash, encodeQueueCreate, encodeWakeAuth, fromBase64Url, toBase64Url, wakeLabel,
} from "@om/home-wire";
import { scratch, startHome, type Home } from "./harness";
import { asBody, bearer, newCaps, rand, relay, sealed, type Caps } from "./kit";

interface Seen { method: string; path: string; headers: IncomingMessage["headers"]; bytes: number }
const seen: Seen[] = [];
let status = 201;
let svc: Server;
let origin = "";
const secret = rand(32);
let home: Home; // admission, default pacing
let paced: Home; // no minimum interval, two a day
let fast: Home; // one-second interval
let open: Home; // no admission: not offered
let offBy: Home; // OM_PUSH=0

const hosts = () => ({ OM_PUSH_HOSTS: origin });
beforeAll(async () => {
  svc = createServer((req, res) => { seen.push({ method: req.method ?? "", path: req.url ?? "", headers: req.headers, bytes: 0 }); req.on("data", (d: Buffer) => { (seen.at(-1) as Seen).bytes += d.length; }); req.on("end", () => { res.statusCode = status; res.end(); }); });
  await new Promise<void>((r) => svc.listen(0, "127.0.0.1", r));
  origin = `http://127.0.0.1:${String((svc.address() as { port: number }).port)}`;
  const adm = await admissionHash(secret);
  home = await startHome({ vars: { OM_ADMISSION: adm, ...hosts() } });
  paced = await startHome({ vars: { OM_ADMISSION: adm, ...hosts(), OM_PUSH_MIN_INTERVAL_SECONDS: "0", OM_PUSH_PER_DAY: "2" } });
  fast = await startHome({ vars: { OM_ADMISSION: adm, ...hosts(), OM_PUSH_MIN_INTERVAL_SECONDS: "1" } });
  open = await startHome({ vars: hosts() });
  offBy = await startHome({ vars: { OM_ADMISSION: adm, OM_PUSH: "0", ...hosts() } });
}, 180_000);
afterAll(async () => { for (const h of [home, paced, fast, open, offBy]) await h.dispose(); await new Promise((r) => svc.close(r)); });

const adm = { [ADMISSION_HEADER]: toBase64Url(secret) };
async function queue(h: Home): Promise<Caps> {
  const c = newCaps();
  const r = await h.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor", ...adm }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })) });
  expect(r.ok).toBe(true);
  return c;
}
const body = (endpoint: string): string => JSON.stringify({ endpoint });
let n = 0;
const endpoint = (): string => `${origin}/push/${String(Date.now())}-${String(n++)}`;
const reg = (h: Home, c: Caps, ep: string, headers: Record<string, string> = adm, cap: Uint8Array = c.receive) =>
  h.fetch(PUSH_PATHS.subscription, { method: "PUT", headers: { "content-type": "application/json", ...bearer(cap), ...headers }, body: body(ep) });
const unreg = (h: Home, c: Caps, ep: string) => h.fetch(PUSH_PATHS.subscription, { method: "DELETE", headers: { "content-type": "application/json", ...bearer(c.receive) }, body: body(ep) });
const deposit = async (h: Home, c: Caps): Promise<void> => { expect((await relay.deposit(h, c.deposit, sealed())).ok).toBe(true); };
const settle = (ms = 400) => new Promise((r) => setTimeout(r, ms));
const to = (ep: string): Seen[] => seen.filter((s) => s.path === new URL(ep).pathname + new URL(ep).search);

describe("PC-P01 offered only with owner admission and not switched off", () => {
  it("is advertised with its limits where admission is configured", async () => {
    const doc = (await (await home.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[]; limits: Record<string, number> };
    expect(doc.features).toContain(FEATURES.relayPush);
    expect(doc.limits["push_subscriptions_per_queue"]).toBe(4);
    expect(doc.limits["push_min_interval_seconds"]).toBe(30);
  });
  it("is absent and 404 on an open Home and where OM_PUSH=0", async () => {
    for (const h of [open, offBy]) {
      const doc = (await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] };
      expect(doc.features).not.toContain(FEATURES.relayPush);
      expect((await h.fetch(PUSH_PATHS.key)).status).toBe(404);
    }
  });
});

describe("PC-P02 the Home's own key", () => {
  it("keeps one key across concurrent first use and restart", async () => {
    const state = scratch();
    const vars = { OM_ADMISSION: await admissionHash(secret), ...hosts() };
    let h = await startHome({ persist: state.dir, vars });
    const publicKey = async () => {
      const response = await h.fetch(PUSH_PATHS.key);
      expect(response.status).toBe(200);
      return (await response.json() as { key: string }).key;
    };
    try {
      const c = await queue(h);
      const ep = endpoint();
      const [registration, ...keys] = await Promise.all([
        reg(h, c, ep), ...Array.from({ length: 16 }, publicKey),
      ]);
      expect(registration.status).toBe(200);
      expect(new Set(keys).size).toBe(1);
      await h.dispose();
      h = await startHome({ persist: state.dir, vars });
      expect(await publicKey()).toBe(keys[0]);
      await deposit(h, c);
      await settle();
      expect(to(ep)[0]?.headers.authorization).toContain(`k=${String(keys[0])}`);
    } finally {
      await h.dispose();
      state.remove();
    }
  }, 120_000);

  it("serves one stable P-256 public key and holds the private half itself", async () => {
    const a = await (await home.fetch(PUSH_PATHS.key)).json() as { key: string };
    const b = await (await home.fetch(PUSH_PATHS.key)).json() as { key: string };
    expect(fromBase64Url(a.key)).toHaveLength(65);
    expect(fromBase64Url(a.key)[0]).toBe(4);
    expect(a.key).toBe(b.key);
    expect(JSON.stringify(a)).toBe(`{"key":"${a.key}"}`); // nothing else: no private part
    const other = await (await paced.fetch(PUSH_PATHS.key)).json() as { key: string };
    expect(other.key).not.toBe(a.key); // each Home makes its own
  });
});

describe("PC-P03 registering", () => {
  it("needs the receive capability and the owner's admission, and refuses wrong ones identically", async () => {
    const c = await queue(home);
    const other = await queue(home);
    const ep = endpoint();
    const bad = [await reg(home, c, ep, {}), await reg(home, c, ep, { [ADMISSION_HEADER]: toBase64Url(rand(32)) }), await reg(home, c, ep, adm, c.deposit), await reg(home, c, ep, adm, c.admin), await reg(home, c, ep, adm, rand(32)), await reg(home, c, ep, adm, other.deposit)];
    const texts = await Promise.all(bad.map(async (r) => `${String(r.status)} ${await r.text()}`));
    expect(new Set(texts).size).toBe(1);
    expect(bad[0]?.status).toBe(401);
    const good = await reg(home, c, ep);
    expect(good.status).toBe(200);
    const j = await good.json() as { id: string; expires_at: number };
    expect(j.id).toMatch(/^[0-9a-f]{32}$/);
    expect(j.expires_at).toBeGreaterThan(Date.now() / 1000 + 29 * 24 * 3600);
  });
  it("refuses an address the Home would not send to, and a malformed body", async () => {
    const c = await queue(home);
    for (const ep of ["https://example.com/x", "http://127.0.0.1:1/x", "https://169.254.169.254/", "nonsense", "https://user:pw@fcm.googleapis.com/x"]) expect((await reg(home, c, ep)).status, ep).toBe(400);
    const r = await home.fetch(PUSH_PATHS.subscription, { method: "PUT", headers: { "content-type": "application/json", ...bearer(c.receive), ...adm }, body: '{"endpoint":"x","extra":1}' });
    expect(r.status).toBe(400);
  });
  it("is idempotent for one address and bounded per mailbox", async () => {
    const c = await queue(home);
    const eps = [endpoint(), endpoint(), endpoint(), endpoint()];
    for (const ep of eps) expect((await reg(home, c, ep)).status).toBe(200);
    expect((await reg(home, c, eps[0] as string)).status).toBe(200); // the same one again: a refresh, not a fifth
    expect((await reg(home, c, endpoint())).status).toBe(429);
  });
  it("is removed by the receive capability alone, with no admission", async () => {
    const c = await queue(home);
    const ep = endpoint();
    await reg(home, c, ep);
    expect((await unreg(home, c, ep)).status).toBe(204);
    const before = to(ep).length;
    await deposit(home, c);
    await settle();
    expect(to(ep).length).toBe(before);
    const wrong = await home.fetch(PUSH_PATHS.subscription, { method: "DELETE", headers: { "content-type": "application/json", ...bearer(rand(32)) }, body: body(ep) });
    expect(wrong.status).toBe(401);
  });
});

describe("PC-P04 a wake is an empty signed push", () => {
  it("sends one empty request with a valid VAPID signature, a TTL and nothing about the message", async () => {
    const c = await queue(home);
    const ep = endpoint();
    await reg(home, c, ep);
    await deposit(home, c);
    await settle();
    const got = to(ep);
    expect(got).toHaveLength(1);
    const r = got[0] as Seen;
    expect(r.method).toBe("POST");
    expect(r.bytes).toBe(0);
    expect(Number(r.headers["ttl"])).toBeGreaterThan(0);
    expect(r.headers["urgency"]).toBe("normal");
    expect(Object.keys(r.headers).sort()).not.toContain("content-encoding");
    const m = /^vapid t=([\w-]+)\.([\w-]+)\.([\w-]+), k=([\w-]+)$/.exec(String(r.headers["authorization"]));
    expect(m).not.toBeNull();
    const [, h, p, s, k] = m as RegExpExecArray;
    const key = (await (await home.fetch(PUSH_PATHS.key)).json() as { key: string }).key;
    expect(k).toBe(key);
    const pub = await crypto.subtle.importKey("raw", fromBase64Url(k as string), { name: "ECDSA", namedCurve: "P-256" }, false, ["verify"]);
    const sig = fromBase64Url(s as string);
    expect(sig).toHaveLength(64);
    expect(await crypto.subtle.verify({ name: "ECDSA", hash: "SHA-256" }, pub, sig, new TextEncoder().encode(`${String(h)}.${String(p)}`))).toBe(true);
    const claims = JSON.parse(new TextDecoder().decode(fromBase64Url(p as string))) as { aud: string; exp: number; sub: string };
    expect(claims.aud).toBe(origin);
    expect(claims.exp).toBeGreaterThan(Date.now() / 1000);
    expect(claims.exp).toBeLessThanOrEqual(Date.now() / 1000 + 12 * 3600);
    expect(claims.sub).toMatch(/^https?:\/\/127\.0\.0\.1:\d+$/); // the Home's own address: no operator identity
    expect(JSON.stringify(r.headers)).not.toContain(toBase64Url(c.receive));
  });
  it("is not sent while a client is connected on the wake channel", async () => {
    const c = await queue(home);
    const ep = endpoint();
    await reg(home, c, ep);
    const ws = new WebSocket(home.url.replace("http", "ws") + "/v1/relay/wake", [WAKE_SUBPROTOCOL, wakeLabel(c.prefix)]);
    ws.binaryType = "arraybuffer";
    await new Promise<void>((res, rej) => { ws.addEventListener("open", () => { res(); }); ws.addEventListener("error", () => { rej(new Error("ws")); }); });
    const ready = new Promise<void>((res) => { ws.addEventListener("message", () => { res(); }, { once: true }); });
    ws.send(encodeWakeAuth({ cap: c.receive }));
    await ready;
    await deposit(home, c);
    await settle();
    expect(to(ep)).toHaveLength(0);
    ws.close();
  });
  it("is not sent for a deposit that is an exact retry", async () => {
    const c = await queue(home);
    const ep = endpoint();
    await reg(home, c, ep);
    const e = sealed();
    const id = rand(16);
    expect((await relay.deposit(home, c.deposit, e, id)).ok).toBe(true);
    expect((await relay.deposit(home, c.deposit, e, id)).ok).toBe(true);
    await settle();
    expect(to(ep)).toHaveLength(1);
  });
});

describe("PC-P05 quota-aware and polite", () => {
  it("covers arrivals inside the minimum interval with the first push", async () => {
    const c = await queue(home);
    const ep = endpoint();
    await reg(home, c, ep);
    await deposit(home, c); await deposit(home, c); await deposit(home, c);
    await settle();
    expect(to(ep)).toHaveLength(1);
  });
  it("sends again after the interval", async () => {
    const c = await queue(fast);
    const ep = endpoint();
    await reg(fast, c, ep);
    await deposit(fast, c);
    await settle(1300);
    await deposit(fast, c);
    await settle();
    expect(to(ep)).toHaveLength(2);
  });
  it("stops at the daily cap", async () => {
    const c = await queue(paced);
    const ep = endpoint();
    await reg(paced, c, ep);
    for (let i = 0; i < 4; i++) { await deposit(paced, c); await settle(150); }
    expect(to(ep)).toHaveLength(2);
  });
});

describe("PC-P06 graceful absence and cleanup", () => {
  it("drops a registration the push service reports gone", async () => {
    const c = await queue(paced);
    const ep = endpoint();
    await reg(paced, c, ep);
    status = 410;
    await deposit(paced, c);
    await settle();
    status = 201;
    expect(to(ep)).toHaveLength(1);
    await deposit(paced, c);
    await settle();
    expect(to(ep)).toHaveLength(1); // forgotten: not tried again
  });
  it("never fails or delays a deposit because the push service is unreachable", async () => {
    const c = await queue(fast);
    const dead = "http://127.0.0.1:9/x"; // not listed: refused at registration
    expect((await reg(fast, c, dead)).status).toBe(400);
    const stop = createServer(); await new Promise<void>((r) => stop.listen(0, "127.0.0.1", r));
    const port = (stop.address() as { port: number }).port; await new Promise((r) => stop.close(r));
    const h = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret), OM_PUSH_HOSTS: `http://127.0.0.1:${String(port)}` } });
    try {
      const q = await queue(h);
      expect((await reg(h, q, `http://127.0.0.1:${String(port)}/x`)).status).toBe(200);
      const t0 = Date.now();
      await deposit(h, q);
      expect(Date.now() - t0).toBeLessThan(3000);
      const items = await relay.drain(h, q.receive);
      expect(items.ok && items.value.items.length).toBe(1);
    } finally { await h.dispose(); }
  });
  it("a Home that never registered anything makes no push call", async () => {
    const c = await queue(home);
    const before = seen.length;
    await deposit(home, c);
    await settle();
    expect(seen.length).toBe(before);
  });
});
