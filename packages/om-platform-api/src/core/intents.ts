import type { RecoveryCustodianCode, RecoveryScope } from "./devices";
import type { ConversationId, DeviceId, MessageId, OperationId, PersonId, ProfileExecutionContext, ProfileId, Brand } from "./ids";

/**
 * Proof of an explicit user confirmation: a single-use nonce that Core issued (`IssueConfirmation`) for exactly one
 * destructive intent kind and target in this profile, and that expires. The Action Registry only requests one for a
 * direct user action. Core answers `Rejected(unauthorized)` for anything else (UI-11, ADR-0181).
 */
export type ConfirmationToken = Brand<{ readonly nonce: string }, "ConfirmationToken">;

export type PrivacyPolicyLevel = "recommended" | "hardened" | "maximum" | "custom";

interface IntentBase {
  readonly ctx: ProfileExecutionContext;
  /** Idempotency key: re-dispatching the same id never creates a second semantic operation. */
  readonly operationId: OperationId;
}

/** What is composed: the text, and optionally the message of the same conversation it replies to (`reply_to`). */
export interface Draft { readonly text: string; readonly replyTo?: MessageId }
export interface SendMessage extends IntentBase { readonly kind: "SendMessage"; readonly conversationId: ConversationId; readonly draft: Draft }
/** A mention in `draft.text`: the code-point range `[start, end)` that is the label (with its `@`) and the person's AccountID (hex). */
export interface MentionInput { readonly start: number; readonly end: number; readonly accountId: string }
/** A text message with mentions (ADR-0222). Otherwise as `SendMessage`. */
export interface SendMessageMentioning extends IntentBase { readonly kind: "SendMessageMentioning"; readonly conversationId: ConversationId; readonly draft: Draft; readonly mentions: readonly MentionInput[] }
/** Files the host prepared with `AttachmentApi.prepare` (optionally with text). Each is uploaded first; the message goes out once every one is stored. */
export interface SendAttachments extends IntentBase {
  readonly kind: "SendAttachments";
  readonly conversationId: ConversationId;
  readonly draft: Draft;
  readonly attachments: readonly { readonly blobId: string; readonly voiceNote?: { readonly waveform?: string } }[];
}
/**
 * A file too big for the delivery service, offered to be sent while the other person is online. The files were prepared with
 * `AttachmentApi.prepareDirect`. The message goes out at once; only the file waits.
 */
export interface SendAttachmentsDirect extends IntentBase {
  readonly kind: "SendAttachmentsDirect";
  readonly conversationId: ConversationId;
  readonly draft: Draft;
  readonly attachments: readonly { readonly blobId: string; readonly voiceNote?: { readonly waveform?: string } }[];
}
/** Take a direct offer back: nothing is served any more. The message stays and says the file was taken back. */
export interface WithdrawDirectOffer extends IntentBase { readonly kind: "WithdrawDirectOffer"; readonly messageId: MessageId }
/** One-shot static location (`om.location-card.v1`). Never live location. E7 = degrees times ten million. */
export interface SendLocation extends IntentBase {
  readonly kind: "SendLocation";
  readonly conversationId: ConversationId;
  readonly latE7: number;
  readonly lonE7: number;
  readonly accuracyM?: number;
  readonly label?: string;
  readonly note?: string;
}
/** Add or remove your reaction (a Unicode token, 1 to 64 bytes) on a message of the conversation. Per person, message and key; the latest wins on every device. */
export interface SetReaction extends IntentBase { readonly kind: "SetReaction"; readonly conversationId: ConversationId; readonly messageId: MessageId; readonly reaction: string; readonly present: boolean }
/** Replace the text of one of your own messages (`message.edit`). Text only: a photo or file is never replaced. */
export interface EditMessage extends IntentBase { readonly kind: "EditMessage"; readonly conversationId: ConversationId; readonly messageId: MessageId; readonly text: string }
/** Delete one of your own messages for everyone in the conversation (`message.delete`). Does not promise a copy already read is gone. */
export interface DeleteMessage extends IntentBase { readonly kind: "DeleteMessage"; readonly conversationId: ConversationId; readonly messageId: MessageId }
export interface RetryDelivery extends IntentBase { readonly kind: "RetryDelivery"; readonly messageId: MessageId }
export interface SwitchIdentityProfile extends IntentBase { readonly kind: "SwitchIdentityProfile"; readonly targetProfileId: ProfileId }
export interface AcceptIntroduction extends IntentBase { readonly kind: "AcceptIntroduction"; readonly requestId: string }
export interface BlockPerson extends IntentBase { readonly kind: "BlockPerson"; readonly personId: PersonId; readonly scope: "local" | "provider"; readonly confirmation: ConfirmationToken }
export interface ShareContact extends IntentBase { readonly kind: "ShareContact"; readonly personId: PersonId; readonly scope: "name_only" | "full" }
export interface SuspendDevice extends IntentBase { readonly kind: "SuspendDevice"; readonly deviceId: DeviceId; readonly confirmation: ConfirmationToken }
export interface ChangePrivacyPolicy extends IntentBase { readonly kind: "ChangePrivacyPolicy"; readonly policy: PrivacyPolicyLevel; readonly confirmation: ConfirmationToken }
export interface ClearAdaptiveNavigation extends IntentBase { readonly kind: "ClearAdaptiveNavigation" }
export interface OpenOmnibarResult extends IntentBase { readonly kind: "OpenOmnibarResult"; readonly resultId: string }
export interface ShareStaticLocation extends IntentBase { readonly kind: "ShareStaticLocation"; readonly conversationId: ConversationId; readonly locationCardId: string }
export interface RevokeInvite extends IntentBase { readonly kind: "RevokeInvite"; readonly inviteId: string }
export interface StartCall extends IntentBase { readonly kind: "StartCall"; readonly target: PersonId; readonly media: "audio" | "video"; readonly sdp?: readonly number[] }
export interface AcceptCall extends IntentBase { readonly kind: "AcceptCall"; readonly callSessionId: string; readonly sdp: readonly number[]; readonly mediaFlags: number }
export interface DeclineCall extends IntentBase { readonly kind: "DeclineCall"; readonly callSessionId: string }
export interface EndCall extends IntentBase { readonly kind: "EndCall"; readonly callSessionId: string }
export interface UpdateCallIce extends IntentBase { readonly kind: "UpdateCallIce"; readonly callSessionId: string; readonly sdp: readonly number[] }
export interface SetCallMedia extends IntentBase { readonly kind: "SetCallMedia"; readonly callSessionId: string; readonly mediaFlags: number }
export interface SetContactAllowCalls extends IntentBase { readonly kind: "SetContactAllowCalls"; readonly conversationId: ConversationId; readonly allowed: boolean }
export interface PinNavigationTarget extends IntentBase { readonly kind: "PinNavigationTarget"; readonly targetId: string }
export interface RestoreHistory extends IntentBase { readonly kind: "RestoreHistory"; readonly source: "linked_device" | "encrypted_backup" }

export interface CreateIdentityProfile extends IntentBase { readonly kind: "CreateIdentityProfile"; readonly label: string }
/** `keyOperationId` is the CreateIdentityProfile operation whose RecoveryKeyView the user confirmed. */
export interface ConfirmRecoveryKeyStored extends IntentBase { readonly kind: "ConfirmRecoveryKeyStored"; readonly keyOperationId: OperationId }
/** Asks Core for a confirmation nonce bound to a destructive intent `intentKind` and its `target` (see `confirmationTarget`). */
export interface IssueConfirmation extends IntentBase { readonly kind: "IssueConfirmation"; readonly intentKind: DestructiveIntentKind; readonly target: string }
export interface CreateInvite extends IntentBase { readonly kind: "CreateInvite" }
export interface ReconnectContact extends IntentBase { readonly kind: "ReconnectContact"; readonly accountId: string }
export interface AcceptInvitation extends IntentBase { readonly kind: "AcceptInvitation"; readonly link: string }

// Device linking (NoSilentLink): the new device offers, the existing device opens the offer, sees who is asking and the
// shared code, then explicitly approves or rejects. There is no field or intent for a link without enrollment notice.
/** New device: create a short-lived link offer to show as a QR code or link. */
export interface OfferDeviceLink extends IntentBase { readonly kind: "OfferDeviceLink" }
/** Existing device: open a scanned/pasted `om:link/` offer. Produces a LinkRequestView; links nothing by itself. */
export interface RequestDeviceLink extends IntentBase { readonly kind: "RequestDeviceLink"; readonly link: string }
/** Existing device: explicit approval after the user compared the verification code. Core may still need authorization. */
export interface ApproveDeviceLink extends IntentBase { readonly kind: "ApproveDeviceLink"; readonly requestId: string; readonly confirmation: ConfirmationToken }
export interface RejectDeviceLink extends IntentBase { readonly kind: "RejectDeviceLink"; readonly requestId: string }
/** Try again now to tell a device about the devices added to the account (ADR-0193). Removal reuses `RevokeDevice`. */
export interface RetryDeviceNotice extends IntentBase { readonly kind: "RetryDeviceNotice"; readonly deviceId: DeviceId }
/** "Only add a device when all my other devices can be told right away": device-local, off by default (ADR-0193 section 8). */
export interface SetStrictDeviceNotice extends IntentBase { readonly kind: "SetStrictDeviceNotice"; readonly enabled: boolean }
/** Permanent removal (Stage B), distinct from SuspendDevice. May need authorization; terminal for that DeviceID. */
/** A name for a contact that only this device's person sees (ADR-0196). Empty or absent clears it. Local: never sent to the contact, a group or a relay. */
export interface SetContactNickname extends IntentBase { readonly kind: "SetContactNickname"; readonly conversationId: ConversationId; readonly nickname?: string }
export interface RevokeDevice extends IntentBase { readonly kind: "RevokeDevice"; readonly deviceId: DeviceId; readonly confirmation: ConfirmationToken }
export interface StartRecovery extends IntentBase { readonly kind: "StartRecovery"; readonly scope: RecoveryScope; readonly custodian: RecoveryCustodianCode; readonly confirmation: ConfirmationToken }
/** Stops an in-progress recovery: a veto for one started elsewhere, a cancel for one started here. Timing is Core's. */
export interface VetoRecovery extends IntentBase { readonly kind: "VetoRecovery"; readonly requestId: string; readonly confirmation: ConfirmationToken }

/** "That was me" on an incoming recovery: this controller co-signs the recovering device's successor, completing it at once. */
export interface ConfirmRecovery extends IntentBase { readonly kind: "ConfirmRecovery"; readonly requestId: string; readonly confirmation: ConfirmationToken }

/** "This isn't them" on a contact's recovery of their account (ADR-0190 section 4): ends that request for this contact only, tells no one. `requestId` is `PeerRecoveryView.requestId`. */
export interface DeclinePeerRecovery extends IntentBase { readonly kind: "DeclinePeerRecovery"; readonly conversationId: ConversationId; readonly requestId: string; readonly confirmation: ConfirmationToken }
/** "Yes, it's them" once the wait is over; Hardened and Maximum need it before the contact accepts. */
export interface ConfirmPeerRecovery extends IntentBase { readonly kind: "ConfirmPeerRecovery"; readonly conversationId: ConversationId; readonly requestId: string; readonly confirmation: ConfirmationToken }

/**
 * On a profile with no device and no account: the key sheet (ADR-0200: any text form or the QR payload; it names where the account is
 * saved) or the older key-only text. A key-only entry says where through `place` (a place-only string) or, as before, `home`, which names
 * no scope and so only keeps the entry. Core keeps the key and the place in the restricted recovery state and asks the Home for the
 * account's record; it grants no authority and sends nothing else.
 */
export interface BeginRecovery extends IntentBase { readonly kind: "BeginRecovery"; readonly recoveryKey: string; readonly home?: string; readonly place?: string }

/** On a recovering device whose record was found: ask for the account back (ADR-0202). Explicit user action only; it grants nothing until the device's own interval, the checks and the owner's own checkpoint all hold. */
export interface RequestRecovery extends IntentBase { readonly kind: "RequestRecovery" }
/** Close this device's own recovery attempt. */
export interface CancelRecovery extends IntentBase { readonly kind: "CancelRecovery" }
/** Ask again now: re-read the record (before an attempt) or the Homes' boards and record (during one). */
export interface CheckRecoveryAgain extends IntentBase { readonly kind: "CheckRecoveryAgain" }

/** On a surviving controller (a live control device): open a short, single-use recovery approval exchange (ADR-0203). It shows a private `om:recover-approve/` link or QR for the restricted device; the person then confirms the exact request with `ConfirmRecovery`. */
export interface OfferRecoveryApproval extends IntentBase { readonly kind: "OfferRecoveryApproval" }
/** A controller device makes a new recovery key (ADR-0205): shown once under this operation id; nothing changes until `ConfirmRecoveryKeyStored` names it. */
export interface ReplaceRecoveryKey extends IntentBase { readonly kind: "ReplaceRecoveryKey" }
/** On the restricted device with its request waiting: send that exact request to the controller whose `om:recover-approve/` link it was given. An enrollment link is refused. */
export interface JoinRecoveryApproval extends IntentBase { readonly kind: "JoinRecoveryApproval"; readonly link: string }

export interface OfferControllerChange extends IntentBase { readonly kind: "OfferControllerChange"; readonly deviceId: DeviceId; readonly remove: boolean }
export interface JoinControllerChange extends IntentBase { readonly kind: "JoinControllerChange"; readonly link: string }
export interface ConfirmControllerChange extends IntentBase { readonly kind: "ConfirmControllerChange"; readonly requestId: string; readonly confirmation: ConfirmationToken }
export interface CancelControllerChange extends IntentBase { readonly kind: "CancelControllerChange"; readonly requestId: string }

export type CoreIntent =
  | OfferControllerChange | JoinControllerChange | ConfirmControllerChange | CancelControllerChange
  | SendMessage | SendMessageMentioning | SendAttachments | SendAttachmentsDirect | WithdrawDirectOffer | SendLocation | SetReaction | EditMessage | DeleteMessage | RetryDelivery | SwitchIdentityProfile | AcceptIntroduction | BlockPerson | ShareContact
  | SuspendDevice | ChangePrivacyPolicy | ClearAdaptiveNavigation | OpenOmnibarResult | ShareStaticLocation
  | RevokeInvite | StartCall | AcceptCall | DeclineCall | EndCall | UpdateCallIce | SetCallMedia | SetContactAllowCalls | PinNavigationTarget | RestoreHistory
  | CreateIdentityProfile | ConfirmRecoveryKeyStored | IssueConfirmation | CreateInvite | ReconnectContact | AcceptInvitation
  | OfferDeviceLink | RequestDeviceLink | ApproveDeviceLink | RejectDeviceLink | RetryDeviceNotice | SetStrictDeviceNotice | SetContactNickname | RevokeDevice | StartRecovery | VetoRecovery | ConfirmRecovery | DeclinePeerRecovery | ConfirmPeerRecovery | BeginRecovery | RequestRecovery | CancelRecovery | CheckRecoveryAgain | OfferRecoveryApproval | JoinRecoveryApproval | ReplaceRecoveryKey;

export type CoreIntentKind = CoreIntent["kind"];

export const INTENT_KINDS: readonly CoreIntentKind[] = [
  "OfferControllerChange", "JoinControllerChange", "ConfirmControllerChange", "CancelControllerChange",
  "SendMessage", "SendMessageMentioning", "SendAttachments", "SendAttachmentsDirect", "WithdrawDirectOffer", "SendLocation", "SetReaction", "EditMessage", "DeleteMessage", "RetryDelivery", "SwitchIdentityProfile", "AcceptIntroduction", "BlockPerson", "ShareContact",
  "SuspendDevice", "ChangePrivacyPolicy", "ClearAdaptiveNavigation", "OpenOmnibarResult", "ShareStaticLocation",
  "RevokeInvite", "StartCall", "AcceptCall", "DeclineCall", "EndCall", "UpdateCallIce", "SetCallMedia", "SetContactAllowCalls", "PinNavigationTarget", "RestoreHistory",
  "CreateIdentityProfile", "ConfirmRecoveryKeyStored", "IssueConfirmation", "CreateInvite", "ReconnectContact", "AcceptInvitation",
  "OfferDeviceLink", "RequestDeviceLink", "ApproveDeviceLink", "RejectDeviceLink", "RetryDeviceNotice", "SetStrictDeviceNotice", "SetContactNickname", "RevokeDevice", "StartRecovery", "VetoRecovery", "ConfirmRecovery", "DeclinePeerRecovery", "ConfirmPeerRecovery", "BeginRecovery", "RequestRecovery", "CancelRecovery", "CheckRecoveryAgain", "OfferRecoveryApproval", "JoinRecoveryApproval", "ReplaceRecoveryKey",
];

/** Intents that require an explicit confirmation token; learned/ranked suggestions can never supply one (UI-11). */
export const DESTRUCTIVE_INTENT_KINDS = [
  "BlockPerson", "SuspendDevice", "ChangePrivacyPolicy", "ApproveDeviceLink", "RevokeDevice", "StartRecovery", "VetoRecovery", "ConfirmRecovery", "DeclinePeerRecovery", "ConfirmPeerRecovery", "ConfirmControllerChange",
] as const;
export type DestructiveIntentKind = (typeof DESTRUCTIVE_INTENT_KINDS)[number];

/**
 * The target a confirmation nonce is bound to, per destructive intent: the same string Core derives from the intent
 * (`confirmation_target` in `api.rs`). A mismatch is `Rejected(unauthorized)`.
 */
export function confirmationTarget(intent: Extract<CoreIntent, { readonly confirmation: ConfirmationToken }>): string {
  switch (intent.kind) {
    case "BlockPerson": return intent.personId;
    case "SuspendDevice": case "RevokeDevice": return intent.deviceId;
    case "ConfirmControllerChange": case "ApproveDeviceLink": case "VetoRecovery": case "ConfirmRecovery": case "DeclinePeerRecovery": case "ConfirmPeerRecovery": return intent.requestId;
    case "ChangePrivacyPolicy": return intent.policy;
    case "StartRecovery": return `${intent.scope}:${intent.custodian}`;
  }
}
