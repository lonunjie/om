// SPDX-License-Identifier: MPL-2.0
import type { UserActionCode } from "@om/platform-api";
import type {
  PluginActionKind,
  PluginCapability,
  PluginErrorCode,
  PluginInstallationView,
  PluginOutcome,
  PluginTaintView,
} from "./plugins";

export interface PluginsCopy {
  readonly heading: string;
  readonly loading: string;
  readonly changing: string;
  readonly loadFailed: string;
  readonly changeUnknown: string;
  readonly refresh: string;
  readonly close: string;
  readonly empty: string;
  readonly publisher: string;
  readonly version: string;
  readonly requested: string;
  readonly granted: string;
  readonly disabled: string;
  readonly approvalRequired: string;
  readonly beyondCeiling: string;
  readonly noCapabilities: string;
  readonly requestedOrigins: string;
  readonly readDoesNotAllowEgress: string;
  readonly tainted: string;
  readonly egress: Readonly<Record<PluginTaintView["egress"], string>>;
  readonly conversationPolicy: Readonly<Record<PluginTaintView["conversation"], string>>;
  readonly runtime: Readonly<Record<PluginInstallationView["runtime"]["kind"], string>>;
  readonly capability: Readonly<Record<PluginCapability, string>>;
  readonly review: Readonly<Record<"install" | "update" | "confirm_send", string>>;
  readonly reviewGrant: (capability: string) => string;
  readonly grantScope: string;
  readonly title: Readonly<Record<Exclude<PluginActionKind, "cancel_send">, string>>;
  readonly action: Readonly<Record<PluginActionKind, string>>;
  readonly consequence: Readonly<Record<"install" | "update" | "confirm_send" | "revoke" | "remove", string>>;
  readonly destination: string;
  readonly message: string;
  readonly pluginControls: string;
  readonly pluginInput: string;
  readonly pluginButton: string;
  readonly pluginSubmit: string;
  readonly pluginControlUnavailable: string;
  readonly outcome: Readonly<Record<Exclude<PluginOutcome["kind"], "Rejected" | "NeedsUserAction">, string>>;
  readonly needsAction: Readonly<Record<UserActionCode, string>>;
  readonly errors: Readonly<Record<PluginErrorCode, string>>;
}
