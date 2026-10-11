// Test-only two-profile CoreClient stand-in (@om/core-harness is not a dependency of @om/ui).
import { asOperationId, captureProfileContext, asProfileGeneration, asProfileId } from "@om/platform-api";
import type { CoreClient, CoreIntent, CoreQuery, OperationHandle, ProfileExecutionContext, ProfileId, ProfileSummary, QueryResult } from "@om/platform-api";
import type { ProfileContextSource } from "./context";

export interface FakeCore extends CoreClient, ProfileContextSource {
  readonly dispatched: CoreIntent[];
  release(): void;
}

/** `hold` makes dispatch resolve only after `release()`, simulating in-flight work. */
export function createFakeCore(profiles: readonly ProfileSummary[], hold = false): FakeCore {
  const dispatched: CoreIntent[] = [];
  const waiters: (() => void)[] = [];
  let n = 0;
  return {
    dispatched,
    contextFor: (id: ProfileId): ProfileExecutionContext => captureProfileContext(id, asProfileGeneration(`g-${id}`)),
    release: () => { waiters.splice(0).forEach((w) => { w(); }); },
    async dispatch(intent): Promise<OperationHandle> {
      dispatched.push(intent);
      if (hold) await new Promise<void>((r) => { waiters.push(r); });
      return { operationId: asOperationId(`op-${String(++n)}`), profileId: intent.ctx.profileId };
    },
    observe: () => () => undefined,
    query: <Q extends CoreQuery>(q: Q) => Promise.resolve((q.kind === "GetProfileList" ? profiles : []) as QueryResult<Q>),
    subscribe: () => () => undefined,
    pendingOperations: () => Promise.resolve([]),
  };
}
export const P1 = asProfileId("p1");
export const P2 = asProfileId("p2");
export const TWO_PROFILES: readonly ProfileSummary[] = [{ id: P1, label: "Alice" }, { id: P2, label: "Work" }];
