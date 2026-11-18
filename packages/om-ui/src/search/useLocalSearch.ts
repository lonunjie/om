import { shownName } from "../conversations/shownName";
import { useCallback, useEffect, useMemo, useState } from "react";
import { asConversationId } from "@om/platform-api";
import type { ConversationId, GroupMessageView } from "@om/platform-api";
import type { ConversationServices } from "../conversations/services";
import { groupApiOf } from "../groups/services";
import { LocalSearchIndex, type SearchHit, type SearchSource } from "./LocalSearchIndex";
import type { SearchStore } from "./store";

export interface LocalSearch {
  /** Synchronous, from memory. A new function identity whenever the index changed, so a list built from it recomputes. */
  readonly search: (query: string) => readonly SearchHit[];
  /** True once every conversation has been read at least once after unlock. */
  readonly ready: boolean;
}

/** Writes to the store are batched for this long after a change, so a burst of messages is one write. */
const SAVE_AFTER_MS = 800;
/** Only a host with no change feed re-reads groups on a timer. */
const GROUP_FALLBACK_MS = 2500;

const groupSource = (m: GroupMessageView): SearchSource => ({ messageId: m.id as SearchSource["messageId"], sentAt: m.atMs, text: m.text });

/**
 * Builds the search index from Core's decrypted views (1:1 chats and groups) and keeps it current. In memory it lives exactly as long as
 * the profile context it was built for: leaving the screen (lock), switching identity (a new context) or a conversation going away wipes
 * it. With a `store` (ADR-0208) the index is also kept at rest, sealed by the identity's vault: it is read back on unlock so search works
 * before any chat is re-read, and only what changed is written. Nothing is sent anywhere; the reads are the same local Core queries the
 * lists already make.
 */
export function useLocalSearch(s: ConversationServices, store?: SearchStore): LocalSearch {
  const [version, setVersion] = useState(0);
  const [ready, setReady] = useState(false);
  const index = useMemo(() => new LocalSearchIndex(), [s.client, s.ctx]);
  const groupApi = groupApiOf(s);

  useEffect(() => {
    let alive = true;
    const unsubs = new Map<ConversationId, () => void>();
    const loaded = new Set<ConversationId>();
    const pending = new Set<ConversationId>();
    let saveTimer: ReturnType<typeof setTimeout> | undefined;
    let wanted = 0;
    setReady(false);
    const touch = (): void => { if (alive) { setVersion((v) => v + 1); setReady(loaded.size >= wanted); } };

    const writeNow = (): void => {
      if (saveTimer !== undefined) { clearTimeout(saveTimer); saveTimer = undefined; }
      if (store === undefined) { pending.clear(); return; }
      for (const id of pending) {
        const snap = index.snapshot(id);
        if (snap !== undefined) void store.save(snap);
      }
      pending.clear();
    };
    const changed = (id: ConversationId): void => {
      if (store === undefined) return;
      pending.add(id);
      saveTimer ??= setTimeout(() => { saveTimer = undefined; writeNow(); }, SAVE_AFTER_MS);
    };
    const gone = (id: ConversationId): void => {
      index.remove(id);
      pending.delete(id);
      if (store !== undefined) void store.remove(id);
    };

    // What was kept at rest and is neither a chat nor a group now (gone while the app was closed) is forgotten. Without the group module
    // loaded a stored group cannot be told from a vanished chat, so nothing is forgotten then.
    let chatIds: ReadonlySet<ConversationId> | null = null;
    const groupIds = new Set<ConversationId>();
    const reconcile = (): void => {
      if (chatIds === null || groupApi === null) return;
      for (const id of index.ids()) if (!chatIds.has(id) && !groupIds.has(id)) gone(id);
    };

    // What was kept at rest is searchable at once; a live read replaces it as each chat is read.
    if (store !== undefined) {
      void store.load().then((kept) => {
        if (!alive) return;
        let any = false;
        for (const c of kept) any = index.restore(c) || any;
        if (any) touch();
        reconcile();
      }, () => undefined);
    }

    const watch = (id: ConversationId, displayName: string): void => {
      const q = { kind: "GetConversationView", ctx: s.ctx, conversationId: id } as const;
      const refresh = (): void => {
        s.client.query(q).then((view) => {
          if (!alive) return;
          if (index.replace(id, shownName(view.summary) || displayName, view.timeline)) changed(id);
          loaded.add(id);
          touch();
        }, () => { loaded.add(id); touch(); });
      };
      refresh();
      try { unsubs.set(id, s.client.subscribe(q, refresh)); } catch { /* a stale context: this chat is simply not searchable */ }
    };

    // Groups are read straight from the group views (synchronous, local); the feed says when to look again.
    const syncGroups = (): void => {
      if (!alive || groupApi === null) return;
      let list;
      try { list = groupApi.list(s.ctx); } catch { return; } // a stale context after a profile switch
      const now = new Set<ConversationId>();
      for (const g of list) {
        const id = asConversationId(g.conversationId);
        now.add(id);
        let messages: readonly GroupMessageView[] = [];
        try { messages = groupApi.messages(s.ctx, g.conversationId); } catch { continue; }
        if (index.replace(id, g.name, messages.map(groupSource))) changed(id);
      }
      for (const id of groupIds) if (!now.has(id)) gone(id); // left, or removed from, the group
      groupIds.clear();
      for (const id of now) groupIds.add(id);
      touch();
    };

    const list = { kind: "GetConversationList", ctx: s.ctx } as const;
    const sync = (): void => {
      s.client.query(list).then((chats) => {
        if (!alive) return;
        const ids = new Set(chats.map((c) => c.id));
        for (const [id, unsub] of unsubs) {
          if (!ids.has(id)) { unsub(); unsubs.delete(id); loaded.delete(id); gone(id); }
        }
        chatIds = ids;
        reconcile();
        wanted = chats.length;
        for (const c of chats) if (!unsubs.has(c.id)) watch(c.id, shownName(c));
        touch();
      }, () => undefined);
    };
    syncGroups();
    sync();
    let unsubList: (() => void) | null = null;
    try { unsubList = s.client.subscribe(list, sync); } catch { /* stale context */ }
    let offGroups: (() => void) | undefined;
    let groupTimer: ReturnType<typeof setInterval> | undefined;
    if (groupApi !== null) {
      if (groupApi.onChange !== undefined) offGroups = groupApi.onChange((p) => { if (p === s.ctx.profileId) syncGroups(); });
      else groupTimer = setInterval(syncGroups, GROUP_FALLBACK_MS);
    }

    return () => {
      alive = false;
      unsubList?.();
      offGroups?.();
      if (groupTimer !== undefined) clearInterval(groupTimer);
      for (const u of unsubs.values()) u();
      unsubs.clear();
      writeNow(); // best effort: a locked vault cannot seal, and what is not written is re-read after the next unlock
      index.clear(); // lock, identity switch, or the screen going away: nothing stays in memory
    };
  }, [s.client, s.ctx, index, store, groupApi]);

  const search = useCallback((query: string) => index.search(query), [index, version]);
  return { search, ready };
}
