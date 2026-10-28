// The two-step, Core-issued confirmation for a destructive action (UI-11, ADR-0181 section 8).
import type { CoreClient, OperationId, OutcomeClass, ProfileExecutionContext } from "@om/platform-api";
import type { ActionRegistry, ConfirmationIssue } from "./registry";

export interface ConfirmationServices {
  readonly client: CoreClient;
  readonly registry: ActionRegistry;
  readonly ctx: ProfileExecutionContext;
}

function firstOutcome(s: ConfirmationServices, operationId: OperationId): Promise<OutcomeClass> {
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

/**
 * Ask Core for a confirmation nonce for this exact action and wrap it in a registry token. Only a direct user action
 * gets one: a suggestion, learned rank or Omnibar completion is refused before anything is sent to Core.
 */
export async function confirmWithCore(s: ConfirmationServices, actionId: string, params: Readonly<Record<string, unknown>>): Promise<ConfirmationIssue> {
  const req = { actionId, params, ctx: s.ctx, origin: "direct" } as const;
  const prep = s.registry.prepareConfirmation(req);
  if (prep.status !== "intent") return prep;
  const handle = await s.client.dispatch(prep.intent);
  const outcome = await firstOutcome(s, handle.operationId);
  if (outcome.kind !== "Committed") return { status: "rejected", code: "invalid_confirmation" };
  const view = await s.client.query({ kind: "GetConfirmation", ctx: s.ctx, operationId: handle.operationId });
  if (view === null) return { status: "rejected", code: "invalid_confirmation" };
  return s.registry.issueConfirmation({ ...req, nonce: view.nonce });
}
