import { useEffect, useState } from "react";
import type { ConversationId, ConversationSummary, ConversationView, OperationId, OutcomeClass } from "@om/platform-api";
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
