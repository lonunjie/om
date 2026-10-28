import type { CoreClient, CoreIntent, ConfirmationToken, OperationId, OutcomeClass, ProfileExecutionContext } from "@om/platform-api";
import type { ActionRegistry, ActionResult } from "../actions/registry";

/** What the conversation slice needs from the host: Core, the Action Registry, and the captured profile context. */
export interface ConversationServices {
  readonly client: CoreClient;
  readonly registry: ActionRegistry;
  readonly ctx: ProfileExecutionContext;
}

export type ResolvedIntent = { readonly status: "intent"; readonly intent: CoreIntent } | { readonly status: "blocked"; readonly result: ActionResult };

/** Every intent comes from the Action Registry (which allocates the OperationId); components never build intents. */
export function resolveIntent(s: ConversationServices, actionId: string, params: Readonly<Record<string, unknown>>, confirmation?: ConfirmationToken): ResolvedIntent {
  const result = s.registry.resolve(confirmation
    ? { actionId, params, ctx: s.ctx, origin: "direct", confirmation }
    : { actionId, params, ctx: s.ctx, origin: "direct" });
  return result.status === "intent" ? { status: "intent", intent: result.intent } : { status: "blocked", result };
}

/**
 * Resolves with the first outcome that is not waiting on the network (anything but PendingNetwork or
 * PendingReconciliation). For flows whose result is only usable once the provider has acted, such as publishing an
 * invite or sending a request: the real Core reports `PendingNetwork` first and settles later. A flow that never
 * settles (no provider) simply keeps waiting; the host's connection status explains why.
 */
export function settledOutcome(s: ConversationServices, operationId: OperationId): Promise<OutcomeClass> {
  return new Promise((resolve, reject) => {
    const st: { done: boolean; unsub: (() => void) | null } = { done: false, unsub: null };
    try {
      st.unsub = s.client.observe(s.ctx, operationId, (o) => {
        if (st.done || o.kind === "PendingNetwork" || o.kind === "PendingReconciliation") return;
        st.done = true;
        resolve(o);
        st.unsub?.();
      });
      if (st.done) st.unsub();
    } catch (e) {
      reject(e instanceof Error ? e : new Error("observe failed"));
    }
  });
}

/** Resolves with the first outcome Core reports for the operation, then stops observing. Observing never creates an operation. */
export function firstOutcome(s: ConversationServices, operationId: OperationId): Promise<OutcomeClass> {
  return new Promise((resolve, reject) => {
    const st: { done: boolean; unsub: (() => void) | null } = { done: false, unsub: null };
    try {
      st.unsub = s.client.observe(s.ctx, operationId, (o) => {
        if (st.done) return;
        st.done = true;
        resolve(o);
        st.unsub?.();
      });
      if (st.done) st.unsub();
    } catch (e) {
      reject(e instanceof Error ? e : new Error("observe failed"));
    }
  });
}
