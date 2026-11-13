import type { ConversationId, OperationId, ProfileExecutionContext, ProfileId } from "./ids";
import type { CoreIntent } from "./intents";
import type { OutcomeClass } from "./outcome";
import type { AttachmentApi } from "./attachments";
import type { DevicesView, HistoryTransferView, LinkOfferView, LinkRequestView, RecoveryStatusView } from "./devices";
import type { AttachmentPlanView, ConfirmationView, DirectOfferView, ConversationSummary, ConversationView, HomeSummary, IntroductionLoad, IntroductionRequest, InviteView, PendingOperationView, ProfileSummary, RecoveryKeyView } from "./views";

export interface GetConversationView { readonly kind: "GetConversationView"; readonly ctx: ProfileExecutionContext; readonly conversationId: ConversationId }
export interface GetConversationList { readonly kind: "GetConversationList"; readonly ctx: ProfileExecutionContext }
export interface GetProfileList { readonly kind: "GetProfileList" }
export interface GetIntroductions { readonly kind: "GetIntroductions"; readonly ctx: ProfileExecutionContext }
export interface GetRecoveryKey { readonly kind: "GetRecoveryKey"; readonly ctx: ProfileExecutionContext; readonly keyOperationId: OperationId }
export interface GetInvite { readonly kind: "GetInvite"; readonly ctx: ProfileExecutionContext; readonly operationId: OperationId }
export interface GetHomeSummary { readonly kind: "GetHomeSummary"; readonly ctx: ProfileExecutionContext }
export interface GetDevices { readonly kind: "GetDevices"; readonly ctx: ProfileExecutionContext }
/** The request produced by a RequestDeviceLink operation (`null` until Core has one). */
export interface GetLinkRequest { readonly kind: "GetLinkRequest"; readonly ctx: ProfileExecutionContext; readonly operationId: OperationId }
/** The offer produced by an OfferDeviceLink operation. */
export interface GetLinkOffer { readonly kind: "GetLinkOffer"; readonly ctx: ProfileExecutionContext; readonly operationId: OperationId }
/** Progress of the same-owner history transfer of the latest device link (`null` when none was announced). */
export interface GetHistoryTransfer { readonly kind: "GetHistoryTransfer"; readonly ctx: ProfileExecutionContext }
export interface GetRecoveryStatus { readonly kind: "GetRecoveryStatus"; readonly ctx: ProfileExecutionContext }
/** The nonce an `IssueConfirmation` operation produced (`null` once expired or unknown). */
export interface GetConfirmation { readonly kind: "GetConfirmation"; readonly ctx: ProfileExecutionContext; readonly operationId: OperationId }
/** Whether first-contact requests from strangers are currently being dropped. */
export interface GetIntroductionLoad { readonly kind: "GetIntroductionLoad"; readonly ctx: ProfileExecutionContext }
/** How a file of `size` bytes may travel, before anything is sealed or sent. */
export interface PlanAttachment { readonly kind: "PlanAttachment"; readonly ctx: ProfileExecutionContext; readonly size: number }
/** The direct offers of this profile, newest last. */
export interface GetDirectOffers { readonly kind: "GetDirectOffers"; readonly ctx: ProfileExecutionContext }
export type CoreQuery =
  | GetConversationView | GetConversationList | GetProfileList | GetIntroductions | GetRecoveryKey | GetInvite | GetHomeSummary
  | GetDevices | GetLinkRequest | GetLinkOffer | GetHistoryTransfer | GetRecoveryStatus | GetConfirmation | GetIntroductionLoad
  | PlanAttachment | GetDirectOffers;

export interface QueryResultMap {
  GetConversationView: ConversationView;
  GetConversationList: readonly ConversationSummary[];
  GetProfileList: readonly ProfileSummary[];
  GetIntroductions: readonly IntroductionRequest[];
  /** `null` once the key was confirmed stored (or never existed): Core never shows it again. */
  GetRecoveryKey: RecoveryKeyView | null;
  GetInvite: InviteView | null;
  GetHomeSummary: HomeSummary;
  GetDevices: DevicesView;
  GetLinkRequest: LinkRequestView | null;
  GetLinkOffer: LinkOfferView | null;
  GetHistoryTransfer: HistoryTransferView | null;
  GetRecoveryStatus: RecoveryStatusView;
  GetConfirmation: ConfirmationView | null;
  GetIntroductionLoad: IntroductionLoad;
  PlanAttachment: AttachmentPlanView;
  GetDirectOffers: readonly DirectOfferView[];
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
  /** Files, voice notes and location cards need a delivery service that holds files; a client without one leaves this out. */
  readonly attachments?: AttachmentApi;
}
