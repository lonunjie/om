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
/**
 * `unverified`: the requester's intro key is not bound to any account yet, so `displayName` is a label the requester
 * chose for itself. `bound`: an IntroIdentityBinding verified. A pending request is always `unverified` (ADR-0181).
 */
export type VerificationCode = "unverified" | "bound";
export interface IntroductionRequest { readonly id: string; readonly from: PersonId; readonly displayName: string; readonly verification: VerificationCode }
/** Whether first-contact requests from strangers are being dropped (a machine flag; the UI owns the notice copy). */
export interface IntroductionLoad { readonly throttled: boolean; readonly dropped: number }
/** A Core-issued, single-use confirmation nonce (UI-11); read once after `IssueConfirmation`. */
export interface ConfirmationView { readonly nonce: string; readonly expiresAt: number }
export interface ProfileSummary { readonly id: ProfileId; readonly label: string }

/**
 * Where a relationship is in first contact. `accepted_unbound`: accepted, but both IntroIdentityBindings have not
 * verified, so the contact is unverified and nothing can be sent. `establishing`: both verified, session being set up.
 * `restored`: history brought back from a backup file (ADR-0184): read-only, it has no connection to send or receive on.
 */
export type RelationshipStateCode = "awaiting_acceptance" | "accepted_unbound" | "establishing" | "active" | "restored";

export interface ConversationSummary {
  readonly id: ConversationId;
  readonly personId: PersonId;
  readonly displayName: string;
  readonly lastMessageAt: number | null;
  readonly notices: readonly SecurityNotice[];
  readonly relationship: RelationshipStateCode;
}

/** Where an attachment's bytes are. "Stored for later" is a hand-over to a store with a lifetime, not delivery (ADR-0150). */
export type AttachmentState =
  | "uploading" | "stored_for_later" | "remote"
  /** Theirs, offered straight from the sender's device: fetchable only while the sender is online; nothing is stored for later. */
  | "remote_direct"
  /** Ours, too big for the delivery service: waiting to be sent while the other person is online. */
  | "waiting_for_recipient"
  /** Ours: being sent to the other person right now. */
  | "transferring"
  /** Ours: the other person's device has it. That device only; not a copy kept anywhere else (AT-06). */
  | "delivered_locally"
  /** Ours: no longer offered (taken back, or it lapsed unfetched). */
  | "not_offered";

/** How a file may travel, decided before anything is sealed or sent (`PlanAttachment`). `limit` is a byte count. */
export type AttachmentPlanView =
  | { readonly kind: "spool" }
  /** Too big for the delivery service: not sent that way. It can be offered straight to the other person while they are online. `limit`: the largest file the service takes (0: none is set up). */
  | { readonly kind: "offer_direct"; readonly limit: number }
  | { readonly kind: "too_large"; readonly limit: number }
  | { readonly kind: "empty" };

/** One direct offer of this profile (`GetDirectOffers`, newest last). `needsFile` is true after a restart until the host hands the file back; nothing is served meanwhile. */
export interface DirectOfferView {
  readonly conversationId: string;
  readonly messageId: string;
  readonly blobId: string;
  readonly size: number;
  readonly state: AttachmentState;
  /** Wall ms after which the offer lapses. */
  readonly expiresAt: number;
  readonly needsFile: boolean;
}
/** Sender-authored display data (untrusted) except `size`. The store sees `size` plus a fixed 16 bytes: size is not hidden from it. */
export interface AttachmentView {
  readonly index: number;
  readonly blobId: string;
  readonly mediaType: string;
  readonly size: number;
  readonly filename?: string;
  readonly width?: number;
  readonly height?: number;
  readonly durationMs?: number;
  readonly state: AttachmentState;
  /** Wall ms the store keeps our copy until (outgoing only; a display hint). */
  readonly storedUntil?: number;
}
export interface VoiceNoteView { readonly attachmentIndex: number; /** Untrusted hint, 16..256 samples. */ readonly waveform?: readonly number[] }
export interface LocationCardView {
  readonly latE7: number;
  readonly lonE7: number;
  /** The canonical `geo:` token, the same text the message carries. */
  readonly geo: string;
  readonly accuracyM?: number;
  readonly label?: string;
}
/** Present only while the message's extensions are valid for the message as it currently reads. */
export interface PresentationView { readonly voiceNote?: VoiceNoteView; readonly locationCard?: LocationCardView }
interface TimelineExtras { readonly attachments?: readonly AttachmentView[]; readonly presentation?: PresentationView }

export type ConversationTimelineItem =
  | ({ readonly direction: "outgoing"; readonly messageId: MessageId; readonly operationId: OperationId; readonly text: string; readonly sentAt: number; readonly delivery: DeliveryState } & TimelineExtras)
  | ({ readonly direction: "incoming"; readonly messageId: MessageId; readonly text: string; readonly sentAt: number } & TimelineExtras);

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
