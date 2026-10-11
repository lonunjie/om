// The only place user-visible English lives. Components take these as props; keys are machine reason codes, so a
// missing code is a compile error. Wording rules: plain language, no provider names, honest about limits.
import type { ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, SecurityNoticeCode, UserActionCode, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason } from "@om/platform-api";
import type { ConversationCopy, OnboardingCopy, ProfileSwitcherLabels, ShellLabels } from "@om/ui";

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
  request: {
    heading: "Message request",
    unknownSender: "Not in your contacts",
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
    copyUnavailable: "Copying is not available here. Write the key down by hand.",
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
    skip: "Skip for now",
  },
  home: {
    toggle: "Where is my Home?",
    heading: "About your Home",
    loading: "Loading",
    loadFailed: "Facts about your Home could not be loaded.",
    tier: (tier) => `Home level: ${tier}`,
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
    heading: "OM is not connected yet",
    body: "This build has no messaging engine, so nothing here is sent or stored. Your data is safe: this page does not hold any.",
  },
  vaultUnavailable: {
    heading: "This browser cannot protect your data",
    body: "OM needs secure storage and encryption support that this browser does not provide, so it will not start. Try an up-to-date browser, and not a private window.",
  },
  requests: (n) => plural(n, "request", "requests"),
};
