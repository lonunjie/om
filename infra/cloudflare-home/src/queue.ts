// SPDX-License-Identifier: MPL-2.0
// One Durable Object per relay queue (SQLite-backed). It stores sealed ciphertext it cannot open, behind separate deposit
// and receive capabilities it holds only as hashes. It never learns who deposits, who drains, an AccountID or a name.
import { DurableObject } from "cloudflare:workers";
import {
  HomeError, concat, encodeDepositCapabilityRecord, encodeDepositReceipt, encodeDrainResponse, encodeRotateResponse, parseDeposit, quotaDetail,
  type DepositCapabilityRecord, type MintDepositCapabilityRequest, type QueueCreateRequest, type RelayAckRequest, type RelayDepositRequest, type RelayDrainRequest, type RelayRotateRequest,
} from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS } from "./limits";
import { depositCapStatus, receiveStatus, sealedExpiry } from "./rules";
import { MAILBOX_SCHEMA, ack, cursorOf, enqueue, nextExpiry, page, purgeExpired } from "./mailbox";
import { metaNumber, metaString, one, run, setMeta } from "./sql";
import { capHash, fingerprint, nowSeconds, randomBytes, reply, takeToken, type Reply } from "./util";

const SCHEMA = `${MAILBOX_SCHEMA}
CREATE TABLE IF NOT EXISTS dcaps (hash TEXT PRIMARY KEY, id BLOB NOT NULL, generation INTEGER NOT NULL, expires_at INTEGER, revoked INTEGER NOT NULL DEFAULT 0, max_items INTEGER, tokens REAL NOT NULL, tokens_at INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS dcaps_id ON dcaps (id);
`;

interface CapRow { hash: string; id: Uint8Array; generation: number; expires_at: number | null; revoked: number; max_items: number | null; tokens: number; tokens_at: number }

export class MailboxQueue extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }

  private get hard(): boolean {
    return this.env.OM_HARD_FREE_TIER === "1";
  }

  private exists(): boolean {
    return metaString(this.sql, "receive") !== undefined;
  }

  /** Idempotency: the same request_id and the same operation fingerprint returns the stored outcome; a different one is invalid_request. */
  private async replay(requestId: Uint8Array, fp: string): Promise<Uint8Array | undefined> {
    const rid = await fingerprint(requestId);
    const r = one<{ fp: string; result: Uint8Array }>(this.sql, "SELECT fp, result FROM ops WHERE request_id = ?", rid);
    if (!r) return undefined;
    if (r.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
    return new Uint8Array(r.result);
  }

  private async remember(requestId: Uint8Array, fp: string, result: Uint8Array, ttl: number): Promise<void> {
    const rid = await fingerprint(requestId);
    run(this.sql, "INSERT OR REPLACE INTO ops (request_id, fp, result, expires_at) VALUES (?, ?, ?, ?)", rid, fp, result, nowSeconds() + ttl);
    run(this.sql, "DELETE FROM ops WHERE rowid <= (SELECT MAX(rowid) FROM ops) - ?", LIMITS.maxReplayRows);
  }

  private async schedule(): Promise<void> {
    const t = nextExpiry(this.sql);
    if (t === undefined) { await this.ctx.storage.deleteAlarm(); return; }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1000, t * 1000 + 1000));
  }

  override async alarm(): Promise<void> {
    purgeExpired(this.sql, nowSeconds());
    run(this.sql, "DELETE FROM dcaps WHERE expires_at IS NOT NULL AND expires_at + ? < ? AND revoked = 0", LIMITS.skewSeconds, nowSeconds());
    await this.schedule();
  }

  // ---------------------------------------------------------------------------------------------------- create

  create(req: QueueCreateRequest): Promise<Reply> {
    return reply(async () => {
      const fp = await fingerprint(req.receiveCap, req.depositCap, req.adminCap ?? new Uint8Array(0));
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const [rh, dh, ah] = [await capHash(req.receiveCap), await capHash(req.depositCap), req.adminCap ? await capHash(req.adminCap) : undefined];
      if (this.exists()) {
        const same = metaString(this.sql, "receive") === rh && one(this.sql, "SELECT 1 AS x FROM dcaps WHERE hash = ? AND generation = 0", dh) !== undefined && (metaString(this.sql, "admin") ?? undefined) === ah;
        if (!same) throw new HomeError("unauthorized");
      } else {
        setMeta(this.sql, "receive", rh);
        setMeta(this.sql, "generation", 0);
        if (ah !== undefined) setMeta(this.sql, "admin", ah);
        run(this.sql, "INSERT INTO dcaps (hash, id, generation, tokens, tokens_at) VALUES (?, ?, 0, ?, ?)", dh, randomBytes(16), LIMITS.depositBurst, nowSeconds());
      }
      const out = new Uint8Array(0);
      await this.remember(req.requestId, fp, out, LIMITS.idempotencySeconds);
      return out;
    });
  }

  // ---------------------------------------------------------------------------------------------------- deposit

  deposit(req: RelayDepositRequest): Promise<Reply> {
    return reply(async () => {
      const bytes = req.envelope;
      const fp = await fingerprint(req.cap, bytes);
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const now = nowSeconds();
      // A forged, truncated or other-queue capability and a full queue answer identically (PC-A02): no existence oracle.
      const full = (): HomeError => new HomeError("quota", { detail: quotaDetail(1, this.hard) });
      if (!this.exists()) throw full();
      const cap = one<CapRow>(this.sql, "SELECT * FROM dcaps WHERE hash = ?", await capHash(req.cap));
      if (!cap) throw full();
      const status = depositCapStatus({ revoked: cap.revoked === 1, expiresAt: cap.expires_at, generation: cap.generation }, { current: metaNumber(this.sql, "generation"), overlapUntil: metaNumber(this.sql, "overlap_until") }, now);
      if (status !== "ok") throw new HomeError(status);
      if (bytes.length > LIMITS.maxDepositBytes) throw new HomeError("payload_too_large");
      let view;
      try { view = parseDeposit(bytes); } catch { throw new HomeError("invalid_request", { hint: "not a sealed delivery envelope or message request" }); }
      let expiresAt = now + LIMITS.defaultRetentionSeconds;
      let tupleKey: string | undefined;
      if (view.kind === "sealed") {
        const e = sealedExpiry(view.expiresAt, now);
        if (!e.ok) throw new HomeError(e.error);
        expiresAt = e.retainUntil;
        tupleKey = `${await fingerprint(view.deliveryKeyId)}:${await fingerprint(view.deliveryId)}`;
      }
      const wrapperFp = await fingerprint(bytes);
      const known = one(this.sql, "SELECT 1 AS x FROM seen WHERE fp = ?", wrapperFp) !== undefined;
      if (!known) {
        if (cap.max_items !== null) {
          const n = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM items WHERE cap_hash = ?", cap.hash)?.n ?? 0;
          if (n >= cap.max_items) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
        }
        const t = takeToken(cap.tokens, cap.tokens_at, now, LIMITS.depositBurst, LIMITS.depositRefillPerSecond);
        if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
        run(this.sql, "UPDATE dcaps SET tokens = ?, tokens_at = ? WHERE hash = ?", t.tokens, now, cap.hash);
      }
      purgeExpired(this.sql, now);
      const q = enqueue(this.sql, { bytes, fp: wrapperFp, tupleKey, expiresAt, capHash: cap.hash, hard: this.hard });
      const out = encodeDepositReceipt({ itemId: q.id, seq: q.seq });
      await this.remember(req.requestId, fp, out, LIMITS.idempotencySeconds);
      await this.schedule();
      return out;
    });
  }

  // ---------------------------------------------------------------------------------------------------- receive side

  /** Current receive capability always; the previous one only during the rotation overlap. */
  private async receiveAuth(cap: Uint8Array, forAck: boolean): Promise<void> {
    const h = await capHash(cap);
    if (!this.exists()) throw new HomeError("unauthorized");
    if (metaNumber(this.sql, "revoked") === 1) throw new HomeError("revoked");
    const s = receiveStatus(h, metaString(this.sql, "receive"), metaString(this.sql, "prev_receive"), metaNumber(this.sql, "overlap_until"), nowSeconds());
    if (s === "current" || s === "previous_in_overlap") return;
    if (s === "previous_expired") throw new HomeError(forAck ? "stale_generation" : "revoked");
    throw new HomeError("unauthorized");
  }

  drain(req: RelayDrainRequest): Promise<Reply> {
    return reply(async () => {
      await this.receiveAuth(req.cap, false);
      const now = nowSeconds();
      const p = page(this.sql, req.after ?? 0, now);
      const last = p.items.at(-1);
      return encodeDrainResponse({
        items: p.items.map((i) => ({ id: i.id, bytes: i.bytes, seq: i.seq })),
        ...(p.more && last ? { next: cursorOf(last.seq) } : {}),
      });
    });
  }

  ack(req: RelayAckRequest): Promise<Reply> {
    return reply(async () => {
      const fp = await fingerprint(req.cap, ...req.itemIds);
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      await this.receiveAuth(req.cap, true);
      if (ack(this.sql, req.itemIds) === 0) throw new HomeError("not_found");
      const out = new Uint8Array(0);
      await this.remember(req.requestId, fp, out, LIMITS.idempotencySeconds);
      await this.schedule();
      return out;
    });
  }

  rotate(req: RelayRotateRequest): Promise<Reply> {
    return reply(async () => {
      const fp = await fingerprint(req.cap);
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const h = await capHash(req.cap);
      if (!this.exists() || metaString(this.sql, "receive") !== h) {
        if (this.exists() && metaString(this.sql, "prev_receive") === h) throw new HomeError("stale_generation");
        throw new HomeError(metaNumber(this.sql, "revoked") === 1 ? "revoked" : "unauthorized");
      }
      const prefix = req.cap.slice(0, 16);
      const gen = metaNumber(this.sql, "generation") + 1;
      const receiveCap = concat(prefix, randomBytes(32));
      const depositCap = concat(prefix, randomBytes(32));
      const overlapUntil = nowSeconds() + LIMITS.rotateOverlapSeconds;
      setMeta(this.sql, "prev_receive", h);
      setMeta(this.sql, "receive", await capHash(receiveCap));
      setMeta(this.sql, "generation", gen);
      setMeta(this.sql, "overlap_until", overlapUntil);
      run(this.sql, "INSERT INTO dcaps (hash, id, generation, tokens, tokens_at) VALUES (?, ?, ?, ?, ?)", await capHash(depositCap), randomBytes(16), gen, LIMITS.depositBurst, nowSeconds());
      const out = encodeRotateResponse({ receiveCap, depositCap, overlapUntil });
      await this.remember(req.requestId, fp, out, LIMITS.rotateResultSeconds);
      return out;
    });
  }

  // ---------------------------------------------------------------------------------------------------- admin

  private async adminAuth(cap: Uint8Array): Promise<void> {
    const admin = metaString(this.sql, "admin");
    if (!this.exists() || admin === undefined || admin !== (await capHash(cap))) throw new HomeError("unauthorized");
  }

  mint(adminCap: Uint8Array, req: MintDepositCapabilityRequest): Promise<Reply> {
    return reply(async () => {
      await this.adminAuth(adminCap);
      const fp = await fingerprint(req.label, new TextEncoder().encode(`${String(req.ttlSeconds)}:${String(req.maxItems)}`));
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const count = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM dcaps WHERE revoked = 0")?.n ?? 0;
      const rows = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM dcaps")?.n ?? 0; // revoked tombstones count too
      if (count >= LIMITS.maxDepositCapabilities || rows >= LIMITS.maxCapabilityRows) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      const now = nowSeconds();
      const cap = concat(adminCapPrefix(adminCap), randomBytes(32));
      const id = randomBytes(16);
      const expiresAt = req.ttlSeconds === undefined ? null : now + Math.min(req.ttlSeconds, LIMITS.maxRetentionSeconds * 4);
      run(this.sql, "INSERT INTO dcaps (hash, id, generation, expires_at, max_items, tokens, tokens_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        await capHash(cap), id, metaNumber(this.sql, "generation"), expiresAt, req.maxItems ?? null, LIMITS.depositBurst, now);
      const rec: DepositCapabilityRecord = { id, cap, ...(expiresAt === null ? {} : { expiresAt }) };
      const out = encodeDepositCapabilityRecord(rec);
      await this.remember(req.requestId, fp, out, LIMITS.rotateResultSeconds);
      return out;
    });
  }

  revokeDepositCapability(adminCap: Uint8Array, id: Uint8Array): Promise<Reply> {
    return reply(async () => {
      await this.adminAuth(adminCap);
      run(this.sql, "UPDATE dcaps SET revoked = 1 WHERE id = ?", id); // unknown id: success-equivalent
      return new Uint8Array(0);
    });
  }
}

const adminCapPrefix = (cap: Uint8Array): Uint8Array => cap.slice(0, 16);
