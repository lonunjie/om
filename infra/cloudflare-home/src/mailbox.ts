// SPDX-License-Identifier: MPL-2.0
// Item storage shared by the relay queue and the IntroGate accepted-request queue: durable, ordered by a per-queue
// sequence, bounded by count and bytes, with explicit acknowledgement and a replay set that outlives the item.
import { HomeError, quotaDetail } from "@om/home-wire";
import { LIMITS } from "./limits";
import { all, metaNumber, one, run, setMeta } from "./sql";

export const MAILBOX_SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS items (seq INTEGER PRIMARY KEY, id BLOB NOT NULL, bytes BLOB NOT NULL, len INTEGER NOT NULL, fp TEXT NOT NULL, expires_at INTEGER NOT NULL, cap_hash TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS items_id ON items (id);
CREATE TABLE IF NOT EXISTS seen (fp TEXT PRIMARY KEY, tuple_key TEXT, item_id BLOB NOT NULL, seq INTEGER NOT NULL, expires_at INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS seen_tuple ON seen (tuple_key) WHERE tuple_key IS NOT NULL;
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, result BLOB NOT NULL, expires_at INTEGER NOT NULL);
`;

export interface StoredItem { readonly seq: number; readonly id: Uint8Array; readonly bytes: Uint8Array; readonly expires_at: number }

export function usage(sql: SqlStorage): { items: number; bytes: number } {
  const r = one<{ n: number; b: number | null }>(sql, "SELECT COUNT(*) AS n, SUM(len) AS b FROM items");
  return { items: r?.n ?? 0, bytes: r?.b ?? 0 };
}

export function purgeExpired(sql: SqlStorage, now: number): void {
  run(sql, "DELETE FROM items WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
  run(sql, "DELETE FROM seen WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
  run(sql, "DELETE FROM ops WHERE expires_at < ?", now);
}

/** Earliest moment something in the mailbox needs purging, or undefined when empty. */
export function nextExpiry(sql: SqlStorage): number | undefined {
  const r = one<{ t: number | null }>(sql, `SELECT MIN(t) AS t FROM (
    SELECT MIN(expires_at) + ${String(LIMITS.skewSeconds)} AS t FROM items UNION ALL SELECT MIN(expires_at) + ${String(LIMITS.skewSeconds)} FROM seen UNION ALL SELECT MIN(expires_at) FROM ops)`);
  return r?.t ?? undefined;
}

export interface Enqueued { readonly id: Uint8Array; readonly seq: number; readonly duplicate: boolean }

/**
 * Stores one deposit exactly once. An exact retry (same bytes) returns the original item and sequence; a different
 * object under an already-seen delivery tuple is `invalid_request` (the sealed-delivery replay rule). Count and byte
 * quotas apply only to new items, so a retry never fails for lack of room.
 */
export function enqueue(sql: SqlStorage, input: { bytes: Uint8Array; fp: string; tupleKey: string | undefined; expiresAt: number; capHash: string | undefined; hard: boolean }): Enqueued {
  const prior = one<{ item_id: Uint8Array; seq: number }>(sql, "SELECT item_id, seq FROM seen WHERE fp = ?", input.fp);
  if (prior) return { id: new Uint8Array(prior.item_id), seq: prior.seq, duplicate: true };
  if (input.tupleKey !== undefined && one(sql, "SELECT 1 AS x FROM seen WHERE tuple_key = ?", input.tupleKey)) throw new HomeError("invalid_request", { hint: "replayed delivery id" });
  const u = usage(sql);
  if (u.items >= LIMITS.maxQueueItems) throw new HomeError("quota", { detail: quotaDetail(1, input.hard) });
  if (u.bytes + input.bytes.length > LIMITS.maxQueueBytes) throw new HomeError("quota", { detail: quotaDetail(2, input.hard) });
  const seq = metaNumber(sql, "next_seq", 1);
  const id = new Uint8Array(16);
  crypto.getRandomValues(id);
  setMeta(sql, "next_seq", seq + 1);
  run(sql, "INSERT INTO items (seq, id, bytes, len, fp, expires_at, cap_hash) VALUES (?, ?, ?, ?, ?, ?, ?)", seq, id, input.bytes, input.bytes.length, input.fp, input.expiresAt, input.capHash ?? null);
  run(sql, "INSERT INTO seen (fp, tuple_key, item_id, seq, expires_at) VALUES (?, ?, ?, ?, ?)", input.fp, input.tupleKey ?? null, id, seq, input.expiresAt);
  // The replay set is an availability aid (the recipient endpoint is the replay authority, SEALED_SENDER_DELIVERY_PROFILE.md section 7); keep it bounded.
  run(sql, "DELETE FROM seen WHERE seq <= ?", seq - LIMITS.maxReplayRows);
  return { id, seq, duplicate: false };
}

/** Items with sequence greater than `after`, at most one page; `more` says whether a further page exists. */
export function page(sql: SqlStorage, after: number, now: number): { items: StoredItem[]; more: boolean } {
  const rows = all<{ seq: number; id: Uint8Array; bytes: Uint8Array; expires_at: number; len: number }>(
    sql, "SELECT seq, id, bytes, expires_at, len FROM items WHERE seq > ? AND expires_at + ? >= ? ORDER BY seq LIMIT ?",
    after, LIMITS.skewSeconds, now, LIMITS.drainPageItems + 1);
  const items: StoredItem[] = [];
  let bytes = 0;
  let more = false;
  for (const r of rows) {
    if (items.length >= LIMITS.drainPageItems || (items.length > 0 && bytes + r.len > LIMITS.drainPageBytes)) { more = true; break; }
    bytes += r.len;
    items.push({ seq: r.seq, id: new Uint8Array(r.id), bytes: new Uint8Array(r.bytes), expires_at: r.expires_at });
  }
  return { items, more };
}

/** Deletes acknowledged items. Returns how many ids were known (pending or already acknowledged). */
export function ack(sql: SqlStorage, ids: readonly Uint8Array[]): number {
  let known = 0;
  for (const id of ids) {
    const had = one(sql, "SELECT 1 AS x FROM items WHERE id = ?", id) !== undefined;
    if (had) run(sql, "DELETE FROM items WHERE id = ?", id);
    if (had || one(sql, "SELECT 1 AS x FROM seen WHERE item_id = ?", id) !== undefined) known += 1;
  }
  return known;
}

export const cursorOf = (seq: number): Uint8Array => {
  const b = new Uint8Array(8);
  new DataView(b.buffer).setBigUint64(0, BigInt(seq));
  return b;
};
