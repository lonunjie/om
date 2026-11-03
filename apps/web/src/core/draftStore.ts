// Unsent drafts, kept on this device so locking OM, reloading or closing the tab does not lose what someone was writing
// (DRAFTS_SCHEDULED_AND_CONTINUITY 1-2). A draft is private writing, so it is never stored as plain text: each one is sealed
// by the identity's vault, in the same encrypted store as the chats, under a key Core ignores (it reads only "core/").
// Locked vault = nothing can be read or written, so drafts stay in memory until the next unlock; nothing reaches the network.
import type { ConversationId, EncryptedKv, ProfileId } from "@om/platform-api";

const PREFIX = "ui/draft/";
const SAVE_AFTER_MS = 600;
/** A draft this long is a pasted document, not a message; keeping it would only slow every save. */
const MAX_DRAFT_CHARS = 20_000;

export type Drafts = ReadonlyMap<ConversationId, string>;

export interface DraftStore {
  /** What was written and not sent for this identity; waits for the vault, so call it after unlock. */
  load(profileId: ProfileId): Promise<Drafts>;
  /** Records the text of one conversation's draft (empty = no draft) and saves it shortly after typing stops. */
  set(profileId: ProfileId, conversationId: ConversationId, text: string): void;
  /** Writes everything pending now. Call before locking: a locked vault cannot seal. */
  flush(): Promise<void>;
  /** Drops the identity's drafts from memory (after locking). They stay sealed on disk. */
  forget(profileId: ProfileId): void;
}

interface Entry { drafts: Map<ConversationId, string>; dirty: Set<ConversationId>; loaded: Promise<void> | null }

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

export function createDraftStore(kvFor: (profileId: ProfileId) => EncryptedKv): DraftStore {
  const entries = new Map<ProfileId, Entry>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let writing: Promise<void> = Promise.resolve();

  const entry = (profileId: ProfileId): Entry => {
    let e = entries.get(profileId);
    if (e === undefined) { e = { drafts: new Map(), dirty: new Set(), loaded: null }; entries.set(profileId, e); }
    return e;
  };

  const readAll = async (profileId: ProfileId, e: Entry): Promise<void> => {
    try {
      const kv = kvFor(profileId);
      const listed = await kv.list();
      if (!listed.ok) return;
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
      if (e.dirty.size === 0) continue;
      const kv = kvFor(profileId);
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
  };
  return store;
}
