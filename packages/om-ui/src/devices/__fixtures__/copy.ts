import type { RejectReason, UserActionCode } from "@om/platform-api";
import type { DevicesCopy, LinkThisDeviceCopy, OutcomeCopy } from "../copy";
import type { RecoveryCopy } from "../../recovery/copy";

const REASONS: readonly RejectReason[] = ["invalid", "unsupported", "unauthorized", "revoked", "expired", "stale_state", "conflict", "policy_denied", "quota", "rate_limited", "integrity_failure", "unavailable", "not_found"];
const ACTIONS: readonly UserActionCode[] = ["authorize_with_device", "confirm_security_change", "reauthenticate", "update_client"];
const rec = <K extends string>(keys: readonly K[], f: (k: K) => string): Readonly<Record<K, string>> => Object.fromEntries(keys.map((k) => [k, f(k)])) as Record<K, string>;

export const testOutcome: OutcomeCopy = {
  working: "Working", pendingAuthorization: "Waiting for approval", pendingNetwork: "Waiting for network", pendingReconciliation: "Checking result",
  needsAction: rec(ACTIONS, (a) => `Action: ${a}`), rejected: rec(REASONS, (r) => `Rejected: ${r}`), failed: "Nothing was sent",
};

export const testDevicesCopy: DevicesCopy = {
  heading: "Devices", loading: "Loading devices", loadFailed: "Could not load",
  controlState: { control_conflict: "Control conflict", safety_freeze: "Safety Freeze is on", fork_detected: "Fork detected" },
  deviceLinkedNotice: (label) => `New device linked: ${label}`,
  linkRequestsHeading: "Devices asking to link", linkRequestRow: (label) => `${label} wants to link`,
  currentHeading: "This device", otherHeading: "Other devices", noOtherDevices: "No other devices", removedHeading: "Removed devices",
  role: { control: "Can manage account", messaging: "Messages only" },
  state: { active: "Active", quarantined: "Suspended", revocation_pending: "Removal waiting for approval", revoked: "Removed" },
  lastSeen: { now: "Used now", recent: "Used recently", this_week: "Used this week", this_month: "Used this month", long_ago: "Not used for a long time", never: "Never used" },
  cautions: {
    suspend_would_freeze: "Suspending would freeze the account", last_voting_controller: "Last controller",
    history_sync_unavailable: "Cannot share history", long_idle: "Long idle",
  },
  enrolled: (ms) => `Linked at ${String(ms)}`, thisDevice: "You are using this device", back: "Back to devices",
  detailsHeading: (label) => `Device: ${label}`, noActions: "No changes available", frozenNoActions: "Changes paused by Safety Freeze",
  suspend: { action: "Suspend this device", heading: (l) => `Suspend ${l}?`, consequences: ["Stops new keys now", "Not removed"], confirm: "Yes, suspend", cancel: "Keep it", done: "Suspended" },
  revoke: { action: "Remove permanently", heading: (l) => `Remove ${l} permanently?`, consequences: ["Can never come back", "Data already on it stays"], confirm: "Yes, remove permanently", cancel: "Keep it", done: "Removed" },
  pendingHeading: "Still in progress",
  pendingKind: { RequestDeviceLink: "Opening link", ApproveDeviceLink: "Approving link", RejectDeviceLink: "Rejecting link", SuspendDevice: "Suspending", RevokeDevice: "Removing" },
  link: {
    start: "Link a new device", heading: "Link a new device", body: "Open the offer from the new device", scan: "Scan QR code", scanNothing: "No code found",
    pasteLabel: "Link from the new device", pasteHint: "Paste the om:link", open: "Continue", parseError: { empty: "Paste first", too_long: "Too long", malformed: "Not a device link" },
    cancel: "Cancel", reviewHeading: "Check the new device", who: (l) => `${l} wants to join your account`, codeLabel: "Code", codeHint: "Must match the new device",
    warning: "Only link a device you are holding", announce: "All your other devices will be told", approve: "The codes match, link it", reject: "Not mine, reject",
    expires: (ms) => `Expires ${String(ms)}`,
    requestState: { awaiting_approval: "Waiting for your decision", pending_authorization: "Waiting for your other devices", approved: "Linked", rejected: "Rejected", expired: "Expired" },
    linked: "Device linked", rejectedDone: "Link rejected", done: "Done",
  },
  outcome: testOutcome,
};

export const testLinkThisDeviceCopy: LinkThisDeviceCopy = {
  heading: "Link this device", body: "Show this to your other device", start: "Show link code", linkLabel: "Link", qrLabel: "QR code",
  expires: (ms) => `Expires ${String(ms)}`, codeLabel: "Code", codeHint: "Check it matches",
  state: { waiting_for_scan: "Waiting for scan", awaiting_approval: "Approve on your other device", linked: "Linked", rejected: "Rejected", expired: "Expired" },
  outcome: testOutcome,
};

export const testRecoveryCopy: RecoveryCopy = {
  heading: "Recovery", loading: "Loading recovery", loadFailed: "Could not load", back: "Back to recovery",
  policy: { on: "Recovery is on", pending: "Recovery is not set up yet", off_explicit: "Recovery is off" },
  custodiansHeading: "Where recovery lives",
  custodian: { synced_passkey: "Your passkeys", recovery_key: "Your recovery key", trusted_contacts: "Trusted people", spare_device: "Spare device" },
  distributionBound: "(this app only)", scopesHeading: "What can be recovered",
  scope: { identity: { title: "Account", body: "control" }, history: { title: "History", body: "messages" }, provider: { title: "Home", body: "connection" } },
  delay: (ms) => `Delay ${String(ms)}`, recoveryKeySaved: "Key saved", recoveryKeyNone: "No key", recoveryKeyOnce: "The key cannot be shown again",
  incomingAlert: "Someone is recovering your account", review: "Review", requestsHeading: "Recoveries",
  direction: { incoming: "On another device", outgoing: "Started here" },
  requestState: { waiting: "Waiting", completed: "Completed", vetoed: "Stopped", cancelled: "Cancelled" },
  started: (ms) => `Started ${String(ms)}`, activatesAt: (ms) => `Completes ${String(ms)}`, activatesUnknown: "Completion time not known yet",
  stopIncoming: "Stop this recovery", cancelOutgoing: "Cancel my recovery",
  pendingHeading: "Still in progress", pendingKind: { StartRecovery: "Starting recovery", VetoRecovery: "Stopping recovery" },
  start: {
    action: "Start a recovery", heading: "Start a recovery", body: "Choose what to recover", scopeLegend: "What to recover", custodianLegend: "Using",
    consequences: { identity: ["Waits before finishing", "Your devices can stop it"], history: ["Restores messages only"], provider: ["Reconnects your Home only"] },
    confirm: "Start recovery", cancel: "Cancel", done: "Recovery done",
  },
  veto: {
    heading: { incoming: "Stop this recovery?", outgoing: "Cancel your recovery?" },
    consequences: { incoming: ["It will not finish", "Change your recovery method"], outgoing: ["It stops"] },
    confirm: { incoming: "Yes, stop it", outgoing: "Yes, cancel it" }, cancel: "Go back", done: "Stopped",
  },
  outcome: testOutcome,
};
