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

export interface SendMessage extends IntentBase { readonly kind: "SendMessage"; readonly conversationId: ConversationId; readonly draft: { readonly text: string } }
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
export interface StartCall extends IntentBase { readonly kind: "StartCall"; readonly target: PersonId; readonly media: "audio" | "video" }
export interface PinNavigationTarget extends IntentBase { readonly kind: "PinNavigationTarget"; readonly targetId: string }
export interface RestoreHistory extends IntentBase { readonly kind: "RestoreHistory"; readonly source: "linked_device" | "encrypted_backup" }

export interface CreateIdentityProfile extends IntentBase { readonly kind: "CreateIdentityProfile"; readonly label: string }
/** `keyOperationId` is the CreateIdentityProfile operation whose RecoveryKeyView the user confirmed. */
export interface ConfirmRecoveryKeyStored extends IntentBase { readonly kind: "ConfirmRecoveryKeyStored"; readonly keyOperationId: OperationId }
/** Asks Core for a confirmation nonce bound to a destructive intent `intentKind` and its `target` (see `confirmationTarget`). */
export interface IssueConfirmation extends IntentBase { readonly kind: "IssueConfirmation"; readonly intentKind: DestructiveIntentKind; readonly target: string }
export interface CreateInvite extends IntentBase { readonly kind: "CreateInvite" }
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
/** Permanent removal (Stage B), distinct from SuspendDevice. May need authorization; terminal for that DeviceID. */
export interface RevokeDevice extends IntentBase { readonly kind: "RevokeDevice"; readonly deviceId: DeviceId; readonly confirmation: ConfirmationToken }
export interface StartRecovery extends IntentBase { readonly kind: "StartRecovery"; readonly scope: RecoveryScope; readonly custodian: RecoveryCustodianCode; readonly confirmation: ConfirmationToken }
/** Stops an in-progress recovery: a veto for one started elsewhere, a cancel for one started here. Timing is Core's. */
export interface VetoRecovery extends IntentBase { readonly kind: "VetoRecovery"; readonly requestId: string; readonly confirmation: ConfirmationToken }

export type CoreIntent =
  | SendMessage | RetryDelivery | SwitchIdentityProfile | AcceptIntroduction | BlockPerson | ShareContact
  | SuspendDevice | ChangePrivacyPolicy | ClearAdaptiveNavigation | OpenOmnibarResult | ShareStaticLocation
  | RevokeInvite | StartCall | PinNavigationTarget | RestoreHistory
  | CreateIdentityProfile | ConfirmRecoveryKeyStored | IssueConfirmation | CreateInvite | AcceptInvitation
  | OfferDeviceLink | RequestDeviceLink | ApproveDeviceLink | RejectDeviceLink | RevokeDevice | StartRecovery | VetoRecovery;

export type CoreIntentKind = CoreIntent["kind"];

export const INTENT_KINDS: readonly CoreIntentKind[] = [
  "SendMessage", "RetryDelivery", "SwitchIdentityProfile", "AcceptIntroduction", "BlockPerson", "ShareContact",
  "SuspendDevice", "ChangePrivacyPolicy", "ClearAdaptiveNavigation", "OpenOmnibarResult", "ShareStaticLocation",
  "RevokeInvite", "StartCall", "PinNavigationTarget", "RestoreHistory",
  "CreateIdentityProfile", "ConfirmRecoveryKeyStored", "IssueConfirmation", "CreateInvite", "AcceptInvitation",
  "OfferDeviceLink", "RequestDeviceLink", "ApproveDeviceLink", "RejectDeviceLink", "RevokeDevice", "StartRecovery", "VetoRecovery",
];

/** Intents that require an explicit confirmation token; learned/ranked suggestions can never supply one (UI-11). */
export const DESTRUCTIVE_INTENT_KINDS = [
  "BlockPerson", "SuspendDevice", "ChangePrivacyPolicy", "ApproveDeviceLink", "RevokeDevice", "StartRecovery", "VetoRecovery",
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
    case "ApproveDeviceLink": case "VetoRecovery": return intent.requestId;
    case "ChangePrivacyPolicy": return intent.policy;
    case "StartRecovery": return `${intent.scope}:${intent.custodian}`;
  }
}
