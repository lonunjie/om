// SPDX-License-Identifier: MPL-2.0
// Descriptor store: signed ContactDescriptors and the introduction-service invite descriptors Core publishes. The body is
// opaque to the Home (clients verify signatures and bindings; a stored descriptor is never authority). One Durable Object
// per descriptor id. The first publish claims the id; a revoked id stays revoked (a tombstone) until the descriptor's own
// expiry, so a revoked invite cannot be republished by anyone who merely learns the id.
import { DurableObject } from "cloudflare:workers";
import { HomeError, encodeDescriptorGet, type DescriptorPutRequest } from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS } from "./limits";
import { metaNumber, metaString, one, run, setMeta } from "./sql";
import { capHash, fingerprint, nowSeconds, reply, takeToken, type Reply } from "./util";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;

export class DescriptorStore extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }

  private async schedule(): Promise<void> {
    const until = metaNumber(this.sql, "keep_until", 0);
    if (until > 0) await this.ctx.storage.setAlarm(until * 1000 + LIMITS.skewSeconds * 1000);
  }

  override async alarm(): Promise<void> {
    if (metaNumber(this.sql, "keep_until", 0) + LIMITS.skewSeconds <= nowSeconds()) {
      await this.ctx.storage.deleteAll(); // the descriptor and its tombstone are past their own expiry
    }
  }

  private async auth(cap: Uint8Array, claim: boolean): Promise<void> {
    const h = await capHash(cap);
    const owner = metaString(this.sql, "owner");
    if (owner === undefined) {
      if (!claim) throw new HomeError("unauthorized");
      setMeta(this.sql, "owner", h);
      return;
    }
    if (owner !== h) throw new HomeError("unauthorized");
  }

  put(cap: Uint8Array, req: DescriptorPutRequest): Promise<Reply> {
    return reply(async () => {
      const fp = await fingerprint(req.body, new TextEncoder().encode(String(req.expiresAt)));
      const rid = await fingerprint(req.requestId);
      const prior = one<{ fp: string }>(this.sql, "SELECT fp FROM ops WHERE request_id = ?", rid);
      await this.auth(cap, true);
      if (metaNumber(this.sql, "revoked") === 1) throw new HomeError("revoked");
      if (prior) {
        if (prior.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
        return new Uint8Array(0);
      }
      const now = nowSeconds();
      if (req.body.length > LIMITS.maxDescriptorBytes) throw new HomeError("payload_too_large");
      if (req.expiresAt + LIMITS.skewSeconds < now) throw new HomeError("expired");
      const expiresAt = Math.min(req.expiresAt, now + LIMITS.maxDescriptorSeconds);
      setMeta(this.sql, "body", req.body);
      setMeta(this.sql, "expires_at", expiresAt);
      setMeta(this.sql, "keep_until", expiresAt);
      run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
      run(this.sql, "INSERT INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
      await this.schedule();
      return new Uint8Array(0);
    });
  }

  /** Public read. Missing, expired and revoked are all `not_found`, so a reader learns nothing about a revoked invite. */
  get(): Promise<Reply> {
    return reply(() => {
      const now = nowSeconds();
      const t = takeToken(metaNumber(this.sql, "read_tokens", LIMITS.descriptorReadBurst), metaNumber(this.sql, "read_at", now), now, LIMITS.descriptorReadBurst, 1 / LIMITS.descriptorReadRefillSeconds);
      setMeta(this.sql, "read_tokens", t.tokens);
      setMeta(this.sql, "read_at", now);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      const body = one<{ v: Uint8Array | null }>(this.sql, "SELECT v FROM meta WHERE k = 'body'")?.v;
      const expiresAt = metaNumber(this.sql, "expires_at");
      if (!body || metaNumber(this.sql, "revoked") === 1 || expiresAt + LIMITS.skewSeconds < now) throw new HomeError("not_found");
      return Promise.resolve(encodeDescriptorGet({ body: new Uint8Array(body), expiresAt }));
    });
  }

  revoke(cap: Uint8Array): Promise<Reply> {
    return reply(async () => {
      await this.auth(cap, true); // revoking an id before its publish landed still tombstones it
      setMeta(this.sql, "revoked", 1);
      run(this.sql, "DELETE FROM meta WHERE k = 'body'");
      if (metaNumber(this.sql, "keep_until", 0) === 0) setMeta(this.sql, "keep_until", nowSeconds() + LIMITS.maxDescriptorSeconds);
      await this.schedule();
      return new Uint8Array(0);
    });
  }
}
