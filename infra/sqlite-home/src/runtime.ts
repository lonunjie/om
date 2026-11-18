// SPDX-License-Identifier: AGPL-3.0-or-later
// The SQLite and socket parts of the reference Home's runtime, implemented with Node primitives.
import { chmodSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync, type SQLInputValue } from "node:sqlite";
import type { Env } from "../../cloudflare-home/src/env";
import { wakeEffects, type WakeQueue, type WakeSocket, type WebSocketRequestResponsePair } from "./wake";

const ALARM_TABLE = "CREATE TABLE IF NOT EXISTS _home_alarm (time INTEGER NOT NULL)";
type ObjectConstructor = new (ctx: LocalContext, env: Env) => object;
type ObjectMethod = (...args: unknown[]) => unknown;

class Storage {
  readonly sql = {
    exec: (query: string, ...binds: SQLInputValue[]) => {
      if (binds.length === 0 && query.includes(";")) {
        this.db.exec(query);
        return { toArray: () => [] };
      }
      const statement = this.db.prepare(query);
      if (statement.columns().length === 0) {
        statement.run(...binds);
        return { toArray: () => [] };
      }
      const rows = statement.all(...binds);
      return { toArray: () => rows };
    },
  };

  constructor(private readonly db: DatabaseSync) {}

  transactionSync<T>(action: () => T): T {
    this.db.exec("SAVEPOINT home_operation");
    try {
      const value = action();
      this.db.exec("RELEASE home_operation");
      return value;
    } catch (e) {
      this.db.exec("ROLLBACK TO home_operation; RELEASE home_operation");
      throw e;
    }
  }

  setAlarm(time: number): void {
    this.db.exec(ALARM_TABLE);
    this.db.exec("DELETE FROM _home_alarm");
    this.db.prepare("INSERT INTO _home_alarm VALUES (?)").run(time);
  }

  deleteAlarm(): void {
    this.db.exec(ALARM_TABLE);
    this.db.exec("DELETE FROM _home_alarm");
  }

  alarmTime(): number | undefined {
    if (!this.db.prepare("SELECT 1 FROM sqlite_schema WHERE name = '_home_alarm'").get()) return undefined;
    const value = this.db.prepare("SELECT time FROM _home_alarm").get()?.["time"];
    return typeof value === "number" ? value : undefined;
  }

  deleteAll(): void {
    // Descriptor expiry clears this resource's data, including its tombstone and alarm.
    const tables = this.db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table'").all();
    for (const { name } of tables) {
      if (typeof name === "string" && /^[a-z_]+$/.test(name)) this.db.exec(`DELETE FROM "${name}"`);
    }
  }
}

class LocalContext {
  private auto?: WebSocketRequestResponsePair;

  constructor(readonly storage: Storage, private readonly queue: WakeQueue) {}

  setWebSocketAutoResponse(pair: WebSocketRequestResponsePair): void { this.auto = pair; }
  getWebSockets(): WakeSocket[] { return [...this.queue.sockets]; }
  acceptWebSocket(socket: WakeSocket): void { socket.accept(this.queue, this.auto); }
}

/** aliased to cloudflare:workers by the Node build; never loaded by the Worker. */
export class DurableObject {
  constructor(protected readonly ctx: LocalContext, protected readonly env: Env) {}
}

interface Scope {
  readonly type: ObjectConstructor;
  readonly create: ReadonlySet<string>;
}

export class LocalRuntime {
  private readonly scopes = new Map<string, Scope>();
  private readonly active = new Map<string, Promise<void>>();
  private readonly wakeQueues = new Map<string, WakeQueue>();
  private readonly alarms = new Map<string, number>();
  private readonly lock: DatabaseSync;
  private timer?: ReturnType<typeof setInterval>;
  private closing: Promise<void> | undefined;

  constructor(private readonly directory: string, private readonly env: Env) {
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    // A directory that already existed keeps its old mode otherwise. The database files inside rely on this directory being private.
    chmodSync(directory, 0o700);
    // Exactly one runtime owns the directory, including while async WebCrypto is in flight.
    this.lock = new DatabaseSync(join(directory, "home.sqlite"));
    try { this.lock.exec("BEGIN IMMEDIATE"); } catch (error) { this.lock.close(); throw error; }
  }

  namespace(name: string, implementation: unknown, create: readonly string[]): unknown {
    this.scopes.set(name, { type: implementation as ObjectConstructor, create: new Set(create) });
    return {
      idFromName: (id: string) => id,
      get: (id: string) => {
        if (!/^(?:[a-z]+:[0-9a-f]{16,64}|spool|store|meter)$/.test(id)) throw new Error("Invalid Home storage namespace");
        const key = `${name}--${id.replace(":", "-")}`;
        return new Proxy({}, {
          get: (_target, method) => typeof method === "string"
            ? (...args: unknown[]) => this.invoke(key, method, args)
            : undefined,
        });
      },
    };
  }

  start(): void {
    for (const file of readdirSync(this.directory)) {
      const match = /^([a-z]+--[a-z0-9-]+)\.sqlite$/.exec(file);
      const key = match?.[1];
      if (!key || !this.scope(key)) continue;
      const db = new DatabaseSync(this.path(key), { readOnly: true });
      try { this.recordAlarm(key, new Storage(db).alarmTime()); } finally { db.close(); }
    }
    this.timer = setInterval(() => {
      for (const [key, due] of this.alarms) {
        if (due > Date.now()) continue;
        // A failed cleanup is tried again later. Other resources keep their own schedule.
        this.alarms.set(key, Date.now() + 60_000);
        void this.invoke(key, "alarm", []).catch(() => undefined);
      }
    }, 1000);
    this.timer.unref();
  }

  close(): Promise<void> {
    return this.closing ??= (async () => {
      clearInterval(this.timer);
      for (const queue of this.wakeQueues.values()) for (const socket of queue.sockets) socket.terminate();
      await Promise.all(this.active.values());
      this.lock.close();
    })();
  }

  private path(key: string): string { return join(this.directory, `${key}.sqlite`); }

  private scope(key: string): Scope | undefined { return this.scopes.get(key.split("--", 1)[0] ?? ""); }

  private recordAlarm(key: string, time: number | undefined): void {
    if (time === undefined) this.alarms.delete(key);
    else this.alarms.set(key, time);
  }

  private queue(key: string): WakeQueue {
    const existing = this.wakeQueues.get(key);
    if (existing) return existing;
    const sockets = new Set<WakeSocket>();
    return {
      get sockets() { return sockets; },
      message: async (socket, data) => { await this.invoke(key, "webSocketMessage", [socket, data]); },
      release: () => { if (sockets.size === 0) this.wakeQueues.delete(key); },
    };
  }

  private async invoke(key: string, method: string, args: unknown[]): Promise<unknown> {
    if (this.closing) throw new Error("Home is stopping");
    const scope = this.scope(key);
    if (!scope) throw new Error("Unknown Home storage namespace");
    const prior = this.active.get(key) ?? Promise.resolve();
    const turn = prior.then(async () => {
      const path = this.path(key);
      // An unknown routing label, ticket or descriptor read is never permission to allocate a file.
      const persistent = scope.create.has(method) || existsSync(path);
      const db = new DatabaseSync(persistent ? path : ":memory:");
      const storage = new Storage(db);
      const queue = this.queue(key);
      try {
        db.exec("PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; PRAGMA auto_vacuum=FULL; BEGIN IMMEDIATE");
        // All calls on an object, including across awaits, are serialized. Promise resolution is after COMMIT.
        const obj = new scope.type(new LocalContext(storage, queue), this.env);
        const action = (obj as Record<string, ObjectMethod | undefined>)[method];
        if (!action) throw new Error("Unknown Home storage method");
        const effects: (() => void)[] = [];
        const result = await wakeEffects.run(effects, () => action.apply(obj, args));
        const alarm = persistent ? storage.alarmTime() : undefined;
        db.exec("COMMIT");
        this.recordAlarm(key, alarm);
        for (const effect of effects) {
          try { effect(); } catch { /* delivery is already durable; a wake is best effort */ }
        }
        if (queue.sockets.size > 0) this.wakeQueues.set(key, queue);
        return result;
      } finally {
        db.close(); // an exception before commit rolls the transaction back
      }
    });
    const done = turn.then(() => undefined, () => undefined);
    this.active.set(key, done);
    try { return await turn; } finally { if (this.active.get(key) === done) this.active.delete(key); }
  }
}
