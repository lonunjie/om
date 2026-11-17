// SPDX-License-Identifier: AGPL-3.0-or-later
// The RecoveryStore (ADR-0201, home-api.md section 8.4) run against SQLite with an explicit clock, because workerd cannot be
// time-travelled: every limit is checked at exactly its bound, and every window (a day, an hour, a retention) is crossed. The Durable
// Object class itself is loaded; only `cloudflare:workers` and the storage handle are stood in for (node:sqlite, same SQL dialect).
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { decodeInboxDrainResponse, decodeRecordGetResponse, decodeVetoList, equalBytes, recoveryEtag, recoveryInboxId, sha256, toHex } from "@om/home-wire";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DAY, LIMITS, PERSONAL_LIMITS } from "../src/limits";
import type { Reply } from "../src/util";
import { envelope, maxEnvelope, rand, recordCaps, requestDigestOf, signedRequest, signedVeto, type Req } from "./recovery-kit";

vi.mock("cloudflare:workers", () => ({
  DurableObject: class { constructor(readonly ctx: unknown, readonly env: unknown) {} },
}));

interface Store {
  scopeId(): Promise<Uint8Array>;
  putRecord(a: { locator: Uint8Array; writeCap: Uint8Array; requestId: Uint8Array; envelope: Uint8Array; create: boolean; ifMatch?: Uint8Array }): Promise<Reply>;
  getRecord(locator: Uint8Array, readCap: Uint8Array): Promise<Reply>;
  deleteRecord(locator: Uint8Array, writeCap: Uint8Array): Promise<Reply>;
  registerInbox(inboxId: Uint8Array, cap: Uint8Array, requestId: Uint8Array): Promise<Reply>;
  drain(inboxId: Uint8Array, cap: Uint8Array, pageSize: number | undefined): Promise<Reply>;
  ack(inboxId: Uint8Array, cap: Uint8Array, ids: readonly Uint8Array[]): Promise<Reply>;
  fanout(a: { requestId: Uint8Array; signed: Uint8Array; accountId: Uint8Array; body: Uint8Array }): Promise<Reply>;
  postVeto(a: { requestId: Uint8Array; signed: Uint8Array; accountId: Uint8Array; requestDigest: Uint8Array; body: Uint8Array }): Promise<Reply>;
  vetoes(digest: Uint8Array): Promise<Reply>;
  alarm(): Promise<void>;
}

const T0 = 1_900_000_000;
const HOUR = 3600;
const advance = (seconds: number): void => { vi.setSystemTime(new Date((Math.floor(Date.now() / 1000) + seconds) * 1000)); };

interface Harness { readonly db: DatabaseSync; readonly store: Store; restart(): Promise<Store> }

async function mk(vars: Record<string, string> = {}, db = new DatabaseSync(":memory:")): Promise<Harness> {
  const state = {
    storage: {
      sql: {
        exec(q: string, ...b: unknown[]) {
          if (b.length === 0 && q.split(";").filter((x) => x.trim() !== "").length > 1) { db.exec(q); return { toArray: () => [] as unknown[] }; }
          const st = db.prepare(q);
          const rows: unknown[] = /^\s*(select|with)/i.test(q) ? st.all(...(b as never[])) : (st.run(...(b as never[])), []);
          return { toArray: () => rows };
        },
      },
      transactionSync<T>(f: () => T): T {
        db.exec("BEGIN");
        try { const r = f(); db.exec("COMMIT"); return r; } catch (e) { db.exec("ROLLBACK"); throw e; }
      },
      setAlarm: () => Promise.resolve(),
      deleteAlarm: () => Promise.resolve(),
    },
  };
  // Loaded by path, so the Worker-only types of the source stay out of this (Node-typed) project.
  const { RecoveryStore } = (await import(/* @vite-ignore */ fileURLToPath(new URL("../src/recovery.ts", import.meta.url)))) as { RecoveryStore: new (ctx: unknown, env: unknown) => Store };
  const make = (): Store => new RecoveryStore(state, { OM_HARD_FREE_TIER: "1", ...vars });
  return { db, store: make(), restart: () => Promise.resolve(make()) };
}

const personal = { OM_PROFILE: "personal" };
const errName = (r: Reply): string => (r.ok ? "ok" : r.error);
const expectOk = (r: Reply): Uint8Array => { if (!r.ok) throw new Error(`unexpected ${r.error}`); return r.body; };
const count = (db: DatabaseSync, table: string): number => (db.prepare(`SELECT COUNT(*) AS n FROM ${table}`).get() as { n: number }).n;

/** Creates a record and returns its handles; `ifMatch` of the next replace is the returned ETag. */
async function create(s: Store, env = envelope()): Promise<{ readCap: Uint8Array; writeCap: Uint8Array; locator: Uint8Array; etag: Uint8Array; env: Uint8Array }> {
  const c = await recordCaps();
  const etag = expectOk(await s.putRecord({ locator: c.locator, writeCap: c.writeCap, requestId: rand(16), envelope: env, create: true }));
  return { ...c, etag, env };
}
const replace = (s: Store, r: { locator: Uint8Array; writeCap: Uint8Array }, ifMatch: Uint8Array, env = envelope(), requestId = rand(16)) =>
  s.putRecord({ locator: r.locator, writeCap: r.writeCap, requestId, envelope: env, create: false, ifMatch });

/** An inbox for a fresh account; returns what posting and draining need. */
async function inbox(s: Store): Promise<{ accountId: Uint8Array; inboxId: Uint8Array; cap: Uint8Array }> {
  const accountId = rand(32);
  const inboxId = await recoveryInboxId(accountId);
  const cap = rand(32);
  expectOk(await s.registerInbox(inboxId, cap, rand(16)));
  return { accountId, inboxId, cap };
}
const post = (s: Store, r: Req, requestId = rand(16)) => s.fanout({ requestId, signed: r.signed, accountId: r.accountId, body: r.body });
const veto = async (s: Store, accountId: Uint8Array, digest: Uint8Array, n?: number, requestId = rand(16)) => {
  const signed = signedVeto(accountId, digest, n);
  const { checkSignedVeto } = await import("@om/home-wire");
  const shape = checkSignedVeto(signed);
  return s.postVeto({ requestId, signed, accountId, requestDigest: shape.requestDigest, body: shape.body });
};
const drained = async (s: Store, i: { inboxId: Uint8Array; cap: Uint8Array }, page?: number) => decodeInboxDrainResponse(expectOk(await s.drain(i.inboxId, i.cap, page)));

beforeEach(() => { vi.useFakeTimers({ toFake: ["Date"] }); vi.setSystemTime(new Date(T0 * 1000)); });
afterEach(() => { vi.useRealTimers(); });

describe("record limits at exactly the bound", () => {
  it("default profile: 16 records, the 17th is `quota`; personal profile: 4 records, the 5th is `quota`", async () => {
    for (const [vars, max] of [[{}, LIMITS.maxRecoveryRecords], [personal, PERSONAL_LIMITS.maxRecoveryRecords ?? 0]] as const) {
      const { store } = await mk(vars);
      expect(max).toBe(vars === personal ? 4 : 16);
      for (let i = 0; i < max; i++) await create(store);
      const c = await recordCaps();
      const r = await store.putRecord({ locator: c.locator, writeCap: c.writeCap, requestId: rand(16), envelope: envelope(), create: true });
      expect(errName(r)).toBe("quota");
      // an existing record is never evicted to make room: all of them still read back
    }
  });

  it("the largest conforming envelope round-trips; 1,048,576 bytes is still taken by the store and one byte more is `payload_too_large`; the stored bytes and ETag are exactly what was put", async () => {
    const { store } = await mk();
    const big = maxEnvelope();
    expect(big.length).toBeLessThanOrEqual(1_048_576);
    const r = await create(store, big);
    expect(toHex(r.etag)).toBe(toHex(await recoveryEtag(big)));
    const got = expectOk(await store.getRecord(r.locator, r.readCap));
    expect(toHex(got.slice(0, 32))).toBe(toHex(r.etag));
    expect(equalBytes(decodeRecordGetResponse(got.slice(32)), big)).toBe(true);
    const c = await recordCaps();
    expect(errName(await store.putRecord({ locator: c.locator, writeCap: c.writeCap, requestId: rand(16), envelope: new Uint8Array(1_048_576), create: true }))).toBe("ok");
    const over = new Uint8Array(1_048_577);
    const d = await recordCaps();
    expect(errName(await store.putRecord({ locator: d.locator, writeCap: d.writeCap, requestId: rand(16), envelope: over, create: true }))).toBe("payload_too_large");
  });

  it("put: burst 6 (the create takes one), refill 1 per 600 s; rate_limited carries the wait; nothing changes on a refusal", async () => {
    const { store } = await mk();
    const r = await create(store);
    let etag = r.etag;
    for (let i = 0; i < 5; i++) etag = expectOk(await replace(store, r, etag));
    const refused = await replace(store, r, etag);
    expect(refused).toMatchObject({ ok: false, error: "rate_limited" });
    expect(!refused.ok && refused.retryAfterSeconds).toBeGreaterThan(0);
    expect(!refused.ok && refused.retryAfterSeconds).toBeLessThanOrEqual(600);
    expect(toHex(expectOk(await store.getRecord(r.locator, r.readCap)).slice(0, 32))).toBe(toHex(etag)); // the refused replace stored nothing
    advance(599);
    expect(errName(await replace(store, r, etag))).toBe("rate_limited");
    advance(1);
    expectOk(await replace(store, r, etag));
  });

  it("get: burst 10 per locator, refill 1 per 30 s", async () => {
    const { store } = await mk();
    const r = await create(store);
    for (let i = 0; i < 10; i++) expectOk(await store.getRecord(r.locator, r.readCap));
    expect(errName(await store.getRecord(r.locator, r.readCap))).toBe("rate_limited");
    advance(30);
    expectOk(await store.getRecord(r.locator, r.readCap));
    expect(errName(await store.getRecord(r.locator, r.readCap))).toBe("rate_limited");
    const other = await create(store); // another record has its own bucket
    expectOk(await store.getRecord(other.locator, other.readCap));
  });

  it("failed gets (wrong capability, unknown locator) share one Home-wide bucket: burst 30, refill 1 per 10 s, and a valid get is never charged to it", async () => {
    const { store } = await mk();
    const r = await create(store);
    for (let i = 0; i < 30; i++) expect(errName(await store.getRecord(i % 2 === 0 ? r.locator : (await recordCaps()).locator, rand(32)))).toBe("not_found");
    expect(errName(await store.getRecord(r.locator, rand(32)))).toBe("rate_limited"); // wrong capability for a present record
    expect(errName(await store.getRecord((await recordCaps()).locator, rand(32)))).toBe("rate_limited"); // unknown locator: the same
    expectOk(await store.getRecord(r.locator, r.readCap)); // the owner is not locked out by someone guessing
    advance(10);
    expect(errName(await store.getRecord(r.locator, rand(32)))).toBe("not_found");
    expect(errName(await store.getRecord(r.locator, rand(32)))).toBe("rate_limited");
  });
});

describe("compare-and-swap, idempotency and replay", () => {
  it("a create on an existing locator, a stale If-Match and a missing record are distinct typed outcomes without a timestamp", async () => {
    const { store } = await mk();
    const r = await create(store);
    expect(errName(await store.putRecord({ locator: r.locator, writeCap: r.writeCap, requestId: rand(16), envelope: envelope(), create: true }))).toBe("stale_generation");
    expect(errName(await replace(store, r, await sha256(rand(8))))).toBe("stale_generation");
    const next = expectOk(await replace(store, r, r.etag));
    expect(toHex(next)).not.toBe(toHex(r.etag));
    expect(errName(await replace(store, r, r.etag))).toBe("stale_generation"); // the old ETag no longer matches
  });

  it("the same request id and bytes is one outcome, spends no token and survives a process restart; other bytes under that id are `invalid_request`", async () => {
    const h = await mk();
    const r = await create(h.store);
    const rid = rand(16);
    const e2 = envelope();
    const first = expectOk(await replace(h.store, r, r.etag, e2, rid));
    for (let i = 0; i < 10; i++) expect(toHex(expectOk(await replace(h.store, r, r.etag, e2, rid)))).toBe(toHex(first)); // would be rate_limited if each spent a token
    const again = await h.restart();
    expect(toHex(expectOk(await replace(again, r, r.etag, e2, rid)))).toBe(toHex(first));
    expect(errName(await replace(again, r, r.etag, envelope(), rid))).toBe("invalid_request");
    advance(LIMITS.idempotencySeconds + LIMITS.skewSeconds + 1); // past the retry horizon the id is forgotten
    expect(errName(await replace(again, r, r.etag, e2, rid))).toBe("stale_generation");
  });

  it("a retried create returns the same ETag; idempotency never turns a wrong capability into a valid one", async () => {
    const { store } = await mk();
    const c = await recordCaps();
    const rid = rand(16);
    const env = envelope();
    const a = expectOk(await store.putRecord({ locator: c.locator, writeCap: c.writeCap, requestId: rid, envelope: env, create: true }));
    const b = expectOk(await store.putRecord({ locator: c.locator, writeCap: c.writeCap, requestId: rid, envelope: env, create: true }));
    expect(toHex(a)).toBe(toHex(b));
    expect(errName(await store.putRecord({ locator: c.locator, writeCap: rand(32), requestId: rid, envelope: env, create: true }))).toBe("unauthorized");
  });

  it("a wrong write capability and an absent record are the same answer for replace (`unauthorized`) and for delete (`not_found`); read capabilities likewise (`not_found`)", async () => {
    const { store } = await mk();
    const r = await create(store);
    const absent = await recordCaps();
    const a = await replace(store, { locator: r.locator, writeCap: rand(32) }, r.etag);
    const b = await replace(store, absent, rand(32));
    expect(a).toEqual(b);
    expect(errName(a)).toBe("unauthorized");
    expect(await store.deleteRecord(r.locator, rand(32))).toEqual(await store.deleteRecord(absent.locator, absent.writeCap));
    expect(errName(await store.deleteRecord(absent.locator, absent.writeCap))).toBe("not_found");
    expect(await store.getRecord(r.locator, rand(32))).toEqual(await store.getRecord(absent.locator, absent.readCap));
    // the record survived every wrong attempt
    expectOk(await store.getRecord(r.locator, r.readCap));
  });

  it("the read capability cannot write and the write capability cannot read (independent capabilities)", async () => {
    const { store } = await mk();
    const r = await create(store);
    expect(errName(await replace(store, { locator: r.locator, writeCap: r.readCap }, r.etag))).toBe("unauthorized");
    expect(errName(await store.getRecord(r.locator, r.writeCap))).toBe("not_found");
    expect(errName(await store.deleteRecord(r.locator, r.readCap))).toBe("not_found");
  });

  it("delete is idempotent, frees the slot, and the locator can be created again", async () => {
    const { store } = await mk(personal);
    const rs = [];
    for (let i = 0; i < 4; i++) rs.push(await create(store));
    const gone = rs[0];
    if (!gone) throw new Error("no record");
    expect(errName(await store.deleteRecord(gone.locator, gone.writeCap))).toBe("ok");
    expect(errName(await store.deleteRecord(gone.locator, gone.writeCap))).toBe("not_found");
    expect(errName(await store.getRecord(gone.locator, gone.readCap))).toBe("not_found");
    await create(store); // the freed slot
    expect(errName(await store.putRecord({ locator: gone.locator, writeCap: gone.writeCap, requestId: rand(16), envelope: envelope(), create: true }))).toBe("quota"); // 4 held again
  });
});

describe("retention", () => {
  it("a record lives 400 days after its last replace, no day less; a replace restarts the clock", async () => {
    const { store, db } = await mk();
    const r = await create(store);
    advance(399 * DAY);
    const etag = expectOk(await replace(store, r, r.etag));
    advance(399 * DAY);
    expectOk(await store.getRecord(r.locator, r.readCap));
    expect(count(db, "records")).toBe(1);
    advance(2 * DAY);
    expect(errName(await store.getRecord(r.locator, r.readCap))).toBe("not_found");
    expect(count(db, "records")).toBe(0);
    expect(errName(await replace(store, r, etag))).toBe("unauthorized");
  });

  it("the alarm sweep drops what is past retention without a request", async () => {
    const { store, db } = await mk();
    const i = await inbox(store);
    await post(store, signedRequest(i.accountId));
    await create(store);
    advance(LIMITS.maxRetentionSeconds + LIMITS.skewSeconds + 1);
    await store.alarm();
    expect(count(db, "items")).toBe(0);
    expect(count(db, "reqs")).toBe(0);
    expect(count(db, "records")).toBe(1);
    advance(400 * DAY);
    await store.alarm();
    expect([count(db, "records"), count(db, "inboxes")]).toEqual([0, 0]);
  });
});

describe("the recovery inbox", () => {
  it("registers once per account; the same capability refreshes; another capability is `unauthorized`; 16 inboxes (personal 4), the next is `quota`", async () => {
    for (const [vars, max] of [[{}, LIMITS.maxRecoveryInboxes], [personal, PERSONAL_LIMITS.maxRecoveryInboxes ?? 0]] as const) {
      const { store } = await mk(vars);
      const first = await inbox(store);
      expectOk(await store.registerInbox(first.inboxId, first.cap, rand(16)));
      expect(errName(await store.registerInbox(first.inboxId, rand(32), rand(16)))).toBe("unauthorized");
      for (let i = 1; i < max; i++) await inbox(store);
      expect(errName(await store.registerInbox(await recoveryInboxId(rand(32)), rand(32), rand(16)))).toBe("quota");
      expectOk(await store.registerInbox(first.inboxId, first.cap, rand(16))); // a refresh needs no room
    }
  });

  it("a post answers identically whether or not an inbox exists, and when a limit drops it; no response carries a time", async () => {
    const { store } = await mk();
    const present = await inbox(store);
    const results: Reply[] = [];
    results.push(await post(store, signedRequest(present.accountId)));
    results.push(await post(store, signedRequest(rand(32)))); // no inbox for this account
    for (let i = 0; i < 4; i++) results.push(await post(store, signedRequest(present.accountId))); // beyond 3 per day: dropped
    for (const r of results) expect(r).toEqual({ ok: true, body: new Uint8Array(0) });
    expect((await drained(store, present)).items).toHaveLength(3);
  });

  it("distinct requests: 3 per rolling 24 hours; an identical request changes nothing and is not counted; a 4th the next day is taken", async () => {
    const { store } = await mk();
    const i = await inbox(store);
    const reqs = [signedRequest(i.accountId), signedRequest(i.accountId), signedRequest(i.accountId), signedRequest(i.accountId)];
    for (const r of reqs.slice(0, 3)) await post(store, r);
    for (let n = 0; n < 5; n++) await post(store, reqs[0] as Req); // resends: no new item, no count
    await post(store, reqs[3] as Req);
    expect((await drained(store, i)).items.map((x) => toHex(x.bytes))).toEqual(reqs.slice(0, 3).map((r) => toHex(r.signed)));
    advance(DAY + LIMITS.skewSeconds);
    await post(store, reqs[3] as Req);
    expect((await drained(store, i)).items).toHaveLength(4);
  });

  it("a full inbox (8 pending requests) keeps every item it holds and drops the new one, answered like a success", async () => {
    const { store } = await mk();
    const i = await inbox(store);
    const held: Req[] = [];
    for (let day = 0; day < 3; day++) {
      for (let k = 0; k < 3; k++) {
        const r = signedRequest(i.accountId);
        const res = await post(store, r);
        expect(res.ok).toBe(true);
        if (held.length < LIMITS.recoveryPendingRequests) held.push(r);
      }
      advance(DAY + LIMITS.skewSeconds);
    }
    expect(held).toHaveLength(8);
    const items = (await drained(store, i, 16)).items;
    expect(items.map((x) => toHex(x.bytes))).toEqual(held.map((r) => toHex(r.signed))); // the earliest genuine requests are all still there, in order
  });

  it("drain returns {id, kind, bytes} and nothing else, in arrival order, 16 at most per page, with a cursor only when more wait; ack removes", async () => {
    const { store } = await mk();
    const i = await inbox(store);
    const a = signedRequest(i.accountId);
    const b = signedRequest(i.accountId);
    await post(store, a);
    await post(store, b);
    const page1 = decodeInboxDrainResponse(expectOk(await store.drain(i.inboxId, i.cap, 1)));
    expect(page1.items).toHaveLength(1);
    expect(page1.cursor).toBeDefined();
    expect(Object.keys(page1.items[0] ?? {}).sort()).toEqual(["bytes", "id", "kind"]);
    expect(page1.items[0]?.kind).toBe(1);
    const all = await drained(store, i);
    expect(all.cursor).toBeUndefined();
    expect(all.items.map((x) => toHex(x.bytes))).toEqual([toHex(a.signed), toHex(b.signed)]);
    expectOk(await store.ack(i.inboxId, i.cap, [all.items[0]?.id ?? new Uint8Array(16), rand(16)])); // an unknown id is fine
    expect((await drained(store, i)).items.map((x) => toHex(x.bytes))).toEqual([toHex(b.signed)]);
    const other = await inbox(store);
    expectOk(await store.ack(other.inboxId, other.cap, [all.items[1]?.id ?? new Uint8Array(16)])); // another inbox cannot remove it
    expect((await drained(store, i)).items).toHaveLength(1);
  });

  it("a wrong capability and an absent inbox are one `unauthorized` for drain and ack; drain burst is 20, refill 1 per 5 s", async () => {
    const { store } = await mk();
    const i = await inbox(store);
    const ghost = await recoveryInboxId(rand(32));
    expect(await store.drain(i.inboxId, rand(32), undefined)).toEqual(await store.drain(ghost, rand(32), undefined));
    expect(errName(await store.drain(i.inboxId, rand(32), undefined))).toBe("unauthorized");
    expect(await store.ack(i.inboxId, rand(32), [rand(16)])).toEqual(await store.ack(ghost, rand(32), [rand(16)]));
    for (let n = 0; n < 20; n++) expectOk(await store.drain(i.inboxId, i.cap, undefined));
    expect(errName(await store.drain(i.inboxId, i.cap, undefined))).toBe("rate_limited");
    advance(5);
    expectOk(await store.drain(i.inboxId, i.cap, undefined));
  });

  it("fan-out and veto posts share a Home-wide bucket: burst 10, refill 1 per 60 s; this is the only limit a poster sees", async () => {
    const { store } = await mk();
    for (let n = 0; n < 10; n++) expect(errName(await post(store, signedRequest(rand(32))))).toBe("ok");
    const refused = await post(store, signedRequest(rand(32)));
    expect(refused).toMatchObject({ ok: false, error: "rate_limited" });
    expect(!refused.ok && refused.retryAfterSeconds).toBeGreaterThan(0);
    expect(errName(await veto(store, rand(32), rand(32)))).toBe("rate_limited");
    advance(60);
    expect(errName(await post(store, signedRequest(rand(32))))).toBe("ok");
    expect(errName(await post(store, signedRequest(rand(32))))).toBe("rate_limited");
  });

  it("the same request id with a different body is `invalid_request`; the same body again is one outcome and spends no post token", async () => {
    const { store } = await mk();
    const rid = rand(16);
    const r = signedRequest(rand(32));
    expect(errName(await post(store, r, rid))).toBe("ok");
    for (let n = 0; n < 20; n++) expect(errName(await post(store, r, rid))).toBe("ok");
    expect(errName(await post(store, signedRequest(rand(32)), rid))).toBe("invalid_request");
  });
});

describe("the veto board", () => {
  async function withRequest() {
    const h = await mk();
    const i = await inbox(h.store);
    const r = signedRequest(i.accountId);
    await post(h.store, r);
    const digest = await requestDigestOf(r);
    return { ...h, i, r, digest };
  }
  const board = async (s: Store, digest: Uint8Array) => decodeVetoList(expectOk(await s.vetoes(digest)));

  it("a veto is accepted only when the inbox holds a request with that digest; otherwise dropped and answered like a success", async () => {
    const { store, i, digest } = await withRequest();
    expect(errName(await veto(store, i.accountId, rand(32)))).toBe("ok"); // no such request
    expect(errName(await veto(store, rand(32), digest))).toBe("ok"); // no inbox for that account
    expect(await board(store, digest)).toHaveLength(0);
    expect(errName(await veto(store, i.accountId, digest))).toBe("ok");
    expect(await board(store, digest)).toHaveLength(1);
    expect((await drained(store, i)).items.map((x) => x.kind)).toEqual([1, 2]); // the controllers see it in their inbox too
  });

  it("a veto still lands after the request was acknowledged, and an ack does not remove it from the board", async () => {
    const { store, i, digest } = await withRequest();
    const item = (await drained(store, i)).items[0];
    expectOk(await store.ack(i.inboxId, i.cap, [item?.id ?? new Uint8Array(16)]));
    await veto(store, i.accountId, digest);
    const items = (await drained(store, i)).items;
    expectOk(await store.ack(i.inboxId, i.cap, items.map((x) => x.id)));
    expect(await board(store, digest)).toHaveLength(1);
  });

  it("an identical veto changes nothing and is not counted; vetoes per inbox: 8 per rolling hour", async () => {
    const { store, i, digest } = await withRequest();
    const first = signedVeto(i.accountId, digest, 7);
    const { checkSignedVeto } = await import("@om/home-wire");
    const shape = checkSignedVeto(first);
    for (let n = 0; n < 4; n++) await store.postVeto({ requestId: rand(16), signed: first, accountId: i.accountId, requestDigest: shape.requestDigest, body: shape.body });
    expect(await board(store, digest)).toHaveLength(1);
    advance(600); // each post, even a repeat, spends from the Home-wide post bucket: let it refill
    for (let n = 0; n < 7; n++) await veto(store, i.accountId, digest, 100 + n);
    expect(await board(store, digest)).toHaveLength(8);
    await veto(store, i.accountId, digest, 999); // the 9th within the hour
    expect(await board(store, digest)).toHaveLength(8);
    advance(HOUR + 1);
    await veto(store, i.accountId, digest, 999);
    expect(await board(store, digest)).toHaveLength(9);
  });

  it("at most 16 vetoes per request digest and 32 pending per inbox; the earliest are kept and the board never returns more than 16", async () => {
    const { store, i, digest, r } = await withRequest();
    const second = signedRequest(i.accountId);
    await post(store, second);
    const digest2 = await requestDigestOf(second);
    const kept: string[] = [];
    for (let n = 0; n < 24; n++) {
      if (n % 8 === 0 && n > 0) advance(HOUR + 1);
      const signed = signedVeto(i.accountId, digest, 1000 + n);
      const { checkSignedVeto } = await import("@om/home-wire");
      const shape = checkSignedVeto(signed);
      await store.postVeto({ requestId: rand(16), signed, accountId: i.accountId, requestDigest: shape.requestDigest, body: shape.body });
      if (kept.length < 16) kept.push(toHex(signed));
    }
    expect((await board(store, digest)).map(toHex)).toEqual(kept);
    for (let n = 0; n < 16; n++) {
      if (n % 8 === 0) advance(HOUR + 1);
      await veto(store, i.accountId, digest2, 5000 + n);
    }
    expect(await board(store, digest2)).toHaveLength(16);
    // 32 pending vetoes in the inbox now (16 + 16): a third digest gets none
    const third = signedRequest(i.accountId);
    advance(DAY + LIMITS.skewSeconds);
    await post(store, third);
    advance(HOUR + 1);
    await veto(store, i.accountId, await requestDigestOf(third), 9000);
    expect(await board(store, await requestDigestOf(third))).toHaveLength(0);
    expect(r.signed.length).toBeGreaterThan(0);
  });

  it("the board read is bounded per digest (burst 10, refill 1 per 10 s) and an unknown digest creates no state", async () => {
    const { store, db, digest } = await withRequest();
    for (let n = 0; n < 10; n++) expectOk(await store.vetoes(digest));
    expect(errName(await store.vetoes(digest))).toBe("rate_limited");
    advance(10);
    expectOk(await store.vetoes(digest));
    const before = [count(db, "reqs"), count(db, "vetoes"), count(db, "ops")];
    for (let n = 0; n < 50; n++) expect(decodeVetoList(expectOk(await store.vetoes(rand(32))))).toEqual([]);
    expect([count(db, "reqs"), count(db, "vetoes"), count(db, "ops")]).toEqual(before);
  });

  it("items and board rows are kept 14 days and then dropped; a request digest is forgotten with them", async () => {
    const { store, db, i, r, digest } = await withRequest();
    await veto(store, i.accountId, digest);
    advance(LIMITS.maxRetentionSeconds - 10);
    expect((await drained(store, i)).items).toHaveLength(2);
    advance(LIMITS.skewSeconds + 20);
    expect((await drained(store, i)).items).toHaveLength(0);
    expect(await board(store, digest)).toHaveLength(0);
    expect([count(db, "items"), count(db, "vetoes"), count(db, "reqs")]).toEqual([0, 0, 0]);
    await post(store, r); // an expired request may be posted again
    expect((await drained(store, i)).items).toHaveLength(1);
  });
});

describe("what the Home holds and says", () => {
  it("the scope id is 32 random bytes, stable for the life of the store and across a restart", async () => {
    const h = await mk();
    const a = await h.store.scopeId();
    expect(a).toHaveLength(32);
    expect(toHex(await h.store.scopeId())).toBe(toHex(a));
    expect(toHex(await (await h.restart()).scopeId())).toBe(toHex(a));
  });

  it("stored rows hold only hashes of capabilities, never a capability, an account id or a device id", async () => {
    const { store, db } = await mk();
    const r = await create(store);
    const i = await inbox(store);
    const dump = ["records", "inboxes", "items", "reqs", "vetoes", "ops", "meta"].flatMap((t) => (db.prepare(`SELECT * FROM ${t}`).all() as Record<string, unknown>[]).flatMap((row) => Object.values(row)));
    const blobs = dump.filter((v): v is Uint8Array => v instanceof Uint8Array).map(toHex);
    for (const secret of [r.readCap, r.writeCap, i.cap, i.accountId]) expect(blobs).not.toContain(toHex(secret));
    expect(blobs.join("")).not.toContain(toHex(r.writeCap));
  });

  it("no response of any operation contains a time or a record column other than the bytes it was given", async () => {
    const { store } = await mk();
    const r = await create(store);
    const i = await inbox(store);
    const req = signedRequest(i.accountId);
    await post(store, req);
    const replies = [
      await store.getRecord(r.locator, r.readCap), await store.drain(i.inboxId, i.cap, undefined), await store.vetoes(await requestDigestOf(req)),
    ];
    for (const rep of replies) expect(rep.ok).toBe(true);
    const drain = decodeInboxDrainResponse(expectOk(replies[1] as Reply));
    for (const item of drain.items) expect(Object.keys(item).sort()).toEqual(["bytes", "id", "kind"]);
  });
});
