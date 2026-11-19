// SPDX-License-Identifier: AGPL-3.0-or-later
// ADR-0229 section 3 against the production queue class with the SQLite storage surface and a moving clock: the tombstone
// kept for the retry horizon and then swept, the idle expiry, and what a tombstone answers.
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { afterEach, expect, it, vi } from "vitest";
import type { Reply } from "../src/util";
import { newCaps, rand, sealed } from "./kit";

vi.mock("cloudflare:workers", () => ({
  DurableObject: class { constructor(readonly ctx: unknown, readonly env: unknown) {} },
}));

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

interface Alarm { time: number | undefined; deleted: boolean }

function context(db: DatabaseSync, alarm: Alarm) {
  return {
    storage: {
      sql: {
        exec(query: string, ...bindings: SQLInputValue[]) {
          if (bindings.length === 0 && query.includes(";")) { db.exec(query); return { toArray: () => [] }; }
          const statement = db.prepare(query);
          if (statement.columns().length > 0) return { toArray: () => statement.all(...bindings) };
          statement.run(...bindings);
          return { toArray: () => [] };
        },
      },
      transactionSync<T>(apply: () => T): T {
        db.exec("SAVEPOINT s");
        try { const r = apply(); db.exec("RELEASE s"); return r; } catch (e) { db.exec("ROLLBACK TO s; RELEASE s"); throw e; }
      },
      setAlarm: (t: number) => { alarm.time = t; return Promise.resolve(); },
      deleteAlarm: () => { alarm.time = undefined; return Promise.resolve(); },
      deleteAll: () => {
        alarm.deleted = true;
        for (const { name } of db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table'").all()) db.exec(`DELETE FROM "${String(name)}"`);
        return Promise.resolve();
      },
    },
    setWebSocketAutoResponse() {},
    getWebSockets: () => [],
  };
}

interface Q {
  create(r: { requestId: Uint8Array; receiveCap: Uint8Array; depositCap: Uint8Array; adminCap: Uint8Array }): Promise<Reply>;
  mint(cap: Uint8Array, r: { requestId: Uint8Array; label: Uint8Array; ttlSeconds?: number }): Promise<Reply>;
  retire(cap: Uint8Array, r: { requestId: Uint8Array }): Promise<Reply>;
  deposit(r: { requestId: Uint8Array; envelope: Uint8Array; cap: Uint8Array }): Promise<Reply>;
  drain(r: { requestId: Uint8Array; cap: Uint8Array }): Promise<Reply>;
  alarm(): Promise<void>;
}

async function open() {
  vi.stubGlobal("WebSocketRequestResponsePair", function ResponsePair() {});
  const { MailboxQueue } = await import(/* @vite-ignore */ fileURLToPath(new URL("../src/queue.ts", import.meta.url))) as { MailboxQueue: new (ctx: unknown, env: unknown) => Q };
  const db = new DatabaseSync(":memory:");
  const alarm: Alarm = { time: undefined, deleted: false };
  const queue = new MailboxQueue(context(db, alarm), {});
  const c = newCaps();
  const made = await queue.create({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin });
  expect(made.ok).toBe(true);
  return { queue, db, alarm, c };
}

const rows = (db: DatabaseSync, table: string): number => Number(db.prepare(`SELECT COUNT(*) AS n FROM ${table}`).get()?.["n"]);
const DAY = 86_400_000;

it("retirement deletes everything but a tombstone, answers revoked, and is swept at the horizon", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2027-01-10T12:34:56Z"));
  const { queue, db, alarm, c } = await open();
  const mint = await queue.mint(c.admin, { requestId: rand(16), label: new Uint8Array(0), ttlSeconds: 3600 });
  expect(mint.ok).toBe(true);
  expect((await queue.deposit({ requestId: rand(16), envelope: sealed(), cap: c.deposit })).ok).toBe(true);
  expect(rows(db, "items")).toBe(1);
  const id = rand(16);
  expect((await queue.retire(c.admin, { requestId: id })).ok).toBe(true);
  for (const t of ["items", "seen", "dcaps"]) expect(rows(db, t), t).toBe(0);
  const meta = Object.fromEntries(db.prepare("SELECT k, v FROM meta").all().map((r) => [String(r["k"]), r["v"]]));
  // only the tombstone: the day (no finer time), the sweep moment; no hash, no owner, no counter
  expect(Object.keys(meta).sort()).toEqual(["retired_day", "tomb_until"]);
  expect(meta["retired_day"]).toBe(Math.floor(Date.now() / 1000 / 86_400));
  expect(rows(db, "ops")).toBe(1); // the retire request's idempotency row only
  expect(alarm.time).toBeGreaterThan(Date.now());
  // late exact deposit, drain and retire answer revoked; the same retire request repeats its outcome
  expect(await queue.deposit({ requestId: rand(16), envelope: sealed(), cap: c.deposit })).toMatchObject({ ok: false, error: "revoked" });
  expect(await queue.drain({ requestId: rand(16), cap: c.receive })).toMatchObject({ ok: false, error: "revoked" });
  expect(await queue.retire(c.admin, { requestId: rand(16) })).toMatchObject({ ok: false, error: "revoked" });
  expect((await queue.retire(c.admin, { requestId: id })).ok).toBe(true);
  // The tombstone stays for the later of the retry horizon (7 days) and the longest grant lifetime. The queue's own default
  // capability has no expiry of its own, so it counts as the mint cap (56 days).
  vi.setSystemTime(Date.now() + 8 * DAY);
  await queue.alarm();
  expect(alarm.deleted).toBe(false);
  expect(rows(db, "meta")).toBe(2);
  vi.setSystemTime(Date.now() + 47 * DAY); // 55 days after retirement
  await queue.alarm();
  expect(alarm.deleted).toBe(false);
  vi.setSystemTime(Date.now() + 2 * DAY);
  await queue.alarm();
  expect(alarm.deleted).toBe(true);
  expect(alarm.time).toBeUndefined();
  for (const t of ["meta", "ops", "items", "dcaps"]) expect(rows(db, t), t).toBe(0);
  // the address is unknown again: it answers like any unknown address and allocates nothing
  expect(await queue.deposit({ requestId: rand(16), envelope: sealed(), cap: c.deposit })).toMatchObject({ ok: false, error: "quota" });
});

it("a queue nobody used for 400 days retires itself with the same tombstone; use keeps it alive", async () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2027-01-10T00:00:00Z"));
  const { queue, db, alarm, c } = await open();
  vi.setSystemTime(Date.now() + 8 * DAY);
  await queue.alarm(); // the create's own retry record expires; the next alarm is the idle deadline
  expect(alarm.time).toBeGreaterThan(Date.now() + 390 * DAY);
  vi.setSystemTime(Date.now() + 300 * DAY);
  expect((await queue.drain({ requestId: rand(16), cap: c.receive })).ok).toBe(true); // authenticated use resets the idle clock
  vi.setSystemTime(Date.now() + 200 * DAY); // 500 days since creation, 200 since use
  await queue.alarm();
  expect(rows(db, "meta")).toBeGreaterThan(2);
  expect((await queue.drain({ requestId: rand(16), cap: c.receive })).ok).toBe(true);
  vi.setSystemTime(Date.now() + 401 * DAY);
  await queue.alarm();
  const meta = Object.fromEntries(db.prepare("SELECT k, v FROM meta").all().map((r) => [String(r["k"]), r["v"]]));
  expect(Object.keys(meta).sort()).toEqual(["retired_day", "tomb_until"]);
  expect(await queue.drain({ requestId: rand(16), cap: c.receive })).toMatchObject({ ok: false, error: "revoked" });
  expect(rows(db, "dcaps")).toBe(0);
});
