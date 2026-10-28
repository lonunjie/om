// SPDX-License-Identifier: MPL-2.0
// KeyMaterialProvider role (ASYNC_KEY_MATERIAL.md, home-api.md section 3.3). One Durable Object per DeviceID. It stores the
// device's signed bundle and its one-time prekeys as opaque bytes, hands them out under a fetch throttle, and never
// decides whether a bundle is valid or fresh for a peer: clients verify signatures, the ACS binding and their own
// generation floor (PC-K05). The provider-side checks here are availability aids only.
import { DurableObject } from "cloudflare:workers";
import { Obj, HomeError, decode, encodeKeyFetchResponse, quotaDetail, type CborValue, type KeyPublishRequest } from "@om/home-wire";
import type { Env } from "./env";
import { LIMITS } from "./limits";
import { all, metaNumber, metaString, one, run, setMeta } from "./sql";
import { capHash, fingerprint, nowSeconds, reply, takeToken, type Reply } from "./util";

const SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS prekeys (id INTEGER PRIMARY KEY AUTOINCREMENT, kind INTEGER NOT NULL, item BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;

interface BundleFacts { readonly deviceId: Uint8Array; readonly generation: number; readonly expiresAt: number }

/** Best-effort structural read of a signed-AsyncKeyBundle: {1: bstr .cbor AsyncKeyBundle, 2: [signatures]}. Never verifies anything. */
export function readBundleFacts(bundle: Uint8Array): BundleFacts | undefined {
  try {
    const outer = new Obj(decode(bundle));
    const body = outer.bytes(1, 1, LIMITS.maxBundleBytes);
    const inner = decode(body);
    if (!(inner instanceof Map)) return undefined;
    const m = inner as ReadonlyMap<number | string, CborValue>;
    const [type, deviceId, generation, expiresAt] = [m.get(0), m.get(2), m.get(6), m.get(13)];
    if (type !== 9 || !(deviceId instanceof Uint8Array) || typeof generation !== "number" || typeof expiresAt !== "number") return undefined;
    return { deviceId, generation, expiresAt };
  } catch {
    return undefined;
  }
}

export class KeyPool extends DurableObject<Env> {
  private readonly sql: SqlStorage;

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }

  private get hard(): boolean {
    return this.env.OM_HARD_FREE_TIER === "1";
  }

  /** key_publish_cap: the first publish for a DeviceID claims it; every later publish/revoke must present the same capability. */
  private async publishAuth(cap: Uint8Array, claim: boolean): Promise<void> {
    const h = await capHash(cap);
    const owner = metaString(this.sql, "owner");
    if (owner === undefined) {
      if (!claim) throw new HomeError("unauthorized");
      setMeta(this.sql, "owner", h);
      return;
    }
    if (owner !== h) throw new HomeError("unauthorized");
  }

  publish(cap: Uint8Array, deviceId: Uint8Array, req: KeyPublishRequest): Promise<Reply> {
    return reply(async () => {
      const fp = await fingerprint(req.bundle, ...req.opks, ...req.pqopks);
      const rid = await fingerprint(req.requestId);
      const prior = one<{ fp: string }>(this.sql, "SELECT fp FROM ops WHERE request_id = ?", rid);
      await this.publishAuth(cap, true);
      if (prior) {
        if (prior.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
        return new Uint8Array(0);
      }
      const facts = readBundleFacts(req.bundle);
      const now = nowSeconds();
      if (facts) {
        if (facts.deviceId.length !== deviceId.length || facts.deviceId.some((b, i) => b !== deviceId[i])) throw new HomeError("invalid_request", { hint: "bundle is for another device" });
        if (facts.expiresAt + LIMITS.skewSeconds < now) throw new HomeError("expired");
        if (facts.generation < metaNumber(this.sql, "highest_generation")) throw new HomeError("stale_generation");
        setMeta(this.sql, "highest_generation", facts.generation);
        setMeta(this.sql, "bundle_generation", facts.generation);
      }
      const ec = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM prekeys WHERE kind = 1")?.n ?? 0;
      const pq = one<{ n: number }>(this.sql, "SELECT COUNT(*) AS n FROM prekeys WHERE kind = 2")?.n ?? 0;
      if (ec + req.opks.length > LIMITS.maxOneTimePerKind || pq + req.pqopks.length > LIMITS.maxOneTimePerKind) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      setMeta(this.sql, "bundle", req.bundle);
      for (const k of req.opks) run(this.sql, "INSERT INTO prekeys (kind, item) VALUES (1, ?)", k);
      for (const k of req.pqopks) run(this.sql, "INSERT INTO prekeys (kind, item) VALUES (2, ?)", k);
      run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
      run(this.sql, "INSERT INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
      return new Uint8Array(0);
    });
  }

  /**
   * Returns `[bundle, one-time item?, pq one-time item?]`. Consumption is throttled per device, so a stranger cannot
   * drain the pool faster than it refills; a throttled fetch gets the signed bundle without one-time items, which is
   * the specified degraded (not weaker) handshake.
   */
  fetchItems(): Promise<Reply> {
    return reply(() => {
      const bundle = getBundle(this.sql);
      if (!bundle) throw new HomeError("not_found");
      const now = nowSeconds();
      const items: Uint8Array[] = [bundle];
      const t = takeToken(metaNumber(this.sql, "tokens", LIMITS.fetchBurst), metaNumber(this.sql, "tokens_at", now), now, LIMITS.fetchBurst, 1 / LIMITS.fetchRefillSeconds);
      if (t.allowed) {
        for (const kind of [1, 2]) {
          const row = one<{ id: number; item: Uint8Array }>(this.sql, "SELECT id, item FROM prekeys WHERE kind = ? ORDER BY id LIMIT 1", kind);
          if (!row) continue;
          run(this.sql, "DELETE FROM prekeys WHERE id = ?", row.id);
          items.push(new Uint8Array(row.item));
        }
        setMeta(this.sql, "tokens", t.tokens);
        setMeta(this.sql, "tokens_at", now);
      }
      return Promise.resolve(encodeKeyFetchResponse({ items }));
    });
  }

  /** Best-effort cleanup after rotation or compromise: idempotent, and never evidence that an older generation is safe (PC-K06). */
  revokeGeneration(cap: Uint8Array, generation: number): Promise<Reply> {
    return reply(async () => {
      await this.publishAuth(cap, false);
      if (metaNumber(this.sql, "bundle_generation", -1) === generation) {
        run(this.sql, "DELETE FROM meta WHERE k = 'bundle'");
        run(this.sql, "DELETE FROM prekeys");
      }
      return new Uint8Array(0);
    });
  }

  inventory(): { ec: number; pq: number } {
    const rows = all<{ kind: number; n: number }>(this.sql, "SELECT kind, COUNT(*) AS n FROM prekeys GROUP BY kind");
    return { ec: rows.find((r) => r.kind === 1)?.n ?? 0, pq: rows.find((r) => r.kind === 2)?.n ?? 0 };
  }
}

function getBundle(sql: SqlStorage): Uint8Array | undefined {
  const r = one<{ v: Uint8Array | null }>(sql, "SELECT v FROM meta WHERE k = 'bundle'");
  return r?.v ? new Uint8Array(r.v) : undefined;
}
