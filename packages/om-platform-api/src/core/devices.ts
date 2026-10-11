// Devices, device linking and recovery view models (ACCOUNT_CONTINUITY_AND_CONTROL.md, RECOVERY_AND_BACKUP_POLICY.md).
// Machine codes only: Core decides every state, allowed action and timer; the UI words and renders them.
import type { DeviceId, OperationId } from "./ids";

export type DeviceRole = "control" | "messaging";
/** `quarantined` = suspended (control devices only); `revocation_pending` = permanent removal awaiting authorization. */
export type DeviceStateCode = "active" | "quarantined" | "revocation_pending" | "revoked";
export type LastSeenClass = "now" | "recent" | "this_week" | "this_month" | "long_ago" | "never";
/** Actions Core will accept for a device right now. The UI offers nothing else. */
export type DeviceActionCode = "suspend" | "revoke";
export type DeviceCautionCode =
  | "suspend_would_freeze" // suspending would leave too few controllers: Core offers removal instead (§9)
  | "last_voting_controller" // the account must never be left without a voting controller (§13)
  | "history_sync_unavailable" // this device cannot share device history (ADR-0156/0157)
  | "long_idle"; // linked long ago and unused (R-166 rule 4)

export interface DeviceSummary {
  readonly id: DeviceId;
  readonly label: string;
  readonly role: DeviceRole;
  readonly state: DeviceStateCode;
  readonly isCurrent: boolean;
  readonly enrolledAt: number;
  readonly lastSeen: LastSeenClass;
  readonly actions: readonly DeviceActionCode[];
  readonly cautions: readonly DeviceCautionCode[];
}

/** AccountControlState condition as Core reports it. Never resolved by the UI or by last-write-wins. */
export type AccountControlStateCode = "normal" | "control_conflict" | "safety_freeze" | "fork_detected";

/** Authenticated, non-suppressible enrollment notice (R-166); the UI renders it on a reserved surface with no dismiss. */
export interface DeviceNoticeView { readonly code: "device_linked"; readonly deviceId: DeviceId; readonly label: string; readonly at: number }

export type LinkRequestState = "awaiting_approval" | "pending_authorization" | "approved" | "rejected" | "expired";
/** What the approving (existing) device sees about a device asking to be linked. */
export interface LinkRequestView {
  readonly requestId: string;
  /** The RequestDeviceLink operation that produced this request. */
  readonly operationId: OperationId;
  readonly deviceLabel: string;
  /** Short code Core derived for both sides; the UI only displays it. */
  readonly verificationCode: string;
  readonly expiresAt: number;
  readonly state: LinkRequestState;
}

export type LinkOfferState = "waiting_for_scan" | "awaiting_approval" | "linked" | "rejected" | "expired";
/** What the new device shows while it waits to be linked. */
export interface LinkOfferView {
  readonly operationId: OperationId;
  readonly link: string;
  readonly expiresAt: number;
  /** `null` until an existing device has opened the offer. */
  readonly verificationCode: string | null;
  readonly state: LinkOfferState;
}

export interface DevicesView {
  readonly controlState: AccountControlStateCode;
  readonly devices: readonly DeviceSummary[];
  readonly linkRequests: readonly LinkRequestView[];
  readonly notices: readonly DeviceNoticeView[];
}

export type RecoveryPolicyState = "on" | "pending" | "off_explicit";
export type RecoveryCustodianCode = "synced_passkey" | "recovery_key" | "trusted_contacts" | "spare_device";
/** Identity, history and provider recovery stay distinct (RECOVERY_AND_BACKUP_POLICY.md §2). */
export type RecoveryScope = "identity" | "history" | "provider";
export type RecoveryRequestState = "waiting" | "completed" | "vetoed" | "cancelled";

export interface RecoveryCustodianView {
  readonly code: RecoveryCustodianCode;
  /** Passkey custody bound to this client distribution's origin (not portable recovery). */
  readonly distributionBound: boolean;
}

export interface RecoveryRequestView {
  readonly requestId: string;
  /** The StartRecovery operation, when this device started it. */
  readonly operationId: OperationId | null;
  /** `incoming`: someone is recovering this account elsewhere; `outgoing`: this device started it. */
  readonly direction: "incoming" | "outgoing";
  readonly scope: RecoveryScope;
  readonly custodian: RecoveryCustodianCode;
  readonly startedAt: number;
  /** This device's own earliest completion time (verifier-local, ADR-0131); `null` while Core cannot say yet. */
  readonly activatesAt: number | null;
  readonly state: RecoveryRequestState;
  /** Core accepts VetoRecovery for it now. */
  readonly canStop: boolean;
}

export interface RecoveryStatusView {
  readonly policy: RecoveryPolicyState;
  readonly custodians: readonly RecoveryCustodianView[];
  /** Scopes this policy can recover. */
  readonly scopes: readonly RecoveryScope[];
  /** Policy delay before an uncontested recovery completes alone. */
  readonly activationDelayMs: number;
  /** A recovery key exists. It is display-once and can never be shown again. */
  readonly recoveryKeySaved: boolean;
  readonly requests: readonly RecoveryRequestView[];
}

// Strict `om:link/` grammar for device-link offers. Parsing never fetches or resolves anything.
export const DEVICE_LINK_MAX_LENGTH = 256;
const DEVICE_LINK = /^om:link\/([A-Za-z0-9_-]{16,200})$/;

export type DeviceLinkResult =
  | { readonly ok: true; readonly token: string }
  | { readonly ok: false; readonly reason: "empty" | "too_long" | "malformed" };

export function parseDeviceLink(input: string): DeviceLinkResult {
  const s = input.trim();
  if (s === "") return { ok: false, reason: "empty" };
  if (s.length > DEVICE_LINK_MAX_LENGTH) return { ok: false, reason: "too_long" };
  const m = DEVICE_LINK.exec(s);
  return m?.[1] === undefined ? { ok: false, reason: "malformed" } : { ok: true, token: m[1] };
}
