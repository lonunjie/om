import type {
  ConversationId,
  OutcomeClass,
  RejectReason,
} from "@om/platform-api";

export type BridgeDirection = "outbound" | "inbound" | "both";
export type BridgeMappingState =
  | "active"
  | "revoked"
  | "stale"
  | "endpoint_stopped";

/** Event classes from the W4 mapping, named by the host using the event_kind registry. */
export type BridgeEventClass =
  | "message.create"
  | "message.edit"
  | "message.delete"
  | "reaction.set"
  | "receipt.update"
  | "pin.set"
  | "moderation.redact"
  | "poll.create"
  | "poll.vote"
  | "poll.close"
  | "topic.create"
  | "topic.update"
  | "topic.status.set"
  | "message.topic.set"
  | "group.invite"
  | "group.accept"
  | "group.decline"
  | "group.rejoin_request";

export type BridgeFidelityFlag =
  | "exact"
  | "lossy_formatting"
  | "reply_target_unavailable"
  | "edit_represented_as_new_message"
  | "delete_not_propagated"
  | "reaction_not_supported"
  | "attachment_transcoded"
  | "foreign_actor_unverified"
  | "origin_unverified";

/** Carry states whose meaning is more specific than the accompanying fidelity flag. */
export type BridgeCarry = "flattened" | "read_only_summary" | "not_carried";

export interface BridgePrincipalView {
  readonly id: string;
  readonly name: string;
}

export interface BridgeMappingView {
  readonly id: string;
  readonly network: string;
  readonly scope: string;
  readonly direction: BridgeDirection;
  /** Effective export classes reported by Core. Empty when there is no export in this mapping. */
  readonly exportedClasses: readonly BridgeEventClass[];
  /** An advertised claim about the external leg, or null when no claim is known. */
  readonly externalClaim: string | null;
}

export interface BridgeIntent {
  readonly kind: "admit" | "remove";
  /** Opaque Core-issued reference to the exact offer or membership incarnation reviewed. */
  readonly target: string;
}

type BridgeAction =
  | { readonly availability: "ready"; readonly intent: BridgeIntent }
  | {
      readonly availability: "blocked";
      readonly kind: BridgeIntent["kind"];
      readonly reason: RejectReason;
    };

export type BridgeEntryView = {
  readonly principal: BridgePrincipalView;
  readonly action: BridgeAction | null;
} & (
  | {
      readonly membership: "proposed" | "removed";
      readonly mappings: readonly BridgeMappingView[];
    }
  | {
      readonly membership: "member";
      readonly mappings: readonly (BridgeMappingView & {
        readonly state: BridgeMappingState;
      })[];
    }
);

export interface BridgeRoomView {
  readonly bridges: readonly BridgeEntryView[];
  /** Latest bridge operation, including any durable pending state on a fresh read. */
  readonly outcome: OutcomeClass | null;
}

/**
 * Optional host service, bound to the originating profile. The host supplies complete Core views and only offers an
 * intent that Core can attempt. While an operation is pending, the same change stays unavailable. The host owns
 * authorization, confirmation binding, durable operation identity and reconciliation.
 * A rejected Promise means the view could not be obtained, so the UI can ask for state again without repeating the change.
 */
export interface BridgeService {
  state(conversationId: ConversationId): Promise<BridgeRoomView>;
  change(
    conversationId: ConversationId,
    intent: BridgeIntent,
  ): Promise<BridgeRoomView>;
}

/** Core's message projection. A link is current validated presentation evidence. The bridge remains the author. */
export interface BridgeMessageView {
  readonly principal: BridgePrincipalView;
  readonly network: string;
  readonly actor: string | null;
  readonly identityLink:
    | { readonly kind: "linked"; readonly accountName: string }
    | { readonly kind: "not_linked" };
  readonly flags: readonly BridgeFidelityFlag[];
  readonly carry?: BridgeCarry;
}
