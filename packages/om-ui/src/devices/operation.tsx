// Shared plumbing for security-critical settings screens: live Core queries and continuously observed operations.
// The UI never decides an outcome; it renders the latest OutcomeClass Core reports for the operation.
import { useCallback, useEffect, useRef, useState } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { CoreQuery, OperationId, OutcomeClass, PendingOperationView, QueryResult } from "@om/platform-api";
import { confirmWithCore } from "../actions/confirm";
import type { ConversationServices } from "../conversations/services";
import type { Loadable } from "../conversations/hooks";
import type { OutcomeCopy } from "./copy";

/** Core, the Action Registry and the captured profile context (same shape as the conversation slice). */
export type SettingsServices = ConversationServices;

export interface LiveResult<T> { readonly value: T; readonly ops: readonly PendingOperationView[] }

/**
 * Queries `query` and the durable operation log together and refreshes on every change, so pending state is rebuilt
 * after a restart (UI-13). `key` must change whenever the query's parameters change.
 */
export function useLiveQuery<Q extends CoreQuery>(s: SettingsServices, query: Q, key: string): Loadable<LiveResult<QueryResult<Q>>> {
  const [state, setState] = useState<Loadable<LiveResult<QueryResult<Q>>>>({ status: "loading" });
  const queryRef = useRef(query);
  queryRef.current = query;
  useEffect(() => {
    let alive = true;
    let seq = 0;
    const q = queryRef.current;
    const refresh = (): void => {
      const mine = ++seq;
      Promise.all([s.client.query(q), s.client.pendingOperations(s.ctx)]).then(([value, ops]) => {
        if (alive && mine === seq) setState({ status: "ready", value: { value, ops } });
      }, () => { if (alive && mine === seq) setState({ status: "failed" }); });
    };
    setState({ status: "loading" });
    refresh();
    let unsub: (() => void) | null = null;
    try { unsub = s.client.subscribe(q, refresh); } catch { setState({ status: "failed" }); }
    return () => { alive = false; unsub?.(); };
  }, [s.client, s.ctx, key]);
  return state;
}

export type OperationState =
  | { readonly phase: "idle" }
  | { readonly phase: "working" }
  | { readonly phase: "failed" } // nothing reached Core
  | { readonly phase: "observing"; readonly operationId: OperationId; readonly outcome: OutcomeClass | null };

export interface Operation {
  readonly state: OperationState;
  /** True while an operation is in flight or not yet terminal: the UI must not start a second semantic operation. */
  readonly locked: boolean;
  /**
   * Resolves `actionId` through the registry and dispatches it. Call only from a direct user gesture: for destructive
   * actions this is the confirmation, and a registry token bound to exactly these params is minted here.
   */
  readonly run: (actionId: string, params: Readonly<Record<string, unknown>>) => Promise<OperationId | null>;
  /** Re-attach to an operation found in the durable log (after restart). */
  readonly follow: (operationId: OperationId) => void;
  readonly reset: () => void;
}

export function useOperation(s: SettingsServices): Operation {
  const [state, setState] = useState<OperationState>({ phase: "idle" });
  const unsub = useRef<(() => void) | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; unsub.current?.(); unsub.current = null; }; }, []);

  const follow = useCallback((operationId: OperationId): void => {
    unsub.current?.();
    setState({ phase: "observing", operationId, outcome: null });
    try {
      unsub.current = s.client.observe(s.ctx, operationId, (outcome) => { if (mounted.current) setState({ phase: "observing", operationId, outcome }); });
    } catch {
      setState({ phase: "failed" });
    }
  }, [s.client, s.ctx]);

  const run = useCallback(async (actionId: string, params: Readonly<Record<string, unknown>>): Promise<OperationId | null> => {
    setState({ phase: "working" });
    try {
      let r = s.registry.resolve({ actionId, params, ctx: s.ctx, origin: "direct" });
      if (r.status === "needs_confirmation") {
        const issued = await confirmWithCore(s, actionId, params);
        if (issued.status !== "issued") throw new Error("confirmation refused");
        r = s.registry.resolve({ actionId, params, ctx: s.ctx, origin: "direct", confirmation: issued.token });
      }
      if (r.status !== "intent") throw new Error("not an intent");
      const handle = await s.client.dispatch(r.intent);
      if (mounted.current) follow(handle.operationId);
      return handle.operationId;
    } catch {
      if (mounted.current) setState({ phase: "failed" });
      return null;
    }
  }, [s.client, s.ctx, s.registry, follow]);

  const reset = useCallback((): void => { unsub.current?.(); unsub.current = null; setState({ phase: "idle" }); }, []);

  const locked = state.phase === "working" || (state.phase === "observing" && (state.outcome === null || !isTerminalOutcome(state.outcome)));
  return { state, locked, run, follow, reset };
}

/** Plain-language status for an outcome. `done` is the action-specific Committed wording. */
export function outcomeText(o: OutcomeClass, copy: OutcomeCopy, done: string): string {
  switch (o.kind) {
    case "Committed": return done;
    case "PendingAuthorization": return copy.pendingAuthorization;
    case "PendingNetwork": return copy.pendingNetwork;
    case "PendingReconciliation": return copy.pendingReconciliation;
    case "NeedsUserAction": return copy.needsAction[o.action];
    case "Rejected": return copy.rejected[o.reason];
  }
}

export function operationText(state: OperationState, copy: OutcomeCopy, done: string): { readonly text: string; readonly alert: boolean } | null {
  switch (state.phase) {
    case "idle": return null;
    case "working": return { text: copy.working, alert: false };
    case "failed": return { text: copy.failed, alert: true };
    case "observing":
      if (state.outcome === null) return { text: copy.working, alert: false };
      return { text: outcomeText(state.outcome, copy, done), alert: state.outcome.kind === "Rejected" || state.outcome.kind === "NeedsUserAction" };
  }
}

export const OperationStatus = ({ state, copy, done }: { readonly state: OperationState; readonly copy: OutcomeCopy; readonly done: string }) => {
  const t = operationText(state, copy, done);
  const outcome = state.phase === "observing" ? state.outcome?.kind ?? "pending" : state.phase;
  // Always mounted so screen readers hear changes; alerts only for outcomes that need the person.
  return (
    <p className="om-settings__status" role={t?.alert ? "alert" : "status"} data-outcome={outcome}>{t?.text ?? ""}</p>
  );
};
