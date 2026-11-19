// SPDX-License-Identifier: AGPL-3.0-or-later
// Optional feature om.recovery.store.v1 (ADR-0201, home-api.md section 8): the RecoveryRecord store, the recovery inbox of one
// account, the veto board. One SQLite Durable Object for the whole Home, like the blob spool: the per-Home caps (records, inboxes)
// and the Home-wide rate buckets need one view, and one object keeps the Durable Object calls per request at one.
//
// The Home stays a dumb store. It holds ciphertext it cannot open, capability hashes, and signed objects whose shape the Worker
// checked; it verifies no signature, keeps no account state, returns no timestamp, and never evicts an item it holds (a full inbox
// keeps what it has and drops the new item). Posts are answered the same whether or not an inbox exists, and also when a per-inbox
// limit drops the item; only the Home-wide buckets answer `rate_limited`. Every capability comparison is over fixed-length
// SHA-256 digests with a constant-time compare. Bounded storage: records, inboxes, items, board rows and the replay table all have
// a hard row bound.
import { DurableObject } from "cloudflare:workers";
import {
  HomeError, INBOX_DRAIN_PAGE_MAX, concat, encodeInboxDrainResponse, encodeRecordGetResponse, encodeVetoList, equalBytes, quotaDetail, recoveryInboxId, recoveryLocator, recoveryRequestDigest, sha256, toHex, utf8, type InboxItem,
} from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS, limitsOf } from "./limits";
import { all, getMeta, metaNumber, one, run, setMeta } from "./sql";
import { nowSeconds, randomBytes, reply, takeToken, type Reply } from "./util";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS records (locator BLOB PRIMARY KEY, write_hash BLOB NOT NULL, env BLOB NOT NULL, etag BLOB NOT NULL, len INTEGER NOT NULL, put_tokens REAL NOT NULL, put_at INTEGER NOT NULL, get_tokens REAL NOT NULL, get_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS inboxes (inbox_id BLOB PRIMARY KEY, cap_hash BLOB NOT NULL, expires_at INTEGER NOT NULL, drain_tokens REAL NOT NULL, drain_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS items (seq INTEGER PRIMARY KEY AUTOINCREMENT, id BLOB NOT NULL UNIQUE, inbox_id BLOB NOT NULL, kind INTEGER NOT NULL, bytes BLOB NOT NULL, expires_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS items_inbox ON items (inbox_id, seq);
CREATE TABLE IF NOT EXISTS reqs (inbox_id BLOB NOT NULL, digest BLOB NOT NULL, body_hash BLOB NOT NULL, at INTEGER NOT NULL, expires_at INTEGER NOT NULL, read_tokens REAL NOT NULL, read_at INTEGER NOT NULL, PRIMARY KEY (inbox_id, digest));
CREATE INDEX IF NOT EXISTS reqs_digest ON reqs (digest);
CREATE TABLE IF NOT EXISTS vetoes (seq INTEGER PRIMARY KEY AUTOINCREMENT, inbox_id BLOB NOT NULL, digest BLOB NOT NULL, hash BLOB NOT NULL, bytes BLOB NOT NULL, expires_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS vetoes_digest ON vetoes (digest, seq);
CREATE TABLE IF NOT EXISTS veto_log (inbox_id BLOB NOT NULL, at INTEGER NOT NULL, controller INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS veto_read_buckets (bucket INTEGER PRIMARY KEY CHECK (bucket BETWEEN 0 AND 255), tokens REAL NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, result BLOB NOT NULL, expires_at INTEGER NOT NULL);
`;

const DAY_SECONDS = 86_400;
const HOUR_SECONDS = 3_600;
const EMPTY = new Uint8Array(0);

/** The record operations carry the proof (a capability) and the request; the Worker has already checked the shapes and, for a create, the owner's admission. */
export interface RecordPut { readonly locator: Uint8Array; readonly writeCap: Uint8Array; readonly requestId: Uint8Array; readonly envelope: Uint8Array; readonly create: boolean; readonly ifMatch?: Uint8Array }
export interface Fanout { readonly requestId: Uint8Array; readonly signed: Uint8Array; readonly accountId: Uint8Array; readonly recoveryEpoch: number; readonly body: Uint8Array }
export interface VetoPost { readonly requestId: Uint8Array; readonly signed: Uint8Array; readonly accountId: Uint8Array; readonly recoveryEpoch: number; readonly requestDigest: Uint8Array; readonly body: Uint8Array; readonly cap?: Uint8Array }

interface RecordRow { locator: Uint8Array; write_hash: Uint8Array; env: Uint8Array; etag: Uint8Array; len: number; put_tokens: number; put_at: number; get_tokens: number; get_at: number; expires_at: number }
interface InboxRow { inbox_id: Uint8Array; cap_hash: Uint8Array; expires_at: number; drain_tokens: number; drain_at: number }
const bytesOf = (v: Uint8Array): Uint8Array => new Uint8Array(v);

export class RecoveryStore extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
    if (!one(this.sql, "SELECT 1 FROM pragma_table_info('reqs') WHERE name = 'item_id'")) {
      run(this.sql, "ALTER TABLE reqs ADD COLUMN item_id BLOB");
    }
    if (!one(this.sql, "SELECT 1 FROM pragma_table_info('vetoes') WHERE name = 'controller'")) {
      run(this.sql, "ALTER TABLE vetoes ADD COLUMN controller INTEGER NOT NULL DEFAULT 0");
    }
    // Older log rows have no class. Charge them to the public window until they expire.
    if (!one(this.sql, "SELECT 1 FROM pragma_table_info('veto_log') WHERE name = 'controller'")) {
      run(this.sql, "ALTER TABLE veto_log ADD COLUMN controller INTEGER NOT NULL DEFAULT 0");
    }
    if (getMeta(this.sql, "veto_read_key") === undefined) {
      setMeta(this.sql, "veto_read_key", randomBytes(32));
    }
  }

  private get hard(): boolean {
    return this.env.OM_HARD_FREE_TIER === "1";
  }

  // ------------------------------------------------------------------------------------------------ housekeeping

  /** Drops what is past its retention. Records and inboxes go 400 days after their last replace or register; items and board rows after the mailbox retention. */
  private purge(now: number): void {
    const skew = LIMITS.skewSeconds;
    run(this.sql, "DELETE FROM items WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM vetoes WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM reqs WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM veto_log WHERE at < ?", now - HOUR_SECONDS);
    run(this.sql, "DELETE FROM records WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
    for (const gone of all<{ inbox_id: Uint8Array }>(this.sql, "SELECT inbox_id FROM inboxes WHERE expires_at + ? < ?", skew, now)) {
      for (const t of ["items", "reqs", "vetoes", "veto_log", "inboxes"]) run(this.sql, `DELETE FROM ${t} WHERE inbox_id = ?`, gone.inbox_id);
    }
  }

  /** One alarm at the earliest moment something is due to be purged. (Separate queries: the Durable Object SQLite build limits the terms of a compound SELECT.) */
  private async schedule(): Promise<void> {
    let t: number | undefined;
    for (const table of ["items", "vetoes", "reqs", "records", "inboxes"]) {
      const r = one<{ t: number | null }>(this.sql, `SELECT MIN(expires_at) AS t FROM ${table}`)?.t;
      if (r !== null && r !== undefined) t = Math.min(t ?? Infinity, r + LIMITS.skewSeconds);
    }
    const ops = one<{ t: number | null }>(this.sql, "SELECT MIN(expires_at) AS t FROM ops")?.t;
    if (ops !== null && ops !== undefined) t = Math.min(t ?? Infinity, ops);
    if (t === undefined) { await this.ctx.storage.deleteAlarm(); return; }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1000, t * 1000 + 1000));
  }

  override async alarm(): Promise<void> {
    this.purge(nowSeconds());
    await this.schedule();
  }

  /** The public scope id of this store (32 random bytes made on first use, stable for the life of the store). */
  scopeId(): Promise<Uint8Array> {
    const have = getMeta(this.sql, "scope_id");
    if (have !== undefined && have !== null && typeof have !== "string" && typeof have !== "number") return Promise.resolve(new Uint8Array(have));
    const fresh = randomBytes(32);
    setMeta(this.sql, "scope_id", fresh);
    return Promise.resolve(fresh);
  }

  /** The determined outcome of an earlier request with this id, if any; the same id with other bytes is `invalid_request` (home-api.md section 5). */
  private prior(rid: string, fp: string): Uint8Array | undefined {
    const p = one<{ fp: string; result: Uint8Array }>(this.sql, "SELECT fp, result FROM ops WHERE request_id = ?", rid);
    if (!p) return undefined;
    if (p.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
    return bytesOf(p.result);
  }

  private remember(rid: string, fp: string, result: Uint8Array, now: number): void {
    run(this.sql, "INSERT OR REPLACE INTO ops (request_id, fp, result, expires_at) VALUES (?, ?, ?, ?)", rid, fp, result, now + LIMITS.idempotencySeconds);
    run(this.sql, "DELETE FROM ops WHERE rowid <= (SELECT MAX(rowid) FROM ops) - ?", LIMITS.maxReplayRows);
  }

  /** A Home-wide bucket kept in `meta`; throws `rate_limited` with the wait when empty. */
  private homeBucket(name: string, now: number, burst: number, refillSeconds: number): void {
    const t = takeToken(metaNumber(this.sql, `${name}_tokens`, burst), metaNumber(this.sql, `${name}_at`, now), now, burst, 1 / refillSeconds);
    setMeta(this.sql, `${name}_tokens`, t.tokens);
    setMeta(this.sql, `${name}_at`, now);
    if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
  }

  // ------------------------------------------------------------------------------------------------ records

  /**
   * Put (create or compare-and-swap replace). Returns the new ETag (32 bytes). A create needs the owner's admission (checked by the
   * Worker) and binds SHA-256(write_cap) to the locator; a replace proves the bound capability and `If-Match`. A capability that
   * does not match and a record that is not there are the same `unauthorized`.
   */
  putRecord(a: RecordPut): Promise<Reply> {
    return reply(async () => {
      const writeHash = await sha256(a.writeCap);
      const etag = await sha256(a.envelope);
      const rid = toHex(await sha256(a.requestId));
      const fp = toHex(await sha256(utf8("recovery-put"), a.locator, writeHash, etag, new Uint8Array([a.create ? 1 : 0]), a.ifMatch ?? EMPTY));
      const now = nowSeconds();
      this.purge(now);
      const L = limitsOf(this.env);
      if (a.envelope.length > L.maxRecoveryRecordBytes) throw new HomeError("payload_too_large");
      const row = one<RecordRow>(this.sql, "SELECT * FROM records WHERE locator = ?", a.locator);
      const bound = row !== undefined && equalBytes(bytesOf(row.write_hash), writeHash);
      if (a.create) {
        if (row) {
          if (!bound) throw new HomeError("unauthorized");
          const done = this.prior(rid, fp);
          if (done) return done;
          throw new HomeError("stale_generation");
        }
        const done = this.prior(rid, fp);
        if (done) return done;
        if ((one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM records")?.n ?? 0) >= L.maxRecoveryRecords) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
        this.ctx.storage.transactionSync(() => {
          run(this.sql, "INSERT INTO records (locator, write_hash, env, etag, len, put_tokens, put_at, get_tokens, get_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            a.locator, writeHash, a.envelope, etag, a.envelope.length, LIMITS.recoveryPutBurst - 1, now, LIMITS.recoveryGetBurst, now, now + LIMITS.recoveryRecordRetentionSeconds);
          this.remember(rid, fp, etag, now);
        });
        await this.schedule();
        return etag;
      }
      if (!row || !bound) throw new HomeError("unauthorized");
      const done = this.prior(rid, fp);
      if (done) return done;
      if (a.ifMatch === undefined || !equalBytes(bytesOf(row.etag), a.ifMatch)) throw new HomeError("stale_generation");
      const t = takeToken(row.put_tokens, row.put_at, now, LIMITS.recoveryPutBurst, 1 / LIMITS.recoveryPutRefillSeconds);
      if (!t.allowed) {
        run(this.sql, "UPDATE records SET put_tokens = ?, put_at = ? WHERE locator = ?", t.tokens, now, a.locator);
        throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      }
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "UPDATE records SET env = ?, etag = ?, len = ?, put_tokens = ?, put_at = ?, expires_at = ? WHERE locator = ?", a.envelope, etag, a.envelope.length, t.tokens, now, now + LIMITS.recoveryRecordRetentionSeconds, a.locator);
        this.remember(rid, fp, etag, now);
      });
      await this.schedule();
      return etag;
    });
  }

  /**
   * Get. Returns `etag (32 bytes) || RecoveryRecordGetResponse`. The read capability is proved by DS-HASH(read_cap) = locator. A wrong
   * capability, an unknown locator and a locator whose record is not there are one answer (`not_found`), and each spends from the
   * Home-wide failed-get bucket, so guessing is bounded without any per-locator state for locators that do not exist.
   */
  getRecord(locator: Uint8Array, readCap: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const derived = await recoveryLocator(readCap);
      const now = nowSeconds();
      this.purge(now);
      const match = equalBytes(derived, locator);
      const row = match ? one<RecordRow>(this.sql, "SELECT * FROM records WHERE locator = ?", locator) : undefined;
      if (!row) {
        this.homeBucket("failed_get", now, LIMITS.recoveryFailedGetBurst, LIMITS.recoveryFailedGetRefillSeconds);
        throw new HomeError("not_found");
      }
      const t = takeToken(row.get_tokens, row.get_at, now, LIMITS.recoveryGetBurst, 1 / LIMITS.recoveryGetRefillSeconds);
      run(this.sql, "UPDATE records SET get_tokens = ?, get_at = ? WHERE locator = ?", t.tokens, now, locator);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      return concat(bytesOf(row.etag), encodeRecordGetResponse(bytesOf(row.env)));
    });
  }

  /** Delete; idempotent. A wrong capability and an absent record answer alike (`not_found`, which a client treats as success). */
  deleteRecord(locator: Uint8Array, writeCap: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const writeHash = await sha256(writeCap);
      const now = nowSeconds();
      this.purge(now);
      const row = one<RecordRow>(this.sql, "SELECT write_hash FROM records WHERE locator = ?", locator);
      if (!row || !equalBytes(bytesOf(row.write_hash), writeHash)) throw new HomeError("not_found");
      run(this.sql, "DELETE FROM records WHERE locator = ?", locator);
      await this.schedule();
      return EMPTY;
    });
  }

  // ------------------------------------------------------------------------------------------------ inbox

  /** Registers (or refreshes) the inbox of one account. The Worker has required the owner's admission. A different capability for a registered inbox is `unauthorized`. */
  registerInbox(inboxId: Uint8Array, cap: Uint8Array, requestId: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const capHash = await sha256(cap);
      const rid = toHex(await sha256(requestId));
      const fp = toHex(await sha256(utf8("recovery-register"), inboxId, capHash));
      const now = nowSeconds();
      this.purge(now);
      const L = limitsOf(this.env);
      const row = one<InboxRow>(this.sql, "SELECT * FROM inboxes WHERE inbox_id = ?", inboxId);
      if (row) {
        if (!equalBytes(bytesOf(row.cap_hash), capHash)) throw new HomeError("unauthorized");
        if (this.prior(rid, fp)) return EMPTY;
        this.remember(rid, fp, EMPTY, now);
        run(this.sql, "UPDATE inboxes SET expires_at = ? WHERE inbox_id = ?", now + LIMITS.recoveryInboxRetentionSeconds, inboxId);
        await this.schedule();
        return EMPTY;
      }
      if (this.prior(rid, fp)) return EMPTY;
      if ((one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM inboxes")?.n ?? 0) >= L.maxRecoveryInboxes) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "INSERT INTO inboxes (inbox_id, cap_hash, expires_at, drain_tokens, drain_at) VALUES (?, ?, ?, ?, ?)", inboxId, capHash, now + LIMITS.recoveryInboxRetentionSeconds, LIMITS.recoveryDrainBurst, now);
        this.remember(rid, fp, EMPTY, now);
      });
      await this.schedule();
      return EMPTY;
    });
  }

  /** The inbox a capability opens; an absent inbox and a wrong capability are the same `unauthorized`. */
  private async authInbox(inboxId: Uint8Array, cap: Uint8Array): Promise<InboxRow> {
    const capHash = await sha256(cap);
    const row = one<InboxRow>(this.sql, "SELECT * FROM inboxes WHERE inbox_id = ?", inboxId);
    const same = equalBytes(row ? bytesOf(row.cap_hash) : new Uint8Array(32), capHash);
    if (!row || !same) throw new HomeError("unauthorized");
    return row;
  }

  /** Items in arrival order, `{item_id, kind, bytes}` and no timestamp; ack removes. A cursor (the last sequence, 8 bytes) is present only when more items wait. */
  drain(inboxId: Uint8Array, cap: Uint8Array, pageSize: number | undefined): Promise<Reply> {
    return reply(async () => {
      const now = nowSeconds();
      this.purge(now);
      const row = await this.authInbox(inboxId, cap);
      const t = takeToken(row.drain_tokens, row.drain_at, now, LIMITS.recoveryDrainBurst, 1 / LIMITS.recoveryDrainRefillSeconds);
      run(this.sql, "UPDATE inboxes SET drain_tokens = ?, drain_at = ? WHERE inbox_id = ?", t.tokens, now, inboxId);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      const n = Math.min(pageSize ?? INBOX_DRAIN_PAGE_MAX, INBOX_DRAIN_PAGE_MAX);
      const rows = all<{ seq: number; id: Uint8Array; kind: number; bytes: Uint8Array }>(this.sql, "SELECT seq, id, kind, bytes FROM items WHERE inbox_id = ? ORDER BY seq LIMIT ?", inboxId, n + 1);
      const page = rows.slice(0, n);
      const items: InboxItem[] = page.map((r) => ({ id: bytesOf(r.id), kind: r.kind === 2 ? 2 : 1, bytes: bytesOf(r.bytes) }));
      const last = page[page.length - 1];
      let cursor: Uint8Array | undefined;
      if (rows.length > n && last) {
        cursor = new Uint8Array(8);
        new DataView(cursor.buffer).setBigUint64(0, BigInt(last.seq));
      }
      return encodeInboxDrainResponse({ items, ...(cursor ? { cursor } : {}) });
    });
  }

  /** Removes the named items; an id that is not there is success-equivalent. */
  ack(inboxId: Uint8Array, cap: Uint8Array, ids: readonly Uint8Array[]): Promise<Reply> {
    return reply(async () => {
      this.purge(nowSeconds());
      await this.authInbox(inboxId, cap);
      for (const id of ids) run(this.sql, "DELETE FROM items WHERE inbox_id = ? AND id = ?", inboxId, id);
      await this.schedule();
      return EMPTY;
    });
  }

  // ------------------------------------------------------------------------------------------------ fan-out and veto (shape-checked posts)

  /**
   * Appends a signed request to the inbox of its account when one is registered. Always `Empty`: the answer is the same whether or not
   * an inbox exists, and when a per-inbox limit or a full inbox drops the item. Only the Home-wide post bucket answers `rate_limited`.
   */
  fanout(a: Fanout): Promise<Reply> {
    return reply(async () => {
      const signedHash = await sha256(a.signed);
      const digest = await recoveryRequestDigest(a.body);
      const inboxId = await recoveryInboxId(a.accountId, a.recoveryEpoch);
      const rid = toHex(await sha256(a.requestId));
      const fp = toHex(await sha256(utf8("recovery-fanout"), signedHash));
      const now = nowSeconds();
      this.purge(now);
      if (this.prior(rid, fp)) return EMPTY;
      this.homeBucket("post", now, LIMITS.recoveryPostBurst, LIMITS.recoveryPostRefillSeconds);
      const expires = now + LIMITS.maxRetentionSeconds;
      const changed = this.ctx.storage.transactionSync(() => {
        this.remember(rid, fp, EMPTY, now);
        if (!one(this.sql, "SELECT 1 FROM inboxes WHERE inbox_id = ?", inboxId)) return false;
        const held = one<{ body_hash: Uint8Array; expires_at: number; item_id: Uint8Array | null }>(
          this.sql, "SELECT body_hash, expires_at, item_id FROM reqs WHERE inbox_id = ? AND digest = ?", inboxId, digest,
        );
        if (held && (!equalBytes(bytesOf(held.body_hash), signedHash) || expires - held.expires_at < DAY_SECONDS)) return false;
        const existing = held?.item_id
          ? one<{ id: Uint8Array }>(this.sql, "SELECT id FROM items WHERE id = ? AND inbox_id = ?", bytesOf(held.item_id), inboxId)
          : held && one<{ id: Uint8Array }>(this.sql, "SELECT id FROM items WHERE inbox_id = ? AND kind = 1 AND bytes = ?", inboxId, a.signed);
        if (existing) {
          const id = bytesOf(existing.id);
          run(this.sql, "UPDATE items SET expires_at = ? WHERE id = ? AND inbox_id = ?", expires, id, inboxId);
          run(this.sql, "UPDATE reqs SET expires_at = ?, item_id = ? WHERE inbox_id = ? AND digest = ?", expires, id, inboxId, digest);
          return true;
        }
        const pending = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM items WHERE inbox_id = ? AND kind = 1", inboxId)?.n ?? 0;
        if (pending >= LIMITS.recoveryPendingRequests) return false;
        if (!held) {
          const counts = one<{ total: number; recent: number }>(this.sql, "SELECT COUNT(*) AS total, COUNT(CASE WHEN at > ? THEN 1 END) AS recent FROM reqs WHERE inbox_id = ?", now - DAY_SECONDS, inboxId);
          const maxHeld = LIMITS.recoveryRequestsPerDay * Math.ceil((LIMITS.maxRetentionSeconds + LIMITS.skewSeconds) / DAY_SECONDS);
          if (counts && (counts.recent >= LIMITS.recoveryRequestsPerDay || counts.total >= maxHeld)) return false;
        }
        const itemId = randomBytes(16);
        run(this.sql, "INSERT INTO reqs (inbox_id, digest, body_hash, at, expires_at, read_tokens, read_at, item_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(inbox_id, digest) DO UPDATE SET expires_at = excluded.expires_at, item_id = excluded.item_id", inboxId, digest, signedHash, now, expires, LIMITS.recoveryVetoReadBurst, now, itemId);
        run(this.sql, "INSERT INTO items (id, inbox_id, kind, bytes, expires_at) VALUES (?, ?, 1, ?, ?)", itemId, inboxId, a.signed, expires);
        return true;
      });
      if (changed) await this.schedule();
      return EMPTY;
    });
  }

  /**
   * Accepts a veto only when the inbox of its account holds, or has held within the item retention and since had acknowledged, a
   * request with that digest (the digest is kept after an ack so a controller that drained a request can still veto it). Otherwise it is
   * dropped and answered like a success.
   */
  postVeto(a: VetoPost): Promise<Reply> {
    return reply(async () => {
      const hash = await sha256(a.body);
      const inboxId = await recoveryInboxId(a.accountId, a.recoveryEpoch);
      const capHash = a.cap === undefined ? undefined : await sha256(a.cap);
      const rid = toHex(await sha256(a.requestId));
      const fp = toHex(await sha256(utf8("recovery-veto"), hash));
      const now = nowSeconds();
      this.purge(now);
      if (this.prior(rid, fp)) return EMPTY;
      this.homeBucket("post", now, LIMITS.recoveryPostBurst, LIMITS.recoveryPostRefillSeconds);
      const changed = this.ctx.storage.transactionSync(() => {
        this.remember(rid, fp, EMPTY, now);
        const inbox = one<{ cap_hash: Uint8Array }>(this.sql, "SELECT cap_hash FROM inboxes WHERE inbox_id = ?", inboxId);
        if (!inbox) return false;
        const controller = Number(capHash !== undefined && equalBytes(bytesOf(inbox.cap_hash), capHash));
        if (!one(this.sql, "SELECT 1 FROM reqs WHERE inbox_id = ? AND digest = ?", inboxId, a.requestDigest)) return false;
        if (one(this.sql, "SELECT 1 FROM vetoes WHERE digest = ? AND hash = ? AND controller = ?", a.requestDigest, hash, controller)) return false;
        const n = (q: string, ...b: (Uint8Array | number)[]): number => one<{ n: number }>(this.sql, q, ...b)?.n ?? 0;
        const hourly = controller ? LIMITS.recoveryControllerVetoesPerHour : LIMITS.recoveryVetoesPerHour;
        if (n("SELECT COUNT(*) AS n FROM veto_log WHERE inbox_id = ? AND controller = ? AND at > ?", inboxId, controller, now - HOUR_SECONDS) >= hourly) return false;
        const capacity = controller ? LIMITS.recoveryControllerVetoesPerDigest : LIMITS.recoveryPendingVetoesPerDigest;
        if (n("SELECT COUNT(*) AS n FROM vetoes WHERE digest = ? AND controller = ?", a.requestDigest, controller) >= capacity) return false;
        const inboxFull = n("SELECT COUNT(*) AS n FROM items WHERE inbox_id = ? AND kind = 2", inboxId) >= LIMITS.recoveryPendingVetoes;
        if (!controller && (inboxFull || n("SELECT COUNT(*) AS n FROM vetoes WHERE inbox_id = ? AND controller = 0", inboxId) >= LIMITS.recoveryPendingVetoes)) return false;
        const expires = now + LIMITS.maxRetentionSeconds;
        run(this.sql, "INSERT INTO veto_log (inbox_id, at, controller) VALUES (?, ?, ?)", inboxId, now, controller);
        run(this.sql, "INSERT INTO vetoes (inbox_id, digest, hash, bytes, expires_at, controller) VALUES (?, ?, ?, ?, ?, ?)", inboxId, a.requestDigest, hash, a.signed, expires, controller);
        if (!inboxFull) run(this.sql, "INSERT INTO items (id, inbox_id, kind, bytes, expires_at) VALUES (?, ?, 2, ?, ?)", randomBytes(16), inboxId, a.signed, expires);
        return true;
      });
      if (changed) await this.schedule();
      return EMPTY;
    });
  }

  /** Pace all board reads by the same keyed 256-bucket rule, then return controller entries first (ADR-0226 §§3–4). */
  vetoes(digest: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const secret = getMeta(this.sql, "veto_read_key");
      if (secret === undefined || secret === null || typeof secret === "string" || typeof secret === "number") throw new HomeError("retryable");
      const key = await crypto.subtle.importKey("raw", bytesOf(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
      const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, bytesOf(digest)));
      const bucket = mac[0] ?? 0;
      const now = nowSeconds();
      this.purge(now);
      const burst = LIMITS.recoveryVetoReadBurst;
      const row = one<{ tokens: number; at: number }>(this.sql, "SELECT tokens, at FROM veto_read_buckets WHERE bucket = ?", bucket);
      const t = takeToken(row?.tokens ?? burst, row?.at ?? now, now, burst, 1 / LIMITS.recoveryVetoReadRefillSeconds);
      run(this.sql, "INSERT INTO veto_read_buckets (bucket, tokens, at) VALUES (?, ?, ?) ON CONFLICT(bucket) DO UPDATE SET tokens = excluded.tokens, at = excluded.at", bucket, t.tokens, now);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      const list = all<{ bytes: Uint8Array }>(this.sql, "SELECT bytes FROM vetoes WHERE digest = ? ORDER BY controller DESC, seq LIMIT ?", digest, LIMITS.recoveryPendingVetoesPerDigest + LIMITS.recoveryControllerVetoesPerDigest).map((r) => bytesOf(r.bytes));
      return encodeVetoList(list);
    });
  }
}
