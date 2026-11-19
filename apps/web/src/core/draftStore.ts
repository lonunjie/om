// Unsent drafts, kept on this device so locking OM, reloading or closing the tab does not lose what someone was writing
// (DRAFTS_SCHEDULED_AND_CONTINUITY 1-2). A draft is private writing, so it is never stored as plain text: each one is sealed
// by the identity's vault, in the same encrypted store as the chats, under a key Core ignores (it reads only "core/").
// Locked vault = nothing can be read or written, so drafts stay in memory until the next unlock; nothing reaches the network.
import type { ConversationId, EncryptedKv, ProfileId } from "@om/platform-api";
import { MAX_SCHEDULED, MAX_SCHEDULED_CHARS, MAX_SCHEDULED_ID_CHARS, type ScheduledItem } from "@om/ui";

const PREFIX = "ui/draft/";
/** When this device last showed each chat, so the list can count what arrived since. It is presentation only: never sent anywhere, never a read receipt. */
const SEEN_PREFIX = "ui/seen/";
/** The device-local scheduled-send queue (ADR-0224): compose intent only (words, chat, time), sealed with the rest of this identity's private state. */
const SCHEDULED_KEY = "ui/scheduled";
const SAVE_AFTER_MS = 600;
/** A draft this long is a pasted document, not a message; keeping it would only slow every save. */
const MAX_DRAFT_CHARS = 20_000;

export type Drafts = ReadonlyMap<ConversationId, string>;
export type SeenTimes = ReadonlyMap<ConversationId, number>;

export interface DraftStore {
  /** What was written and not sent for this identity; waits for the vault, so call it after unlock. */
  load(profileId: ProfileId): Promise<Drafts>;
  /** Records the text of one conversation's draft (empty = no draft) and saves it shortly after typing stops. */
  set(profileId: ProfileId, conversationId: ConversationId, text: string): void;
  /** Writes everything pending now. Call before locking: a locked vault cannot seal. */
  flush(): Promise<void>;
  /** Drops the identity's drafts from memory (after locking). They stay sealed on disk. */
  forget(profileId: ProfileId): void;
  /** When each chat was last shown on this device (message time, ms); waits for the vault. */
  loadSeen(profileId: ProfileId): Promise<SeenTimes>;
  /** Records that a chat was shown up to this message time. Only moves forward. */
  setSeen(profileId: ProfileId, conversationId: ConversationId, at: number): void;
  /** The messages waiting to be sent at a time this identity chose; waits for the vault. Device-local: never synced to another device. */
  loadScheduled(profileId: ProfileId): Promise<readonly ScheduledItem[]>;
  setScheduled(profileId: ProfileId, items: readonly ScheduledItem[]): void;
}

/** A stored queue, checked on the way in: anything malformed is dropped, never trusted. */
export function readScheduled(raw: unknown): readonly ScheduledItem[] {
  if (!Array.isArray(raw)) return [];
  const out: ScheduledItem[] = [];
  for (const x of raw as unknown[]) {
    if (typeof x !== "object" || x === null) continue;
    const r = x as Record<string, unknown>;
    if (typeof r["id"] !== "string" || typeof r["conversationId"] !== "string" || typeof r["text"] !== "string" || typeof r["dueAt"] !== "number" || typeof r["createdAt"] !== "number") continue;
    if (r["id"] === "" || r["id"].length > MAX_SCHEDULED_ID_CHARS || r["text"].length > MAX_SCHEDULED_CHARS || !Number.isFinite(r["dueAt"]) || out.length >= MAX_SCHEDULED) continue;
    const attempt = r["attempt"];
    out.push({ id: r["id"], conversationId: r["conversationId"] as ConversationId, text: r["text"], dueAt: r["dueAt"], createdAt: r["createdAt"], ...(typeof attempt === "number" && Number.isSafeInteger(attempt) && attempt > 0 ? { attempt } : {}) });
  }
  return out;
}

interface Entry { drafts: Map<ConversationId, string>; dirty: Set<ConversationId>; seen: Map<ConversationId, number>; seenDirty: Set<ConversationId>; scheduled: readonly ScheduledItem[]; scheduledDirty: boolean; loaded: Promise<void> | null }

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export function createDraftStore(kvFor: (profileId: ProfileId) => EncryptedKv): DraftStore {
  const entries = new Map<ProfileId, Entry>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let writing: Promise<void> = Promise.resolve();

  const entry = (profileId: ProfileId): Entry => {
    let e = entries.get(profileId);
    if (e === undefined) { e = { drafts: new Map(), dirty: new Set(), seen: new Map(), seenDirty: new Set(), scheduled: [], scheduledDirty: false, loaded: null }; entries.set(profileId, e); }
    return e;
  };

  const readAll = async (profileId: ProfileId, e: Entry): Promise<void> => {
    try {
      const kv = kvFor(profileId);
      const listed = await kv.list();
      if (!listed.ok) return;
      if (!e.scheduledDirty && listed.keys.includes(SCHEDULED_KEY)) {
        const got = await kv.get(SCHEDULED_KEY);
        if (got.ok && got.value !== undefined) { try { e.scheduled = readScheduled(JSON.parse(decoder.decode(got.value))); } catch { /* not a queue: start empty */ } }
      }
      for (const key of listed.keys.filter((k) => k.startsWith(SEEN_PREFIX))) {
        const id = key.slice(SEEN_PREFIX.length) as ConversationId;
        if (e.seen.has(id)) continue;
        const got = await kv.get(key);
        const at = got.ok && got.value !== undefined ? Number(decoder.decode(got.value)) : NaN;
        if (Number.isFinite(at)) e.seen.set(id, at);
      }
      for (const key of listed.keys.filter((k) => k.startsWith(PREFIX))) {
        const id = key.slice(PREFIX.length) as ConversationId;
        if (e.dirty.has(id) || e.drafts.has(id)) continue; // what was typed since opening wins over what was saved
        const got = await kv.get(key);
        if (!got.ok || got.value === undefined) continue;
        try { e.drafts.set(id, decoder.decode(got.value)); } catch { /* not text: skip it */ }
      }
    } catch { /* storage unavailable: drafts live in memory for this visit */ }
  };

  const writePending = async (): Promise<void> => {
    for (const [profileId, e] of entries) {
      if (e.dirty.size === 0 && e.seenDirty.size === 0 && !e.scheduledDirty) continue;
      const kv = kvFor(profileId);
      if (e.scheduledDirty) {
        try {
          const r = e.scheduled.length === 0 ? await kv.delete(SCHEDULED_KEY) : await kv.put(SCHEDULED_KEY, encoder.encode(JSON.stringify(e.scheduled)));
          if (r.ok || r.reason === "not_found") e.scheduledDirty = false;
        } catch { /* try again on the next change */ }
      }
      for (const id of [...e.seenDirty]) {
        try {
          const r = await kv.put(SEEN_PREFIX + id, encoder.encode(String(e.seen.get(id) ?? 0)));
          if (r.ok) e.seenDirty.delete(id);
        } catch { /* try again on the next change */ }
      }
      for (const id of [...e.dirty]) {
        const text = e.drafts.get(id) ?? "";
        try {
          const r = text === "" ? await kv.delete(PREFIX + id) : await kv.put(PREFIX + id, encoder.encode(text));
          if (r.ok || r.reason === "not_found") e.dirty.delete(id); // a locked vault stays dirty and is written after unlock
        } catch { /* try again on the next change */ }
      }
    }
  };

  const flush = (): Promise<void> => {
    if (timer !== undefined) { clearTimeout(timer); timer = undefined; }
    writing = writing.then(writePending, writePending);
    return writing;
  };

  const store: DraftStore = {
    load(profileId) {
      const e = entry(profileId);
      e.loaded ??= readAll(profileId, e);
      return e.loaded.then(() => new Map(e.drafts));
    },
    set(profileId, conversationId, text) {
      const e = entry(profileId);
      const next = text.length > MAX_DRAFT_CHARS ? text.slice(0, MAX_DRAFT_CHARS) : text;
      if (next === "") e.drafts.delete(conversationId); else e.drafts.set(conversationId, next);
      e.dirty.add(conversationId);
      timer ??= setTimeout(() => { timer = undefined; void flush(); }, SAVE_AFTER_MS);
    },
    flush,
    forget(profileId) { entries.delete(profileId); },
    loadSeen(profileId) {
      const e = entry(profileId);
      e.loaded ??= readAll(profileId, e);
      return e.loaded.then(() => new Map(e.seen));
    },
    loadScheduled(profileId) {
      const e = entry(profileId);
      e.loaded ??= readAll(profileId, e);
      return e.loaded.then(() => e.scheduled);
    },
    setScheduled(profileId, items) {
      const e = entry(profileId);
      e.scheduled = items;
      e.scheduledDirty = true;
      void flush(); // a queued message is written at once: it must survive closing the page
    },
    setSeen(profileId, conversationId, at) {
      const e = entry(profileId);
      if ((e.seen.get(conversationId) ?? -1) >= at) return;
      e.seen.set(conversationId, at);
      e.seenDirty.add(conversationId);
      timer ??= setTimeout(() => { timer = undefined; void flush(); }, SAVE_AFTER_MS);
    },
  };
  return store;
}
