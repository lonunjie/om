import { useCallback, useEffect, useMemo, useState } from "react";
import type { ConversationId } from "@om/platform-api";
import type { ConversationServices } from "../conversations/services";
import { LocalSearchIndex, type SearchHit } from "./LocalSearchIndex";

export interface LocalSearch {
  /** Synchronous, from memory. A new function identity whenever the index changed, so a list built from it recomputes. */
  readonly search: (query: string) => readonly SearchHit[];
  /** True once every conversation has been read at least once after unlock. */
  readonly ready: boolean;
}

/**
 * Builds the in-memory index from Core's decrypted conversation views and keeps it current. It lives exactly as long as the profile context
 * it was built for: leaving the screen (lock), switching identity (a new context) or a conversation going away wipes it. Nothing is written
 * to disk and nothing is sent anywhere; the only reads are the same Core queries the conversation list already makes.
 */
export function useLocalSearch(s: ConversationServices): LocalSearch {
  const [version, setVersion] = useState(0);
  const [ready, setReady] = useState(false);
  const index = useMemo(() => new LocalSearchIndex(), [s.client, s.ctx]);

  useEffect(() => {
    let alive = true;
    const unsubs = new Map<ConversationId, () => void>();
    const loaded = new Set<ConversationId>();
    let wanted = 0;
    setReady(false);
    const touch = (): void => { if (alive) { setVersion((v) => v + 1); setReady(loaded.size >= wanted); } };

    const watch = (id: ConversationId, displayName: string): void => {
      const q = { kind: "GetConversationView", ctx: s.ctx, conversationId: id } as const;
      const refresh = (): void => {
        s.client.query(q).then((view) => {
          if (!alive) return;
          index.replace(id, view.summary.displayName || displayName, view.timeline);
          loaded.add(id);
          touch();
        }, () => { loaded.add(id); touch(); });
      };
      refresh();
      try { unsubs.set(id, s.client.subscribe(q, refresh)); } catch { /* a stale context: this chat is simply not searchable */ }
    };

    const list = { kind: "GetConversationList", ctx: s.ctx } as const;
    const sync = (): void => {
      s.client.query(list).then((chats) => {
        if (!alive) return;
        const ids = new Set(chats.map((c) => c.id));
        for (const [id, unsub] of unsubs) {
          if (!ids.has(id)) { unsub(); unsubs.delete(id); loaded.delete(id); index.remove(id); }
        }
        wanted = chats.length;
        for (const c of chats) if (!unsubs.has(c.id)) watch(c.id, c.displayName);
        touch();
      }, () => undefined);
    };
    sync();
    let unsubList: (() => void) | null = null;
    try { unsubList = s.client.subscribe(list, sync); } catch { /* stale context */ }

    return () => {
      alive = false;
      unsubList?.();
      for (const u of unsubs.values()) u();
      unsubs.clear();
      index.clear(); // lock, identity switch, or the screen going away: nothing stays behind
    };
  }, [s.client, s.ctx, index]);

  const search = useCallback((query: string) => index.search(query), [index, version]);
  return { search, ready };
}
