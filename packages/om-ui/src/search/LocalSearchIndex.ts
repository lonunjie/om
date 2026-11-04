import type { ConversationId, ConversationTimelineItem, MessageId } from "@om/platform-api";
import { norm } from "../omnibar/match";

/**
 * A search over what this device has already decrypted, kept in memory only.
 *
 * It exists so a person can find "the thing Sam sent about the flat" without anything leaving the device: the index is built from Core's
 * own decrypted conversation views after unlock, never written to disk and never sent anywhere, and `clear()` wipes it (the host calls it on
 * lock, on switching identity and when the screen goes away). Core's encrypted storage stays the only copy at rest.
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
interface Conversation { readonly displayName: string; readonly rows: readonly Row[] }

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

  /** Replaces everything known about one conversation (called again whenever its view changes). */
  replace(id: ConversationId, displayName: string, timeline: readonly ConversationTimelineItem[]): void {
    if (!this.chats.has(id) && this.chats.size >= this.limits.conversations) return;
    const rows = timeline.slice(-this.limits.perConversation).map((m): Row => ({
      messageId: m.messageId,
      sentAt: m.sentAt,
      text: m.text,
      key: norm(m.text),
      files: (m.attachments ?? []).flatMap((a) => (a.filename === undefined || a.filename === "" ? [] : [{ name: a.filename, key: norm(a.filename) }])),
    }));
    this.chats.set(id, { displayName, rows });
  }

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
