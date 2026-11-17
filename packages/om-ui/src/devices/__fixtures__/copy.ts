import type { RejectReason, UserActionCode } from "@om/platform-api";
import type { DevicesCopy, JoinCopy, OutcomeCopy } from "../copy";
import type { RecoveryContactCopy, RecoveryCopy } from "../../recovery/copy";

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
  deviceLinkedNotice: (label) => `New device linked: ${label}`, noticesLabel: "New devices",
  linkRequestsHeading: "Devices asking to link", linkRequestRow: (label) => `${label} wants to link`,
  currentHeading: "This device", otherHeading: "Other devices", noOtherDevices: "No other devices", linkLater: "Linking is not available yet", historyDegraded: { sender: "Some older chats could not be copied to the new device", receiver: "Some older chats could not be copied to this device" }, removedHeading: "Removed devices",
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
  pendingKind: { OfferDeviceLink: "Preparing link", ApproveDeviceLink: "Approving link", RejectDeviceLink: "Rejecting link", SuspendDevice: "Suspending", RevokeDevice: "Removing" },
  notice: {
    notTold: "Not told yet", notToldDetail: (l) => `${l} has not heard about the new device`, lastTried: (ms) => `Last tried ${String(ms)}`, held: (l) => `${l} is paused`,
    tryNow: "Try now", strictOffAsk: "Turn this off?", strictOffConfirm: "Yes, turn it off", strictOffCancel: "Keep it on", strictOption: "Only add a device when all others can be told", strictHelp: "Strict help", tooManyOwed: "Too many devices still to be told", accountTooLarge: "Account too large to add a device",
  },
  link: {
    start: "Add a device", notControl: "Add devices from a device that manages your account", heading: "Add a device", body: "Show a link on this device",
    show: "Show link and QR code", cancel: "Cancel", qrLabel: "QR code that adds a device", qrBadge: "Adds a device", qrNotInvite: "This is not an invitation",
    linkLabel: "Link", expires: (ms) => `Expires ${String(ms)}`, nextStep: "On the new device choose I already use OM",
    offerState: { waiting_for_scan: "Waiting for the new device", awaiting_approval: "The new device is asking", linked: "Device added", rejected: "Declined", expired: "Expired" },
    tryAgain: "Try again",
    reviewHeading: "Check the new device", who: (l) => `${l} wants to join your account`, codeLabel: "Code", codeHint: "Must match the new device",
    warning: "Only link a device you are holding", announce: "All your other devices will be told", approve: "The codes match, link it", reject: "Not mine, reject",
    requestState: { awaiting_approval: "Waiting for your decision", pending_authorization: "Waiting for your other devices", approved: "Linked", rejected: "Rejected", expired: "Expired" },
    linked: "Device linked", rejectedDone: "Link rejected", done: "Done",
  },
  outcome: testOutcome,
};

export const testJoinCopy: JoinCopy = {
  heading: "Join with a link", body: "Open the link from your other device", scan: "Scan QR code", scanNothing: "No code found",
  pasteLabel: "Link from your other device", pasteHint: "Paste the om:link", open: "Continue", parseError: { empty: "Paste first", too_long: "Too long", malformed: "Not a device link" },
  cancel: "Back", codeLabel: "Code", codeHint: "Check it matches",
  state: { awaiting_approval: "Approve on your other device", pending_authorization: "Waiting for your other devices", approved: "Linked", rejected: "Declined", expired: "Expired" },
  joined: "You are linked", tryAgain: "Start again", outcome: testOutcome,
};

export const testContactRecoveryCopy: RecoveryContactCopy = {
  region: (n) => `${n} is recovering`,
  days: (d) => (d <= 0 ? "less than a day" : `about ${String(d)} days`),
  waiting: {
    title: (n) => `${n} is recovering their account`, body: (d) => `Waiting ${d}`, check: (n) => `Check with ${n}`, checkDetail: (n) => `Ask ${n} another way`,
    decline: "This isn't them", declineNote: (n) => `Your chat with ${n} stays as it is`,
  },
  ready: { title: (n) => `Is this really ${n}?`, consent: (n) => `Only if you checked with ${n}`, confirm: "Yes, it's them", decline: "Not them", checking: "One more check is needed" },
  accepted: (n) => `${n} recovered their account on a new device`, vetoed: (n) => `${n}'s own device stopped this`, declined: (n) => `You turned this down for ${n}`,
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
  requestState: { waiting: "Waiting", completed: "Completed", vetoed: "Stopped", cancelled: "Cancelled", stale: "No longer valid" },
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
  contact: testContactRecoveryCopy,
};
