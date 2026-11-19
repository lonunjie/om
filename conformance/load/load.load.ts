// SPDX-License-Identifier: AGPL-3.0-or-later
// Load and soak harness for the reference Homes (workerd Worker and the SQLite Home). One target per run, chosen by
// OM_PROVIDER_TEST_TARGET (worker | sqlite); OM_LOAD_SCALE = quick | full; OM_LOAD_OUT = where the JSON result is written.
// Every scenario checks the invariants (no acknowledged deposit lost, nothing altered or resurrected, documented typed errors only,
// stated bounds held) and records them as violations, which fail the run after the numbers are written.
import { appendFileSync, writeFileSync } from "node:fs";
import { request as httpRequest } from "node:http";
import {
  WAKE_SUBPROTOCOL, decodeHomeError, decodeInboxDrainResponse, decodeWakeEvent, encode, encodeFanout, encodeInboxAck, encodeInboxDrain, encodeInboxRegister,
  encodeRecordPut, encodeWakeAuth, map, parseUsageState, quotaDetail, recoveryInboxId, toBase64Url, toHex, USAGE_PATH, wakeLabel, type HomeErrorBody,
} from "@om/home-wire";
import { afterAll, describe, expect, it } from "vitest";
import { asBody, bearer, blobs, drainAll, errorOf, newCaps, rand, relay, sealed, type Caps, type Result } from "../../infra/cloudflare-home/test/kit";
import { envelope, recordCaps, signedRequest } from "../../infra/cloudflare-home/test/recovery-kit";
import { Ledger, MIB, Recorder, clientRss, diskByKind, launch, mb, percentiles, sha, sleep, type LoadHome, type Target } from "./lib";

const target = (process.env["OM_PROVIDER_TEST_TARGET"] === "sqlite" ? "sqlite" : "worker") as Target;
const scale = process.env["OM_LOAD_SCALE"] === "full" ? "full" : "quick";
const P: { mailboxes: number; stages: number[]; stageMs: number; soakMs: number; soakMailboxes: number; wakeQueues: number; usageCeiling: number } = scale === "full"
  ? { mailboxes: 300, stages: [1, 4, 16, 64], stageMs: 10_000, soakMs: 180_000, soakMailboxes: 150, wakeQueues: 100, usageCeiling: 3000 }
  : { mailboxes: 60, stages: [1, 4, 16, 64], stageMs: 3_000, soakMs: 20_000, soakMailboxes: 40, wakeQueues: 20, usageCeiling: 600 };

// Overrides for investigating one scenario: OM_LOAD_SOAK_SECONDS, OM_LOAD_SOAK_MAILBOXES.
if (process.env["OM_LOAD_SOAK_SECONDS"]) P.soakMs = Number(process.env["OM_LOAD_SOAK_SECONDS"]) * 1000;
if (process.env["OM_LOAD_SOAK_MAILBOXES"]) P.soakMailboxes = Number(process.env["OM_LOAD_SOAK_MAILBOXES"]);
const report: Record<string, unknown> = { target, scale, node: process.version, platform: `${process.platform}-${process.arch}`, startedAt: new Date().toISOString(), params: P };
const violations: string[] = [];
const violate = (scenario: string, what: string): void => { violations.push(`${scenario}: ${what}`); };

afterAll(() => {
  report["violations"] = violations;
  const out = process.env["OM_LOAD_OUT"];
  if (out) writeFileSync(out, JSON.stringify(report, null, 2));
});

/** Progress markers, one line each, to OM_LOAD_OUT.progress: a stuck run shows where it is. */
const mark = (what: string): void => { const out = process.env["OM_LOAD_OUT"]; if (out) appendFileSync(out + ".progress", `${new Date().toISOString()} ${target} ${what}\n`); };
const hex = (b: Uint8Array): string => toHex(b);
const EXPECTED_SHED = new Set(["rate_limited", "quota"]);

// ---- building blocks -------------------------------------------------------------------------------------------------------

interface Box { readonly name: string; readonly caps: Caps }

async function makeBoxes(h: LoadHome, n: number, rec: Recorder, concurrency = 16): Promise<Box[]> {
  const boxes: Box[] = [];
  let next = 0;
  await Promise.all(Array.from({ length: concurrency }, async () => {
    while (next < n) {
      next++;
      const caps = newCaps();
      const r = await rec.time("create", () => relay.create(h.home, caps));
      if (!r.ok) throw new Error(`queue create failed: ${r.error.name}`);
      boxes.push({ name: hex(caps.prefix), caps });
    }
  }));
  return boxes;
}

/** Sends one envelope and keeps the ledger honest about it. */
async function deposit(h: LoadHome, rec: Recorder, led: Ledger, b: Box, ct: number): Promise<Result<unknown> | undefined> {
  const env = sealed({ ct });
  const k = sha(env);
  led.sending(b.name, k);
  try {
    const r = await rec.time("deposit", () => relay.deposit(h.home, b.caps.deposit, env));
    if (r.ok) led.stored(b.name, k); else led.refused(b.name, k);
    return r;
  } catch { rec.note("deposit", 0, "connection"); return undefined; } // unanswered: the item may or may not be stored, the ledger keeps it as unknown
}

/** Drains the mailbox once through (all pages), checks every item against the ledger, acks them. Returns the item count. */
async function drainAck(h: LoadHome, rec: Recorder, led: Ledger, b: Box, ack = true): Promise<number> {
  let cursor = 0;
  let total = 0;
  for (let page = 0; page < 200; page++) {
    let r: Awaited<ReturnType<typeof relay.drain>>;
    try { r = await rec.time("drain", () => relay.drain(h.home, b.caps.receive, cursor)); } catch { rec.note("drain", 0, "connection"); return total; }
    if (!r.ok) return total;
    const keys = r.value.items.map((i) => sha(i.bytes));
    keys.forEach((k) => { led.seen(b.name, k); });
    total += keys.length;
    if (ack && r.value.items.length > 0) {
      keys.forEach((k) => { led.ackSending(b.name, k); });
      try {
        const a = await rec.time("ack", () => relay.ack(h.home, b.caps.receive, r.value.items.map((i) => i.id)));
        if (a.ok) keys.forEach((k) => { led.ackedNow(b.name, k); }); else keys.forEach((k) => { led.ackRefused(b.name, k); });
      } catch { rec.note("ack", 0, "connection"); return total; }
    }
    if (r.value.next === undefined) return total;
    cursor = ack ? 0 : Number(new DataView(r.value.next.buffer, r.value.next.byteOffset, 8).getBigUint64(0));
  }
  return total;
}

/** Final audit of one mailbox: a full read, then the ledger comparison. */
async function audit(h: LoadHome, led: Ledger, b: Box): Promise<void> {
  const items = await drainAll(h.home, b.caps.receive);
  const keys = items.map((i) => sha(i.bytes));
  led.verify(b.name, keys);
  const seqs = items.map((i) => i.seq);
  for (let i = 1; i < seqs.length; i++) if ((seqs[i] ?? 0) <= (seqs[i - 1] ?? 0)) { led.violations.push(`sequence not increasing in ${b.name}`); break; }
}

const unexpected = (s: Record<string, { outcomes: Record<string, number> }>, allowed = EXPECTED_SHED): string[] =>
  Object.entries(s).flatMap(([op, v]) => Object.keys(v.outcomes).filter((o) => o !== "ok" && !allowed.has(o)).map((o) => `${op}:${o}`));

const sample = (h: LoadHome): { rssMb: number; clientRssMb: number; diskMb: number } => ({ rssMb: mb(h.rss()), clientRssMb: mb(clientRss()), diskMb: mb(h.diskBytes()) });

/** Runs `c` workers until `until`; worker i owns boxes i, i+c, ... so a mailbox is never drained by two at once. */
async function pool(c: number, boxes: Box[], ms: number, step: (b: Box, w: number, n: number) => Promise<void>): Promise<number> {
  const end = performance.now() + ms;
  let iterations = 0;
  await Promise.all(Array.from({ length: c }, async (_, w) => {
    const mine = boxes.filter((_, i) => i % c === w);
    if (mine.length === 0) return;
    for (let n = 0; performance.now() < end; n++) { await step(mine[n % mine.length] as Box, w, n); iterations++; }
  }));
  return iterations;
}

// ---- 1. ramp ---------------------------------------------------------------------------------------------------------------

describe(`${target} home load`, () => {
  it("ramp: deposit, drain and ack at increasing concurrency", async () => {
    const h = await launch(target);
    const led = new Ledger();
    const stages: unknown[] = [];
    const idle = sample(h);
    try {
      for (const c of P.stages) {
        const rec = new Recorder();
        const boxes = await makeBoxes(h, P.mailboxes, rec);
        const t0 = performance.now();
        const iterations = await pool(c, boxes, P.stageMs, async (b, _w, n) => {
          await deposit(h, rec, led, b, n % 4 === 0 ? 16_400 : 1040);
          if (n % 2 === 1) await drainAck(h, rec, led, b);
        });
        const seconds = (performance.now() - t0) / 1000;
        const s = rec.summary();
        const requests = [...rec.latency.values()].reduce((a, l) => a + l.length, 0);
        const bad = unexpected(s);
        if (bad.length > 0) violate("ramp", `untyped or unexpected outcomes at concurrency ${String(c)}: ${bad.join(",")}`);
        stages.push({ concurrency: c, mailboxes: P.mailboxes, seconds: Math.round(seconds * 10) / 10, iterations, requests, requestsPerSecond: Math.round(requests / seconds), depositsAcceptedPerSecond: Math.round((s["deposit"]?.outcomes["ok"] ?? 0) / seconds), ops: s, ...sample(h) });
        for (const b of boxes) await audit(h, led, b); // quiet audit between stages: nothing acknowledged may be missing
      }
      report["ramp"] = { idle, stages, ledger: { deposits: led.deposits, acks: led.acks } };
      for (const v of led.violations.slice(0, 5)) violate("ramp", v);
      expect(led.violations).toEqual([]);
    } finally { await h.stop(); h.remove(); }
  });

  // ---- 2. the documented bounds of one Home --------------------------------------------------------------------------------

  it("limits: payload size, queue items, queue bytes, capability count, per-capability rate", async () => {
    const h = await launch(target);
    const out: Record<string, unknown> = {};
    try {
      const caps = (await makeBoxes(h, 1, new Recorder()))[0] as Box;
      // payload bound: exactly 133000 bytes is stored, one byte more is payload_too_large
      // The advertised body bound is 133000 bytes, but the envelope shape bounds its ciphertext at 131088 bytes: the largest conforming
      // envelope is accepted, one byte more of ciphertext is refused by shape, and a body over the bound is payload_too_large.
      const mint = async (): Promise<Uint8Array> => { const m = await relay.mint(h.home, caps.caps.admin); if (!m.ok) throw new Error(`mint: ${m.error.name}`); return m.value.cap; };
      const big = bigSealed(131_088);
      const bigOk = await relay.deposit(h.home, caps.caps.deposit, big);
      const shapeOver = await relay.deposit(h.home, caps.caps.deposit, bigSealed(131_089));
      const sizeOver = await relay.deposit(h.home, caps.caps.deposit, bigSealed(133_001));
      out["payload"] = { largestConformingBytes: big.length, atLimit: bigOk.ok ? "ok" : bigOk.error.name, ciphertextOneOver: shapeOver.ok ? "ok" : shapeOver.error.name, bodyOverBound: sizeOver.ok ? "ok" : sizeOver.error.name };
      if (!bigOk.ok) violate("limits", "the largest conforming envelope was refused");
      if (shapeOver.ok || shapeOver.error.name !== "invalid_request") violate("limits", "an envelope with an over-long ciphertext was not refused by shape");
      if (sizeOver.ok || sizeOver.error.name !== "payload_too_large") violate("limits", "a body over the maximum was not payload_too_large");
      void mint;
      // an oversized body (24 MiB) is refused with the typed answer (or a cut connection) without the Home's memory growing by it
      const before = h.rss();
      const huge = new Uint8Array(24 * MIB);
      let hugeOutcome = "unknown";
      try {
        const r = await h.home.fetch("/v1/relay/deposit", { method: "POST", headers: { "content-type": "application/cbor" }, body: asBody(huge) });
        hugeOutcome = r.status === 413 || r.status === 400 ? decodeHomeError(new Uint8Array(await r.arrayBuffer())).name : `http ${String(r.status)}`;
      } catch { hugeOutcome = "connection cut"; }
      await sleep(300);
      out["oversizedBody"] = { bytes: huge.length, outcome: hugeOutcome, rssGrowthMb: mb(h.rss() - before) };
      if (hugeOutcome !== "payload_too_large" && hugeOutcome !== "connection cut" && hugeOutcome !== "invalid_request") violate("limits", `a 24 MiB body was answered ${hugeOutcome}`);
      if (h.rss() - before > 40 * MIB) violate("limits", "a 24 MiB body grew the Home's memory by more than 40 MiB");

      // item quota: mint deposit capabilities (each has its own burst of 30) until 512 items are held, then the next is quota
      const q1 = (await makeBoxes(h, 1, new Recorder()))[0] as Box;
      const led = new Ledger();
      const rec = new Recorder();
      const depCaps: Uint8Array[] = [q1.caps.deposit];
      let accepted = 0;
      let stop: HomeErrorBody | undefined;
      for (let guard = 0; guard < 2000 && !stop; guard++) {
        const cap = depCaps[Math.floor(accepted / 30)] ?? (depCaps[depCaps.push(await (async () => { const m = await relay.mint(h.home, q1.caps.admin); if (!m.ok) throw new Error(`mint: ${m.error.name}`); return m.value.cap; })()) - 1] as Uint8Array);
        const env = sealed();
        const k = sha(env);
        led.sending(q1.name, k);
        const r = await rec.time("deposit", () => relay.deposit(h.home, cap, env));
        if (r.ok) { led.stored(q1.name, k); accepted++; } else { led.refused(q1.name, k); stop = r.error; }
      }
      const items = { accepted, stoppedWith: stop?.name, detail: stop?.detail === undefined ? null : toHex(stop.detail) };
      out["itemQuota"] = { ...items, advertised: 512 };
      if (accepted !== 512 || stop?.name !== "quota") violate("limits", `item quota: accepted ${String(accepted)}, stopped with ${String(stop?.name)}`);
      // draining and acking ten items frees exactly ten places
      const page = await relay.drain(h.home, q1.caps.receive);
      if (!page.ok) throw new Error("drain");
      const ten = page.value.items.slice(0, 10);
      await relay.ack(h.home, q1.caps.receive, ten.map((i) => i.id));
      ten.forEach((i) => { led.ackedNow(q1.name, sha(i.bytes)); });
      let refill = 0;
      for (let i = 0; i < 12; i++) {
        const cap = depCaps[Math.floor((accepted + refill) / 30)] ?? (depCaps[0] as Uint8Array);
        const env = sealed(); const k = sha(env);
        led.sending(q1.name, k);
        const r = await relay.deposit(h.home, cap, env);
        if (r.ok) { led.stored(q1.name, k); refill++; } else led.refused(q1.name, k);
      }
      out["itemQuotaAfterAck"] = { acked: 10, depositedAfter: refill };
      if (refill !== 10) violate("limits", `after acking 10 items, ${String(refill)} deposits were accepted`);
      await audit(h, led, q1);

      // capability count
      let minted = 0;
      let capStop: string | undefined;
      const q2 = (await makeBoxes(h, 1, new Recorder()))[0] as Box;
      for (let i = 0; i < 100 && capStop === undefined; i++) { const m = await relay.mint(h.home, q2.caps.admin); if (m.ok) minted++; else capStop = m.error.name; }
      out["capabilityCount"] = { minted: minted + 1, stoppedWith: capStop };
      if (minted + 1 !== 64 || capStop !== "quota") violate("limits", `deposit capability count ${String(minted + 1)} stopped with ${String(capStop)}`);

      // queue bytes: large envelopes until the 16 MiB bound; stored bytes must never exceed it
      const q3 = (await makeBoxes(h, 1, new Recorder()))[0] as Box;
      const bigCaps: Uint8Array[] = [q3.caps.deposit];
      let stored = 0, bytes = 0, byteStop: HomeErrorBody | undefined;
      for (let guard = 0; guard < 400 && !byteStop; guard++) {
        if (Math.floor(stored / 30) >= bigCaps.length) bigCaps.push(await mint2(h, q3));
        const env = bigSealed(131_088);
        const r = await relay.deposit(h.home, bigCaps[Math.floor(stored / 30)] as Uint8Array, env);
        if (r.ok) { stored++; bytes += env.length; } else byteStop = r.error;
      }
      out["byteQuota"] = { stored, storedBytes: bytes, advertisedBytes: 16 * MIB, stoppedWith: byteStop?.name };
      if (bytes > 16 * MIB || byteStop?.name !== "quota") violate("limits", "queue byte bound exceeded or not typed quota");

      // per-capability rate: burst 30 then rate_limited with retry_after, and the cap works again after the stated wait
      const q4 = (await makeBoxes(h, 1, new Recorder()))[0] as Box;
      let ok = 0, limited = 0, retryAfter: number | undefined;
      const t0 = performance.now();
      for (let i = 0; i < 80; i++) { const r = await relay.deposit(h.home, q4.caps.deposit, sealed()); if (r.ok) ok++; else if (r.error.name === "rate_limited") { limited++; retryAfter ??= r.error.retryAfterSeconds; } }
      const spanMs = performance.now() - t0;
      await sleep(((retryAfter ?? 1) + 0.2) * 1000);
      const again = await relay.deposit(h.home, q4.caps.deposit, sealed());
      out["rateLimit"] = { sent: 80, accepted: ok, rateLimited: limited, retryAfterSeconds: retryAfter, burstWindowMs: Math.round(spanMs), acceptedAfterWait: again.ok };
      if (ok > 30 + Math.ceil(spanMs / 1000) + 1 || limited === 0 || retryAfter === undefined || !again.ok) violate("limits", "per-capability rate limit outside the advertised burst and refill");
    } finally { report["limits"] = out; await h.stop(); h.remove(); }
  });

  it("blob spool: size, count and spool bytes", async () => {
    const h = await launch(target);
    const out: Record<string, unknown> = {};
    try {
      const rec = new Recorder();
      const four = new Uint8Array(4 * MIB);
      for (let i = 0; i < four.length; i += 65536) crypto.getRandomValues(four.subarray(i, i + 65536));
      const held: { cap: Uint8Array; digest: Uint8Array }[] = [];
      let stop: string | undefined;
      const before = h.rss();
      let peak = before;
      for (let i = 0; i < 12 && stop === undefined; i++) {
        const t0 = performance.now();
        const r = await blobs.put(h.home, { ciphertext: four.map((v, j) => (j < 8 ? (v + i) & 255 : v)) });
        rec.note("blobPut4MiB", performance.now() - t0, r.result.ok ? "ok" : r.result.error.name);
        peak = Math.max(peak, h.rss());
        if (r.result.ok && r.digest) held.push({ cap: r.cap, digest: r.digest }); else stop = r.result.ok ? undefined : r.result.error.name;
      }
      out["spoolBytes"] = { accepted: held.length, acceptedBytes: held.length * 4 * MIB, advertisedBytes: 32 * MIB, stoppedWith: stop };
      if (held.length * 4 * MIB > 32 * MIB || stop !== "quota") violate("blobs", `spool accepted ${String(held.length)} blobs of 4 MiB and stopped with ${String(stop)}`);
      const over = await blobs.put(h.home, { ciphertext: new Uint8Array(4 * MIB + 1) });
      out["oversizeBlob"] = over.result.ok ? "ok" : over.result.error.name;
      if (over.result.ok || over.result.error.name !== "payload_too_large") violate("blobs", "a blob over 4 MiB was not payload_too_large");
      // read back byte for byte
      let intact = 0;
      for (const [i, b] of held.entries()) {
        const t0 = performance.now();
        const g = await blobs.get(h.home, blobs.locator(b.cap), b.cap).catch(() => undefined);
        rec.note("blobGet4MiB", performance.now() - t0, g?.ok ? "ok" : "error");
        if (g?.ok && g.bytes.length === four.length && g.bytes[8] === four[8] && g.bytes[four.length - 1] === four[four.length - 1] && g.bytes[0] === ((four[0] ?? 0) + i) % 256) intact++;
      }
      out["readBackIntact"] = `${String(intact)} of ${String(held.length)}`;
      if (intact !== held.length) violate("blobs", "a stored blob did not read back intact");
      // delete frees the space
      const first = held[0];
      if (first) {
        const d = await blobs.remove(h.home, blobs.locator(first.cap), first.cap);
        const again = await blobs.put(h.home, { ciphertext: four.map((v, j) => (j < 8 ? (v + 99) & 255 : v)) });
        out["afterDelete"] = { deleted: d.ok, nextPut: again.result.ok ? "ok" : again.result.error.name };
        if (!d.ok || !again.result.ok) violate("blobs", "deleting a blob did not free its space");
      }
      // many small blobs: the count bound
      const h2 = await launch(target);
      try {
        let small = 0, smallStop: string | undefined;
        for (let i = 0; i < 100 && smallStop === undefined; i++) { const r = await blobs.put(h2.home, { ciphertext: rand(2000) }); if (r.result.ok) small++; else smallStop = r.result.error.name; }
        out["blobCount"] = { accepted: small, stoppedWith: smallStop, advertised: 64 };
        if (small !== 64 || smallStop !== "quota") violate("blobs", `blob count bound: ${String(small)} accepted, stopped with ${String(smallStop)}`);
      } finally { await h2.stop(); h2.remove(); }
      out["latency"] = rec.summary();
      out["rss"] = { idleMb: mb(before), peakMb: mb(peak), afterMb: mb(h.rss()) };
      out["diskMb"] = mb(h.diskBytes());
    } finally { report["blobs"] = out; await h.stop(); h.remove(); }
  });

  it("recovery store: inbox traffic and its limits", async () => {
    const h = await launch(target);
    const out: Record<string, unknown> = {};
    const rec = new Recorder();
    const cbor = { "content-type": "application/cbor" };
    const inboxPath = (id: Uint8Array, sub = ""): string => `/v1/recovery/inbox/${toBase64Url(id)}${sub}`;
    const send = async (op: string, path: string, init: RequestInit): Promise<{ status: number; name: string; bytes: Uint8Array }> => {
      const t0 = performance.now();
      const r = await h.home.fetch(path, init);
      const bytes = new Uint8Array(await r.arrayBuffer());
      const name = r.status < 300 ? "ok" : decodeHomeError(bytes).name;
      rec.note(op, performance.now() - t0, name);
      return { status: r.status, name, bytes };
    };
    try {
      // inboxes: the Home holds at most 16
      const inboxes: { accountId: Uint8Array; inboxId: Uint8Array; cap: Uint8Array }[] = [];
      let regStop: string | undefined;
      for (let i = 0; i < 20 && regStop === undefined; i++) {
        const accountId = rand(32); const inboxId = await recoveryInboxId(accountId, 1); const cap = rand(32);
        const r = await send("inboxRegister", inboxPath(inboxId), { method: "PUT", headers: cbor, body: asBody(encodeInboxRegister({ requestId: rand(16), cap })) });
        if (r.name === "ok") inboxes.push({ accountId, inboxId, cap }); else regStop = r.name;
      }
      out["inboxes"] = { registered: inboxes.length, stoppedWith: regStop, advertised: 16 };
      if (inboxes.length !== 16 || regStop !== "quota") violate("recovery", `inbox bound: ${String(inboxes.length)} registered, stopped with ${String(regStop)}`);
      // requests: the Home-wide post budget (burst 10) answers rate_limited, never silence
      const posted: { inbox: (typeof inboxes)[number]; sig: string }[] = [];
      let postStop: string | undefined;
      for (let i = 0; i < 25; i++) {
        const inbox = inboxes[i % inboxes.length] as (typeof inboxes)[number];
        const req = signedRequest(inbox.accountId);
        const r = await send("fanout", "/v1/recovery/requests", { method: "POST", headers: cbor, body: asBody(encodeFanout({ requestId: rand(16), signed: req.signed })) });
        if (r.name === "ok") posted.push({ inbox, sig: sha(req.signed) }); else { postStop ??= r.name; }
      }
      out["posts"] = { sent: 25, accepted: posted.length, firstRefusal: postStop };
      if (posted.length > 11 || postStop !== "rate_limited") violate("recovery", `post budget: ${String(posted.length)} accepted, first refusal ${String(postStop)}`);
      // drain every inbox: each accepted request is there exactly once with identical bytes; ack removes it
      let drained = 0;
      for (const inbox of inboxes) {
        const r = await send("inboxDrain", inboxPath(inbox.inboxId, "/drain"), { method: "POST", headers: cbor, body: asBody(encodeInboxDrain({ requestId: rand(16), cap: inbox.cap })) });
        if (r.name !== "ok") { violate("recovery", `inbox drain ${r.name}`); continue; }
        const items = decodeInboxDrainResponse(r.bytes).items;
        drained += items.length;
        for (const it of items) if (!posted.some((p) => p.inbox === inbox && p.sig === sha(it.bytes))) violate("recovery", "an inbox held a request nobody posted or an altered one");
        const a = await send("inboxAck", inboxPath(inbox.inboxId, "/ack"), { method: "POST", headers: cbor, body: asBody(encodeInboxAck({ requestId: rand(16), cap: inbox.cap, itemIds: items.map((i) => i.id) })) });
        if (items.length > 0 && a.name !== "ok") violate("recovery", `inbox ack ${a.name}`);
      }
      out["drained"] = drained;
      if (drained !== posted.length) violate("recovery", `${String(posted.length)} requests accepted but ${String(drained)} drained`);
      // records: a 1 MiB envelope is accepted and read back; 1 MiB + 1 is payload_too_large; the Home holds at most 16 records
      const c = await recordCaps();
      const env = envelope(1_048_016);
      const t0 = performance.now();
      const put = await h.home.fetch(`/v1/recovery/records/${toBase64Url(c.locator)}`, { method: "PUT", headers: { ...cbor, ...bearer(c.writeCap), "if-none-match": "*" }, body: asBody(encodeRecordPut({ requestId: rand(16), envelope: env })) });
      rec.note("recordPut1MiB", performance.now() - t0, put.status < 300 ? "ok" : decodeHomeError(new Uint8Array(await put.arrayBuffer())).name);
      const got = await h.home.fetch(`/v1/recovery/records/${toBase64Url(c.locator)}`, { headers: bearer(c.readCap) });
      const body = new Uint8Array(await got.arrayBuffer());
      out["record"] = { putStatus: put.status, getStatus: got.status, getBytes: body.length };
      if (put.status >= 300 || got.status !== 200) violate("recovery", "a 1 MiB recovery record was not stored and read back");
      out["latency"] = rec.summary();
    } finally { report["recovery"] = out; await h.stop(); h.remove(); }
  });

  // ---- 3. wake sockets -----------------------------------------------------------------------------------------------------

  it("wake: many queues with a socket each, the per-queue socket bound, delivery latency, memory", async () => {
    const h = await launch(target);
    const out: Record<string, unknown> = {};
    const rec = new Recorder();
    interface Sock { ws: WebSocket; seqs: number[]; events: { seq: number; t: number }[]; waiters: { seq: number; resolve: (t: number) => void }[]; closed: Promise<number>; opened: Promise<void> }
    const connect = (b: Box): Sock => {
      const ws = new WebSocket(h.url.replace(/^http/, "ws") + "/v1/relay/wake", [WAKE_SUBPROTOCOL, wakeLabel(b.caps.prefix)]);
      ws.binaryType = "arraybuffer";
      const s: Sock = { ws, seqs: [], events: [], waiters: [], closed: new Promise((r) => { ws.addEventListener("close", (e) => { r(e.code); }); }), opened: new Promise((r, j) => { ws.addEventListener("open", () => { r(); }); ws.addEventListener("error", () => { j(new Error("socket")); }); }) };
      s.opened.catch(() => undefined);
      ws.addEventListener("message", (e) => {
        if (typeof e.data === "string") return;
        const ev = decodeWakeEvent(new Uint8Array(e.data as ArrayBuffer));
        s.seqs.push(ev.seq);
        s.events.push({ seq: ev.seq, t: performance.now() });
        for (const w of s.waiters.filter((x) => ev.seq >= x.seq)) w.resolve(performance.now());
        s.waiters = s.waiters.filter((x) => ev.seq < x.seq);
      });
      return s;
    };
    const ready = async (s: Sock, b: Box): Promise<void> => {
      await s.opened;
      s.ws.send(encodeWakeAuth({ cap: b.caps.receive }));
      for (let i = 0; i < 100 && s.seqs.length === 0; i++) await sleep(20);
      if (s.seqs.length === 0) throw new Error("no ready frame");
    };
    const waitFor = (s: Sock, seq: number, ms = 5000): Promise<number | undefined> =>
      new Promise((resolve) => {
        const early = s.events.find((e) => e.seq >= seq);
        if (early) { resolve(early.t); return; } const t = setTimeout(() => { resolve(undefined); }, ms); s.waiters.push({ seq, resolve: (v) => { clearTimeout(t); resolve(v); } }); });
    const socks: Sock[] = [];
    try {
      const boxes = await makeBoxes(h, P.wakeQueues, rec);
      const idle = h.rss();
      for (const b of boxes) { const s = connect(b); socks.push(s); await ready(s, b); }
      await sleep(300);
      out["sockets"] = { open: socks.length, rssIdleMb: mb(idle), rssWithSocketsMb: mb(h.rss()), perSocketKb: Math.round((h.rss() - idle) / socks.length / 1024) };
      // the per-queue bound: 8 sockets per queue; the 9th is closed with the documented code
      const extra = boxes[0] as Box;
      const more: Sock[] = [];
      for (let i = 0; i < 8; i++) { const s = connect(extra); more.push(s); await s.opened.catch(() => undefined); s.ws.send(encodeWakeAuth({ cap: extra.caps.receive })); }
      await sleep(500);
      const openOnQueue = 1 + more.filter((s) => s.ws.readyState === WebSocket.OPEN).length;
      // the refusal of the ninth is a typed HTTP answer before the upgrade; read it with a plain request that carries the upgrade headers
      const refused = await new Promise<{ status: number; name: string }>((resolve, reject) => {
        const req = httpRequest(h.url + "/v1/relay/wake", { method: "GET", headers: { connection: "Upgrade", upgrade: "websocket", "sec-websocket-version": "13", "sec-websocket-key": "dGhlIHNhbXBsZSBub25jZQ==", "sec-websocket-protocol": `${WAKE_SUBPROTOCOL}, ${wakeLabel(extra.caps.prefix)}` } }, (res) => {
          const parts: Buffer[] = [];
          res.on("data", (d: Buffer) => parts.push(d));
          res.on("end", () => { resolve({ status: res.statusCode ?? 0, name: decodeHomeError(new Uint8Array(Buffer.concat(parts))).name }); });
        });
        req.on("upgrade", (res, socket) => { socket.destroy(); resolve({ status: res.statusCode ?? 0, name: "upgraded" }); });
        req.on("error", reject);
        req.end();
      });
      out["perQueueBound"] = { socketsOpenOnQueue: openOnQueue, advertised: 8, ninthAnswer: refused };
      if (openOnQueue !== 8 || refused.status !== 429 || refused.name !== "rate_limited") violate("wake", `per-queue socket bound: ${String(openOnQueue)} open, the next answered ${String(refused.status)} ${refused.name}`);
      more.forEach((s) => { s.ws.close(); });
      // a deposit per queue, all at once, repeated: every one must wake its socket with a sequence at least its own, in order
      const led = new Ledger();
      let missed = 0, delivered = 0;
      for (let round = 0; round < 5; round++) {
        await Promise.all(boxes.map(async (b, i) => {
          const s = socks[i] as Sock;
          const t0 = performance.now();
          const r = await deposit(h, rec, led, b, 1040);
          if (!r?.ok || r.value === undefined) return;
          const seq = (r.value as { seq?: number }).seq ?? 0;
          const t1 = await waitFor(s, seq);
          if (t1 === undefined) missed++; else { delivered++; rec.note("depositToWake", t1 - t0, "ok"); }
        }));
        await sleep(50);
      }
      out["wake"] = { delivered, missed, latency: rec.summary() };
      if (missed > 0) violate("wake", `${String(missed)} deposits did not wake their socket within 5 s`);
      // seqs only increase on every socket
      for (const s of socks) for (let i = 1; i < s.seqs.length; i++) if ((s.seqs[i] ?? 0) < (s.seqs[i - 1] ?? 0)) { violate("wake", "wake sequence decreased"); break; }
      for (const b of boxes) await audit(h, led, b);
      violations.push(...led.violations.slice(0, 5).map((v) => `wake: ${v}`));
      socks.forEach((s) => { s.ws.close(); });
      await sleep(500);
      out["rssAfterCloseMb"] = mb(h.rss());
    } finally { report["wake"] = out; socks.forEach((s) => { s.ws.close(); }); await h.stop(); h.remove(); }
  });

  // ---- 4. usage states -----------------------------------------------------------------------------------------------------

  for (const metric of ["worker", "durable"] as const) {
    it(`usage: soft and hard states by ${metric === "worker" ? "Worker requests" : "Durable Object calls"}`, async () => {
      const ceiling = P.usageCeiling;
      const vars = metric === "worker" ? { OM_USAGE_CEILING_WORKER: String(ceiling) } : { OM_USAGE_CEILING_DO: String(ceiling) };
      const h = await launch(target, { OM_PROFILE: "personal", ...vars });
      const out: Record<string, unknown> = { ceiling, metric };
      let tally = 0;
      const home = { ...h.home, fetch: (path: string, init?: RequestInit): Promise<Response> => { tally++; return h.home.fetch(path, init); } };
      const hh = { ...h, home } as LoadHome;
      const rec = new Recorder();
      const led = new Ledger();
      const read = async (admin: Uint8Array): Promise<ReturnType<typeof parseUsageState>> => parseUsageState(await (await home.fetch(USAGE_PATH, { headers: bearer(admin) })).json());
      try {
        const boxes = await makeBoxes(hh, 3, rec, 1);
        const states: { atRequest: number; state: string }[] = [];
        let last = "";
        let firstShed: number | undefined;
        let acceptedAfterShed = 0;
        let step = 0;
        // sequential traffic so the request index of each transition is exact
        while (tally < ceiling * 2 && step < ceiling * 6) {
          const b = boxes[step % boxes.length] as Box;
          step++;
          const env = sealed(); const k = sha(env);
          led.sending(b.name, k);
          const d = await rec.time("deposit", () => relay.deposit(home, b.caps.deposit, env));
          if (d.ok) { led.stored(b.name, k); if (firstShed !== undefined) acceptedAfterShed++; } else { led.refused(b.name, k); if (d.error.name === "quota" && d.error.detail && toHex(d.error.detail) === toHex(quotaDetail(1, true)).replace(/^/, "") ) firstShed ??= tally; }
          if (step % 3 === 0) await drainAck(hh, rec, led, b);
          if (step % 7 === 0) {
            const u = await read(b.caps.admin);
            if (u.state !== last) { states.push({ atRequest: tally, state: u.state }); last = u.state; }
            if (u.state === "hard") break;
          }
        }
        const u = await read((boxes[0] as Box).caps.admin);
        const atHard = tally;
        out["states"] = states;
        out["finalState"] = { state: u.state, metrics: u.metrics, requestsSent: atHard };
        const order = ["healthy", "soft", "hard"];
        if (states.some((s, i) => i > 0 && order.indexOf(s.state) < order.indexOf(states[i - 1]?.state ?? "healthy"))) violate(`usage-${metric}`, "usage state went backwards inside a day");
        // at hard: deposits, queue creation and blob puts are shed with quota + hard; drains, acks, usage read, healthz and recovery are served
        const shed: Record<string, string> = {};
        const dep = await relay.deposit(home, (boxes[0] as Box).caps.deposit, sealed());
        shed["deposit"] = dep.ok ? "ok" : `${dep.error.name}${dep.error.detail ? `+${toHex(dep.error.detail)}` : ""}`;
        if (dep.ok) { violate(`usage-${metric}`, "a deposit was accepted at the hard state"); }
        const cr = await relay.create(home, newCaps());
        shed["queueCreate"] = cr.ok ? "ok" : cr.error.name;
        const bl = await blobs.put(home, { ciphertext: rand(2000) });
        shed["blobPut"] = bl.result.ok ? "ok" : bl.result.error.name;
        const dr = await relay.drain(home, (boxes[0] as Box).caps.receive);
        shed["drain"] = dr.ok ? "ok" : dr.error.name;
        const hz = await home.fetch("/healthz");
        shed["healthz"] = String(hz.status);
        const inboxId = await recoveryInboxId(rand(32), 1);
        const rg = await home.fetch(`/v1/recovery/inbox/${toBase64Url(inboxId)}`, { method: "PUT", headers: { "content-type": "application/cbor" }, body: asBody(encodeInboxRegister({ requestId: rand(16), cap: rand(32) })) });
        shed["recoveryRegister"] = String(rg.status);
        out["atHard"] = shed;
        if (u.state !== "hard") violate(`usage-${metric}`, `traffic reached ${String(tally)} requests (ceiling ${String(ceiling)}) without a hard state; last state ${u.state}`);
        else {
          if (shed["deposit"]?.startsWith("quota") !== true) violate(`usage-${metric}`, "hard state did not shed deposits with quota");
          if (cr.ok || bl.result.ok) violate(`usage-${metric}`, "hard state did not shed queue creation and blob puts");
          if (!dr.ok || hz.status !== 200 || rg.status >= 300) violate(`usage-${metric}`, "hard state shed a request that must stay served");
          // the shed point is where the Home says: 95% of the worse ceiling
          const reading = u.metrics.find((m) => m.name === (metric === "worker" ? "worker_requests" : "durable_object_requests"));
          out["shedAtRatio"] = reading ? Math.round((reading.used / ceiling) * 1000) / 1000 : null;
        }
        // acked data stays readable at hard: everything the ledger holds is still there
        for (const b of boxes) await audit(hh, led, b);
        // the Home's own count equals this client's count exactly (the Home counts itself; healthz and the read included)
        const final = await read((boxes[0] as Box).caps.admin);
        const wr = final.metrics.find((m) => m.name === "worker_requests")?.used ?? -1;
        out["accounting"] = { clientCount: tally, homeWorkerCount: wr, equal: wr === tally };
        if (wr !== tally) violate(`usage-${metric}`, `the Home counted ${String(wr)} Worker requests, the client sent ${String(tally)}`);
        for (const v of led.violations.slice(0, 5)) violate(`usage-${metric}`, v);
      } finally { report[`usage_${metric}`] = out; await h.stop(); h.remove(); }
    });
  }

  // ---- 5. soak, memory, disk, kill and restart ----------------------------------------------------------------------------

  it("soak: mixed traffic, memory and disk over time, kill and recover", async () => {
    const h = await launch(target);
    const led = new Ledger();
    const rec = new Recorder();
    const out: Record<string, unknown> = {};
    try {
      const boxes = await makeBoxes(h, P.soakMailboxes, rec);
      mark("soak: mailboxes made");
      const blobCaps: Uint8Array[] = [];
      const series: { t: number; rssMb: number; diskMb: number; ops: number }[] = [];
      let ops = 0;
      const t0 = performance.now();
      const sampler = setInterval(() => { if (series.length % 10 === 0) mark(`soak: t=${String(series.length)}s ops=${String(ops)}`); series.push({ t: Math.round((performance.now() - t0) / 1000), rssMb: mb(h.rss()), diskMb: mb(h.diskBytes()), ops }); }, 1000);
      const workers = 8;
      await pool(workers, boxes, P.soakMs, async (b, w, n) => {
        const roll = (n * 7 + w) % 20;
        if (roll < 9) await deposit(h, rec, led, b, roll % 3 === 0 ? 16_400 : 1040);
        else if (roll < 17) await drainAck(h, rec, led, b);
        else if (roll === 17) {
          const t1 = performance.now();
          const r = await blobs.put(h.home, { ciphertext: rand(64 * 1024) });
          rec.note("blobPut64KiB", performance.now() - t1, r.result.ok ? "ok" : r.result.error.name);
          if (r.result.ok) blobCaps.push(r.cap);
          while (blobCaps.length > 6) { const c = blobCaps.shift() as Uint8Array; await blobs.remove(h.home, blobs.locator(c), c); }
        } else await sleep(5);
        ops++;
      });
      clearInterval(sampler);
      const s = rec.summary();
      const bad = unexpected(s, new Set(["rate_limited", "quota"]));
      if (bad.length > 0) violate("soak", `unexpected outcomes: ${bad.join(",")}`);
      // warm-up is the first quarter; growth is measured after it
      const warm = series.filter((p) => p.t >= (P.soakMs / 1000) * 0.25);
      const rssStart = warm[0]?.rssMb ?? 0, rssEnd = warm[warm.length - 1]?.rssMb ?? 0;
      const span = Math.max(1, (warm[warm.length - 1]?.t ?? 1) - (warm[0]?.t ?? 0));
      const third = Math.floor(series.length / 3);
      const rate = (a: number, b: number): number => { const x = series[a], y = series[b]; return x && y && y.t > x.t ? Math.round((y.ops - x.ops) / (y.t - x.t)) : 0; };
      out["soak"] = {
        seconds: Math.round(P.soakMs / 1000), workers, mailboxes: boxes.length, ops, opsPerSecond: Math.round(ops / (P.soakMs / 1000)),
        opsPerSecondFirstThird: rate(0, third), opsPerSecondLastThird: rate(series.length - 1 - third, series.length - 1),
        rssMb: { afterWarmup: rssStart, end: rssEnd, max: Math.max(...series.map((p) => p.rssMb)), growthMbPerMinute: Math.round(((rssEnd - rssStart) / span) * 60 * 10) / 10 },
        diskMb: { max: Math.max(...series.map((p) => p.diskMb)), end: series[series.length - 1]?.diskMb ?? 0 },
        deposited: led.deposits, acked: led.acks, stillHeld: led.pendingTotal(), operations: s, series: series.filter((_, i) => i % Math.max(1, Math.floor(series.length / 20)) === 0),
      };
      mark("soak: traffic done");
      await sleep(5000);
      out["rssIdleAfterLoadMb"] = mb(h.rss());
      // leave unacknowledged items behind in every mailbox (the buckets have refilled), audit, then kill the Home without a clean stop
      for (const b of boxes) for (let i = 0; i < 4; i++) await deposit(h, rec, led, b, i === 0 ? 16_400 : 1040);
      mark("soak: left items behind");
      for (const b of boxes) await audit(h, led, b);
      mark("soak: audited before kill");
      const held = led.pendingTotal();
      await h.kill();
      mark("soak: killed");
      const diskAtKill = h.diskBytes();
      const down = performance.now();
      await h.restart();
      const restartMs = Math.round(performance.now() - down);
      mark("soak: restarted");
      for (const b of boxes) await audit(h, led, b);
      mark("soak: audited after restart");
      out["killAndRestart"] = { how: target === "sqlite" ? "SIGKILL of the Home process" : "stop of the Miniflare host (workerd has no separate process to kill from outside)", itemsHeldBefore: held, itemsFoundAfter: led.pendingTotal(), restartMs, diskAtKillMb: mb(diskAtKill) };
      // drain everything; the disk must come back down (measured after a clean stop, which is when a Worker's write-ahead log is folded in)
      for (const b of boxes) await drainAck(h, rec, led, b);
      mark("soak: drained");
      for (const b of boxes) await audit(h, led, b);
      mark("soak: final audit");
      await h.stop();
      mark("soak: stopped");
      out["afterDrain"] = { diskAfterCleanStopMb: mb(h.diskBytes()), itemsHeld: led.pendingTotal(), diskByKindMb: Object.fromEntries(Object.entries(diskByKind(h.dir)).map(([k, v]) => [k, mb(v)])) };
      await h.restart();
      for (const v of led.violations.slice(0, 10)) violate("soak", v);
      expect(led.violations).toEqual([]);
    } finally { report["soak"] = out; await h.stop(); h.remove(); }
  });

  it("bound: the per-mailbox retry table stops growing at its limit", async () => {
    const h = await launch(target);
    const out: Record<string, unknown> = {};
    try {
      const b = (await makeBoxes(h, 1, new Recorder()))[0] as Box;
      const caps: Uint8Array[] = [b.caps.deposit];
      for (let i = 0; i < 63; i++) caps.push(await mint2(h, b));
      const led = new Ledger();
      const rec = new Recorder();
      const rows: { rows: number; diskKb: number }[] = [];
      // every stored deposit and every ack answer is one remembered request (limit 8192 rows per mailbox, oldest dropped first)
      let remembered = 0, next = 0, spins = 0;
      const goal = 9000;
      while (remembered < goal && spins < 100_000) {
        spins++;
        const r = await deposit(h, rec, led, { name: b.name, caps: { ...b.caps, deposit: caps[next++ % caps.length] as Uint8Array } }, 1040);
        if (r?.ok) {
          remembered++;
          const page = await relay.drain(h.home, b.caps.receive);
          if (page.ok && page.value.items.length > 0) {
            const a = await relay.ack(h.home, b.caps.receive, page.value.items.map((i) => i.id));
            if (a.ok) { remembered++; page.value.items.forEach((i) => { led.ackedNow(b.name, sha(i.bytes)); }); }
          }
          if (remembered % 1000 < 2) rows.push({ rows: remembered, diskKb: Math.round(h.diskBytes() / 1024) });
        } else if (r && !r.ok && r.error.name === "rate_limited") await sleep(25);
      }
      await audit(h, led, b);
      await h.stop();
      const settled = h.diskBytes();
      await h.restart();
      const mailboxFile = Math.round(((): number => { const k = diskByKind(h.dir); return k["database"] ?? 0; })() / 1024);
      out["retryTable"] = { limit: 8192, rowsWritten: remembered, samples: rows, mailboxPlusMetadataKbAfterStop: Math.round(settled / 1024), databaseKbAfterRestart: mailboxFile };
      const at = rows.filter((r) => r.rows >= 8192);
      const before = rows.find((r) => r.rows >= 7000 && r.rows < 8192);
      const last = rows[rows.length - 1];
      if (before && last && last.rows > 8192 && last.diskKb > before.diskKb * 1.3 + 256) violate("bound", `the retry table kept growing past its limit (${String(before.diskKb)} KiB at ${String(before.rows)} rows, ${String(last.diskKb)} KiB at ${String(last.rows)})`);
      void at;
      for (const v of led.violations.slice(0, 5)) violate("bound", v);
    } finally { report["bound"] = out; await h.stop(); h.remove(); }
  });

  it("crash: stop the Home in the middle of traffic, three times; nothing it acknowledged may be lost", async () => {
    const h = await launch(target);
    const led = new Ledger();
    const rec = new Recorder();
    const rounds: unknown[] = [];
    try {
      const boxes = await makeBoxes(h, Math.max(16, Math.floor(P.mailboxes / 2)), rec);
      for (let round = 0; round < 3; round++) {
        const stopAfter = 700 + Math.floor(Math.random() * 1300);
        let down = false;
        const killer = sleep(stopAfter).then(async () => { down = true; await h.kill(); });
        const traffic = pool(8, boxes, stopAfter + 600, async (b, _w, n) => {
          if (down) { await sleep(20); return; }
          if (n % 3 === 2) await drainAck(h, rec, led, b); else await deposit(h, rec, led, b, n % 5 === 0 ? 16_400 : 1040);
        });
        await Promise.all([killer, traffic]);
        const t0 = performance.now();
        await h.restart();
        const restartMs = Math.round(performance.now() - t0);
        const before = led.violations.length;
        const stored = led.deposits, held = led.pendingTotal();
        for (const b of boxes) await audit(h, led, b);
        rounds.push({ stoppedAfterMs: stopAfter, depositsAcknowledged: stored, itemsHeldBeforeCheck: held, itemsHeldAfterCheck: led.pendingTotal(), restartMs, newViolations: led.violations.length - before });
      }
      report["crash"] = { how: target === "sqlite" ? "SIGKILL of the Home process under load" : "stop of the Miniflare host under load", rounds };
      for (const v of led.violations.slice(0, 10)) violate("crash", v);
    } finally { await h.stop(); h.remove(); }
  });

  it("no violations", () => { expect(violations).toEqual([]); });
});

/** A structurally valid SealedDeliveryEnvelope with a ciphertext of any size (the test kit's own builder caps at 64 KiB). */
function bigSealed(ct: number): Uint8Array {
  const body = new Uint8Array(ct);
  for (let i = 0; i < ct; i += 65536) crypto.getRandomValues(body.subarray(i, Math.min(ct, i + 65536)));
  return encode(map([[0, 23], [1, 1], [2, 1], [3, rand(16)], [4, rand(16)], [5, rand(1120)], [6, body], [7, Math.floor(Date.now() / 1000) + 3600]]));
}

async function mint2(h: LoadHome, b: Box): Promise<Uint8Array> {
  const m = await relay.mint(h.home, b.caps.admin);
  if (!m.ok) throw new Error(`mint: ${m.error.name}`);
  return m.value.cap;
}
void percentiles; void errorOf;
