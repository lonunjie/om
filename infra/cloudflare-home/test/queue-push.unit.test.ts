// SPDX-License-Identifier: AGPL-3.0-or-later
// Concurrent operations against the production classes with the SQLite storage surface.
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { expect, it, vi } from "vitest";
import { decodeDepositCapabilityRecord } from "@om/home-wire";
import type { Reply } from "../src/util";
import { newCaps, rand, sealed } from "./kit";

vi.mock("cloudflare:workers", () => ({
  DurableObject: class { constructor(readonly ctx: unknown, readonly env: unknown) {} },
}));

function context(db: DatabaseSync) {
  return {
    storage: {
      sql: {
        exec(query: string, ...bindings: SQLInputValue[]) {
          if (bindings.length === 0 && query.includes(";")) {
            db.exec(query);
            return { toArray: () => [] };
          }
          const statement = db.prepare(query);
          if (statement.columns().length > 0) return { toArray: () => statement.all(...bindings) };
          statement.run(...bindings);
          return { toArray: () => [] };
        },
      },
      transactionSync<T>(apply: () => T): T {
        db.exec("BEGIN");
        try {
          const result = apply();
          db.exec("COMMIT");
          return result;
        } catch (error) {
          db.exec("ROLLBACK");
          throw error;
        }
      },
      setAlarm: () => Promise.resolve(),
      deleteAlarm: () => Promise.resolve(),
    },
    setWebSocketAutoResponse() {},
    getWebSockets: () => [],
  };
}

it("keeps the same push key for concurrent first use and a new instance", async () => {
  const { PushHub } = await import(/* @vite-ignore */ fileURLToPath(new URL("../src/push.ts", import.meta.url))) as {
    PushHub: new (ctx: unknown, env: unknown) => {
      publicKey(): Promise<string>;
      register(label: string, endpoint: string, subject: string): Promise<{ ok: boolean }>;
    };
  };
  using db = new DatabaseSync(":memory:");
  const ctx = context(db);
  const hub = new PushHub(ctx, {});
  const [registration, ...keys] = await Promise.all([
    hub.register("mailbox", "https://fcm.googleapis.com/test", "https://home.example"),
    ...Array.from({ length: 8 }, () => hub.publicKey()),
  ]);
  expect(registration.ok).toBe(true);
  expect(new Set(keys).size).toBe(1);
  expect(await new PushHub(ctx, {}).publicKey()).toBe(keys[0]);
});

it("linearizes a deposit at the grant check after preparing its digests", async () => {
  vi.stubGlobal("WebSocketRequestResponsePair", function ResponsePair() {});
  try {
    const { MailboxQueue } = await import(/* @vite-ignore */ fileURLToPath(new URL("../src/queue.ts", import.meta.url))) as {
      MailboxQueue: new (ctx: unknown, env: unknown) => {
        create(request: { requestId: Uint8Array; receiveCap: Uint8Array; depositCap: Uint8Array; adminCap: Uint8Array }): Promise<Reply>;
        mint(cap: Uint8Array, request: { requestId: Uint8Array; label: Uint8Array; ttlSeconds: number }): Promise<Reply>;
        revokeDepositCapability(cap: Uint8Array, id: Uint8Array): Promise<Reply>;
        deposit(request: { requestId: Uint8Array; envelope: Uint8Array; cap: Uint8Array }): Promise<Reply>;
      };
    };
    using db = new DatabaseSync(":memory:");
    const queue = new MailboxQueue(context(db), {});
    const c = newCaps();
    expect((await queue.create({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })).ok).toBe(true);
    const mint = await queue.mint(c.admin, { requestId: rand(16), label: rand(16), ttlSeconds: 3600 });
    if (!mint.ok) throw new Error("mint failed");
    const grant = decodeDepositCapabilityRecord(mint.body);
    const envelope = sealed();
    const digest = crypto.subtle.digest.bind(crypto.subtle);
    let hashing = (): void => {};
    let resume = (): void => {};
    const hashingStarted = new Promise<void>((resolve) => { hashing = resolve; });
    const finishHash = new Promise<void>((resolve) => { resume = resolve; });
    const spy = vi.spyOn(crypto.subtle, "digest").mockImplementation(async (algorithm, bytes) => {
      if (bytes.byteLength === envelope.length) {
        hashing();
        await finishHash;
      }
      return digest(algorithm, bytes);
    });
    try {
      const deposit = queue.deposit({ requestId: rand(16), envelope, cap: grant.cap });
      await Promise.race([hashingStarted, deposit.then(() => { throw new Error("envelope digest not observed"); })]);
      expect((await queue.revokeDepositCapability(c.admin, grant.id)).ok).toBe(true);
      resume();
      expect(await deposit).toMatchObject({ ok: false, error: "revoked" });
      expect(db.prepare("SELECT seq FROM items").all()).toEqual([]);
    } finally {
      resume();
      spy.mockRestore();
    }
  } finally { vi.unstubAllGlobals(); }
});
