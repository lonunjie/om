// SPDX-License-Identifier: AGPL-3.0-or-later
// Capacity of one person's (or one small group's) relay on a free-plan-sized request budget. One target per run, chosen by
// OM_PROVIDER_TEST_TARGET (worker | sqlite); OM_LOAD_SCALE = quick | full; OM_LOAD_OUT = where the JSON result is written.
//   1. unit cost: what each operation adds to the Home's own usage meter (Worker requests and Durable Object calls)
//   2. idle accounting with the wake channel: the real client wake hub on N mailboxes against the same mailboxes polled on the timer
//   3. N simulated users on one relay: compressed day of messages delivered through wake sockets; the Home's meter is reconciled with
//      the client's own count, every accepted deposit must arrive exactly once, and the free-plan budget is read off the measured cost
//   4. the same users driven into the usage ceiling: the hard state sheds deposits, keeps serving reads, and nothing accepted is lost
// The harness measures the Home's logical units. They are not Cloudflare's bill: the SQLite Home has no bill, and Cloudflare's
// current free-plan numbers must be checked on the day (see USAGE_CONFIG in infra/cloudflare-home/src/limits.ts).
import { writeFileSync } from "node:fs";
import {
  WAKE_SUBPROTOCOL, decodeWakeEvent, encodeWakeAuth, parseUsageState, quotaDetail, toHex, USAGE_PATH, wakeLabel,
} from "@om/home-wire";
import { afterAll, describe, expect, it } from "vitest";
import { createWakeHub } from "../../packages/om-provider-http/src/wake";
import { bearer, drainAll, newCaps, relay, sealed, type Caps } from "../../infra/cloudflare-home/test/kit";
import { Recorder, launch, mb, sha, sleep, type LoadHome, type Target } from "./lib";

const target = (process.env["OM_PROVIDER_TEST_TARGET"] === "sqlite" ? "sqlite" : "worker") as Target;
const scale = process.env["OM_LOAD_SCALE"] === "full" ? "full" : "quick";
// The user model. Everything the capacity figure depends on is here, so a reader can change it and read off a new figure.
const MODEL = {
  /** Messages received per user per day. Used only to turn the measured cost of one message into a cost per day. */
  receivedPerDay: 100,
  /** Offered load of the soak, per user and in total: far above a real user (0.2 messages/s is 17,000 a day), capped so a Home on a slow disk is not asked for more than it commits. */
  soakMessagesPerUserSecond: 0.2,
  soakMessagesPerSecondCap: 3,
  /** Distinct senders per mailbox (one minted deposit capability each); the personal profile allows 16 per queue including its own. */
  sendersPerUser: 8,
  /** Hours per day the app is on screen with a live wake socket, per mailbox. */
  visibleHours: 3,
  /** Hours per day a tab stays open but hidden (polling every 60 s, no socket), per mailbox. */
  hiddenHours: 8,
  /** Mailboxes the client holds at this relay per user. The figures below are per mailbox. */
  mailboxesPerUser: 1,
};
const P = scale === "full"
  ? { users: [1, 5, 25, 100], windowMs: 180_000, idleMailboxes: 10, idleMs: 7 * 60_000, quotaUsers: 10, quotaCeiling: 6000 }
  : { users: [1, 5, 25], windowMs: 20_000, idleMailboxes: 5, idleMs: 70_000, quotaUsers: 5, quotaCeiling: 1500 };
// Timers of the idle run are the client's own defaults except the safety interval of the quick scale, so a quick run still sees a safety drain.
const SAFETY_MS = scale === "full" ? 5 * 60_000 : 20_000;
const POLL_ACTIVE_MS = 3000; // apps/web/src/core/pollPace.ts
const POLL_HIDDEN_MS = 60_000;

const report: Record<string, unknown> = { target, scale, node: process.version, platform: `${process.platform}-${process.arch}`, startedAt: new Date().toISOString(), model: MODEL, params: P };
const violations: string[] = [];
const violate = (scenario: string, what: string): void => { violations.push(`${scenario}: ${what}`); };
afterAll(() => {
  report["violations"] = violations;
  const out = process.env["OM_LOAD_OUT"];
  if (out) writeFileSync(out, JSON.stringify(report, null, 2));
});

// ---- helpers ---------------------------------------------------------------------------------------------------------------

interface Counted { home: LoadHome["home"]; tally: () => number; bump: (n?: number) => void }

/** The Home as the client sees it, counting every HTTP request sent (a socket upgrade is counted by `bump`). */
function counted(h: LoadHome): Counted {
  let n = 0;
  return { home: { ...h.home, fetch: (path, init) => { n++; return h.home.fetch(path, init); } }, tally: () => n, bump: (k = 1) => { n += k; } };
}

interface Meter { worker: number; durable: number; state: string; workerCeiling: number; durableCeiling: number }
async function readMeter(c: Counted, admin: Uint8Array): Promise<Meter> {
  const u = parseUsageState(await (await c.home.fetch(USAGE_PATH, { headers: bearer(admin) })).json());
  const m = (name: string): { used: number; ceiling: number } => {
    const x = u.metrics.find((e) => e.name === name);
    return { used: x?.used ?? 0, ceiling: x?.ceiling ?? 0 };
  };
  return { worker: m("worker_requests").used, durable: m("durable_object_requests").used, state: u.state, workerCeiling: m("worker_requests").ceiling, durableCeiling: m("durable_object_requests").ceiling };
}

interface Mailbox { readonly caps: Caps; readonly name: string; senders: Uint8Array[] }
async function makeMailbox(c: Counted, senders: number): Promise<Mailbox> {
  const caps = newCaps();
  const r = await relay.create(c.home, caps);
  if (!r.ok) throw new Error(`queue create failed: ${r.error.name}`);
  const box: Mailbox = { caps, name: toHex(caps.prefix), senders: [caps.deposit] };
  for (let i = 1; i < senders; i++) {
    const m = await relay.mint(c.home, caps.admin, { ttlSeconds: 3600 });
    if (!m.ok) throw new Error(`mint failed: ${m.error.name}`);
    box.senders.push(m.value.cap);
  }
  return box;
}

interface Live { ws: WebSocket; ready: Promise<void>; opened: boolean }
/** One wake socket as the client opens it: bare path, two subprotocols, receive capability as the first frame. */
function openWake(h: LoadHome, c: Counted, box: Mailbox, onWake: () => void): Live {
  const ws = new WebSocket(h.url.replace(/^http/, "ws") + "/v1/relay/wake", [WAKE_SUBPROTOCOL, wakeLabel(box.caps.prefix)]);
  ws.binaryType = "arraybuffer";
  c.bump(); // one Worker request for the upgrade
  const live: Live = { ws, opened: false, ready: Promise.resolve() };
  live.ready = new Promise<void>((resolve, reject) => {
    ws.addEventListener("open", () => { ws.send(encodeWakeAuth({ cap: box.caps.receive })); });
    ws.addEventListener("error", () => { reject(new Error("socket error")); });
    ws.addEventListener("message", (e) => {
      if (typeof e.data === "string") return;
      const ev = decodeWakeEvent(new Uint8Array(e.data as ArrayBuffer));
      if (ev.kind === "ready") { live.opened = true; resolve(); } else onWake();
    });
  });
  live.ready.catch(() => undefined);
  return live;
}

// ---- 1. unit cost ----------------------------------------------------------------------------------------------------------

describe(`${target} relay capacity`, () => {
  it("unit cost: what each operation adds to the usage meter", async () => {
    const h = await launch(target, { OM_PROFILE: "personal" });
    const c = counted(h);
    const out: Record<string, { worker: number; durable: number }> = {};
    try {
      const box = await makeMailbox(c, 1);
      const probe = await makeMailbox(c, 1); // its admin reads the meter
      const step = async (name: string, action: () => Promise<unknown>): Promise<void> => {
        const a = await readMeter(c, probe.caps.admin);
        await action();
        const b = await readMeter(c, probe.caps.admin);
        // the second read is itself counted: subtract the cost of one read, measured between two reads
        out[name] = { worker: b.worker - a.worker, durable: b.durable - a.durable };
      };
      await step("read", async () => undefined);
      const readCost = { ...(out["read"] as { worker: number; durable: number }) };
      const sub = (name: string): void => { const e = out[name] as { worker: number; durable: number }; e.worker -= readCost.worker; e.durable -= readCost.durable; };
      await step("deposit", async () => { await relay.deposit(c.home, box.caps.deposit, sealed()); }); sub("deposit");
      await step("drainWithItem", async () => { await relay.drain(c.home, box.caps.receive, 0); }); sub("drainWithItem");
      const items = await drainAll(h.home, box.caps.receive);
      await step("ack", async () => { await relay.ack(c.home, box.caps.receive, items.map((i) => i.id)); }); sub("ack");
      await step("drainEmpty", async () => { await relay.drain(c.home, box.caps.receive, 0); }); sub("drainEmpty");
      await step("mint", async () => { await relay.mint(c.home, box.caps.admin, { ttlSeconds: 3600 }); }); sub("mint");
      await step("queueCreate", async () => { await relay.create(c.home, newCaps()); }); sub("queueCreate");
      await step("healthz", async () => { await c.home.fetch("/healthz"); }); sub("healthz");
      // a wake socket: the upgrade costs what a request costs; pings and wake frames after it are not counted
      let wakes = 0;
      const wake = openWake(h, c, box, () => { wakes++; });
      await step("wakeUpgrade", async () => { await wake.ready; }); // the upgrade was counted by the Home before ready
      sub("wakeUpgrade");
      await step("pingsAndWakeFrame", async () => {
        wake.ws.send("ping");
        wake.ws.send("ping");
        await relay.deposit(h.home, box.caps.deposit, sealed()); // not through the counter: its own cost is the deposit's, subtracted below
        await sleep(300);
      });
      const e = out["pingsAndWakeFrame"] as { worker: number; durable: number };
      e.worker -= readCost.worker + (out["deposit"] as { worker: number }).worker; e.durable -= readCost.durable + (out["deposit"] as { durable: number }).durable;
      wake.ws.close();
      out["_readCost"] = readCost;
      report["unitCost"] = out;
      expect(wakes).toBeGreaterThan(0);
      if (e.worker !== 0 || e.durable !== 0) violate("unit", `pings or a wake frame were metered (${String(e.worker)} Worker, ${String(e.durable)} Durable Object)`);
    } finally { await h.stop(); h.remove(); }
  });

  // ---- 2. idle accounting ---------------------------------------------------------------------------------------------------

  it("idle: the wake hub against timer polling on the same mailboxes", async () => {
    const h = await launch(target, { OM_PROFILE: "personal" });
    const c = counted(h);
    const out: Record<string, unknown> = {};
    try {
      const probe = await makeMailbox(c, 1);
      const boxes = await Promise.all(Array.from({ length: P.idleMailboxes }, () => makeMailbox(c, 1)));
      const meterAt = async (): Promise<Meter> => readMeter(c, probe.caps.admin);

      // (a) timer polling, one drain per mailbox every 3 s while visible, for 30 s
      const pollMs = 30_000;
      const m0 = await meterAt();
      const t0 = performance.now();
      await Promise.all(boxes.map(async (b) => { for (let i = 0; i * POLL_ACTIVE_MS < pollMs; i++) { await relay.drain(c.home, b.caps.receive, 0); await sleep(POLL_ACTIVE_MS); } }));
      const m1 = await meterAt();
      out["timerPolling"] = { mailboxes: boxes.length, seconds: Math.round((performance.now() - t0) / 1000), workerRequests: m1.worker - m0.worker - 1, durableCalls: m1.durable - m0.durable - 2, perMailboxPerHour: { workerRequests: Math.round(((m1.worker - m0.worker - 1) / boxes.length) * (3600_000 / pollMs)) } };

      // (b) the real wake hub, ticked at the visible poll pace like the runtime; quiet polls cost nothing
      const held = new Map<string, number>(boxes.map((b) => [b.name, 0]));
      const hubSockets: WebSocket[] = [];
      const hub = createWakeHub({
        base: h.url, supported: () => Promise.resolve(true), safetyMs: SAFETY_MS,
        webSocket: (url, protocols) => { const ws = new WebSocket(url, protocols); c.bump(); hubSockets.push(ws); return ws; },
        credentials: (mailboxHex) => { const b = boxes.find((x) => x.name === mailboxHex.slice(0, 32)) ?? boxes[0] as Mailbox; return Promise.resolve({ queueId: b.caps.prefix, receive: b.caps.receive }); },
      });
      hub.setActive(true);
      let polls = 0, quiet = 0;
      const stop = { v: false };
      const ticks = boxes.map(async (b) => {
        while (!stop.v) {
          const after = held.get(b.name) ?? 0;
          hub.watch(b.name, after);
          if (hub.quiet(b.name, after)) quiet++;
          else {
            polls++;
            const r = await relay.drain(c.home, b.caps.receive, after);
            if (r.ok) hub.drained(b.name, r.value.items.at(-1)?.seq ?? after);
          }
          await sleep(POLL_ACTIVE_MS);
        }
      });
      const w0 = await meterAt();
      const tw = performance.now();
      await sleep(P.idleMs);
      const w1 = await meterAt();
      const seconds = (performance.now() - tw) / 1000;
      stop.v = true; await Promise.all(ticks);
      const stats = hub.stats();
      hub.dispose();
      const worker = w1.worker - w0.worker - 1; // the closing read
      out["wakeHub"] = {
        mailboxes: boxes.length, seconds: Math.round(seconds), safetyMs: SAFETY_MS, timerTicks: polls + quiet, quietTicks: quiet, drainsSent: polls,
        workerRequests: worker, durableCalls: w1.durable - w0.durable - 2, socketsLive: stats.live, connects: stats.connects,
        perMailboxPerHour: { workerRequests: Math.round((worker / boxes.length) * (3600 / seconds) * 10) / 10 },
      };
      // what the wake hub must cost: the opening drain and the socket upgrade per mailbox, then one safety drain per interval
      const expectedSafety = Math.floor(P.idleMs / SAFETY_MS);
      const budget = boxes.length * (2 + expectedSafety + 1);
      if (worker > budget) violate("idle", `wake hub used ${String(worker)} Worker requests for ${String(boxes.length)} idle mailboxes in ${String(Math.round(seconds))} s (expected at most ${String(budget)})`);
      if (stats.live !== boxes.length) violate("idle", `${String(stats.live)} of ${String(boxes.length)} sockets were live`);
    } finally { report["idle"] = out; await h.stop(); h.remove(); }
  });

  // ---- 3. N users on one relay -------------------------------------------------------------------------------------------------

  for (const users of P.users) {
    it(`soak: ${String(users)} simulated user(s) with a wake socket each, steady message load`, async () => {
      const h = await launch(target, { OM_PROFILE: "personal" });
      const c = counted(h);
      const out: Record<string, unknown> = { users };
      const rec = new Recorder();
      try {
        const probe = await makeMailbox(c, 1);
        const boxes: Mailbox[] = [];
        for (let i = 0; i < users; i++) boxes.push(await makeMailbox(c, Math.min(users, MODEL.sendersPerUser + 1)));
        const accepted = new Map<string, Set<string>>(boxes.map((b) => [b.name, new Set()]));
        const received = new Map<string, string[]>(boxes.map((b) => [b.name, []]));
        const refused: Record<string, number> = {};
        const draining = new Set<string>();
        const again = new Set<string>();
        const collect = async (b: Mailbox): Promise<void> => {
          if (draining.has(b.name)) { again.add(b.name); return; }
          draining.add(b.name);
          try {
            do {
              again.delete(b.name);
              const r = await rec.time("drain", () => relay.drain(c.home, b.caps.receive, 0));
              if (!r.ok || r.value.items.length === 0) continue;
              const keys = r.value.items.map((i) => sha(i.bytes));
              (received.get(b.name) as string[]).push(...keys);
              await rec.time("ack", () => relay.ack(c.home, b.caps.receive, r.value.items.map((i) => i.id)));
            } while (again.has(b.name));
          } finally { draining.delete(b.name); }
        };
        const sockets = boxes.map((b) => openWake(h, c, b, () => { void collect(b); }));
        await Promise.all(sockets.map((s) => s.ready));
        const m0 = await readMeter(c, probe.caps.admin);
        const base = c.tally();

        // messages arrive for random users from random senders at random moments of the window, at a steady offered rate
        const rate = Math.min(users * MODEL.soakMessagesPerUserSecond, MODEL.soakMessagesPerSecondCap);
        const total = Math.max(2, Math.round(rate * (P.windowMs / 1000)));
        const t0 = performance.now();
        const sends: Promise<void>[] = [];
        for (let i = 0; i < total; i++) {
          const b = boxes[i % users] as Mailbox;
          const cap = b.senders[Math.floor(Math.random() * b.senders.length)] as Uint8Array;
          const at = Math.random() * P.windowMs;
          sends.push((async () => {
            await sleep(at);
            const env = sealed(); const k = sha(env);
            const r = await rec.time("deposit", () => relay.deposit(c.home, cap, env));
            if (r.ok) (accepted.get(b.name) as Set<string>).add(k); else refused[r.error.name] = (refused[r.error.name] ?? 0) + 1;
          })());
        }
        await Promise.all(sends);
        await sleep(1000);
        const elapsed = (performance.now() - t0) / 1000;
        const m1 = await readMeter(c, probe.caps.admin);
        const clientCount = c.tally() - base;
        sockets.forEach((s) => { s.ws.close(); });
        // every accepted deposit arrives exactly once; after one last quiet drain nothing is left behind
        let lost = 0, dupes = 0, unknown = 0, left = 0;
        for (const b of boxes) {
          await collect(b);
          const got = received.get(b.name) as string[];
          const want = accepted.get(b.name) as Set<string>;
          if (new Set(got).size !== got.length) dupes++;
          for (const k of want) if (!got.includes(k)) lost++;
          for (const k of got) if (!want.has(k)) unknown++;
          left += (await drainAll(h.home, b.caps.receive)).length;
        }
        const msgs = [...accepted.values()].reduce((s, x) => s + x.size, 0);
        const worker = m1.worker - m0.worker;
        const durable = m1.durable - m0.durable;
        // the cost of one delivered message (deposit, drain, ack) times the messages one user receives a day
        const perMsg = { workerRequests: worker / Math.max(1, msgs), durableCalls: durable / Math.max(1, msgs) };
        const perUserDay = { workerRequests: perMsg.workerRequests * MODEL.receivedPerDay, durableCalls: perMsg.durableCalls * MODEL.receivedPerDay };
        out["day"] = {
          seconds: Math.round(elapsed), offeredPerSecond: rate, messagesAccepted: msgs, refused, workerRequests: worker, durableCalls: durable, clientCountedRequests: clientCount,
          perMessage: { workerRequests: Math.round(perMsg.workerRequests * 100) / 100, durableCalls: Math.round(perMsg.durableCalls * 100) / 100 },
          perUserDayAtModel: { workerRequests: Math.round(perUserDay.workerRequests), durableCalls: Math.round(perUserDay.durableCalls) },
          latencyMs: rec.summary(), state: m1.state, ceilings: { worker: m1.workerCeiling, durable: m1.durableCeiling },
          lost, duplicates: dupes, unknown, leftAfterFinalDrain: left, homeRssMb: mb(h.rss()), diskMb: mb(h.diskBytes()),
        };
        // messages a day that fit under the soft state (80% of the lower ceiling), by the measured cost
        const maxWorker = (m1.workerCeiling * 0.8) / perUserDay.workerRequests;
        const maxDurable = (m1.durableCeiling * 0.8) / perUserDay.durableCalls;
        out["usersUnderSoft"] = { byWorkerRequests: Math.floor(maxWorker), byDurableCalls: Math.floor(maxDurable), binding: maxDurable < maxWorker ? "durable_object_calls" : "worker_requests" };
        // the Home's count is the client's count plus the meter's own reads (two, before and after) and the socket upgrades already in `clientCount`
        // between the two meter reads the Home counts every request the client sent, including the second read
        const unexplained = (m1.worker - m0.worker) - clientCount;
        out["reconciliation"] = { homeWorkerRequests: worker, clientRequests: clientCount, difference: unexplained };
        if (unexplained !== 0) violate(`soak-${String(users)}`, `the Home counted ${String(worker)} Worker requests in the window, the client sent ${String(clientCount)}`);
        if (lost > 0 || dupes > 0 || unknown > 0 || left > 0) violate(`soak-${String(users)}`, `delivery: ${String(lost)} lost, ${String(dupes)} duplicated, ${String(unknown)} unknown, ${String(left)} left behind`);
        const unexpectedRefusals = Object.keys(refused).filter((k) => k !== "rate_limited" && k !== "quota");
        if (unexpectedRefusals.length > 0) violate(`soak-${String(users)}`, `unexpected refusals ${unexpectedRefusals.join(",")}`);
        if (m1.state !== "healthy") violate(`soak-${String(users)}`, `usage state ${m1.state} after the window`);
        expect(msgs).toBeGreaterThan(0);
      } finally { report["soak"] ??= {}; (report["soak"] as Record<string, unknown>)[String(users)] = out; await h.stop(); h.remove(); }
    });
  }

  // ---- 4. at the quota -------------------------------------------------------------------------------------------------------

  it("quota: users driven past the ceiling; deposits shed, reads served, nothing accepted lost", async () => {
    const ceiling = P.quotaCeiling;
    const h = await launch(target, { OM_PROFILE: "personal", OM_USAGE_CEILING_WORKER: String(ceiling), OM_USAGE_CEILING_DO: String(ceiling * 4) });
    const c = counted(h);
    const out: Record<string, unknown> = { ceilingWorker: ceiling, users: P.quotaUsers };
    try {
      const probe = await makeMailbox(c, 1);
      const boxes = await Promise.all(Array.from({ length: P.quotaUsers }, () => makeMailbox(c, 1)));
      const accepted = new Map<string, Set<string>>(boxes.map((b) => [b.name, new Set()]));
      const got = new Map<string, string[]>(boxes.map((b) => [b.name, []]));
      const sheds: { atRequest: number; hard: boolean }[] = [];
      let firstShedAt: number | undefined, acceptedAfterShed = 0, round = 0, hardSeen = false, softSeenAt: number | undefined;
      const states: { atRequest: number; state: string }[] = [];
      let last = "";
      const quotaHard = toHex(quotaDetail(3, true));
      // every user receives one message, drains (as a wake would), acks; the probe reads the meter every few rounds
      while (!hardSeen && round < ceiling * 2) {
        round++;
        await Promise.all(boxes.map(async (b) => {
          const env = sealed(); const k = sha(env);
          const r = await relay.deposit(c.home, b.caps.deposit, env);
          if (r.ok) { (accepted.get(b.name) as Set<string>).add(k); if (firstShedAt !== undefined) acceptedAfterShed++; }
          else if (r.error.name === "quota") { firstShedAt ??= c.tally(); sheds.push({ atRequest: c.tally(), hard: r.error.detail !== undefined && toHex(r.error.detail) === quotaHard }); }
          const d = await relay.drain(c.home, b.caps.receive, 0);
          if (d.ok && d.value.items.length > 0) {
            (got.get(b.name) as string[]).push(...d.value.items.map((i) => sha(i.bytes)));
            await relay.ack(c.home, b.caps.receive, d.value.items.map((i) => i.id));
          }
        }));
        if (round % 5 === 0) {
          const m = await readMeter(c, probe.caps.admin);
          if (m.state !== last) { states.push({ atRequest: c.tally(), state: m.state }); last = m.state; if (m.state === "soft") softSeenAt ??= c.tally(); }
          if (m.state === "hard") hardSeen = true;
        }
      }
      out["states"] = states;
      out["requestsAtHard"] = c.tally();
      out["ratioAtHard"] = Math.round((c.tally() / ceiling) * 1000) / 1000;
      // at hard: new deposits are shed with the typed quota answer and the hard flag; drains, acks, reads and the health check still work
      const b0 = boxes[0] as Mailbox;
      const env = sealed();
      const dep = await relay.deposit(c.home, b0.caps.deposit, env);
      const dr = await relay.drain(c.home, b0.caps.receive, 0);
      const hz = await c.home.fetch("/healthz");
      const rd = await readMeter(c, probe.caps.admin);
      const created = await relay.create(c.home, newCaps());
      out["atHard"] = { depositAccepted: dep.ok, depositError: dep.ok ? undefined : dep.error.name, drainServed: dr.ok, healthz: hz.status, usageRead: rd.state, queueCreateAccepted: created.ok, depositsShed: sheds.length, shedWithHardFlag: sheds.filter((s) => s.hard).length };
      // a wake socket opened at hard still authenticates (the upgrade is a read of the queue) and a drain still works
      const wake = openWake(h, c, b0, () => undefined);
      const wakeOk = await Promise.race([wake.ready.then(() => true, () => false), sleep(3000).then(() => false)]);
      wake.ws.close();
      (out["atHard"] as Record<string, unknown>)["wakeSocketAtHard"] = wakeOk;
      // nothing the Home accepted is missing: drain every mailbox once more and compare
      let lost = 0, dupes = 0, unknown = 0;
      for (const b of boxes) {
        const rest = await drainAll(h.home, b.caps.receive);
        const all = [...(got.get(b.name) as string[]), ...rest.map((i) => sha(i.bytes))];
        if (new Set(all).size !== all.length) dupes++;
        const want = accepted.get(b.name) as Set<string>;
        for (const k of want) if (!all.includes(k)) lost++;
        for (const k of all) if (!want.has(k)) unknown++;
      }
      out["delivery"] = { accepted: [...accepted.values()].reduce((s, x) => s + x.size, 0), lost, duplicates: dupes, unknown, acceptedAfterFirstShed: acceptedAfterShed };
      if (!hardSeen) violate("quota", "the ceiling was never reached");
      if (dep.ok) violate("quota", "a deposit was accepted at the hard state");
      if (!dr.ok || hz.status !== 200 || rd.state !== "hard") violate("quota", "a request that must stay served was shed at the hard state");
      if (created.ok) violate("quota", "a queue was created at the hard state");
      if (sheds.some((s) => !s.hard)) violate("quota", "a shed deposit did not carry the hard flag");
      if (lost > 0 || dupes > 0 || unknown > 0) violate("quota", `delivery: ${String(lost)} lost, ${String(dupes)} duplicated, ${String(unknown)} unknown`);
      if (softSeenAt === undefined) violate("quota", "no soft state was reported before hard");
    } finally { report["quota"] = out; await h.stop(); h.remove(); }
  });

  it("no violations", () => { expect(violations).toEqual([]); });
});
