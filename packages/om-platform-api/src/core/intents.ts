import type { ConversationId, MessageId, OperationId, PersonId, ProfileExecutionContext, ProfileId, Brand } from "./ids";

/** Proof of an explicit user confirmation. Minted only by the Action Registry's confirmation flow. */
export type ConfirmationToken = Brand<object, "ConfirmationToken">;

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
export interface SuspendDevice extends IntentBase { readonly kind: "SuspendDevice"; readonly deviceId: string; readonly confirmation: ConfirmationToken }
export interface ChangePrivacyPolicy extends IntentBase { readonly kind: "ChangePrivacyPolicy"; readonly policy: PrivacyPolicyLevel; readonly confirmation: ConfirmationToken }
export interface ClearAdaptiveNavigation extends IntentBase { readonly kind: "ClearAdaptiveNavigation" }
export interface OpenOmnibarResult extends IntentBase { readonly kind: "OpenOmnibarResult"; readonly resultId: string }
export interface ShareStaticLocation extends IntentBase { readonly kind: "ShareStaticLocation"; readonly conversationId: ConversationId; readonly locationCardId: string }
export interface RevokeInvite extends IntentBase { readonly kind: "RevokeInvite"; readonly inviteId: string }
export interface StartCall extends IntentBase { readonly kind: "StartCall"; readonly target: PersonId; readonly media: "audio" | "video" }
export interface PinNavigationTarget extends IntentBase { readonly kind: "PinNavigationTarget"; readonly targetId: string }
export interface RestoreHistory extends IntentBase { readonly kind: "RestoreHistory"; readonly source: "linked_device" | "encrypted_backup" }

export type CoreIntent =
  | SendMessage | RetryDelivery | SwitchIdentityProfile | AcceptIntroduction | BlockPerson | ShareContact
  | SuspendDevice | ChangePrivacyPolicy | ClearAdaptiveNavigation | OpenOmnibarResult | ShareStaticLocation
  | RevokeInvite | StartCall | PinNavigationTarget | RestoreHistory;

export type CoreIntentKind = CoreIntent["kind"];

export const INTENT_KINDS: readonly CoreIntentKind[] = [
  "SendMessage", "RetryDelivery", "SwitchIdentityProfile", "AcceptIntroduction", "BlockPerson", "ShareContact",
  "SuspendDevice", "ChangePrivacyPolicy", "ClearAdaptiveNavigation", "OpenOmnibarResult", "ShareStaticLocation",
  "RevokeInvite", "StartCall", "PinNavigationTarget", "RestoreHistory",
];

/** Intents that require an explicit confirmation token; learned/ranked suggestions can never supply one (UI-11). */
export const DESTRUCTIVE_INTENT_KINDS = ["BlockPerson", "SuspendDevice", "ChangePrivacyPolicy"] as const;
export type DestructiveIntentKind = (typeof DESTRUCTIVE_INTENT_KINDS)[number];
