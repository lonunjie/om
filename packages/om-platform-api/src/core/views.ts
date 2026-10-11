import type { ConversationId, MessageId, OperationId, PersonId, ProfileId } from "./ids";
import type { CoreIntentKind } from "./intents";
import type { OutcomeClass, RejectReason } from "./outcome";

export type DeliveryState =
  | { readonly state: "pending_network" }
  | { readonly state: "pending_reconciliation" }
  | { readonly state: "sent" }
  | { readonly state: "delivered" }
  | { readonly state: "rejected"; readonly reason: RejectReason };

/** Machine reason codes only; the UI owns all copy. */
export type SecurityNoticeCode = "safety_number_changed" | "device_added" | "unverified_contact" | "session_reset" | "integrity_failure";
export interface SecurityNotice { readonly code: SecurityNoticeCode; readonly subject?: PersonId }

export interface PersonSummary { readonly id: PersonId; readonly displayName: string; readonly blocked: boolean }
export interface IntroductionRequest { readonly id: string; readonly from: PersonId; readonly displayName: string }
export interface ProfileSummary { readonly id: ProfileId; readonly label: string }

export interface ConversationSummary {
  readonly id: ConversationId;
  readonly personId: PersonId;
  readonly displayName: string;
  readonly lastMessageAt: number | null;
  readonly notices: readonly SecurityNotice[];
}

export type ConversationTimelineItem =
  | { readonly direction: "outgoing"; readonly messageId: MessageId; readonly operationId: OperationId; readonly text: string; readonly sentAt: number; readonly delivery: DeliveryState }
  | { readonly direction: "incoming"; readonly messageId: MessageId; readonly text: string; readonly sentAt: number };

export interface ConversationView { readonly summary: ConversationSummary; readonly timeline: readonly ConversationTimelineItem[] }

export interface PendingOperationView { readonly operationId: OperationId; readonly intentKind: CoreIntentKind; readonly outcome: OutcomeClass }

/** Display-once secret. Core returns it only until ConfirmRecoveryKeyStored; UI must never persist or log it. */
export interface RecoveryKeyView { readonly keyOperationId: OperationId; readonly groups: readonly string[] }
export interface InviteView { readonly operationId: OperationId; readonly link: string; readonly expiresAt: number }

/** Machine codes only (who can see what at this Home tier); the UI owns the wording. */
export type HomeObserverCode = "sponsor_sees_metadata" | "host_sees_metadata" | "host_sees_ciphertext" | "no_third_party";
export type ClientOriginChoiceCode = "default_origin" | "independent_origin_available" | "independent_origin_active";
export interface HomeSummary {
  /** `T0`, `T1`, `T2`, ... */
  readonly tier: string;
  readonly observers: readonly HomeObserverCode[];
  readonly migrationAvailable: boolean;
  readonly clientOriginChoice: readonly ClientOriginChoiceCode[];
}
