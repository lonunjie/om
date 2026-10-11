import type { CapabilityReport } from "./adapter";
import type { CapabilityState } from "./capability";
import type { FamilyName } from "./families";

export type CapabilityCheck =
  | { readonly ok: true; readonly state: "available" | "partial" }
  | { readonly ok: false; readonly family: FamilyName; readonly reason: "capability_unavailable" };

/** Never throws; the caller must branch on `ok`. */
export function requireCapability(report: CapabilityReport, family: FamilyName): CapabilityCheck {
  const state = report.families[family].state;
  return state === "unavailable" ? { ok: false, family, reason: "capability_unavailable" } : { ok: true, state };
}

export interface DegradeBranches<T> {
  readonly available: () => T;
  /** Omitted `partial` fails closed to `unavailable`. */
  readonly partial?: () => T;
  readonly unavailable: () => T;
}

/** Explicit degradation: the caller states what happens in each state; there is no implicit fallback. */
export function degrade<T>(report: CapabilityReport, family: FamilyName, branches: DegradeBranches<T>): T {
  const state: CapabilityState = report.families[family].state;
  if (state === "available") return branches.available();
  if (state === "partial") return (branches.partial ?? branches.unavailable)();
  return branches.unavailable();
}

export type LimitCode =
  | "secure_surface_unavailable"
  | "backup_exclusion_unavailable"
  | "clipboard_sensitive_unsupported"
  | "clipboard_unavailable"
  | "notification_removal_unavailable"
  | "notification_removal_partial"
  | "input_privacy_partial"
  | "input_privacy_unavailable"
  | "background_push_none"
  | "local_discovery_not_automatic"
  | "local_discovery_unavailable"
  | "call_system_integration_unavailable";

/** Machine codes for protections this platform cannot honor; the UI words the claims (R-167). */
export function describeLimits(report: CapabilityReport): readonly LimitCode[] {
  const f = report.families;
  const out: LimitCode[] = [];
  if (f.secureSurface.state !== "available") out.push("secure_surface_unavailable");
  if (f.backupExclusion.state !== "available") out.push("backup_exclusion_unavailable");
  if (f.clipboard.state === "unavailable") out.push("clipboard_unavailable");
  else if (f.clipboard.detail === "sensitive_unsupported") out.push("clipboard_sensitive_unsupported");
  if (f.notificationRemoval.state === "unavailable") out.push("notification_removal_unavailable");
  else if (f.notificationRemoval.state === "partial") out.push("notification_removal_partial");
  if (f.inputPrivacy.state === "unavailable") out.push("input_privacy_unavailable");
  else if (f.inputPrivacy.state === "partial") out.push("input_privacy_partial");
  if (report.backgroundPush === "none") out.push("background_push_none");
  if (report.localDiscovery === "unavailable") out.push("local_discovery_unavailable");
  else if (report.localDiscovery !== "automatic") out.push("local_discovery_not_automatic");
  if (!report.callSystemIntegration) out.push("call_system_integration_unavailable");
  return out;
}
