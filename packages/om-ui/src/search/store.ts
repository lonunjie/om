import type { ConversationId, EncryptedKv, MessageId } from "@om/platform-api";
import type { StoredConversation, StoredRow } from "./LocalSearchIndex";

/**
 * The search index at rest (ADR-0208). What the index holds is derived from decrypted messages, so it is never written as plain text:
 * every record goes through an `EncryptedKv` the host opened for this identity under the vault's search-index subkey, in the identity's own
 * vault database. It therefore goes away with the identity, is unreadable while the vault is locked, and is never sent anywhere.
 *
 * Layout (all keys under `ui/search/v1/`, a prefix Core never reads):
 *   m/<chat>        a manifest: name, position of the oldest kept message, newest message time, byte size, and per segment its hash and count
 *   s/<chat>/<n>    a segment: the rows whose position in the whole timeline falls in [n * SEGMENT, (n + 1) * SEGMENT)
 * A new message therefore rewrites only the newest segment and the manifest. Segments are written before the manifest that names them,
 * and a segment no manifest names is swept at the next load, so a crash at any point leaves a usable store.
 *
 * It is a cache: anything unreadable, unrecognised or over budget is dropped and rebuilt from Core's views, never repaired. Version
 * handling: a record of another format version under this prefix family is purged (older) or left alone (newer, written by a newer build).
 * Writes that fail (locked vault, full storage) are skipped; the live index is unaffected and the next change retries.
 */

export const SEARCH_PREFIX = "ui/search/";
const VERSION = 1;
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

/** A small non-cryptographic hash, used only to notice that a segment changed (the record itself is authenticated by the vault). */
function hashOf(text: string): string {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < text.length; i++) {
    const ch = text.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 2654435761);
    h2 = Math.imul(h2 ^ ch, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(16);
}

const isRow = (r: unknown): r is readonly [string, number, string, readonly string[]] =>
  Array.isArray(r) && r.length === 4 && typeof r[0] === "string" && typeof r[1] === "number" && Number.isFinite(r[1]) && typeof r[2] === "string" && Array.isArray(r[3]) && r[3].every((f) => typeof f === "string");

function parseManifest(value: unknown): Manifest | undefined {
  if (typeof value !== "object" || value === null) return undefined;
  const m = value as Record<string, unknown>;
  if (m["v"] !== VERSION || typeof m["name"] !== "string" || !Number.isSafeInteger(m["first"]) || (m["first"] as number) < 0 || typeof m["last"] !== "number" || !Number.isSafeInteger(m["bytes"]) || !Array.isArray(m["segs"])) return undefined;
  const segs: [number, string, number, number][] = [];
  for (const s of m["segs"] as unknown[]) {
    if (!Array.isArray(s) || s.length !== 4 || !Number.isSafeInteger(s[0]) || typeof s[1] !== "string" || !Number.isSafeInteger(s[2]) || !Number.isSafeInteger(s[3])) return undefined;
    segs.push([s[0] as number, s[1], s[2] as number, s[3] as number]);
  }
  return { v: VERSION, name: m["name"], first: m["first"] as number, last: m["last"], bytes: m["bytes"] as number, segs };
}

const chatKey = (id: ConversationId): string => encodeURIComponent(id);
const manifestKey = (id: ConversationId): string => MANIFEST + chatKey(id);
const segmentKey = (id: ConversationId, n: number): string => `${SEGMENTS}${chatKey(id)}/${String(n)}`;

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

  const del = async (key: string): Promise<void> => { try { await kv.delete(key); } catch { /* swept at the next load */ } };
  const total = (): number => { let n = 0; for (const m of held.values()) n += m.bytes; return n; };

  async function forget(id: ConversationId, m: Manifest | undefined): Promise<void> {
    held.delete(id);
    for (const s of m?.segs ?? []) await del(segmentKey(id, s[0]));
    await del(manifestKey(id));
  }

  /** Drops the least recently active chats until the budget holds. */
  async function enforce(): Promise<void> {
    while (total() > maxBytes && held.size > 0) {
      let oldest: ConversationId | undefined;
      for (const [id, m] of held) if (oldest === undefined || m.last < (held.get(oldest)?.last ?? Infinity)) oldest = id;
      if (oldest === undefined) return;
      dropped.add(oldest);
      await forget(oldest, held.get(oldest));
    }
  }

  async function readJson(key: string): Promise<{ readonly ok: true; readonly value: unknown } | { readonly ok: false; readonly corrupt: boolean } | { readonly ok: "none" }> {
    let got;
    try { got = await kv.get(key); } catch { return { ok: false, corrupt: false }; }
    if (!got.ok) return { ok: false, corrupt: CORRUPT.has(got.reason) };
    if (got.value === undefined) return { ok: "none" };
    try { return { ok: true, value: JSON.parse(dec.decode(got.value)) as unknown }; } catch { return { ok: false, corrupt: true }; }
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
        const slash = rest.lastIndexOf("/");
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
      const m = got.ok === true ? parseManifest(got.value) : undefined;
      if (m === undefined) { await forget(id, undefined); continue; } // damaged, unknown shape or no value: rebuilt later
      const rows: StoredRow[] = [];
      let intact = true;
      for (const s of m.segs) {
        const seg = await readJson(segmentKey(id, s[0]));
        const list = seg.ok === true && Array.isArray(seg.value) ? (seg.value as unknown[]) : undefined;
        if (list === undefined || list.length !== s[2] || !list.every(isRow)) { intact = false; break; }
        for (const r of list as readonly (readonly [string, number, string, readonly string[]])[]) rows.push({ messageId: r[0] as MessageId, sentAt: r[1], text: r[2], files: r[3] });
      }
      if (!intact) { await forget(id, m); segments.delete(chatKey(id)); continue; }
      const named = new Set(m.segs.map((s) => segmentKey(id, s[0])));
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
    if (!usable || dropped.has(c.id)) return;
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
      segs.push([n, hashOf(text), rows.length, body.length]);
      bodies.set(n, body);
      bytes += body.length;
    }
    const last = c.rows.reduce((t, r) => Math.max(t, r.sentAt), 0);
    const next: Manifest = { v: VERSION, name: c.name, first: c.first, last, bytes, segs };
    const unchanged = before !== undefined && before.name === next.name && before.first === next.first && before.last === next.last && before.bytes === next.bytes
      && before.segs.length === segs.length && before.segs.every((s, i) => s[0] === segs[i]?.[0] && s[1] === segs[i]?.[1]);
    if (unchanged) return;
    for (const s of segs) {
      if (known.get(s[0]) === s[1]) continue;
      const r = await kv.put(segmentKey(c.id, s[0]), bodies.get(s[0]) ?? new Uint8Array());
      if (!r.ok) return; // locked or full: the manifest still names the old segments, so what is at rest stays consistent
    }
    const r = await kv.put(manifestKey(c.id), enc.encode(JSON.stringify(next)));
    if (!r.ok) return;
    held.set(c.id, next);
    const kept = new Set(segs.map((s) => s[0]));
    for (const s of before?.segs ?? []) if (!kept.has(s[0])) await del(segmentKey(c.id, s[0]));
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
