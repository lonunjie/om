// SPDX-License-Identifier: MPL-2.0
import type { OutcomeClass } from "@om/platform-api";

/** W4 capability names from plugin_host.rs. These names carry no authority in the UI. */
export type PluginCapability =
  | "ui.composer_action"
  | "ui.message_action"
  | "ui.panel"
  | "ui.settings_page"
  | "content.read_selected"
  | "content.read_current_conversation"
  | "content.search_local_scope"
  | "content.read_attachment_selected"
  | "compose.insert"
  | "compose.create_draft"
  | "conversation.send_message"
  | "conversation.set_reaction"
  | "conversation.open"
  | "net.fetch_origin_scoped"
  | "storage.plugin_private"
  | "notifications.plugin_local"
  | "service.invoke";

/** Numeric plugin_error_code values, in the same order as Core's PluginError. */
export const pluginErrorCode = {
  malformedRequest: 1,
  unknownCapability: 2,
  capabilityNotGranted: 3,
  grantExpired: 4,
  taintedEgressDenied: 5,
  userConfirmationRequired: 6,
  bilateralConsentRequired: 7,
  originChannelMismatch: 8,
  resourceBudgetExceeded: 9,
  hostPolicyDenied: 10,
  releaseNotAuthorized: 11,
  invalidUiTree: 12,
} as const;
export type PluginErrorCode = (typeof pluginErrorCode)[keyof typeof pluginErrorCode];

/** Display projections of plugin_control.rs Release and Carry, without signature or control state. */
export interface PluginReleaseView {
  readonly id: string;
  readonly seq: number;
  readonly label: string;
  readonly requested: readonly PluginCapability[];
}

export interface PluginCarryView {
  readonly ceiling: readonly PluginCapability[];
  readonly disabledUntilGranted: readonly PluginCapability[];
}

export type PluginActionKind = "install" | "update" | "approve" | "revoke" | "remove" | "confirm_send" | "cancel_send";
export interface PluginActionIntent<Kind extends PluginActionKind = PluginActionKind> {
  readonly kind: Kind;
  /** Opaque host reference to the exact reviewed action, release, grant or pending send. */
  readonly target: string;
}

export type PluginAction<Kind extends PluginActionKind = PluginActionKind> =
  | { readonly availability: "ready"; readonly intent: PluginActionIntent<Kind> }
  | { readonly availability: "blocked"; readonly kind: Kind; readonly reason: PluginErrorCode };

export type PluginPermissionView =
  | { readonly kind: "granted" }
  | { readonly kind: "disabled"; readonly reason: "approval_required" | PluginErrorCode };

export interface PluginPermissionsView {
  /** Effective status of each requested capability, supplied by the host independently of the continuity ceiling. */
  readonly permissions: Readonly<Partial<Record<PluginCapability, PluginPermissionView>>>;
  readonly requestedOrigins: readonly string[];
}

export type PluginReleaseReview = PluginPermissionsView & {
  readonly release: PluginReleaseView;
} & (
  | { readonly kind: "install"; readonly action: PluginAction<"install"> }
  | { readonly kind: "update"; readonly carry: PluginCarryView; readonly action: PluginAction<"update"> }
);

export type PluginNodeKind = "text" | "button" | "input" | "card" | "form" | "group";

/** Limits on the host-accepted flat tree, from plugin_host.rs. */
export const pluginUiLimits = { nodes: 128, depth: 8, textBytes: 1024, actionBytes: 128 } as const;

export interface PluginUiNodeView {
  readonly id: string;
  readonly kind: PluginNodeKind;
  readonly parent?: string;
  readonly text?: string;
  /** An action declared for this node. The renderer sends only node references, which the service resolves. */
  readonly action?: string;
}

export interface PluginUiTreeView {
  /** Identifies this host-accepted tree and its invocation. */
  readonly id: string;
  readonly surface: string;
  readonly nodes: readonly PluginUiNodeView[];
}

export type PluginUiIntent = {
  readonly kind: "ui_interaction";
  readonly target: string;
  readonly nodeId: string;
} & (
  | { readonly event: "activate" }
  | { readonly event: "submit"; readonly fields: readonly { readonly nodeId: string; readonly value: string }[] }
);

export type PluginIntent = PluginActionIntent | PluginUiIntent;

/** Only the host can report which rule allowed or blocked this tainted invocation. */
export type PluginTaintView = {
  readonly conversation: "one_to_one" | "room";
  readonly egress: "blocked" | "policy_authorized" | "one_shot_selected" | "export_authorized";
};

export interface PluginGrantReview {
  readonly id: string;
  readonly capability: PluginCapability;
  /** The exact scope presented for the host's offered grant. */
  readonly scope: string;
  readonly action: PluginAction<"approve">;
}

export interface PluginInstallationView extends PluginPermissionsView {
  readonly release: PluginReleaseView;
  readonly carry?: PluginCarryView;
  readonly runtime:
    | { readonly kind: "active"; readonly taint: PluginTaintView | null; readonly settings: PluginUiTreeView | null }
    | { readonly kind: "isolation_unavailable" | "revoked" };
  readonly grantReviews: readonly PluginGrantReview[];
  readonly actions: readonly PluginAction<"revoke" | "remove">[];
}

export interface PluginSendView {
  /** A fresh reference for every send, even when the message and destination match a prior send. */
  readonly id: string;
  /** Host-renderable exact destination and decoded outgoing text. Neither comes from the plugin UI tree. */
  readonly destination: string;
  readonly content: string;
  readonly confirm: PluginAction<"confirm_send">;
  readonly cancel: PluginAction<"cancel_send">;
}

export type PluginOutcome =
  | Exclude<OutcomeClass, { readonly kind: "Rejected" }>
  | { readonly kind: "Rejected"; readonly code: PluginErrorCode };

export interface PluginSettingsView {
  readonly name: string;
  readonly publisher: string;
  readonly installation: PluginInstallationView | null;
  readonly releaseReview: PluginReleaseReview | null;
  readonly send: PluginSendView | null;
  readonly outcome: PluginOutcome | null;
}

/**
 * Optional injected service for one plugin and one profile. Core and the host own accepted trees, release and
 * grant state, exact confirmation binding, operation identity and reconciliation. A rejected Promise means the
 * view could not be obtained. Read again without resubmitting the change.
 */
export interface PluginsService {
  state(): Promise<PluginSettingsView>;
  change(intent: PluginIntent): Promise<PluginSettingsView>;
}
