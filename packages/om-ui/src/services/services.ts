import type { BridgeEventClass } from "../bridges/bridges";
// SPDX-License-Identifier: MPL-2.0
import type {
  ConversationId,
  OutcomeClass,
  RejectReason,
} from "@om/platform-api";

export type ServiceAccessMode =
  "selected_context" | "full_room_member" | "local_personal";
export type ServiceScope =
  | { readonly kind: "personal" }
  | { readonly kind: "room" | "space"; readonly name: string };

export interface ServicePrincipalView {
  readonly id: string;
  readonly name: string;
}

/** Room roster facts from the scope engine, with the installed case made explicit. */
export type ServiceRoomRoster =
  | { readonly installed: false }
  | {
      readonly installed: true;
      readonly canReadAllContent: true;
      readonly endpoints: readonly string[];
      readonly filterClasses: readonly number[] | null;
      readonly admittedVersion: number;
      readonly heldVersion: number | null;
    };

/** Effective read access returned by Core, independent of the mode requested at installation. */
export type ServiceReadAccess =
  | { readonly kind: "none" | "selected_context" }
  | {
      readonly kind: "full_room_member";
      readonly roster: Extract<ServiceRoomRoster, { readonly installed: true }>;
    }
  | { readonly kind: "local_personal"; readonly scopes: readonly string[] };

export type ServiceActionKind = "install" | "invoke" | "revoke" | "remove";

export interface ServiceIntent<
  Kind extends ServiceActionKind = ServiceActionKind,
> {
  readonly kind: Kind;
  /** Opaque reference to the exact reviewed offer, disclosure, grant or installation incarnation. */
  readonly target: string;
}

export type ServiceAction<Kind extends ServiceActionKind = ServiceActionKind> =
  | { readonly availability: "ready"; readonly intent: ServiceIntent<Kind> }
  | {
      readonly availability: "blocked";
      readonly kind: Kind;
      readonly reason: RejectReason;
    };

export interface ServiceCapabilityView {
  readonly id: string;
  /** Display name for this exact capability from the host's capability vocabulary. */
  readonly label: string;
}

export interface ServiceEntryView {
  /** Stable row identity for this offer or installation, distinct from the ServiceID. */
  readonly id: string;
  readonly principal: ServicePrincipalView;
  readonly status: "offered" | "installed" | "revoked" | "removed";
  readonly scope: ServiceScope;
  readonly mode: ServiceAccessMode;
  /** Requested capabilities on an offer and effective capabilities on an installation. */
  readonly capabilities: readonly ServiceCapabilityView[];
  readonly readAccess: ServiceReadAccess;
  readonly triggerClasses?: readonly BridgeEventClass[] | null;
  readonly installedBy?: {
    readonly name: string;
    readonly device: string;
    readonly atMs: number;
  };
  readonly actions: readonly ServiceAction<
    Exclude<ServiceActionKind, "invoke">
  >[];
}

export type ServiceDisclosureKind =
  | "trigger"
  | "message"
  | "attachment"
  | "draft"
  | "thread_or_topic"
  | "text"
  | "participant"
  | "scope";

export type ServiceEgressState =
  | "blocked_by_conversation"
  | "blocked_by_capability"
  | "allowed"
  | "not_requested";

/** Core's information-flow result for this invocation. One-time selected sharing is shown separately. */
export interface ServiceFlowView {
  readonly tainted: boolean;
  readonly egress: ServiceEgressState;
}

export interface ServiceDisclosureItem {
  readonly id: string;
  readonly kind: ServiceDisclosureKind;
  readonly value: string;
}

export interface ServiceInvocationView {
  /** Identifies one review. Material changes require a fresh reference from the host. */
  readonly id: string;
  readonly principal: ServicePrincipalView;
  readonly command: string;
  readonly disclosure:
    | { readonly kind: "local" }
    | {
        readonly kind: "selected_context";
        readonly destination: string;
        readonly expiresAtMs: number;
        /** In order, including the trigger and any metadata actually disclosed. */
        readonly items: readonly [
          ServiceDisclosureItem,
          ...ServiceDisclosureItem[],
        ];
      };
  readonly flow: ServiceFlowView;
  readonly action: ServiceAction<"invoke">;
}

/** Representation is a validated display fact. The author is always the service. */
export interface ServiceAuthorshipView {
  readonly principal: ServicePrincipalView;
  readonly representation: { readonly accountName: string } | null;
}

export interface ConnectedServicesView {
  readonly entries: readonly ServiceEntryView[];
  readonly invocation: ServiceInvocationView | null;
  /** Includes durable pending outcomes on a fresh read. */
  readonly outcome: OutcomeClass | null;
}

/**
 * Optional profile-bound host service. Core owns the view, authorization, exact confirmation binding, operation
 * identity and reconciliation. The host normalizes scope engine refusals to the shared outcome and reason codes.
 * A rejected Promise means the UI could not obtain state. Read again without resubmitting the mutation.
 */
export interface ConnectedServicesService {
  state(
    conversationId: ConversationId | undefined,
  ): Promise<ConnectedServicesView>;
  change(
    conversationId: ConversationId | undefined,
    intent: ServiceIntent,
  ): Promise<ConnectedServicesView>;
}
