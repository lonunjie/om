import type { RejectReason, UserActionCode } from "@om/platform-api";
import type { DevicesCopy, JoinCopy, OutcomeCopy } from "../copy";
import type { RecoveryContactCopy, RecoveryCopy } from "../../recovery/copy";
import type { ApproveRecoveryCopy, LostDevicesCopy } from "../../recovery/lostCopy";

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
    tryNow: "Try now", strictOffAsk: "Turn this off?", strictOffConfirm: "Yes, turn it off", strictOffCancel: "Keep it on", strictOption: "Only add a device when all others can be told", strictHelp: "Strict help", notSetUp: "Not set up yet", notSetUpDetail: (l) => `${l} never finished setting up`, tooManyOwed: "Too many devices still to be told", accountTooLarge: "Account too large to add a device",
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

export const testLostCopy: LostDevicesCopy = {
  heading: "Get your account back",
  intro: "Enter the recovery key you saved when you made your account. Nothing is sent until you ask for your account back.",
  keyLabel: "Your recovery key",
  keyHint: "Type it, or paste it. Capital letters, spaces and dashes do not matter.",
  placeLabel: "Where it is saved (only if your key does not say)",
  placeHint: "Leave this empty unless your saved sheet gave you an address.",
  scan: "Scan the recovery code",
  scanNothing: "No code was found. Type or paste your key instead.",
  find: "Find my account",
  cancel: "Back",
  looking: "Looking for your saved account. This can take a moment.",
  stage: {
    needs_place: "This key does not say where your account is saved. Start over and add the address from your saved sheet.",
    not_found: "No saved account was found for this key. Check it and try again.",
    cant_read: "Something is saved, but this key cannot open it. Check the key and try again.",
    out_of_date: "What was found is older than what this device already saw, so it was not used. Try again later.",
    conflict: "What was found does not match your saved sheet. It was not used, and nothing was sent.",
  },
  foundHeading: "Your saved account was found",
  foundBody: "You can ask for your account back on this device. Your other devices will be asked to stop it if it was not you, and you will have to wait before it finishes unless one of them approves it.",
  evidence: {
    pinned: "Your saved sheet is enough for this device to finish on its own once the wait is over.",
    other: "To finish, this device needs one of your other devices to approve it, or the sheet you saved from one of your own devices.",
  },
  ask: "Ask for my account back",
  waitingHeading: "Waiting to get your account back",
  waitingBody: "Your request is out. Your account comes back on this device when the wait is over and nobody has stopped it. Keep OM open; the wait only counts while it is running.",
  remaining: (ms) => {
    if (ms <= 0) return "The wait is over. Making sure nothing changed.";
    const hours = Math.ceil(ms / 3_600_000);
    if (hours >= 36) return `About ${String(Math.round(hours / 24))} days left.`;
    if (hours >= 2) return `About ${String(hours)} hours left.`;
    return "Less than an hour left.";
  },
  waitDone: "Your account is back on this device.",
  checkAgain: "Check again now",
  stop: "Stop asking",
  approval: {
    heading: "Do you still have another device?",
    body: "If one of your devices still works, it can approve this right away. On that device, open Settings, then Recovery, and choose to help another device. Then enter the link it shows.",
    linkLabel: "Link from your other device",
    linkHint: "It starts with om:recover-approve/. This is not a link to add a device.",
    join: "Continue",
    state: {
      waiting_for_controller: "Waiting for your other device. Compare the six digits with the ones it shows, then approve there.",
      receiving: "Your other device approved. Checking everything it sent.",
      done: "Approved and checked. Finishing now.",
      failed: "That did not work. Nothing changed, and your request is still out. Ask your other device for a new link.",
      expired: "That link ran out of time. Your request is still out. Ask your other device for a new link.",
    },
    codeLabel: "Check code",
    codeHint: "These six digits must match the ones on your other device.",
    parseError: "That does not look like a recovery link from your other device.",
  },
  ended: {
    vetoed: "One of your devices said this was not you, so it stopped. Nothing changed on this device.",
    stale: "Your account changed while you waited, so this request ended. Start over to ask again.",
    cancelled: "You stopped asking. Nothing changed on this device.",
  },
  startOver: "Start over",
  outcome: testOutcome,
};

/** A device that still works helps another device get the account back (ADR-0203). */
export const testApproveCopy: ApproveRecoveryCopy = {
  action: "Help another device get the account back",
  heading: "Help another device get the account back",
  body: "Use this only if you lost your other devices and are setting up a new one. It shows a private link and a code. Never use it for someone else.",
  show: "Show the link and code",
  back: "Back",
  linkHeading: "Link for the new device",
  notInvitation: "This is not an invitation to chat and not a link to add a device.",
  waiting: "Waiting for the new device to use this link. It works for five minutes.",
  codeLabel: "Check code",
  codeHint: "Compare these six digits with the new device. Approve only if they match and you are the one setting it up.",
  confirm: "Yes, this is me",
  confirmNote: "Approving takes the account away from every old device, including this one.",
  decline: "No, stop this",
  state: {
    waiting_for_device: "Waiting for the new device.",
    awaiting_confirmation: "The new device is asking. Compare the digits.",
    approved: "Approved. The new device now has the account.",
    declined: "Stopped. Nothing was approved.",
    failed: "That did not work. Nothing was approved. Start again for a new link.",
    expired: "The link ran out of time. Start again for a new one.",
  },
  outcome: testOutcome,
};

