import type { ConversationId, MessageId } from "@om/platform-api";
import { norm } from "../omnibar/match";

/**
 * A search over what this device has already decrypted.
 *
 * It exists so a person can find "the thing Sam sent about the flat" without anything leaving the device: the index is built from Core's
 * own decrypted conversation views (1:1 chats and groups alike) after unlock and never sent anywhere. In memory it is wiped by `clear()`
 * (the host calls it on lock, on switching identity and when the screen goes away). At rest it exists only as ciphertext: `SearchStore`
 * (store.ts, ADR-0208) seals it in the identity's vault under the search-index subkey, so a reload finds it again without a rebuild.
 *
 * Matching is local and deterministic (case, accents and compatibility forms folded; every typed word must appear; no learned ranking).
 */

export interface SearchLimits {
  /** Newest messages kept per conversation. */
  readonly perConversation: number;
  /** Conversations indexed at all. */
  readonly conversations: number;
}
export const DEFAULT_LIMITS: SearchLimits = { perConversation: 5000, conversations: 200 };
/** Characters of one message kept for search (and at rest). A pasted document is not worth indexing past this. */
export const MAX_TEXT = 4000;

/** What the index needs of one message: a chat's timeline item and a group's message both fit once mapped. */
export interface SearchSource {
  readonly messageId: MessageId;
  readonly sentAt: number;
  readonly text: string;
  readonly attachments?: readonly { readonly filename?: string | undefined }[] | undefined;
}

export interface SearchHit {
  readonly kind: "message" | "file";
  readonly conversationId: ConversationId;
  readonly messageId: MessageId;
  readonly displayName: string;
  /** What matched, trimmed around the match for a one-line result. */
  readonly snippet: string;
  readonly sentAt: number;
}

interface Row {
  readonly messageId: MessageId;
  readonly sentAt: number;
  readonly text: string;
  readonly key: string;
  readonly files: readonly { readonly name: string; readonly key: string }[];
}
interface Conversation { readonly displayName: string; readonly rows: readonly Row[]; readonly first: number }

/** One message as kept at rest (the folded key is recomputed on load, never stored). */
export interface StoredRow { readonly messageId: MessageId; readonly sentAt: number; readonly text: string; readonly files: readonly string[] }
/**
 * One conversation as kept at rest. `first` is the position of its oldest kept message in the whole timeline, so appending a message
 * only changes the newest part of what is stored.
 */
export interface StoredConversation { readonly id: ConversationId; readonly name: string; readonly first: number; readonly rows: readonly StoredRow[] }

const fileKey = (name: string): { readonly name: string; readonly key: string } => ({ name, key: norm(name) });
const sameFiles = (a: Row["files"], b: Row["files"]): boolean => a.length === b.length && a.every((f, i) => f.name === b[i]?.name);

const SNIPPET = 90;

/** The part of `text` around position `at`, on one line. Positions in the folded text only map onto the original when folding kept every length. */
function snippetOf(text: string, key: string, at: number): string {
  const flat = text.replace(/\s+/g, " ").trim();
  const mapped = key.length === text.length && flat.length === text.length ? at : 0;
  if (flat.length <= SNIPPET) return flat;
  const start = Math.max(0, Math.min(mapped - Math.floor(SNIPPET / 3), flat.length - SNIPPET));
  return `${start > 0 ? "…" : ""}${flat.slice(start, start + SNIPPET)}${start + SNIPPET < flat.length ? "…" : ""}`;
}

export class LocalSearchIndex {
  private readonly chats = new Map<ConversationId, Conversation>();
  private readonly limits: SearchLimits;
  constructor(limits: SearchLimits = DEFAULT_LIMITS) { this.limits = limits; }

  /**
   * Replaces everything known about one conversation (called again whenever its view changes). Messages that did not change keep their
   * folded form, so a re-read of a large chat costs a comparison, not a re-fold. Returns false when nothing differs from what is held.
   */
  replace(id: ConversationId, displayName: string, timeline: readonly SearchSource[]): boolean {
    const before = this.chats.get(id);
    if (before === undefined && this.chats.size >= this.limits.conversations) return false;
    const kept = timeline.slice(-this.limits.perConversation);
    const first = timeline.length - kept.length;
    const old = new Map<MessageId, Row>();
    for (const r of before?.rows ?? []) old.set(r.messageId, r);
    const rows = kept.map((m): Row => {
      const text = m.text.length > MAX_TEXT ? m.text.slice(0, MAX_TEXT) : m.text;
      const files = (m.attachments ?? []).flatMap((a) => (a.filename === undefined || a.filename === "" ? [] : [fileKey(a.filename)]));
      const had = old.get(m.messageId);
      return had !== undefined && had.text === text && had.sentAt === m.sentAt && sameFiles(had.files, files) ? had : { messageId: m.messageId, sentAt: m.sentAt, text, key: norm(text), files };
    });
    const same = before !== undefined && before.displayName === displayName && before.first === first && before.rows.length === rows.length && before.rows.every((r, i) => r === rows[i]);
    if (same) return false;
    this.chats.set(id, { displayName, rows, first });
    return true;
  }

  /**
   * Loads what was kept at rest. A conversation the live views already replaced in this visit wins over the stored copy (it is newer),
   * and the same bounds apply as for a live one. Returns false when nothing was taken.
   */
  restore(c: StoredConversation): boolean {
    if (this.chats.has(c.id) || this.chats.size >= this.limits.conversations) return false;
    const drop = Math.max(0, c.rows.length - this.limits.perConversation);
    this.chats.set(c.id, {
      displayName: c.name,
      first: c.first + drop,
      rows: c.rows.slice(drop).map((r): Row => ({ messageId: r.messageId, sentAt: r.sentAt, text: r.text, key: norm(r.text), files: r.files.map(fileKey) })),
    });
    return true;
  }

  /** What `restore` takes back: one conversation as kept at rest. */
  snapshot(id: ConversationId): StoredConversation | undefined {
    const c = this.chats.get(id);
    return c === undefined ? undefined : { id, name: c.displayName, first: c.first, rows: c.rows.map((r) => ({ messageId: r.messageId, sentAt: r.sentAt, text: r.text, files: r.files.map((f) => f.name) })) };
  }

  /** The conversations held. */
  ids(): readonly ConversationId[] { return [...this.chats.keys()]; }

  remove(id: ConversationId): void { this.chats.delete(id); }

  /** Forgets everything. After this the index holds no text at all. */
  clear(): void { this.chats.clear(); }

  /** Number of messages held (for tests and diagnostics; never shown). */
  get size(): number {
    let n = 0;
    for (const c of this.chats.values()) n += c.rows.length;
    return n;
  }

  /**
   * Messages and file names containing every typed word. A whole-phrase match ranks above scattered words, then newest first; ties break by
   * the chat's name and message id so the order never depends on map iteration. Capped at `limit`.
   */
  search(query: string, limit = 20): readonly SearchHit[] {
    const q = norm(query).trim();
    if (q === "" || limit <= 0) return [];
    const words = q.split(/\s+/);
    const scored: { readonly hit: SearchHit; readonly phrase: boolean }[] = [];
    for (const [conversationId, chat] of this.chats) {
      for (const row of chat.rows) {
        const at = row.key.indexOf(q);
        if (words.every((w) => row.key.includes(w))) {
          scored.push({ phrase: at >= 0, hit: { kind: "message", conversationId, messageId: row.messageId, displayName: chat.displayName, snippet: snippetOf(row.text, row.key, at >= 0 ? at : row.key.indexOf(words[0] ?? "")), sentAt: row.sentAt } });
        }
        for (const f of row.files) {
          if (words.every((w) => f.key.includes(w))) {
            scored.push({ phrase: f.key.includes(q), hit: { kind: "file", conversationId, messageId: row.messageId, displayName: chat.displayName, snippet: f.name, sentAt: row.sentAt } });
          }
        }
      }
    }
    scored.sort((a, b) => Number(b.phrase) - Number(a.phrase) || b.hit.sentAt - a.hit.sentAt || a.hit.displayName.localeCompare(b.hit.displayName) || a.hit.messageId.localeCompare(b.hit.messageId) || a.hit.kind.localeCompare(b.hit.kind));
    return scored.slice(0, limit).map((x) => x.hit);
  }
}
