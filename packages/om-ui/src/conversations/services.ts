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

/** How long a flow waits for the provider before it settles on an honest "still pending" state. */
export const SETTLE_TIMEOUT_MS = 15_000;

export interface SettleOptions {
  /** Wait bound; on expiry the promise resolves with the latest pending outcome (default {@link SETTLE_TIMEOUT_MS}). */
  readonly timeoutMs?: number;
  /** Ends the wait early, for example when the step that started it unmounts. */
  readonly signal?: AbortSignal;
}

/**
 * Resolves with the first outcome that is not waiting on the network (anything but PendingNetwork or
 * PendingReconciliation). For flows whose result is only usable once the provider has acted, such as publishing an
 * invite or sending a request: the real Core reports `PendingNetwork` first and settles later.
 *
 * The wait is bounded. Without a provider the operation stays pending, so on timeout or abort this resolves with the
 * latest pending outcome (`PendingNetwork` when none was seen) and stops observing: callers show "queued, will send
 * when connected" and the user can leave. It never reports success it did not observe, and a terminal outcome that
 * arrives after the wait ended is ignored (it is still durable in Core).
 */
export function settledOutcome(s: ConversationServices, operationId: OperationId, opts: SettleOptions = {}): Promise<OutcomeClass> {
  return new Promise((resolve, reject) => {
    const st: { done: boolean; last: OutcomeClass; unsub: (() => void) | null; timer: ReturnType<typeof setTimeout> | null } = {
      done: false, last: { kind: "PendingNetwork" }, unsub: null, timer: null,
    };
    const finish = (o: OutcomeClass): void => {
      if (st.done) return;
      st.done = true;
      if (st.timer !== null) clearTimeout(st.timer);
      opts.signal?.removeEventListener("abort", onAbort);
      st.unsub?.();
      resolve(o);
    };
    const onAbort = (): void => { finish(st.last); };
    try {
      st.unsub = s.client.observe(s.ctx, operationId, (o) => {
        if (st.done) return;
        if (o.kind === "PendingNetwork" || o.kind === "PendingReconciliation") { st.last = o; return; }
        finish(o);
      });
      if (st.done) st.unsub();
      else if (opts.signal?.aborted) finish(st.last);
      else {
        opts.signal?.addEventListener("abort", onAbort, { once: true });
        st.timer = setTimeout(() => { finish(st.last); }, opts.timeoutMs ?? SETTLE_TIMEOUT_MS);
      }
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
