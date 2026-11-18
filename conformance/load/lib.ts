// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared parts of the Home load harness: Home launchers (workerd and the SQLite Home as its own process), a latency recorder,
// process memory and disk samplers, and the no-loss ledger. Both Homes are reached over real loopback HTTP and the same raw wire client.
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { ADMISSION_HEADER, admissionHash, toBase64Url } from "@om/home-wire";
import { startHome as startWorker, type Home } from "../../infra/cloudflare-home/test/harness";
import type { Result } from "../../infra/cloudflare-home/test/kit";

export const MIB = 1024 * 1024;
export type Target = "worker" | "sqlite";

export interface LoadHome {
  readonly target: Target;
  readonly url: string;
  readonly dir: string;
  readonly home: Home;
  /** Resident memory of the Home's own processes (workerd, or the Node Home), in bytes. */
  rss(): number;
  /** A clean stop. */
  stop(): Promise<void>;
  /** Stops without letting the Home close anything (SIGKILL) where the target is its own process; a clean stop otherwise. */
  kill(): Promise<void>;
  /** The disk the Home's data occupies. */
  diskBytes(): number;
  /** Starts again on the same data directory. */
  restart(): Promise<void>;
  /** Removes the data directory. */
  remove(): void;
}

export const owner = { secret: crypto.getRandomValues(new Uint8Array(32)) };

/** Adds the owner admission header to every request: queue creation, the blob spool and the recovery store need it. */
function admitted(h: Home): Home {
  const header = { [ADMISSION_HEADER]: toBase64Url(owner.secret) };
  // A request that is never answered is a finding, not a reason to hang the run: every call has a 30 s ceiling.
  return { ...h, fetch: (path, init) => h.fetch(path, { signal: AbortSignal.timeout(30_000), ...init, headers: { ...(init?.headers as Record<string, string> | undefined), ...header } }) };
}

const ps = (): { pid: number; ppid: number; rss: number; comm: string }[] =>
  execFileSync("ps", ["-A", "-o", "pid=,ppid=,rss=,comm="], { encoding: "utf8" }).split("\n").flatMap((line) => {
    const m = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/.exec(line);
    return m ? [{ pid: Number(m[1]), ppid: Number(m[2]), rss: Number(m[3]) * 1024, comm: m[4] ?? "" }] : [];
  });

/** Resident bytes of this process (the load client, plus Miniflare's host side for the worker target). */
export const clientRss = (): number => process.memoryUsage().rss;

export function dirBytes(dir: string): number {
  let total = 0;
  for (const e of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (e.isFile()) total += statSync(join(e.parentPath, e.name)).size;
  }
  return total;
}

/** Bytes on disk by kind of file: databases, write-ahead logs, shared-memory indexes, rollback journals. */
export function diskByKind(dir: string): Record<string, number> {
  const out: Record<string, number> = { database: 0, wal: 0, shm: 0, journal: 0 };
  for (const e of readdirSync(dir, { withFileTypes: true, recursive: true })) {
    if (!e.isFile()) continue;
    const size = statSync(join(e.parentPath, e.name)).size;
    const kind = e.name.endsWith("-wal") ? "wal" : e.name.endsWith("-shm") ? "shm" : e.name.endsWith("-journal") ? "journal" : "database";
    out[kind] = (out[kind] ?? 0) + size;
  }
  return out;
}

let sqliteBundle: string | undefined;
function bundle(): string {
  if (sqliteBundle) return sqliteBundle;
  const directory = mkdtempSync(join(tmpdir(), "om-load-bundle-"));
  const out = join(directory, "home.mjs");
  process.once("exit", () => { rmSync(directory, { recursive: true, force: true }); });
  execFileSync(process.execPath, [fileURLToPath(new URL("../../infra/sqlite-home/build.mjs", import.meta.url)), out]);
  sqliteBundle = out;
  return out;
}

export async function launch(target: Target, extraVars: Record<string, string> = {}): Promise<LoadHome> {
  const dir = mkdtempSync(join(tmpdir(), "om-load-data-"));
  const vars: Record<string, string> = { OM_ADMISSION: await admissionHash(owner.secret), OM_PROVIDER_ID: "om-load-home", ...extraVars };
  let home!: Home;
  let child: ChildProcess | undefined;

  const start = async (): Promise<void> => {
    if (target === "worker") {
      home = admitted(await startWorker({ persist: dir, vars }));
      return;
    }
    // The standalone entry forces admission on; the load harness sends the owner header on every request, as a real client does.
    const c = spawn(process.execPath, [fileURLToPath(new URL("./home-process.mjs", import.meta.url)), bundle(), dir, JSON.stringify({ OM_ADMISSION_REQUIRED: "1", OM_PROFILE: "default", ...vars })], { stdio: ["ignore", "pipe", "inherit"] });
    child = c;
    const url = await new Promise<string>((resolve, reject) => {
      let buf = "";
      c.stdout.on("data", (d: Buffer) => {
        buf += d.toString();
        const m = /LISTENING (\S+)/.exec(buf);
        if (m?.[1]) resolve(m[1]);
      });
      c.once("exit", (code) => { reject(new Error(`Home exited with ${String(code)}`)); });
    });
    const h: Home = { url, fetch: (path, init) => fetch(url + path, init), dispose: () => Promise.resolve() };
    home = admitted(h);
  };
  await start();

  const out: LoadHome = {
    target, dir,
    get url() { return home.url; },
    get home() { return home; },
    rss() {
      const all = ps();
      if (target === "sqlite") return all.find((p) => p.pid === child?.pid)?.rss ?? 0;
      const mine = new Set<number>([process.pid]);
      for (let grew = true; grew;) { grew = false; for (const p of all) if (!mine.has(p.pid) && mine.has(p.ppid)) { mine.add(p.pid); grew = true; } }
      return all.filter((p) => mine.has(p.pid) && /workerd/.test(p.comm)).reduce((s, p) => s + p.rss, 0);
    },
    async stop() {
      if (target === "worker") { await home.dispose(); return; }
      const c = child;
      if (!c) return;
      if (c.exitCode !== null || c.signalCode !== null) { child = undefined; return; }
      const done = new Promise<void>((r) => { c.once("exit", () => { r(); }); });
      c.kill("SIGTERM");
      await done;
      child = undefined;
    },
    async kill() {
      if (target === "worker") { await home.dispose(); return; }
      const c = child;
      if (!c) return;
      if (c.exitCode !== null || c.signalCode !== null) { child = undefined; return; }
      const done = new Promise<void>((r) => { c.once("exit", () => { r(); }); });
      c.kill("SIGKILL");
      await done;
      child = undefined;
    },
    diskBytes: () => dirBytes(dir),
    async restart() { await start(); },
    remove() { rmSync(dir, { recursive: true, force: true }); },
  };
  return out;
}

// ---- measurement -----------------------------------------------------------------------------------------------------------

export interface Percentiles { n: number; p50: number; p95: number; p99: number; max: number; mean: number }
export function percentiles(values: readonly number[]): Percentiles {
  if (values.length === 0) return { n: 0, p50: 0, p95: 0, p99: 0, max: 0, mean: 0 };
  const s = [...values].sort((a, b) => a - b);
  const at = (q: number): number => s[Math.min(s.length - 1, Math.max(0, Math.ceil(q * s.length) - 1))] ?? 0;
  const r = (x: number): number => Math.round(x * 100) / 100;
  return { n: s.length, p50: r(at(0.5)), p95: r(at(0.95)), p99: r(at(0.99)), max: r(s[s.length - 1] ?? 0), mean: r(s.reduce((a, b) => a + b, 0) / s.length) };
}

/** Latency (ms) and outcome counts per operation name. */
export class Recorder {
  readonly latency = new Map<string, number[]>();
  readonly outcomes = new Map<string, Map<string, number>>();

  async time<T>(op: string, fn: () => Promise<Result<T>>): Promise<Result<T>> {
    const t0 = performance.now();
    const r = await fn();
    const dt = performance.now() - t0;
    this.note(op, dt, r.ok ? "ok" : r.error.name);
    return r;
  }

  note(op: string, ms: number, outcome: string): void {
    let l = this.latency.get(op);
    if (!l) this.latency.set(op, (l = []));
    l.push(ms);
    let o = this.outcomes.get(op);
    if (!o) this.outcomes.set(op, (o = new Map()));
    o.set(outcome, (o.get(outcome) ?? 0) + 1);
  }

  summary(): Record<string, { latencyMs: Percentiles; outcomes: Record<string, number> }> {
    const out: Record<string, { latencyMs: Percentiles; outcomes: Record<string, number> }> = {};
    for (const [op, l] of this.latency) out[op] = { latencyMs: percentiles(l), outcomes: Object.fromEntries(this.outcomes.get(op) ?? []) };
    return out;
  }
}

export const sha = (b: Uint8Array): string => createHash("sha256").update(b).digest("hex");
export const sleep = (ms: number): Promise<void> => new Promise((r) => { setTimeout(r, ms); });
export const mb = (n: number): number => Math.round((n / MIB) * 10) / 10;

// ---- the no-loss ledger ----------------------------------------------------------------------------------------------------

/** What each mailbox must hold: every deposit the Home acknowledged and the client has not acked. */
export class Ledger {
  /** hash -> mailbox name, for deposits sent and not yet answered. */
  private readonly maybe = new Map<string, Set<string>>();
  private readonly pending = new Map<string, Set<string>>();
  private readonly acked = new Map<string, Set<string>>();
  readonly violations: string[] = [];
  deposits = 0;
  acks = 0;

  private set(m: Map<string, Set<string>>, q: string): Set<string> {
    let s = m.get(q);
    if (!s) m.set(q, (s = new Set()));
    return s;
  }
  sending(q: string, h: string): void { this.set(this.maybe, q).add(h); }
  stored(q: string, h: string): void { this.set(this.maybe, q).delete(h); this.set(this.pending, q).add(h); this.deposits++; }
  refused(q: string, h: string): void { this.set(this.maybe, q).delete(h); }
  /** Checks one drained item against the ledger. */
  seen(q: string, h: string): void {
    if (this.set(this.acked, q).has(h)) this.violations.push(`acked item came back in ${q}`);
    else if (!this.set(this.pending, q).has(h) && !this.set(this.maybe, q).has(h)) this.violations.push(`unknown or altered item in ${q}`);
  }
  ackedNow(q: string, h: string): void { this.set(this.pending, q).delete(h); this.set(this.maybe, q).delete(h); this.set(this.acked, q).add(h); this.acks++; }
  /** An ack is on its way: until it is answered the item may or may not still be held. */
  ackSending(q: string, h: string): void { this.set(this.pending, q).delete(h); this.set(this.maybe, q).add(h); }
  /** The ack was refused with a typed error: the item is still held. */
  ackRefused(q: string, h: string): void { this.set(this.maybe, q).delete(h); this.set(this.pending, q).add(h); }
  /** After a quiet read: what an unanswered request left behind is now known. */
  settle(q: string, got: ReadonlySet<string>): void {
    for (const h of this.set(this.maybe, q)) if (got.has(h)) this.set(this.pending, q).add(h);
    this.set(this.maybe, q).clear();
  }
  pendingOf(q: string): ReadonlySet<string> { return this.set(this.pending, q); }
  pendingTotal(): number { let n = 0; for (const s of this.pending.values()) n += s.size; return n; }
  /** After a quiet drain of `q`: every stored, unacked item must be there, byte for byte (by hash), once. */
  verify(q: string, hashes: readonly string[]): void {
    const got = new Set(hashes);
    if (got.size !== hashes.length) this.violations.push(`duplicate item in ${q}`);
    for (const h of hashes) this.seen(q, h);
    for (const h of this.pendingOf(q)) if (!got.has(h)) this.violations.push(`lost item in ${q}`);
    this.settle(q, got);
  }
}
