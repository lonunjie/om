import type { ConversationId, EncryptedKv, MessageId } from "@om/platform-api";
import { DEFAULT_LIMITS, MAX_TEXT, type StoredConversation, type StoredRow } from "./LocalSearchIndex";

/**
 * The search index at rest (ADR-0208). What the index holds is derived from decrypted messages, so it is never written as plain text:
 * every record goes through an `EncryptedKv` the host opened for this identity under the vault's search-index subkey, in the identity's own
 * vault database. It therefore goes away with the identity, is unreadable while the vault is locked, and is never sent anywhere.
 *
 * Layout (all keys under `ui/search/v2/`, a prefix Core never reads):
 *   m/<chat>        a manifest: name, position of the oldest kept message, newest message time, byte size, and per segment its hash and count
 *   s/<chat>/<n>/<digest>    an immutable segment at position n, identified by the SHA-256 of its encoded rows
 * A new message writes the newest segment and then publishes the manifest. The preceding manifest and its segments stay readable
 * until publication. The next load sweeps leftover segments.
 *
 * It is a cache: anything unreadable, unrecognised or over budget is dropped and rebuilt from Core's views, never repaired. Version
 * handling: a record of another format version under this prefix family is purged (older) or left alone (newer, written by a newer build).
 * Writes that fail (locked vault, full storage) are skipped; the live index is unaffected and the next change retries.
 */

export const SEARCH_PREFIX = "ui/search/";
const VERSION = 2;
const ROOT = `${SEARCH_PREFIX}v${String(VERSION)}/`;
const MANIFEST = `${ROOT}m/`;
const SEGMENTS = `${ROOT}s/`;
/** Messages per segment. */
export const SEGMENT = 256;
/** Bytes (of text, as stored) one identity's index may occupy at rest; the least recently active chats are dropped first. */
export const DEFAULT_MAX_BYTES = 24 * 1024 * 1024;

export interface SearchStoreOptions {
  /** Upper bound on bytes at rest (default {@link DEFAULT_MAX_BYTES}). */
  readonly maxBytes?: number;
  /** Messages per segment (default {@link SEGMENT}); tests shrink it. */
  readonly segment?: number;
}

export interface SearchStore {
  /**
   * Everything readable at rest, after sweeping what is not (unknown or damaged records, orphan segments, chats over budget).
   * Resolves to nothing, and changes nothing, while the vault cannot be read (locked).
   */
  load(): Promise<readonly StoredConversation[]>;
  /** Records one conversation (only the segments that changed are written). Calls queue and run in order. */
  save(c: StoredConversation): Promise<void>;
  /** Forgets one conversation at rest (it went away, or the person left the group). */
  remove(id: ConversationId): Promise<void>;
  /** Forgets everything at rest (the index is being reset). */
  wipe(): Promise<void>;
  /** Resolves when every queued call has run. */
  idle(): Promise<void>;
  /** Bytes the store believes it holds (for tests and diagnostics). */
  bytes(): number;
}

interface Manifest {
  readonly v: number;
  readonly name: string;
  readonly first: number;
  readonly last: number;
  readonly bytes: number;
  /** `[segment index, hash, row count, bytes]` in order. */
  readonly segs: readonly (readonly [number, string, number, number])[];
}

const enc = new TextEncoder();
const dec = new TextDecoder("utf-8", { fatal: true });
const CORRUPT = new Set(["malformed", "integrity_failure", "unsupported_version", "wrong_purpose"]);

async function hashOf(bytes: Uint8Array): Promise<string> {
  const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", new Uint8Array(bytes)));
  return Array.from(hash, (b) => b.toString(16).padStart(2, "0")).join("");
}

const isRow = (r: unknown): r is readonly [string, number, string, readonly string[]] =>
  Array.isArray(r) && r.length === 4 && typeof r[0] === "string" && typeof r[1] === "number" && Number.isFinite(r[1]) && typeof r[2] === "string" && r[2].length <= MAX_TEXT && Array.isArray(r[3]) && r[3].every((f) => typeof f === "string");

function parseManifest(value: unknown, per: number, cap: number): Manifest | undefined {
  if (typeof value !== "object" || value === null) return undefined;
  const m = value as Record<string, unknown>;
  if (m["v"] !== VERSION || typeof m["name"] !== "string" || !Number.isSafeInteger(m["first"]) || (m["first"] as number) < 0 || typeof m["last"] !== "number" || !Number.isSafeInteger(m["bytes"]) || !Array.isArray(m["segs"])) return undefined;
  const first = m["first"] as number;
  const bytes = m["bytes"] as number;
  if (!Number.isFinite(m["last"]) || bytes < 0 || bytes > cap || m["segs"].length > DEFAULT_LIMITS.perConversation) return undefined;
  const segs: [number, string, number, number][] = [];
  let position = first;
  let totalBytes = 0;
  for (const [index, s] of m["segs"].entries()) {
    if (!Array.isArray(s) || s.length !== 4 || !Number.isSafeInteger(s[0]) || typeof s[1] !== "string" || !Number.isSafeInteger(s[2]) || !Number.isSafeInteger(s[3])) return undefined;
    const [n, hash, count, size] = s as [number, string, number, number];
    const capacity = per - (position % per);
    if (n !== Math.floor(position / per) || !/^[a-f0-9]{64}$/.test(hash) || size < 1
      || count < 1 || count > capacity || (index < m["segs"].length - 1 && count !== capacity)) return undefined;
    segs.push([n, hash, count, size]);
    position += count;
    totalBytes += size;
  }
  if (!Number.isSafeInteger(position) || position - first > DEFAULT_LIMITS.perConversation || totalBytes !== bytes) return undefined;
  return { v: VERSION, name: m["name"], first, last: m["last"], bytes, segs };
}

const chatKey = (id: ConversationId): string => encodeURIComponent(id);
const manifestKey = (id: ConversationId): string => MANIFEST + chatKey(id);
const segmentKey = (id: ConversationId, [n, hash]: Manifest["segs"][number]): string => `${SEGMENTS}${chatKey(id)}/${String(n)}/${hash}`;

export function createSearchStore(kv: EncryptedKv, opts: SearchStoreOptions = {}): SearchStore {
  const maxBytes = opts.maxBytes ?? DEFAULT_MAX_BYTES;
  const per = Math.max(1, opts.segment ?? SEGMENT);
  /** What is at rest, by chat; filled by `load`. */
  const held = new Map<ConversationId, Manifest>();
  /** Chats dropped for budget in this visit: not written again until the next load (no write-then-evict loop). */
  const dropped = new Set<ConversationId>();
  let usable = false;
  let queue: Promise<unknown> = Promise.resolve();
  const run = <T>(fn: () => Promise<T>): Promise<T> => {
    const next = queue.then(fn, fn);
    queue = next.catch(() => undefined);
    return next;
  };

  const del = async (key: string): Promise<boolean> => { try { return (await kv.delete(key)).ok; } catch { return false; } };
  const total = (): number => { let n = 0; for (const m of held.values()) n += m.bytes; return n; };

  async function forget(id: ConversationId, m: Manifest | undefined): Promise<void> {
    if (!(await del(manifestKey(id)))) return;
    held.delete(id);
    for (const s of m?.segs ?? []) await del(segmentKey(id, s));
  }

  /** Drops the least recently active chats until the budget holds. */
  async function enforce(): Promise<void> {
    while (total() > maxBytes && held.size > 0) {
      let oldest: ConversationId | undefined;
      for (const [id, m] of held) if (oldest === undefined || m.last < (held.get(oldest)?.last ?? Infinity)) oldest = id;
      if (oldest === undefined) return;
      dropped.add(oldest);
      await forget(oldest, held.get(oldest));
      if (held.has(oldest)) return;
    }
  }

  async function readJson(key: string, segment?: Manifest["segs"][number]): Promise<{ readonly ok: true; readonly value: unknown } | { readonly ok: false; readonly corrupt: boolean } | { readonly ok: "none" }> {
    let got;
    try { got = await kv.get(key); } catch { return { ok: false, corrupt: false }; }
    if (!got.ok) return { ok: false, corrupt: CORRUPT.has(got.reason) };
    if (got.value === undefined) return { ok: "none" };
    try {
      if (segment !== undefined && (got.value.byteLength !== segment[3] || await hashOf(got.value) !== segment[1])) {
        return { ok: false, corrupt: true };
      }
      return { ok: true, value: JSON.parse(dec.decode(got.value)) as unknown };
    } catch { return { ok: false, corrupt: true }; }
  }

  async function doLoad(): Promise<readonly StoredConversation[]> {
    held.clear();
    dropped.clear();
    let keys: readonly string[];
    try {
      const listed = await kv.list();
      if (!listed.ok) { usable = false; return []; }
      keys = listed.keys.filter((k) => k.startsWith(SEARCH_PREFIX));
    } catch { usable = false; return []; }
    usable = true;
    // Another format version: older ones are purged (derived data is rebuilt, never migrated); a newer one belongs to a newer build.
    const manifests: string[] = [];
    const segments = new Map<string, string[]>();
    for (const k of keys) {
      if (k.startsWith(MANIFEST)) manifests.push(k);
      else if (k.startsWith(SEGMENTS)) {
        const rest = k.slice(SEGMENTS.length);
        const slash = rest.indexOf("/");
        if (slash > 0) segments.set(rest.slice(0, slash), [...(segments.get(rest.slice(0, slash)) ?? []), k]);
        else await del(k);
      } else {
        const v = /^ui\/search\/v(\d+)\//.exec(k)?.[1];
        if (v === undefined || Number(v) < VERSION) await del(k);
      }
    }
    const out: StoredConversation[] = [];
    for (const mk of manifests) {
      let id: ConversationId;
      try { id = decodeURIComponent(mk.slice(MANIFEST.length)) as ConversationId; } catch { await del(mk); continue; }
      const got = await readJson(mk);
      if (got.ok === false && !got.corrupt) { segments.delete(chatKey(id)); continue; } // unreadable right now (locked, storage): leave it
      const m = got.ok === true ? parseManifest(got.value, per, maxBytes) : undefined;
      if (m === undefined) { await forget(id, undefined); continue; } // damaged, unknown shape or no value: rebuilt later
      const rows: StoredRow[] = [];
      let intact = true;
      let unavailable = false;
      for (const s of m.segs) {
        const seg = await readJson(segmentKey(id, s), s);
        if (seg.ok === false && !seg.corrupt) { unavailable = true; break; }
        const list = seg.ok === true && Array.isArray(seg.value) ? (seg.value as unknown[]) : undefined;
        if (list === undefined || list.length !== s[2] || !list.every(isRow)) { intact = false; break; }
        for (const r of list as readonly (readonly [string, number, string, readonly string[]])[]) rows.push({ messageId: r[0] as MessageId, sentAt: r[1], text: r[2], files: r[3] });
      }
      if (unavailable) { segments.delete(chatKey(id)); continue; }
      if (!intact) { await forget(id, m); continue; }
      const named = new Set(m.segs.map((s) => segmentKey(id, s)));
      for (const k of segments.get(chatKey(id)) ?? []) if (!named.has(k)) await del(k); // a crashed write's leftover
      segments.delete(chatKey(id));
      held.set(id, m);
      out.push({ id, name: m.name, first: m.first, rows });
    }
    for (const orphans of segments.values()) for (const k of orphans) await del(k); // segments of a chat with no manifest
    await enforce();
    return out.filter((c) => held.has(c.id));
  }

  async function doSave(c: StoredConversation): Promise<void> {
    if (!usable || dropped.has(c.id) || c.rows.length > DEFAULT_LIMITS.perConversation) return;
    const before = held.get(c.id);
    const known = new Map<number, string>((before?.segs ?? []).map((s) => [s[0], s[1]]));
    const bySeg = new Map<number, StoredRow[]>();
    c.rows.forEach((r, j) => {
      const n = Math.floor((c.first + j) / per);
      const list = bySeg.get(n);
      if (list === undefined) bySeg.set(n, [r]); else list.push(r);
    });
    const segs: [number, string, number, number][] = [];
    const bodies = new Map<number, Uint8Array>();
    let bytes = 0;
    for (const n of [...bySeg.keys()].sort((a, b) => a - b)) {
      const rows = bySeg.get(n) ?? [];
      const text = JSON.stringify(rows.map((r) => [r.messageId, r.sentAt, r.text, r.files]));
      const body = enc.encode(text);
      bytes += body.length;
      if (bytes > maxBytes) {
        dropped.add(c.id);
        if (before !== undefined) await forget(c.id, before);
        return;
      }
      segs.push([n, await hashOf(body), rows.length, body.length]);
      bodies.set(n, body);
    }
    const last = c.rows.reduce((t, r) => Math.max(t, r.sentAt), 0);
    const next: Manifest = { v: VERSION, name: c.name, first: c.first, last, bytes, segs };
    const unchanged = before !== undefined && before.name === next.name && before.first === next.first && before.last === next.last && before.bytes === next.bytes
      && before.segs.length === segs.length && before.segs.every((s, i) => { const t = segs[i]; return t !== undefined && s[0] === t[0] && s[1] === t[1]; });
    if (unchanged) return;
    // An update stages at most one conversation's worth of bytes alongside the committed cache.
    const staged: string[] = [];
    const discard = async (): Promise<void> => { for (const key of staged) await del(key); };
    for (const s of segs) {
      if (known.get(s[0]) === s[1]) continue;
      const key = segmentKey(c.id, s);
      const r = await kv.put(key, bodies.get(s[0]) ?? new Uint8Array());
      if (!r.ok) { await discard(); return; }
      staged.push(key);
    }
    const r = await kv.put(manifestKey(c.id), enc.encode(JSON.stringify(next)));
    if (!r.ok) { await discard(); return; }
    held.set(c.id, next);
    const kept = new Set(segs.map((s) => segmentKey(c.id, s)));
    for (const s of before?.segs ?? []) {
      const key = segmentKey(c.id, s);
      if (!kept.has(key)) await del(key);
    }
    await enforce();
  }

  return {
    load: () => run(doLoad),
    save: (c) => run(() => doSave(c)),
    remove: (id) => run(async () => { if (!usable) return; dropped.delete(id); await forget(id, held.get(id)); }),
    wipe: () => run(async () => {
      if (!usable) return;
      for (const id of [...held.keys()]) await forget(id, held.get(id));
      held.clear();
      dropped.clear();
    }),
    idle: () => queue.then(() => undefined, () => undefined),
    bytes: total,
  };
}
