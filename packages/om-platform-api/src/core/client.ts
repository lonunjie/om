import type { ConversationId, OperationId, ProfileExecutionContext, ProfileId } from "./ids";
import type { CoreIntent } from "./intents";
import type { OutcomeClass } from "./outcome";
import type { ConversationSummary, ConversationView, IntroductionRequest, PendingOperationView, ProfileSummary } from "./views";

export interface GetConversationView { readonly kind: "GetConversationView"; readonly ctx: ProfileExecutionContext; readonly conversationId: ConversationId }
export interface GetConversationList { readonly kind: "GetConversationList"; readonly ctx: ProfileExecutionContext }
export interface GetProfileList { readonly kind: "GetProfileList" }
export interface GetIntroductions { readonly kind: "GetIntroductions"; readonly ctx: ProfileExecutionContext }
export type CoreQuery = GetConversationView | GetConversationList | GetProfileList | GetIntroductions;

export interface QueryResultMap {
  GetConversationView: ConversationView;
  GetConversationList: readonly ConversationSummary[];
  GetProfileList: readonly ProfileSummary[];
  GetIntroductions: readonly IntroductionRequest[];
}
export type QueryResult<Q extends CoreQuery> = QueryResultMap[Q["kind"]];

export type Unsubscribe = () => void;

/** Dispatch resolves to a handle, never to success: success is whatever outcome Core reports. */
export interface OperationHandle { readonly operationId: OperationId; readonly profileId: ProfileId }

export interface CoreClient {
  dispatch(intent: CoreIntent): Promise<OperationHandle>;
  /** Emits the current outcome immediately, then every change. Observing never creates an operation. */
  observe(ctx: ProfileExecutionContext, operationId: OperationId, listener: (outcome: OutcomeClass) => void): Unsubscribe;
  query<Q extends CoreQuery>(query: Q): Promise<QueryResult<Q>>;
  /** Emits on each change after subscription (use `query` for the initial value). */
  subscribe<Q extends CoreQuery>(selector: Q, listener: (result: QueryResult<Q>) => void): Unsubscribe;
  /** All operations for the profile reconstructed from the durable log (pending, committed, rejected). Used after restart. */
  pendingOperations(ctx: ProfileExecutionContext): Promise<readonly PendingOperationView[]>;
}
