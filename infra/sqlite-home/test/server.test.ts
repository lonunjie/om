// SPDX-License-Identifier: AGPL-3.0-or-later
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cpSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { toHex } from "@om/home-wire";
import { scratch } from "../../cloudflare-home/test/harness";
import { relay, newCaps, sealed } from "../../cloudflare-home/test/kit";
import { startSqliteHome } from "./harness";

let dir: ReturnType<typeof scratch>;
beforeEach(() => { dir = scratch(); });
let home: Awaited<ReturnType<typeof startSqliteHome>> | undefined;
afterEach(async () => {
  await home?.dispose();
  home = undefined;
  dir.remove();
});

it("persists accepted mail and its exact retry outcome through a Home restart", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const c = newCaps();
  expect((await relay.create(home, c)).ok).toBe(true);
  const envelope = sealed();
  const id = crypto.getRandomValues(new Uint8Array(16));
  const delivered = await relay.deposit(home, c.deposit, envelope, id);
  expect(delivered.ok).toBe(true);
  await home.dispose();

  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const retried = await relay.deposit(home, c.deposit, envelope, id);
  expect(retried.ok && retried.value).toEqual(delivered.ok && delivered.value);
  const drained = await relay.drain(home, c.receive);
  expect(drained.ok && drained.value.items).toHaveLength(1);
});

it("PC-H01 rolls back the deposit if saving its retry outcome fails", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const c = newCaps();
  expect((await relay.create(home, c)).ok).toBe(true);
  const db = new DatabaseSync(join(dir.dir, `queue--q-${toHex(c.prefix)}.sqlite`));
  const id = crypto.getRandomValues(new Uint8Array(16));
  const bytes = sealed();
  try {
    db.exec("CREATE TRIGGER fault BEFORE INSERT ON ops BEGIN SELECT RAISE(ABORT, 'storage fault'); END");
    const result = await relay.deposit(home, c.deposit, bytes, id);
    expect(result.ok ? result : result.error.name).toBe("retryable");
    expect(result.headers.get("retry-after")).toBe("2");
    const before = await relay.drain(home, c.receive);
    expect(before.ok && before.value.items).toEqual([]);

    db.exec("DROP TRIGGER fault");
    const delivery = await relay.deposit(home, c.deposit, bytes, id);
    expect(delivery.ok && delivery.value.seq).toBe(1);
    const after = await relay.drain(home, c.receive);
    expect(after.ok && after.value.items).toHaveLength(1);
  } finally {
    db.close();
  }
});

it("serializes concurrent retries without allocating a second sequence", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const c = newCaps();
  expect((await relay.create(home, c)).ok).toBe(true);
  const bytes = sealed();
  const id = crypto.getRandomValues(new Uint8Array(16));
  const running = home;
  const receipts = await Promise.all(Array.from({ length: 8 }, () => relay.deposit(running, c.deposit, bytes, id)));
  expect(receipts.every((r) => r.ok)).toBe(true);
  expect(receipts.map((r) => r.ok && toHex(r.value.itemId))).toEqual(Array.from({ length: 8 }, () => receipts[0]?.ok && toHex(receipts[0].value.itemId)));
  const drained = await relay.drain(home, c.receive);
  expect(drained.ok && drained.value.items).toHaveLength(1);
});

it("unknown capabilities and discovery locators allocate no persistent object", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const files = readdirSync(dir.dir);
  const c = newCaps();
  expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(false);
  expect((await relay.drain(home, c.receive)).ok).toBe(false);
  const fingerprint = toHex(c.prefix);
  expect((await home.fetch(`/v1/discovery/descriptors/${fingerprint}`)).status).toBe(404);
  expect(readdirSync(dir.dir)).toEqual(files);
});

it("keeps creation closed when a setup value is required and missing", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true, vars: { OM_ADMISSION_REQUIRED: "1" } });
  const files = readdirSync(dir.dir);
  const denied = await relay.create(home, newCaps());
  expect(denied.ok ? denied : denied.error.name).toBe("unauthorized");
  expect((await home.fetch("/v1/push/key")).status).toBe(404);
  expect(readdirSync(dir.dir)).toEqual(files);
});

it("runs a pending retention alarm after restart even without client requests", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const c = newCaps();
  expect((await relay.create(home, c)).ok).toBe(true);
  expect((await relay.deposit(home, c.deposit, sealed())).ok).toBe(true);
  await home.dispose();

  const db = new DatabaseSync(join(dir.dir, `queue--q-${toHex(c.prefix)}.sqlite`));
  try {
    db.exec("UPDATE items SET expires_at = 0; UPDATE _home_alarm SET time = 0");
    home = await startSqliteHome({ persist: dir.dir, direct: true });
    await vi.waitFor(() => {
      expect(db.prepare("SELECT COUNT(*) AS n FROM items").get()?.["n"]).toBe(0);
    }, { timeout: 3000 });
  } finally {
    db.close();
  }
});

async function postLarge(url: string, path: string, bytes: number): Promise<number> {
  const { request } = await import("node:http");
  return new Promise((resolve, reject) => {
    const req = request(url + path, { method: "POST", headers: { "content-type": "application/octet-stream", "content-length": String(bytes) } }, (res) => {
      res.resume();
      res.on("end", () => { resolve(res.statusCode ?? 0); });
    });
    req.on("error", reject);
    const chunk = Buffer.alloc(64 * 1024, 7);
    let sent = 0;
    const pump = (): void => {
      while (sent < bytes) {
        const part = chunk.subarray(0, Math.min(chunk.length, bytes - sent));
        sent += part.length;
        if (!req.write(part)) { req.once("drain", pump); return; }
      }
      req.end();
    };
    pump();
  });
}

it("PC-H02 answers a request whose body is refused unread with its typed error, not a reset connection", async () => {
  home = await startSqliteHome({ persist: dir.dir });
  // A queue-create body is bounded at 1 KiB, so a multi-megabyte body is refused with the typed 413.
  for (let i = 0; i < 5; i++) expect(await postLarge(home.url, "/v1/relay/queues", 6 * 1024 * 1024)).toBe(413);
});

it("PC-H03 bounds what it drains from an oversize body, and closes the connection after the reply", async () => {
  home = await startSqliteHome({ persist: dir.dir });
  const status = await postLarge(home.url, "/v1/relay/queues", 64 * 1024 * 1024).catch((error: unknown) => (error as { code?: string }).code);
  // Either the reply arrives first or the connection is cut after the bound; it never hangs and the Home stays usable.
  expect([413, "EPIPE", "ECONNRESET"]).toContain(status);
  expect((await fetch(home.url + "/.well-known/om-provider-capabilities")).status).toBe(200);
});

it("PC-H04 keeps the data directory private to its owner, including one that already existed", async () => {
  const { chmodSync, statSync } = await import("node:fs");
  chmodSync(dir.dir, 0o755);
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  expect(statSync(dir.dir).mode & 0o077).toBe(0);
});

it("PC-H05 starts again after a crash that left a hot rollback journal beside a mailbox file", async () => {
  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const c = newCaps();
  expect((await relay.create(home, c)).ok).toBe(true);
  const delivered = await relay.deposit(home, c.deposit, sealed());
  expect(delivered.ok).toBe(true);
  await home.dispose();

  // Reproduce what a SIGKILL in the middle of a write leaves behind: the database file partly rewritten and its journal still
  // there. The copy is taken while the transaction is open, then the original is rolled back and discarded.
  const name = `queue--q-${toHex(c.prefix)}.sqlite`;
  const source = join(dir.dir, name);
  const writer = new DatabaseSync(source);
  const snapshot = scratch();
  try {
    writer.exec("PRAGMA cache_size=1; PRAGMA synchronous=FULL; BEGIN IMMEDIATE");
    writer.exec("CREATE TABLE crash_junk (x BLOB)");
    const insert = writer.prepare("INSERT INTO crash_junk VALUES (?)");
    for (let i = 0; i < 400; i++) insert.run(new Uint8Array(4096).fill(i % 251));
    writer.exec("DELETE FROM ops");
    for (const f of readdirSync(dir.dir)) if (f.startsWith(name)) cpSync(join(dir.dir, f), join(snapshot.dir, f));
    expect(readdirSync(snapshot.dir).some((f) => f.endsWith("-journal"))).toBe(true);
  } finally {
    writer.close();
  }
  for (const f of readdirSync(dir.dir)) if (f.startsWith(name)) rmSync(join(dir.dir, f));
  for (const f of readdirSync(snapshot.dir)) cpSync(join(snapshot.dir, f), join(dir.dir, f));
  snapshot.remove();

  home = await startSqliteHome({ persist: dir.dir, direct: true });
  const drained = await relay.drain(home, c.receive);
  expect(drained.ok && drained.value.items).toHaveLength(1);
});
