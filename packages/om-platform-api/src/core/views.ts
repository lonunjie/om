import type { ConversationId, MessageId, OperationId, PersonId, ProfileId } from "./ids";
import type { CoreIntentKind } from "./intents";
import type { OutcomeClass, RejectReason } from "./outcome";

export type DeliveryState =
  | { readonly state: "pending_network" }
  | { readonly state: "pending_reconciliation" }
  /** Written while the contact is recovering their account: kept on this device, sent once their new device is confirmed (ADR-0190 section 5, D131). */
  | { readonly state: "waiting_for_peer_recovery" }
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
  /** The name the contact chose for itself: the original, never replaced by `nickname`. */
  readonly displayName: string;
  /** The name this device's person gave the contact (ADR-0196): local, never sent. Shown instead of `displayName`; absent when none. */
  readonly nickname?: string;
  /** Omitted when there is no established, unblocked contact. */
  readonly allowCalls?: boolean;
  readonly lastMessageAt: number | null;
  readonly notices: readonly SecurityNotice[];
  readonly relationship: RelationshipStateCode;
  /** Hex of the contact's account once bound; absent before that and for restored history. Two entries with the same account are one person. */
  readonly peerAccount?: string;
  readonly relinkedFrom?: ConversationId;
  /** The recovery of this person's account this device is tracking (ADR-0190 section 4); absent when there is none. */
  readonly peerRecovery?: PeerRecoveryView;
}

/** Where this device stands on one recovery of a contact's account. `stale` is never shown (the account moved on). */
export type PeerRecoveryState = "waiting" | "ready" | "accepted" | "vetoed" | "declined" | "stale";

/**
 * One tracked recovery of a contact's account, as the conversation header needs it. No digest, epoch, policy text or
 * clock: only whole days left of THIS device's own interval, which a UI shows and never computes (ADR-0190 sections 4, 6).
 */
export interface PeerRecoveryView {
  /** Names the request in `ConfirmPeerRecovery` and `DeclinePeerRecovery` (Details only; never shown as copy). */
  readonly requestId: string;
  readonly state: PeerRecoveryState;
  /** Whole days left (rounded up; 0 once the wait ran). */
  readonly daysLeft: number;
  /** "This isn't them" is offered. */
  readonly canDecline: boolean;
  /** This device's own confirmation is required before the recovery can finish (Hardened and Maximum). */
  readonly needsConfirm: boolean;
  /** The recovering account asked that contacts see a completion notice. */
  readonly notifyOnCompletion: boolean;
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
  /** Ours: their device says it has it and nothing has checked that yet (ADR-0194). Never shown as delivered. */
  | "peer_says_received"
  /** Ours: their device checked the file and has it (an authenticated receipt). That device only; not a copy kept anywhere else (AT-06). */
  | "delivered_locally"
  /** Ours: they said they had it, the offer lapsed, and it was never confirmed (ADR-0194). */
  | "not_confirmed"
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
/**
 * The message a reply answers, as this device can show it. The quoted text is read from the local copy when the timeline is built and is never
 * carried in the reply itself (only the EventID is). When the original is missing, deleted or expired, `available` is false and nothing else is set.
 */
export interface ReplyView {
  readonly messageId: MessageId;
  readonly available: boolean;
  /** The original is our own message. */
  readonly outgoing?: boolean;
  /** The start of the original's text, at most 140 characters. Empty for an attachment-only original. */
  readonly preview?: string;
  readonly hasAttachment?: boolean;
}
/** One reaction on a message: how many people have it set (at most two in a one-to-one chat) and whether one of them is you. */
export interface ReactionView { readonly key: string; readonly count: number; readonly mine: boolean }
/** A mention in a message text: a code-point range, the person (hex AccountID) and whether it names us (ADR-0222). */
export interface MentionView { readonly start: number; readonly end: number; readonly accountId: string; readonly me?: boolean }
interface TimelineExtras { readonly mentions?: readonly MentionView[]; readonly readOnly?: boolean; readonly attachments?: readonly AttachmentView[]; readonly presentation?: PresentationView; readonly reply?: ReplyView; readonly reactions?: readonly ReactionView[]; /** An accepted edit replaced the text. */ readonly edited?: boolean }

export type ConversationTimelineItem =
  | ({ readonly direction: "outgoing"; readonly messageId: MessageId; readonly operationId: OperationId; readonly text: string; readonly sentAt: number; readonly delivery: DeliveryState } & TimelineExtras)
  | ({ readonly direction: "incoming"; readonly messageId: MessageId; readonly text: string; readonly sentAt: number } & TimelineExtras);

export interface ConversationView {
  readonly summary: ConversationSummary;
  readonly timeline: readonly ConversationTimelineItem[];
  /** This profile's AccountID in an active, bound chat, for private account comparison. */
  readonly ownAccount?: string;
}

/** Advisory contact and progress of an explicitly started invitation after owner recovery. */
export interface ReconnectContactView {
  readonly accountId: string;
  readonly label: string;
  readonly inviteOperationId: OperationId | null;
  readonly invite: InviteView | null;
  readonly conversationId: ConversationId | null;
}

export interface PendingOperationView { readonly operationId: OperationId; readonly intentKind: CoreIntentKind; readonly outcome: OutcomeClass }

/** Display-once secret. Core returns it only until ConfirmRecoveryKeyStored; UI must never persist or log it. */
export interface RecoveryKeyView {
  readonly keyOperationId: OperationId;
  readonly groups: readonly string[];
  /** The key sheet text (ADR-0200: the key with its place) when this account has a recovery place; absent when the key is shown alone. */
  readonly sheet?: string;
}
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


export type CallState = "incoming" | "starting" | "ringing" | "connecting" | "active" | "reconnecting" | "ending" | "ended" | "failed";
export type CallReason = "busy" | "declined" | "answered_elsewhere" | "timeout" | "failed" | "hangup" | "canceled";
export type CallRoute = "lan_direct" | "direct" | "relay";

export interface CallView {
  readonly callSessionId: string;
  readonly conversationId: ConversationId;
  readonly media: "audio" | "video";
  readonly state: CallState;
  readonly outgoing: boolean;
  readonly ringDeadlineMs: number | null;
  readonly reason: CallReason | null;
  readonly localMediaFlags: number;
  readonly peerMediaFlags: number;
  readonly route: CallRoute | null;
}

export interface CallNotice {
  readonly callSessionId: string;
  readonly conversationId: ConversationId;
  readonly reason: CallReason;
  readonly atMs: number;
  readonly folded: boolean;
}
