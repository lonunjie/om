// SPDX-License-Identifier: MPL-2.0
// Optional feature om.blob.spool.v1: a bounded temporary spool of client-side-encrypted blobs in one SQLite Durable Object
// (STORAGE_AND_BLOB_ARCHITECTURE "Zero-payment personal bootstrap"; no R2, no card). The Home sees ciphertext, its length and
// a digest, never a key, a name, a type or an identity. Creating a blob needs the owner's admission (the Worker checks it
// before this object is reached), exactly as creating a queue does; the minted read capability then lets a peer fetch it.
//
// Two capabilities per blob, both stored only as hashes: the owner `blob_cap` the sender chose (read and delete) and a read
// `blob_cap` the Home derives from it with a per-spool secret (read only). Deriving rather than storing it makes a retried
// put return the same capability without keeping the capability itself anywhere.
import { DurableObject } from "cloudflare:workers";
import { HomeError, blobCiphertextDigest, concat, encodeBlobPutResponse, equalBytes, quotaDetail, utf8, type BlobPutRequest } from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS } from "./limits";
import { all, one, run } from "./sql";
import { capHash, fingerprint, nowSeconds, randomBytes, reply, type Reply } from "./util";

/** One row of a SQLite Durable Object must stay under 2 MB; blobs are stored in chunks of this size. */
const CHUNK = 1024 * 1024;

const SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS blobs (id BLOB PRIMARY KEY, owner_hash TEXT NOT NULL, read_hash TEXT NOT NULL, len INTEGER NOT NULL, chunks INTEGER NOT NULL, digest BLOB NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS chunks (id BLOB NOT NULL, idx INTEGER NOT NULL, bytes BLOB NOT NULL, PRIMARY KEY (id, idx));
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;

interface BlobRow { id: Uint8Array; owner_hash: string; read_hash: string; len: number; chunks: number; digest: Uint8Array; expires_at: number }

export class BlobSpool extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }

  private get hard(): boolean {
    return this.env.OM_HARD_FREE_TIER === "1";
  }

  /** The per-spool secret the read capabilities derive from; created on first use and never leaves this object. */
  private readSecret(): Uint8Array {
    const row = one<{ v: Uint8Array | null }>(this.sql, "SELECT v FROM meta WHERE k = 'spool_key'");
    if (row?.v) return new Uint8Array(row.v);
    const key = randomBytes(32);
    run(this.sql, "INSERT INTO meta (k, v) VALUES ('spool_key', ?)", key);
    return key;
  }

  private async readCapFor(id: Uint8Array, ownerHash: string): Promise<Uint8Array> {
    const k = await crypto.subtle.importKey("raw", this.readSecret(), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const mac = new Uint8Array(await crypto.subtle.sign("HMAC", k, concat(utf8("OM-BLOB-READ-CAP-v1"), id, utf8(ownerHash))));
    return concat(id, mac);
  }

  private usage(): { count: number; bytes: number } {
    const r = one<{ n: number; b: number | null }>(this.sql, "SELECT COUNT(*) AS n, SUM(len) AS b FROM blobs");
    return { count: r?.n ?? 0, bytes: r?.b ?? 0 };
  }

  private purge(now: number): void {
    run(this.sql, "DELETE FROM chunks WHERE id IN (SELECT id FROM blobs WHERE expires_at + ? < ?)", LIMITS.skewSeconds, now);
    run(this.sql, "DELETE FROM blobs WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
    run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
  }

  private async schedule(): Promise<void> {
    const t = one<{ t: number | null }>(this.sql, "SELECT MIN(expires_at) + ? AS t FROM blobs", LIMITS.skewSeconds)?.t;
    if (t === null || t === undefined) { await this.ctx.storage.deleteAlarm(); return; }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1000, t * 1000 + 1000));
  }

  override async alarm(): Promise<void> {
    this.purge(nowSeconds());
    await this.schedule();
  }

  private drop(id: Uint8Array): void {
    run(this.sql, "DELETE FROM chunks WHERE id = ?", id);
    run(this.sql, "DELETE FROM blobs WHERE id = ?", id);
  }

  private async putResponse(row: BlobRow): Promise<Uint8Array> {
    return encodeBlobPutResponse({ readCap: await this.readCapFor(new Uint8Array(row.id), row.owner_hash), digest: new Uint8Array(row.digest), expiresAt: row.expires_at });
  }

  /**
   * Stores one ciphertext. `expected` is the digest the sender computed over it (OM-BLOB-CIPHERTEXT-HASH-v1); the Home
   * recomputes and refuses a mismatch, so a truncated or altered upload is never stored. The same owner capability with the
   * same bytes is an exact retry and returns the original answer; anything else under an existing locator is `invalid_request`.
   */
  put(req: BlobPutRequest, expected: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const len = req.ciphertext.length;
      if (len > LIMITS.maxBlobBytes) throw new HomeError("payload_too_large");
      let ttl = LIMITS.defaultRetentionSeconds;
      if (req.retentionSeconds !== undefined) {
        // Refused rather than shortened: a sender must never believe in a longer life than it gets (as for a sealed wrapper).
        if (req.retentionSeconds < 1 || req.retentionSeconds > LIMITS.maxRetentionSeconds) throw new HomeError("invalid_request", { hint: "retention outside this Home's bound" });
        ttl = req.retentionSeconds;
      }
      const id = req.cap.slice(0, 16);
      const ownerHash = await capHash(req.cap);
      const fp = await fingerprint(req.cap, expected);
      const rid = await fingerprint(req.requestId);
      const seen = one<{ fp: string }>(this.sql, "SELECT fp FROM ops WHERE request_id = ?", rid);
      if (seen && seen.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
      const now = nowSeconds();
      this.purge(now);
      const existing = one<BlobRow>(this.sql, "SELECT * FROM blobs WHERE id = ?", id);
      if (existing) {
        if (existing.owner_hash === ownerHash && equalBytes(new Uint8Array(existing.digest), expected)) return this.putResponse(existing);
        throw new HomeError("invalid_request", { hint: "locator in use" });
      }
      const got = await blobCiphertextDigest(req.ciphertext);
      if (!equalBytes(got, expected)) throw new HomeError("invalid_request", { hint: "ciphertext digest mismatch" });
      const u = this.usage();
      if (u.count >= LIMITS.maxBlobs || u.bytes + len > LIMITS.maxBlobSpoolBytes) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      const readCap = await this.readCapFor(id, ownerHash);
      const row: BlobRow = { id, owner_hash: ownerHash, read_hash: await capHash(readCap), len, chunks: Math.ceil(len / CHUNK), digest: got, expires_at: now + ttl };
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "INSERT INTO blobs (id, owner_hash, read_hash, len, chunks, digest, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)", row.id, row.owner_hash, row.read_hash, row.len, row.chunks, row.digest, row.expires_at);
        for (let i = 0; i < row.chunks; i++) run(this.sql, "INSERT INTO chunks (id, idx, bytes) VALUES (?, ?, ?)", id, i, req.ciphertext.slice(i * CHUNK, (i + 1) * CHUNK));
        run(this.sql, "INSERT OR REPLACE INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
        run(this.sql, "DELETE FROM ops WHERE rowid <= (SELECT MAX(rowid) FROM ops) - ?", LIMITS.maxReplayRows);
      });
      await this.schedule();
      return encodeBlobPutResponse({ readCap, digest: got, expiresAt: row.expires_at });
    });
  }

  /** The row a presented capability opens, and which kind it is. A wrong capability and an absent locator answer alike (`not_found`): no existence oracle. */
  private async open(locator: Uint8Array, cap: Uint8Array): Promise<{ row: BlobRow; owner: boolean }> {
    this.purge(nowSeconds());
    const row = one<BlobRow>(this.sql, "SELECT * FROM blobs WHERE id = ?", locator);
    const h = await capHash(cap);
    if (!row || !equalBytes(cap.slice(0, 16), locator)) throw new HomeError("not_found");
    const owner = row.owner_hash === h;
    if (!owner && row.read_hash !== h) throw new HomeError("not_found");
    return { row, owner };
  }

  /** The ciphertext, re-hashed against the digest stored at put: a blob that no longer matches is deleted and reported absent. */
  get(locator: Uint8Array, cap: Uint8Array): Promise<Reply> {
    return reply(async () => {
      const { row } = await this.open(locator, cap);
      if (nowSeconds() > row.expires_at) throw new HomeError("expired");
      const parts = all<{ bytes: Uint8Array }>(this.sql, "SELECT bytes FROM chunks WHERE id = ? ORDER BY idx", locator).map((c) => new Uint8Array(c.bytes));
      const bytes = concat(...parts);
      if (bytes.length !== row.len || !equalBytes(await blobCiphertextDigest(bytes), new Uint8Array(row.digest))) {
        this.drop(locator);
        await this.schedule();
        throw new HomeError("not_found");
      }
      return bytes;
    });
  }

  /** Owner only. Deleting a blob that is already gone is success-equivalent (home-api: idempotent). */
  remove(locator: Uint8Array, cap: Uint8Array): Promise<Reply> {
    return reply(async () => {
      let opened;
      try { opened = await this.open(locator, cap); } catch (e) { if (e instanceof HomeError && e.errorName === "not_found") return new Uint8Array(0); throw e; }
      if (!opened.owner) throw new HomeError("unauthorized"); // a valid read capability is not the owner's
      this.drop(locator);
      await this.schedule();
      return new Uint8Array(0);
    });
  }
}

