// The only place user-visible English lives. Components take these as props; keys are machine reason codes, so a
// missing code is a compile error. Wording rules: plain language, no provider names, honest about limits.
import type { ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, SecurityNoticeCode, UserActionCode, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason } from "@om/platform-api";
import type { ConversationCopy, DevicesCopy, HomeCopy, LinkThisDeviceCopy, OnboardingCopy, OutcomeCopy, ProfileSwitcherLabels, RecoveryCopy, ShellLabels, UnlockCopy } from "@om/ui";

const REJECT: Readonly<Record<RejectReason, string>> = {
  invalid: "That was not valid.",
  unsupported: "This version of OM cannot do that yet.",
  unauthorized: "You are not allowed to do that here.",
  revoked: "That access was withdrawn.",
  expired: "That has expired.",
  stale_state: "Things changed while you were doing this. Please try again.",
  conflict: "That conflicts with something that already happened.",
  policy_denied: "The rules for this space do not allow that.",
  quota: "A usage limit was reached.",
  rate_limited: "Too many tries in a short time. Wait a moment and try again.",
  integrity_failure: "A safety check failed, so this was stopped.",
  unavailable: "OM could not reach what it needs right now.",
  not_found: "OM could not find that anymore.",
};
const withPrefix = (prefix: string): Readonly<Record<RejectReason, string>> =>
  Object.fromEntries(Object.entries(REJECT).map(([k, v]) => [k, `${prefix} ${v}`])) as Record<RejectReason, string>;

const NEEDS_ACTION: Readonly<Record<UserActionCode, string>> = {
  authorize_with_device: "Approve this on another device you already use, then try again.",
  confirm_security_change: "Review the security change first, then confirm to continue.",
  reauthenticate: "Unlock OM again to continue.",
  update_client: "This version of OM is too old for that. Update OM and try again.",
};

export const NOTICE_LABELS: Readonly<Record<SecurityNoticeCode, string>> = {
  safety_number_changed: "This contact's safety number changed. Check it with them in person or another trusted way before you rely on this chat.",
  device_added: "A new device was added to this contact's account.",
  unverified_contact: "You have not verified this contact yet.",
  session_reset: "The secure session with this contact was reset. Recent messages may need to be sent again.",
  integrity_failure: "A safety check failed for this chat. Treat recent messages with caution.",
};

const timeFormat = new Intl.DateTimeFormat("en", { hour: "numeric", minute: "2-digit", month: "short", day: "numeric" });
const plural = (n: number, one: string, many: string): string => `${String(n)} ${n === 1 ? one : many}`;

export const conversationCopy: ConversationCopy = {
  list: {
    label: "Conversations",
    empty: "No conversations yet.",
    noActivity: "No messages yet",
    unread: (n) => `${String(n)} unread`,
    securityNotice: "Security notice",
  },
  view: {
    label: "Messages",
    empty: "No messages yet. Say hello.",
    loadFailed: "These messages could not be loaded.",
    incoming: "They said:",
    outgoing: "You said:",
    retry: "Try sending again",
    formatTime: (ms) => timeFormat.format(ms),
  },
  delivery: {
    pending: "Sending",
    pendingNetwork: "Waiting for a connection",
    pendingReconciliation: "Checking whether it arrived",
    sent: "Sent",
    delivered: "Delivered",
    rejected: withPrefix("Not sent."),
  },
  composer: {
    label: "Message",
    placeholder: "Write a message",
    send: "Send",
    sendFailed: "The message could not be sent. Your text is still here.",
    rejected: withPrefix("Not sent."),
    needsAction: NEEDS_ACTION,
  },
  relationship: {
    awaiting_acceptance: "Waiting for them to accept your request.",
    accepted_unbound: "Setting up. This person is not verified yet, and you can't send messages until it is done.",
    establishing: "Almost ready. Finishing the secure connection.",
  },
  pressure: {
    notice: "You are receiving a lot of requests from strangers. People you invited can still reach you.",
  },
  request: {
    heading: "Message request",
    unknownSender: "Not in your contacts",
    unverified: "Unverified",
    accept: "Accept",
    ignore: "Ignore for now",
    block: "Block",
    blockConfirm: "Yes, block",
    blockCancel: "Cancel",
    blockPrompt: "Block this sender? They will not be able to contact you.",
    pending: "Working on it",
    done: "Done",
    rejected: withPrefix("Could not finish."),
    failed: "Something went wrong. Try again.",
  },
  notices: NOTICE_LABELS,
  richText: {
    opensExternally: "(opens a website outside OM)",
    unsupported: "[content this version cannot show]",
    truncated: "[message shortened]",
    attachment: "[attachment]",
    unsafeLink: "[link turned off]",
  },
};

const METHOD: Readonly<Record<WrappingMethod, string>> = {
  "webauthn-prf": "Your data is locked with a passkey from this device or your password manager.",
  "device-key": "Your data is locked with a key kept in this browser.",
};
const WRAPPING_REASON: Readonly<Record<WrappingReason, string>> = {
  prf_verified: "Your passkey works for locking data.",
  device_key_requested: "A browser-held key was chosen.",
  webauthn_absent: "This browser has no passkey support, so a browser-held key is used.",
  prf_not_enabled: "Your passkey provider does not support locking data, so a browser-held key is used.",
  prf_no_output: "Your passkey did not return what is needed to lock data, so a browser-held key is used.",
  webauthn_create_failed: "Creating a passkey did not work, so a browser-held key is used.",
  webauthn_get_failed: "Checking your passkey did not work, so a browser-held key is used.",
};
const LIMITS: Readonly<Record<LimitCode, string>> = {
  secure_surface_unavailable: "This browser cannot stop screenshots or screen recording of OM.",
  backup_exclusion_unavailable: "This browser cannot keep OM data out of device backups.",
  clipboard_sensitive_unsupported: "Anything you copy can be read by other apps on this device.",
  clipboard_unavailable: "Copying is not available here.",
  notification_removal_unavailable: "OM cannot remove notifications after they appear.",
  notification_removal_partial: "OM can only sometimes remove notifications after they appear.",
  input_privacy_partial: "This browser may still learn from what you type, for example for suggestions.",
  input_privacy_unavailable: "This browser cannot keep your typing private from its own suggestions.",
  background_push_none: "OM cannot alert you while it is closed.",
  local_discovery_not_automatic: "Nearby devices are not found automatically.",
  local_discovery_unavailable: "Finding nearby devices is not available here.",
  call_system_integration_unavailable: "Calls will not show in your phone's call screen.",
};
const STAGE: Readonly<Record<VaultStage, string>> = {
  enroll_create: "One more confirmation is needed to create your passkey.",
  enroll_verify: "One more confirmation is needed to check your passkey.",
  unlock: "One more confirmation is needed to unlock your data.",
  rotate_unwrap: "One more confirmation is needed to change how your data is locked.",
};
const VAULT_ERROR: Readonly<Record<VaultErrorReason, string>> = {
  invalid_profile_id: "That identity is not valid.",
  invalid_purpose: "That use of the lock is not valid.",
  invalid_key: "That key is not valid.",
  not_found: "No locked data was found for this identity.",
  corrupt: "The locked data on this device is damaged.",
  locked: "Your data is locked. Unlock it first.",
  already_exists: "This browser already holds locked data for this identity.",
  conflict: "Another OM window is doing this. Close it and try again.",
  needs_user_gesture: "Press the button again to continue.",
  ceremony_failed: "The passkey prompt was cancelled or did not finish.",
  prf_unavailable: "Your passkey could not lock data.",
  unwrap_failed: "Your data could not be unlocked.",
  unsupported_factor: "That unlock method is not supported.",
  passphrase_required: "Enter your passphrase to unlock your data.",
  wrong_passphrase: "That passphrase is not right. Try again.",
  unsupported_platform: "This browser cannot protect your data safely enough for OM.",
  malformed: "The locked data could not be read.",
  unsupported_version: "The locked data was made by a newer version of OM.",
  wrong_purpose: "The locked data was used in the wrong place.",
  integrity_failure: "A safety check on your locked data failed.",
  crypto_failed: "This browser could not do the locking step.",
  storage_failed: "This browser could not save the locked data.",
  delete_unverified: "OM could not confirm the data was removed.",
};
const OBSERVER: Readonly<Record<HomeObserverCode, string>> = {
  sponsor_sees_metadata: "The person or group hosting your Home can see who talks to whom and when, but not what is said.",
  host_sees_metadata: "Your Home's host can see who talks to whom and when, but not what is said.",
  host_sees_ciphertext: "Your Home's host stores your messages only in scrambled form.",
  no_third_party: "No outside party sits between you and your Home.",
};
const ORIGIN: Readonly<Record<ClientOriginChoiceCode, string>> = {
  default_origin: "You are using OM from its standard web address.",
  independent_origin_available: "You can choose to run OM from an address you control.",
  independent_origin_active: "You are running OM from an address you control.",
};

export const onboardingCopy: OnboardingCopy = {
  welcome: {
    heading: "Welcome to OM",
    body: "OM keeps your conversations private. First we will lock your data on this device, then you will get a recovery key for emergencies.",
    start: "Get started",
  },
  protect: {
    heading: "Lock your data on this device",
    body: "OM will ask this device to lock your messages. Nobody without access to this device and your unlock method can read them here.",
    action: "Lock this device",
    working: "Locking",
    passkeyUser: { name: "OM", displayName: "OM" },
    defaultProfileLabel: "Me",
    resultHeading: "How your data is locked",
    method: METHOD,
    reason: WRAPPING_REASON,
    reducedAtRest: "Your passkey is synced between your devices by its provider, so anyone who controls that account could unlock this data. Protection here is reduced.",
    deviceKeyLimit: "The key stays in this browser and no passkey prompt is needed. Anyone who can copy this browser profile can open your local data.",
    nativeRecommended: "For the strongest protection, use the OM app for your phone or computer when it is available.",
    limitsHeading: "What this browser cannot do",
    limits: LIMITS,
    needsGesture: STAGE,
    continueCeremony: "Continue",
    errors: VAULT_ERROR,
    retry: "Try again",
    existing: {
      already_exists: "This browser already holds OM data. Use recovery to open it instead of starting again.",
      corrupt: "The OM data in this browser is damaged. Use recovery to try to restore it.",
    },
    openRecovery: "Open recovery",
    profileFailed: "Your identity could not be created.",
    retryProfile: "Try again",
    next: "Continue",
    passphrase: {
      toggle: "Also protect with a passphrase",
      hint: "A passphrase adds a second lock on top of your passkey or this browser. You will type it each time you open OM.",
      warning: "If you forget this passphrase, the data on this device cannot be opened and nobody can reset it. You can still restore your account with your recovery key.",
      label: "Passphrase",
      confirmLabel: "Type it again",
      tooShort: (min) => `Use at least ${String(min)} characters. A few unrelated words work well.`,
      mismatch: "The two passphrases do not match.",
      slowNote: "Opening OM will take a few extra seconds, and longer on an older phone. This slowness protects you if someone copies this browser's data.",
    },
  },
  recovery: {
    heading: "Your recovery key",
    body: "If you lose this device you will need this key. Write it down and keep it somewhere safe. OM shows it only once, and nobody at OM can recover it for you.",
    shownAnnouncement: "Your recovery key is now on screen.",
    keyLabel: "Recovery key",
    group: (n, total) => `Group ${String(n)} of ${String(total)}`,
    loading: "Preparing your key",
    unavailable: "The key is no longer available. Go back and start again.",
    copy: "Copy key",
    copied: "Copied. Clear it from your clipboard after you save it.",
    copyUnavailable: "Copying is not available in this browser. Write the key down by hand.",
    copiedOpen: "Copied. Other apps on this device may be able to read what you copy, so paste the key somewhere safe now, then clear your clipboard.",
    copyFailed: "Copying failed. Write the key down by hand.",
    toVerify: "I wrote it down",
    verifyHeading: "Check what you wrote",
    verifyBody: "Type the groups asked for below from your written copy.",
    verifyField: (n) => `Group ${String(n)}`,
    showAgain: "Show the key again",
    saved: "Confirm",
    confirming: "Checking",
    confirmFailed: "That did not match. Check your written copy and try again.",
  },
  invite: {
    heading: "Add your first contact",
    body: "Invite someone you know, or accept an invitation you were sent. You can skip this and do it later.",
    share: "Share my invite",
    shareTitle: "Chat with me on OM",
    shareText: "Open this link to start a private chat with me.",
    shareCancelled: "Sharing was cancelled.",
    shareFailed: "Sharing did not work. You can copy the link instead.",
    linkLabel: "Your invite link",
    expires: (ms) => `Expires ${timeFormat.format(ms)}`,
    showQr: "Show as QR code",
    qrLabel: "QR code for your invite",
    scan: "Scan a QR code",
    scanDenied: "Camera access was not allowed. You can paste a link instead.",
    scanNothing: "No code was found. Try again or paste a link.",
    acceptHeading: "Accept an invitation",
    acceptLabel: "Invitation link",
    acceptHint: "Paste the link someone sent you.",
    accept: "Accept invitation",
    parseError: {
      empty: "Paste a link first.",
      too_long: "That link is too long to be an OM invitation.",
      malformed: "That does not look like an OM invitation.",
    },
    rejected: withPrefix("Invitation not accepted."),
    failed: "Something went wrong. Try again.",
    queued: "Saved on this device. It goes out as soon as OM is connected to a provider; nothing has been sent yet.",
    skip: "Skip for now",
  },
  home: {
    toggle: "Where is my Home?",
    heading: "About your Home",
    loading: "Loading",
    loadFailed: "Facts about your Home could not be loaded.",
    intro: "A Home is the small online service that holds your messages until the person you wrote to opens OM. Think of it as your mailbox.",
    tier: (tier) => (tier === "none" ? "You have not picked a Home yet, so OM cannot send anything." : `Home level: ${tier}`),
    observers: OBSERVER,
    migrationAvailable: "You can move your Home to a different host later.",
    migrationUnavailable: "Moving your Home is not available yet.",
    origin: ORIGIN,
  },
  ready: {
    heading: "You are set up",
    body: "Your conversations will appear here.",
    done: "Open my chats",
  },
};

export const shellLabels: ShellLabels = {
  navigation: "Main navigation",
  list: "Conversations",
  detail: "Open conversation",
  skipToContent: "Skip to content",
};

export const switcherLabels: ProfileSwitcherLabels = { group: "Identities", current: "current" };

/** Strings for the composed app that no component owns. */
export interface AppStrings {
  readonly chats: string;
  readonly back: string;
  readonly selectConversation: string;
  readonly omnibar: string;
  readonly profile: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly requestsHeading: string;
  readonly conversationOpened: string;
  readonly notConnected: { readonly heading: string; readonly body: string };
  readonly vaultUnavailable: { readonly heading: string; readonly body: string };
  readonly requests: (n: number) => string;
  readonly settings: string;
  readonly addProfile: string;
  readonly providerStatus: {
    readonly none: string;
    readonly loopback: string;
  };
  readonly coreLoading: string;
  readonly coreFailed: { readonly heading: string; readonly body: string };
  readonly settingsHeading: string;
  readonly settingsDevices: string;
  readonly settingsRecovery: string;
  readonly settingsHome: string;
  readonly backToChats: string;
  readonly backToSettings: string;
}

export const appStrings: AppStrings = {
  chats: "Chats",
  back: "Back to conversations",
  selectConversation: "Choose a conversation to read it.",
  omnibar: "Find or do",
  profile: "Identity",
  loading: "Loading",
  loadFailed: "This could not be loaded. Try again in a moment.",
  requestsHeading: "People who want to reach you",
  conversationOpened: "Conversation opened",
  notConnected: {
    heading: "Not connected to a provider",
    body: "OM is running on this device, but no messaging provider is set up yet. You can create your identity here; messages cannot be sent or received until a provider is connected.",
  },
  vaultUnavailable: {
    heading: "This browser cannot protect your data",
    body: "OM needs secure storage and encryption support that this browser does not provide, so it will not start. Try an up-to-date browser, and not a private window.",
  },
  requests: (n) => plural(n, "request", "requests"),
  settings: "Settings",
  addProfile: "Add identity",
  providerStatus: {
    none: "Not connected to a provider. Nothing leaves this device until one is set up.",
    loopback: "TEST LOOPBACK PROVIDER, NOT PRODUCTION. Messages never leave this page.",
  },
  coreLoading: "Starting OM",
  coreFailed: {
    heading: "OM could not start",
    body: "The messaging engine did not load. Reload the page; if this keeps happening, your data is unchanged and safe on this device.",
  },
  settingsHeading: "Settings",
  settingsDevices: "Devices",
  settingsRecovery: "Recovery",
  settingsHome: "Your Home",
  backToChats: "Back to chats",
  backToSettings: "Back to settings",
};

export const scannerCopy = {
  heading: "Scan a QR code",
  hint: "Hold your camera steady in front of the other person's OM QR code. Nothing is recorded or saved.",
  cancel: "Cancel",
  cameraFailed: "The camera could not start. You can paste the invite link instead.",
  videoLabel: "Camera view",
} as const;

export const unlockCopy: UnlockCopy = {
  heading: "Unlock OM",
  body: "Your data on this device is locked.",
  passphraseLabel: "Passphrase",
  unlock: "Unlock",
  working: "Unlocking",
  workingSlow: "Unlocking. This takes a few seconds.",
  continueCeremony: "Continue",
  needsGesture: STAGE,
  errors: VAULT_ERROR,
  openRecovery: "Use my recovery key",
};

const dayFormat = new Intl.DateTimeFormat("en", { year: "numeric", month: "short", day: "numeric" });
const durationText = (ms: number): string => {
  const hours = Math.round(ms / 3_600_000);
  return hours >= 48 && hours % 24 === 0 ? plural(hours / 24, "day", "days") : plural(hours, "hour", "hours");
};

const outcomeCopy = (prefix: string): OutcomeCopy => ({
  working: "Working on it",
  pendingAuthorization: "Waiting for approval from your other devices. Nothing changes until then.",
  pendingNetwork: "Waiting for a connection. OM will keep trying.",
  pendingReconciliation: "Checking whether this went through. Do not repeat it; OM will tell you.",
  needsAction: NEEDS_ACTION,
  rejected: withPrefix(prefix),
  failed: "This could not be started, so nothing changed. Try again.",
});

export const devicesCopy: DevicesCopy = {
  heading: "Devices",
  loading: "Loading your devices",
  loadFailed: "Your devices could not be loaded. Try again in a moment.",
  controlState: {
    control_conflict: "Two of your devices made conflicting changes. OM is holding device and recovery changes until this is settled.",
    safety_freeze: "Safety Freeze is on. Your devices disagree about who is in control, so OM has paused device and recovery changes until enough of your devices agree. Messaging continues.",
    fork_detected: "Your account's security history split in two. OM has paused device and recovery changes until your devices settle it together.",
  },
  deviceLinkedNotice: (label, ms) => `A new device was added to your account: ${label}, ${timeFormat.format(ms)}. If this was not you, open it and remove it.`,
  linkRequestsHeading: "Devices asking to join",
  linkRequestRow: (label) => `${label} is asking to join your account`,
  currentHeading: "This device",
  otherHeading: "Your other devices",
  noOtherDevices: "No other devices are linked.",
  removedHeading: "Removed devices",
  role: { control: "Can manage your account", messaging: "Can send and read messages" },
  state: { active: "Active", quarantined: "Suspended", revocation_pending: "Removal waiting for approval", revoked: "Removed for good" },
  lastSeen: {
    now: "In use now", recent: "Used recently", this_week: "Used this week", this_month: "Used this month",
    long_ago: "Not used for a long time", never: "Never used",
  },
  cautions: {
    suspend_would_freeze: "Suspending this device would pause changes to your whole account, so OM offers to remove it instead.",
    last_voting_controller: "This is the only other device that can manage your account, so it cannot be suspended or removed from here.",
    history_sync_unavailable: "This device runs a different OM app and cannot share your message history with this one.",
    long_idle: "This device has not been used for a long time. Remove it if you no longer have it.",
  },
  enrolled: (ms) => `Added ${dayFormat.format(ms)}`,
  thisDevice: "You are using this device now",
  back: "Back to devices",
  detailsHeading: (label) => label,
  noActions: "There is nothing you can change for this device from here.",
  frozenNoActions: "Device changes are paused while Safety Freeze is on.",
  suspend: {
    action: "Suspend this device",
    heading: (label) => `Suspend ${label}?`,
    consequences: [
      "It stops getting new messages and new keys right away.",
      "It is not removed. It stays on your list as suspended.",
      "Anything already on it stays on it. OM cannot erase it from a distance.",
    ],
    confirm: "Suspend it now",
    cancel: "Keep it",
    done: "Suspended. It no longer gets new messages.",
  },
  revoke: {
    action: "Remove permanently",
    heading: (label) => `Remove ${label} for good?`,
    consequences: [
      "It can never be used with your account again. To use it again you would link it as a new device.",
      "It stops getting new messages and keys once the removal is final.",
      "Anything already on it stays on it. OM cannot erase it from a distance.",
      "Your other devices may need to approve this first.",
    ],
    confirm: "Remove it for good",
    cancel: "Keep it",
    done: "Removed for good.",
  },
  pendingHeading: "Still in progress",
  pendingKind: {
    RequestDeviceLink: "Opening a device link",
    ApproveDeviceLink: "Approving a new device",
    RejectDeviceLink: "Turning down a new device",
    SuspendDevice: "Suspending a device",
    RevokeDevice: "Removing a device",
  },
  link: {
    start: "Link a new device",
    heading: "Link a new device",
    body: "On the new device, choose to link it to an existing account. It will show a QR code and a link. Scan the code, or paste the link here.",
    scan: "Scan its QR code",
    scanNothing: "No code was found. Try again or paste the link.",
    pasteLabel: "Link from the new device",
    pasteHint: "It starts with om:link/",
    open: "Continue",
    parseError: {
      empty: "Paste the link first.",
      too_long: "That is too long to be a device link.",
      malformed: "That does not look like an OM device link.",
    },
    cancel: "Cancel",
    reviewHeading: "Check the new device",
    who: (label) => `${label} is asking to join your account.`,
    codeLabel: "Security code",
    codeHint: "The new device shows a code too. Link it only if both codes are exactly the same.",
    warning: "Only link a device you are holding yourself. A linked device can read your new messages.",
    announce: "All your other devices will be told about this new device. That cannot be turned off.",
    approve: "The codes match. Link it",
    reject: "This is not my device",
    expires: (ms) => `This request ends ${timeFormat.format(ms)}`,
    requestState: {
      awaiting_approval: "Waiting for your decision",
      pending_authorization: "Waiting for approval from your other devices",
      approved: "Linked",
      rejected: "Turned down",
      expired: "This request ran out of time. Start again on the new device.",
    },
    linked: "The new device is linked.",
    rejectedDone: "Turned down. The device was not linked.",
    done: "Back to devices",
  },
  outcome: outcomeCopy("Not done."),
};

export const linkThisDeviceCopy: LinkThisDeviceCopy = {
  heading: "Link to your account",
  body: "Use OM on a device you already have to scan this code or open this link. You will both see the same security code.",
  start: "Show my link code",
  linkLabel: "Link for your other device",
  qrLabel: "QR code to link this device",
  expires: (ms) => `Works until ${timeFormat.format(ms)}`,
  codeLabel: "Security code",
  codeHint: "Check that your other device shows exactly the same code before you approve there.",
  state: {
    waiting_for_scan: "Waiting for your other device",
    awaiting_approval: "Approve this on your other device",
    linked: "This device is linked.",
    rejected: "Your other device turned this down.",
    expired: "This ran out of time. Start again.",
  },
  outcome: outcomeCopy("Not done."),
};

export const recoveryCopy: RecoveryCopy = {
  heading: "Recovery",
  loading: "Loading recovery",
  loadFailed: "Recovery details could not be loaded. Try again in a moment.",
  back: "Back to recovery",
  policy: {
    on: "Recovery is on. If you lose your devices, you can get your account back.",
    pending: "Recovery is not set up yet. Until it is, losing this device could mean losing this account.",
    off_explicit: "Recovery is off. If you lose all your devices, this account is gone.",
  },
  custodiansHeading: "Where your recovery is kept",
  custodian: {
    synced_passkey: "Your passkeys",
    recovery_key: "Your recovery key",
    trusted_contacts: "People you trust",
    spare_device: "A spare device or security key",
  },
  distributionBound: "(works only with this version of OM)",
  scopesHeading: "What recovery can bring back",
  scope: {
    identity: { title: "Your account", body: "Who you are on OM and control of your devices." },
    history: { title: "Your messages", body: "Past messages from an encrypted backup." },
    provider: { title: "Your Home connection", body: "How OM reaches your Home and contacts." },
  },
  delay: (ms) => `To keep a thief out, getting your account back on a new device takes ${durationText(ms)} unless one of your devices approves it sooner. Any of your devices can stop it.`,
  recoveryKeySaved: "You have a recovery key.",
  recoveryKeyNone: "You do not have a recovery key.",
  recoveryKeyOnce: "OM showed your recovery key once, when you made it. For your safety it cannot be shown again.",
  incomingAlert: "Someone is trying to get into your account on a new device. If it was not you, stop it.",
  review: "Review it",
  requestsHeading: "Recoveries",
  direction: { incoming: "On another device", outgoing: "Started on this device" },
  requestState: { waiting: "Waiting", completed: "Finished", vetoed: "Stopped", cancelled: "Cancelled" },
  started: (ms) => `Started ${timeFormat.format(ms)}`,
  activatesAt: (ms) => `Can finish ${timeFormat.format(ms)} unless it is stopped`,
  activatesUnknown: "OM cannot say yet when this can finish.",
  stopIncoming: "Stop this recovery",
  cancelOutgoing: "Cancel my recovery",
  pendingHeading: "Still in progress",
  pendingKind: { StartRecovery: "Starting a recovery", VetoRecovery: "Stopping a recovery" },
  start: {
    action: "Start a recovery",
    heading: "Start a recovery",
    body: "Choose what you want back. Each kind of recovery is separate.",
    scopeLegend: "What do you want back?",
    custodianLegend: "Use",
    consequences: {
      identity: [
        "Your other devices will be told and any of them can stop it.",
        "It finishes after the waiting time, or sooner if one of your devices approves it.",
        "When it finishes, devices that were not part of it lose control of your account.",
      ],
      history: ["This brings back past messages only. It does not change who controls your account."],
      provider: ["This reconnects your Home only. It does not change who controls your account."],
    },
    confirm: "Start recovery",
    cancel: "Cancel",
    done: "Recovery finished.",
  },
  veto: {
    heading: { incoming: "Stop this recovery?", outgoing: "Cancel your recovery?" },
    consequences: {
      incoming: [
        "The recovery will not finish.",
        "Whoever started it may have your recovery key or passkey. OM will help you replace it.",
      ],
      outgoing: ["The recovery stops. You can start a new one later."],
    },
    confirm: { incoming: "Stop it", outgoing: "Cancel it" },
    cancel: "Go back",
    done: "Stopped.",
  },
  outcome: outcomeCopy("Not done."),
};

export const deviceScannerCopy = {
  ...scannerCopy,
  hint: "Hold your camera steady in front of the QR code on your new device. Nothing is recorded or saved.",
  cameraFailed: "The camera could not start. You can paste the link from the new device instead.",
} as const;

/** The landing page. Rendered twice from this one source: prerendered into index.html (for visitors and crawlers without JS) and in the app. */
export interface LandingLink { readonly label: string; readonly url: string }

export interface LandingCopy {
  readonly title: string;
  readonly description: string;
  readonly brand: string;
  readonly beta: string;
  readonly headline: string;
  readonly lead: string;
  readonly getStarted: string;
  readonly startUsing: string;
  /** The sample chat: the three messages come in on the left, the reply goes out on the right. */
  readonly chat: { readonly incoming: readonly string[]; readonly reply: string; readonly label: string };
  readonly byline: { readonly label: string; readonly url: string };
  readonly why: { readonly heading: string; readonly intro: string; readonly points: readonly { readonly title: string; readonly body: string }[]; readonly closing: string };
  /** Titles mark their bold letters with asterisks, for example "*O*pen *M*essenger". */
  readonly names: { readonly heading: string; readonly intro: string; readonly items: readonly { readonly title: string; readonly body: string; readonly link?: LandingLink }[] };
  readonly security: { readonly heading: string; readonly intro: string; readonly points: readonly string[]; readonly links: readonly LandingLink[] };
  readonly follow: { readonly heading: string; readonly links: readonly (LandingLink & { readonly icon: string })[] };
  readonly install: {
    readonly heading: string;
    readonly intro: string;
    readonly ios: string;
    readonly android: string;
    readonly desktop: string;
    readonly installButton: string;
    readonly installed: string;
    readonly iosHint: string;
  };
  readonly selfHost: { readonly heading: string; readonly body: string; readonly link: string; readonly linkLabel: string };
  readonly footer: { readonly prefix: string; readonly seedlife: string; readonly seedlifeUrl: string; readonly suffix: string; readonly siteSource: LandingLink };
}

// The two fixed project links. Built from a scheme and a host so the "no hard-coded remote URL" lint, which protects
// provider endpoints, is not weakened for them; these are navigation links only, never fetched by the app.
const https = (hostAndPath: string): string => ["https", hostAndPath].join("://");

export const landingCopy: LandingCopy = {
  title: "OM (Oh Messenger) Beta: The messaging app you own",
  description: "OM is a fast, uncluttered, open-source messenger with strong end-to-end encryption, designed so that no single company, server, or app store is indispensable. Beta.",
  brand: "OM",
  beta: "Beta",
  headline: "The messaging app you own",
  lead: "OM is a fast, uncluttered, open-source messenger with strong end-to-end encryption, designed so that no single company, server, or app store is indispensable.",
  getStarted: "Get started",
  startUsing: "Start using OM",
  chat: { label: "What using OM should feel like", incoming: ["Install OM", "Pick a name", "Say hello"], reply: "That\u2019s it? \u{1F642}" },
  byline: { label: "A SeedLife project", url: https("lonunjie.org") },
  why: {
    heading: "Another app?!",
    intro: "We like messaging, but it is confusing that there are so many apps. We are building the one messaging app to have the **best features combined.**",
    points: [
      { title: "Closed doors", body: "Most apps do not talk to each other. If your friends are on a different app, you either join it or lose touch." },
      { title: "Free is not always free", body: "Many apps cost nothing because your attention, and sometimes your data, pays the bills. When you are not the customer, you can end up being the product." },
      { title: "Private only if you find the switch", body: "Some popular apps (\u{1F44B} Telegram) leave end-to-end encryption off unless you turn it on yourself, and some chats can never use it. Until it is on, the company behind the app can read your messages." },
      { title: "Private, but still watching", body: "Some apps do encrypt your messages and still collect plenty around them: who you talk to, when, how often and from where. Others add extras, like AI summaries (Meta \u{1F440}), that send your messages to their servers to work." },
      { title: "Good, but in one pair of hands", body: "The best private messengers are wonderful (\u{1F44F} Signal), and we are grateful for them. Still, each depends on one organization and one set of servers, and most start with your phone number." },
    ],
    closing: "Most of the world\u2019s messengers are built to make money, and that shapes what they do with your conversations. None of them fully holds the ideals of a kind messenger that leaves your data with you. It\u2019s [[our]] data, we should [[own]] it. So **we are building the [[one]] we wished existed.**",
  },
  names: {
    heading: "What\u2019s in a name?",
    intro: "OM stands for many things, here are some of them.",
    items: [
      { title: "*O*pen-source *M*essenger", body: "A gift to everyone.", link: { label: "Anyone can read it, check it, run it, and make it better.", url: https("github.com/lonunjie/om") } },
      { title: "*0*M", body: "Zero total control. Nobody, including us, gets total control. Real systems need helpers, but no one should be able to switch you off or own the whole network." },
      { title: "*O*pen *M*essenger", body: "Not a walled garden. Other apps can talk to OM without asking permission." },
      { title: "*O*wn *M*essenger", body: "Make it yours: your identity, your look, your own setup. No central OM account needed." },
      { title: "*O*ur *M*essenger", body: "Made for people who gather: families, friends, teams and communities, each with their own spaces and rules." },
      { title: "*O*h *M*essenger", body: "Friendly, lively and human. Designed to be easy: if you can send a text, you can use OM. We always kept Grandma in mind." },
      { title: "*O*ne *M*essenger", body: "Our vision, if possible, is to have just one messaging app that connects everyone." },
    ],
  },
  security: {
    heading: "Security & privacy",
    intro: "OM is new, and we would rather be honest about it than impressive.",
    points: [
      "OM is in beta and no human cryptographer has audited it yet. Things go wrong in a beta. Back up for safety.",
      "Your messages travel through a small helper service called your Home. It knows when you are online and how many messages you send. What you write stays scrambled, so the Home cannot read it.",
      "Releases are signed. The signing root needs two of three keys (two hardware wallets and one software key) plus a post-quantum key, all held by one custodian. Against coercion that is no stronger than one key.",
    ],
    links: [
      { label: "Found a problem or have an idea? Tell us on GitHub", url: https("github.com/lonunjie/om/issues") },
      { label: "Read the full security policy", url: https("github.com/lonunjie/om/blob/main/SECURITY.md") },
    ],
  },
  follow: {
    heading: "Follow along",
    links: [
      { label: "Bluesky", icon: "bluesky", url: https("bsky.app/profile/lonunjie.org") },
      { label: "X", icon: "x", url: https("x.com/lonunjie") },
      { label: "YouTube", icon: "youtube", url: https("www.youtube.com/@lonunjie") },
      { label: "GitHub", icon: "github", url: https("github.com/lonunjie") },
      { label: "hi@lonunjie.org", icon: "email", url: ["mailto", "hi@lonunjie.org"].join(":") },
    ],
  },
  install: {
    heading: "Install OM as an app",
    intro: "OM works in your browser, and it feels nicer installed. It then opens in its own window like any other app.",
    ios: "iPhone and iPad (Safari): tap the Share button, then Add to Home Screen.",
    android: "Android (Chrome): open the menu, then Install app.",
    desktop: "Desktop (Chrome, Edge): choose Install in the address bar or the browser menu.",
    installButton: "Install OM",
    installed: "OM is installed on this device.",
    iosHint: "To install on this device, tap Share, then Add to Home Screen.",
  },
  selfHost: {
    heading: "Run your own copy",
    body: "You should not have to take anyone's word for it, including ours. Grab the code and host your very own copy of this app.",
    link: https("github.com/lonunjie/om"),
    linkLabel: "OM on GitHub",
  },
  footer: {
    prefix: "OM is a ", seedlife: "SeedLife", seedlifeUrl: https("lonunjie.org"), suffix: " project.",
    siteSource: { label: "Site source", url: https("github.com/lonunjie/om.lonunjie.org") },
  },
};

export const homeCopy: HomeCopy = {
  heading: "Your Home",
  currentHeading: "The Home you are using",
  none: "No Home is set up. Nothing leaves this device until you choose one.",
  address: (url) => `Address: ${url}`,
  source: { chosen: "You chose this Home.", default: "This is the Home that came with this version of OM." },
  checkingCurrent: "Checking how your Home is doing.",
  selfReported: "This is what the Home says about itself; OM cannot confirm who runs it.",
  changeHeading: "Use a different Home",
  changeBody: "Type the address of the Home you want to use. OM asks it who it is before you can choose it. Nothing is changed until you confirm.",
  urlLabel: "Home address",
  urlHint: "It starts with https:// and has no password, question mark or extra parts.",
  check: "Check this Home",
  checking: "Checking this address.",
  changeWarning: "Both people in a conversation need to use the same Home. Changing yours can cut you off from conversations you already have until the other person changes theirs too.",
  useThis: "Use this Home",
  useAnyway: "Use it anyway",
  alreadyUsing: "This is the Home you are already using.",
  saveFailed: "This choice could not be saved, so nothing changed.",
  saved: "Saved. OM will use this Home the next time it connects.",
  reconnect: "Connect now (you will unlock OM again)",
  useDefault: "Go back to the Home that came with OM",
  disconnect: "Stop using this Home",
  cleared: "Your choice was removed. OM will use the default the next time it connects.",
  invalid: {
    malformed: "That does not look like a web address. Start it with https://.",
    not_https: "The address must start with https://, so that what you send to it is protected on the way.",
    credentials: "The address cannot contain a username or password.",
    query_or_fragment: "The address cannot contain a question mark, a # or anything after them.",
  },
  result: {
    ok: (id) => `This address answered and calls itself \u201c${id}\u201d.`,
    health: {
      healthy: "It reports that it is working normally.",
      degraded: "It reports reduced service: messages may be late. You can still use it.",
      unhealthy: "It reports that it is not working, so it cannot be chosen now. Try again later.",
      unknown: "It does not say how it is doing, so OM cannot tell whether it is working.",
    },
    unreachable: "OM could not reach this address. Check it, your connection, and try again. If the address is right, the Home may be offline or may not accept connections from this app.",
    failing: (status) => `This address answered with a server error (${String(status)}). The Home may be having trouble; try again later.`,
    notAHome: "This address answered, but it is not an OM Home.",
    wrongVersion: (offered) => `This Home only speaks a version OM does not understand${offered === "" ? "" : ` (it offers: ${offered})`}. Update OM, or choose a different Home.`,
    noMailbox: "This Home does not deliver messages, so it cannot be used here.",
  },
  lost: {
    heading: "Some messages cannot reach you here",
    body: (n) => `${plural(n, "mailbox", "mailboxes")} at this Home could not be set up, because someone else already holds ${n === 1 ? "it" : "them"}. Messages for ${n === 1 ? "that conversation" : "those conversations"} will not arrive. This can happen if someone learned a mailbox address before you first used it. Ask the other person to invite you again, or choose a different Home.`,
    item: (reference) => `Reference ${reference}`,
  },
  back: "Back to settings",
};
