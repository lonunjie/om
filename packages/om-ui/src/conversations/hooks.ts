import { useEffect, useMemo, useState } from "react";
import type { ConversationId, ConversationSummary, ConversationView, OperationId, OutcomeClass } from "@om/platform-api";
import { deliveryOf, type DeliveryDisplay } from "./delivery";
import type { ConversationServices } from "./services";

export type Loadable<T> = { readonly status: "loading" } | { readonly status: "failed" } | { readonly status: "ready"; readonly value: T };

export function useConversationList(s: ConversationServices): Loadable<readonly ConversationSummary[]> {
  const [state, setState] = useState<Loadable<readonly ConversationSummary[]>>({ status: "loading" });
  useEffect(() => {
    let alive = true;
    const q = { kind: "GetConversationList", ctx: s.ctx } as const;
    const refresh = (): void => {
      s.client.query(q).then((value) => { if (alive) setState({ status: "ready", value }); }, () => { if (alive) setState({ status: "failed" }); });
    };
    refresh();
    let unsub: (() => void) | null = null;
    try { unsub = s.client.subscribe(q, refresh); } catch { setState({ status: "failed" }); }
    return () => { alive = false; unsub?.(); };
  }, [s.client, s.ctx]);
  return state;
}

export interface ConversationViewState { readonly view: ConversationView; readonly ops: ReadonlyMap<OperationId, OutcomeClass> }

/** Queries the view and the durable operation log together, so pending state is rebuilt after a restart (UI-13). */
export function useConversationView(s: ConversationServices, conversationId: ConversationId): Loadable<ConversationViewState> {
  const [state, setState] = useState<Loadable<ConversationViewState>>({ status: "loading" });
  useEffect(() => {
    let alive = true;
    let seq = 0;
    const q = { kind: "GetConversationView", ctx: s.ctx, conversationId } as const;
    setState({ status: "loading" });
    const refresh = (): void => {
      const mine = ++seq;
      Promise.all([s.client.query(q), s.client.pendingOperations(s.ctx)]).then(([view, ops]) => {
        if (alive && mine === seq) setState({ status: "ready", value: { view, ops: new Map(ops.map((o) => [o.operationId, o.outcome])) } });
      }, () => { if (alive && mine === seq) setState({ status: "failed" }); });
    };
    refresh();
    let unsub: (() => void) | null = null;
    try { unsub = s.client.subscribe(q, refresh); } catch { setState({ status: "failed" }); }
    return () => { alive = false; unsub?.(); };
  }, [s.client, s.ctx, conversationId]);
  return state;
}

/**
 * What a chat row shows beyond the name and time: the latest message, who sent it, whether it is still on its way, and when each
 * incoming message arrived (the shell turns those into an unread count against the times this device last showed the chat).
 * Core's conversation list carries no preview line, but its conversation view already returns the messages, so the row is built
 * here from that view; nothing new is asked of Core.
 */
export interface ConversationDigest {
  readonly lastText: string | null;
  readonly lastFromMe: boolean;
  /** The state of the newest message you sent, when it is that one. */
  readonly lastDelivery: DeliveryDisplay | null;
  readonly lastSentAt: number | null;
  readonly incomingAt: readonly number[];
}

/** Rows past this many are shown without a preview: each digest is a live query. */
const MAX_DIGESTS = 100;

const digestOf = (view: ConversationView): ConversationDigest => {
  const last = view.timeline[view.timeline.length - 1];
  return {
    lastText: last === undefined ? null : last.text.replace(/\s+/g, " ").trim(),
    lastFromMe: last?.direction === "outgoing",
    lastDelivery: last?.direction === "outgoing" ? deliveryOf(last.delivery) : null,
    lastSentAt: last?.sentAt ?? null,
    incomingAt: view.timeline.filter((i) => i.direction === "incoming").map((i) => i.sentAt),
  };
};

export function useConversationDigests(s: ConversationServices, ids: readonly ConversationId[]): ReadonlyMap<ConversationId, ConversationDigest> {
  const [digests, setDigests] = useState<ReadonlyMap<ConversationId, ConversationDigest>>(() => new Map());
  const key = ids.slice(0, MAX_DIGESTS).join("\u0000");
  const wanted = useMemo(() => (key === "" ? [] : key.split("\u0000") as ConversationId[]), [key]);
  useEffect(() => {
    let alive = true;
    const unsubs: (() => void)[] = [];
    const put = (id: ConversationId, view: ConversationView): void => {
      if (alive) setDigests((prev) => new Map(prev).set(id, digestOf(view)));
    };
    for (const conversationId of wanted) {
      const q = { kind: "GetConversationView", ctx: s.ctx, conversationId } as const;
      const refresh = (): void => { s.client.query(q).then((view) => { put(conversationId, view); }, () => undefined); };
      refresh();
      try { unsubs.push(s.client.subscribe(q, refresh)); } catch { /* a stale context: the row simply has no preview */ }
    }
    return () => { alive = false; for (const u of unsubs) u(); };
  }, [s.client, s.ctx, wanted]);
  return digests;
}
