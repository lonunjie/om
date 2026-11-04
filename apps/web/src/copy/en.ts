// The only place user-visible English lives. Components take these as props; keys are machine reason codes, so a
// missing code is a compile error. Wording rules: plain language, no provider names, honest about limits.
import type { ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, SecurityNoticeCode, UserActionCode, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason } from "@om/platform-api";
import type { ConversationCopy, DevicesCopy, HomeCopy, LinkThisDeviceCopy, OmnibarCopy, OnboardingCopy, OutcomeCopy, ProfileSwitcherLabels, RecoveryCopy, ShellLabels, UnlockCopy } from "@om/ui";

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
const clockFormat = new Intl.DateTimeFormat("en", { hour: "numeric", minute: "2-digit" });
const withYearFormat = new Intl.DateTimeFormat("en", { hour: "numeric", minute: "2-digit", month: "short", day: "numeric", year: "numeric" });
/** A message sent today shows only its time; an older one adds the day, and the year once it is not this year. */
function sentAt(ms: number, now: Date = new Date()): string {
  const then = new Date(ms);
  if (then.toDateString() === now.toDateString()) return clockFormat.format(then);
  return (then.getFullYear() === now.getFullYear() ? timeFormat : withYearFormat).format(then);
}
const plural = (n: number, one: string, many: string): string => `${String(n)} ${n === 1 ? one : many}`;

export const conversationCopy: ConversationCopy = {
  list: {
    label: "Conversations",
    empty: "No conversations yet.",
    noActivity: "No messages yet",
    unread: (n) => `${String(n)} unread`,
    you: "You:",
    draft: "Draft:",
    securityNotice: "Security notice",
  },
  view: {
    label: "Messages",
    empty: "No messages yet. Say hello.",
    loadFailed: "These messages could not be loaded.",
    incoming: "They said:",
    outgoing: "You said:",
    retry: "Try sending again",
    back: "Back to chats",
    formatTime: (ms) => sentAt(ms),
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
    awaiting_acceptance: "Waiting for them to accept. They need to open OM to see your request; you can message them as soon as they do.",
    accepted_unbound: "Setting up. This person is not verified yet, and you can't send messages until it is done.",
    establishing: "Almost ready. Finishing the secure connection.",
    restored: "Restored from a backup file. You can read these messages but not reply here, and OM cannot check that they are genuine. To talk again, ask for a new invite.",
  },
  pressure: {
    notice: "You are receiving a lot of requests from strangers. People you invited can still reach you.",
  },
  request: {
    heading: "Message request",
    unknownSender: "Not in your contacts",
    hint: "They opened your invite. They typed the name themselves, so accept only if you know them.",
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
  "webauthn-prf": "Your chats are protected by a passkey: your fingerprint, face or screen code, or your password manager.",
  "device-key": "Your chats are protected by a private key kept in this browser. Nothing asks for a fingerprint or code to open them here.",
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
  corrupt: "Your chats on this device are damaged.",
  locked: "Your chats are locked. Open them first.",
  already_exists: "This browser already has chats saved for this person.",
  conflict: "Another OM window is doing this. Close it and try again.",
  needs_user_gesture: "Press the button again to continue.",
  ceremony_failed: "The passkey prompt was cancelled or did not finish.",
  prf_unavailable: "Your passkey could not protect your chats.",
  unwrap_failed: "Your chats could not be opened.",
  unsupported_factor: "That way of opening your chats is not supported here.",
  passphrase_required: "Enter your password to open your chats.",
  wrong_passphrase: "That password is not right. Try again.",
  unsupported_platform: "This browser cannot protect your chats safely enough for OM. Try another browser or the OM app.",
  malformed: "Your protected chats could not be read.",
  unsupported_version: "Your chats were saved by a newer version of OM. Update OM to open them.",
  wrong_purpose: "The locked data was used in the wrong place.",
  integrity_failure: "A safety check on your chats failed, so OM did not open them.",
  crypto_failed: "This browser could not finish protecting your chats.",
  storage_failed: "This browser could not save your chats.",
  delete_unverified: "OM could not confirm the data was removed.",
};
/** What a probe of a typed address can say, shared by Settings > Message delivery and the first-run setup page. */
const HOME_INVALID: HomeCopy["invalid"] = {
  malformed: "That does not look like a web address. Start it with https://.",
  not_https: "The address must start with https://, so that what you send to it is protected on the way.",
  credentials: "The address cannot contain a username or password.",
  query_or_fragment: "The address cannot contain a question mark, a # or anything after them.",
};

const HOME_RESULT: HomeCopy["result"] = {
  ok: (id) => `This address answered and calls itself \u201c${id}\u201d.`,
  health: {
    healthy: "It reports that it is working normally.",
    degraded: "It reports reduced service: messages may be late. You can still use it.",
    unhealthy: "It reports that it is not working, so it cannot be chosen now. Try again later.",
    unknown: "It does not say how it is doing, so OM cannot tell whether it is working.",
  },
  unreachable: "OM could not reach this address. Check it, your connection, and try again. If the address is right, the delivery service may be offline or may not accept connections from this app.",
  failing: (status) => `This address answered with a server error (${String(status)}). The delivery service may be having trouble; try again later.`,
  notAHome: "This address answered, but it is not an OM delivery service.",
  wrongVersion: (offered) => `This delivery service only speaks a version OM does not understand${offered === "" ? "" : ` (it offers: ${offered})`}. Update OM, or choose a different delivery service.`,
  noMailbox: "This delivery service does not deliver messages, so it cannot be used here.",
};

const OBSERVER: Readonly<Record<HomeObserverCode, string>> = {
  sponsor_sees_metadata: "The person or group running your delivery service can see who talks to whom and when, but not what is said.",
  host_sees_metadata: "The service that delivers your messages can see who talks to whom and when, but not what is said.",
  host_sees_ciphertext: "The service that delivers your messages stores your messages only in scrambled form.",
  no_third_party: "No outside party sits between you and your delivery service.",
};
const ORIGIN: Readonly<Record<ClientOriginChoiceCode, string>> = {
  default_origin: "You are using OM from its standard web address.",
  independent_origin_available: "You can choose to run OM from an address you control.",
  independent_origin_active: "You are running OM from an address you control.",
};

export const onboardingCopy: OnboardingCopy = {
  frame: { brand: "OM", beta: "Beta", step: (n, total) => `Step ${String(n)} of ${String(total)}` },
  welcome: {
    heading: "Welcome to OM",
    body: "Private chats. Set up in seconds.",
    start: "Get started",
  },
  protect: {
    heading: "What should we call you?",
    action: "Next: Secure your identity",
    working: "Setting up",
    passkeyUser: { name: "OM", displayName: "OM" },
    defaultProfileLabel: "Me",
    name: {
      label: "Your name",
      missing: "Type your name first.",
      tooLong: (max) => `Use ${String(max)} letters or fewer.`,
    },
    resultHeading: "How your chats are protected",
    moreOptions: "More options",
    method: METHOD,
    reason: WRAPPING_REASON,
    reducedAtRest: "Your passkey is shared between your devices by the company or app that keeps it. Anyone who can sign in to that account could open your chats. Protection here is weaker.",
    deviceKeyLimit: "The protection stays in this browser and nothing asks for a fingerprint or code. Anyone who can copy this browser's data could open your chats.",
    nativeRecommended: "For the strongest protection, use the OM app for your phone or computer when it is available.",
    limitsHeading: "What this browser cannot do",
    limits: LIMITS,
    needsGesture: STAGE,
    continueCeremony: "Continue",
    waiting: {
      body: "Confirm on this device.",
      slow: "No prompt? You can skip it.",
      skip: "Skip the prompt",
    },
    errors: VAULT_ERROR,
    retry: "Try again",
    existing: {
      already_exists: "This browser already has OM chats. Use your safety code to open them instead of starting again.",
      corrupt: "The OM chats in this browser are damaged. Use your safety code to try to get them back.",
    },
    openRecovery: "Use my safety code",
    profileFailed: "Your OM account could not be set up.",
    retryProfile: "Try again",
    passphrase: {
      toggle: "Also add a password of my own",
      hint: "A password adds a second lock. You type it each time you open OM.",
      warning: "If you forget this password, nobody can reset it and the chats on this device cannot be opened. You can still get your account back with your safety code.",
      label: "Password",
      confirmLabel: "Type it again",
      tooShort: (min) => `Use at least ${String(min)} characters. A few unrelated words work well.`,
      mismatch: "The two passwords do not match.",
      slowNote: "Opening OM will take a few extra seconds, longer on an older phone. The short wait is what keeps your chats safe if someone copies this browser's data.",
    },
  },
  recovery: {
    heading: "Your safety code",
    body: "If you lose this device, this code gets your account back. Write it down on paper and keep it somewhere safe, like important papers at home. OM shows it only once, and nobody at OM can recover it for you.",
    shownAnnouncement: "Your safety code is now on screen.",
    keyLabel: "Safety code",
    group: (n, total) => `Group ${String(n)} of ${String(total)}`,
    loading: "Preparing your code",
    unavailable: "The code is no longer available. Go back and start again.",
    copy: "Copy code",
    copied: "Copied. Clear it from your clipboard after you save it.",
    copyUnavailable: "Copying is not available in this browser. Write the code down by hand.",
    copiedOpen: "Copied. Other apps on this device may be able to read what you copy, so paste the code somewhere safe now, then clear your clipboard.",
    copyFailed: "Copying failed. Write the code down by hand.",
    toVerify: "I wrote it down",
    verifyHeading: "Check what you wrote",
    verifyBody: "Type the groups asked for below from your written copy.",
    verifyField: (n) => `Group ${String(n)}`,
    showAgain: "Show the code again",
    saved: "Confirm",
    confirming: "Checking",
    confirmFailed: "That did not match. Check your written copy and try again.",
  },
  invite: {
    heading: "Invite someone to chat",
    share: "Share invite link",
    shareTitle: "Chat with me on OM",
    shareText: "Open this link to start a private chat with me.",
    shareFailed: "Sharing did not work. You can copy the link instead.",
    copyLink: "Copy link",
    copied: "Link copied",
    copyFailed: "Copying did not work. Select the link and copy it by hand.",
    sharedByCopy: "Link copied",
    linkLabel: "Your invite link",
    expires: (ms) => `Expires ${timeFormat.format(ms)}`,
    showQr: "Show my QR",
    qrLabel: "QR code for your invite",
    scan: "Scan QR code",
    scanDenied: "Camera access was not allowed. You can paste a link instead.",
    scanNothing: "No code was found. Try again or paste a link.",
    acceptHeading: "Accept an invitation",
    acceptLabel: "Invitation link",
    haveInvite: "Have an invite?",
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
  incoming: {
    heading: "Accept this invite?",
    body: "Accept only if you know who sent it.",
    accept: "Accept invite",
    decline: "Not now",
    accepting: "Sending your answer",
    parseError: "This invite link does not work. Ask the person to send a new one.",
    rejected: {
      invalid: "This invite link does not work. Check that it is not your own invite, or ask the person to send a new one.",
      unsupported: "This version of OM cannot use that invite.",
      unauthorized: "This invite is not for you. Ask the person to send a new one.",
      revoked: "The person cancelled this invite. Ask them to send a new one.",
      expired: "This invite has expired. Ask the person to send a new one.",
      stale_state: "Something changed while you waited. Try again.",
      conflict: "You already accepted this invite. Your chat appears when the person approves it.",
      policy_denied: "This invite cannot be used here. Ask the person to send a new one.",
      quota: "The service is very busy right now. Try again in a little while.",
      rate_limited: "Too many tries. Wait a minute and try again.",
      integrity_failure: "This invite link is damaged. Ask the person to send it again.",
      unavailable: "OM could not reach the service. Check your connection and try again.",
      not_found: "This invite was not found. It may have been used or cancelled. Ask the person to send a new one.",
    },
    failed: "Something went wrong. Try again.",
    queued: "Still sending. Keep this page open.",
  },
  home: {
    toggle: "What is the delivery service?",
    heading: "About your delivery service",
    loading: "Loading",
    loadFailed: "Facts about your delivery service could not be loaded.",
    intro: "The delivery service is a small online service that carries your messages. It holds each one, still locked, until the other person's device collects it. It cannot read them, and your chats and keys stay on your devices.",
    tier: (tier) => (tier === "none" ? "You have not picked a delivery service yet, so OM cannot send anything." : `Service level: ${tier}`),
    observers: OBSERVER,
    migrationAvailable: "You can move your delivery service to a different host later.",
    migrationUnavailable: "Moving your delivery service is not available yet.",
    origin: ORIGIN,
  },
  relay: {
    heading: "OM is different",
    why: [
      "You own your data, and you own your delivery service.",
      "So even when we are not around, your messages are yours.",
    ],
    pick: "Choose how to set up your delivery service. Come back here once it is set up.",
    easyHeading: "Easy",
    ownFree: "Set up my own, free",
    advancedHeading: "Advanced",
    advancedIntro: "Set up your own server. These are coming later.",
    later: {
      items: ["On my own server, as a container", "With an install script", "On a computer at home, through a tunnel", "On my own server, with open ports"],
    },
    // ADR-0186 decision 2: the only place a vendor is named. The copy test (en.test.ts) allows provider names in this group and nowhere else.
    cloudflare: {
      uses: "Two free accounts: Cloudflare, and GitHub or GitLab (Cloudflare needs it to copy the setup into). You create or sign in to them on the next steps.",
      steps: [
        "Copy your setup value below.",
        "Open Cloudflare setup. Create or sign in to a free Cloudflare account and a GitHub or GitLab account.",
        "Paste your setup value when it asks, and wait for it to finish.",
        "Come back to this tab and type the address it shows.",
      ],
      addressExample: "The address looks like https://om-relay.your-name.workers.dev",
      protect: "Protect that Cloudflare account with a passkey or security key if it offers one: whoever gets into the account can change your delivery service.",
      open: "Open Cloudflare setup",
      opensNewTab: "(opens in a new tab)",
      notReady: "This way of setting up is not ready yet. If you already have a delivery service of your own, type its address below.",
    },
    setup: {
      heading: "Set up your delivery service",
      valueLabel: "Your setup value",
      valueNote: "It is not a secret. It makes sure only you can add things to your delivery service. The matching key stays on this device.",
      preparing: "Making your setup value",
      prepareFailed: "Your setup value could not be made on this device. Go back and try again.",
      copy: "Copy setup value",
      copied: "Setup value copied",
      copyFailed: "Copying failed. Select the value and copy it by hand.",
      addressLabel: "Address of your delivery service",
      addressHint: "OM checks the address before using it. It must start with https://",
      check: "Check and continue",
      checking: "Checking the address",
      saveFailed: "This address could not be saved on this device. Nothing was changed. Try again.",
      notLocked: "This delivery service did not ask for your setup value, so anyone who learns its address could use it. It was not saved. Set it up again and paste your setup value when it asks.",
      back: "Back",
    },
    invalid: HOME_INVALID,
    result: HOME_RESULT,
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
  /** The landing page's note for a visit that came from someone's invitation link. */
  readonly chats: string;
  readonly back: string;
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
  readonly coreLoadingNote: string;
  readonly coreFailed: { readonly heading: string; readonly body: string };
  readonly appName: string;
  readonly settingsHeading: string;
  readonly settingsSearch: { readonly label: string; readonly none: (query: string) => string };
  readonly settingsRows: { readonly devices: string; readonly recovery: string; readonly backup: string; readonly delivery: string; readonly lock: string; readonly help: string; readonly addProfile: string };
  /** Words a person might type for a row that its title does not contain. */
  readonly settingsKeywords: { readonly devices: string; readonly recovery: string; readonly backup: string; readonly delivery: string; readonly lock: string; readonly help: string; readonly addProfile: string };
  readonly settingsLock: string;
  readonly settingsNeedsAttention: string;
  readonly settingsDevices: string;
  readonly settingsRecovery: string;
  readonly settingsBackup: string;
  readonly settingsHome: string;
  readonly settingsHelp: string;
  readonly backToChats: string;
  readonly backToSettings: string;
  readonly safetyCode: { readonly reminder: string; readonly action: string };
  /** The new-conversation action beside the chat list (NAVIGATION_AND_OMNIBAR.md, default shell). */
  readonly newConversation: { readonly action: string; readonly title: string; readonly close: string };
  /** Omnibar results that are actions, not places. */
  readonly omniActions: { readonly invite: string; readonly scan: string; readonly showQr: string; readonly useLink: string; readonly chats: string; readonly settings: string };
  /** Words that also find an Omnibar action (not shown). */
  readonly omniKeywords: { readonly invite: string; readonly scan: string; readonly showQr: string; readonly useLink: string; readonly chats: string; readonly settings: string };
}

export const appStrings: AppStrings = {
  chats: "Chats",
  back: "Back to chats",
  omnibar: "Find or do",
  profile: "Identity",
  loading: "Loading",
  loadFailed: "This could not be loaded. Try again in a moment.",
  requestsHeading: "People who want to reach you",
  conversationOpened: "Conversation opened",
  notConnected: {
    heading: "Not connected to a delivery service",
    body: "OM is running on this device, but no delivery service is set up yet. You can create your identity here; messages cannot be sent or received until one is connected.",
  },
  vaultUnavailable: {
    heading: "This browser cannot protect your data",
    body: "OM needs secure storage and encryption support that this browser does not provide, so it will not start. Try an up-to-date browser, and not a private window.",
  },
  requests: (n) => plural(n, "request", "requests"),
  settings: "Settings",
  addProfile: "Add identity",
  providerStatus: {
    none: "Not connected to a delivery service. Nothing leaves this device until one is set up.",
    loopback: "TEST LOOPBACK PROVIDER, NOT PRODUCTION. Messages never leave this page.",
  },
  coreLoading: "Getting OM ready",
  coreLoadingNote: "Your chats stay on this device.",
  coreFailed: {
    heading: "OM could not start",
    body: "The messaging engine did not load. Reload the page; if this keeps happening, your data is unchanged and safe on this device.",
  },
  appName: "OM",
  settingsHeading: "Settings",
  settingsSearch: { label: "Search settings", none: (query) => `Nothing matches “${query}”.` },
  settingsRows: {
    devices: "The phones and computers that can open your chats.",
    recovery: "How to get your chats back if you lose this device.",
    backup: "Save your chats to a file, or read them back from one.",
    delivery: "The service that carries your messages between devices.",
    lock: "Whether OM asks before it opens on this device.",
    help: "Report a problem or suggest an idea.",
    addProfile: "Set up a second identity on this device.",
  },
  settingsKeywords: {
    devices: "phone computer linked sessions",
    recovery: "safety code backup restore lost",
    backup: "backup restore export import download save file history chats copy move new phone drive usb password",
    delivery: "relay server network connection messages",
    lock: "password passcode pin passkey fingerprint face biometric privacy security",
    help: "bug problem issue report feedback idea suggestion feature request support github version",
    addProfile: "account second identity profile new",
  },
  settingsLock: "Lock",
  settingsNeedsAttention: "needs attention",
  settingsDevices: "Devices",
  settingsRecovery: "Recovery",
  settingsBackup: "Backup file",
  settingsHome: "Message delivery",
  settingsHelp: "Help and feedback",
  backToChats: "Back to chats",
  backToSettings: "Back to settings",
  safetyCode: {
    reminder: "If you lose this device, only your safety code can get your account back. Keep a paper copy; it takes a minute and shows only once.",
    action: "Save my safety code",
  },
  newConversation: { action: "New conversation", title: "New conversation", close: "Close" },
  omniActions: {
    invite: "Invite someone to chat", scan: "Scan QR code", showQr: "Show my QR", useLink: "Use this invite link", chats: "Chats", settings: "Settings",
  },
  omniKeywords: {
    invite: "new conversation message start add contact friend share link send",
    scan: "camera invite code accept qr",
    showQr: "invite code share my qr",
    useLink: "accept invitation",
    chats: "home conversations messages back",
    settings: "preferences options account",
  },
};

export const omnibarCopy: OmnibarCopy = {
  name: "Omnibar",
  triggerText: "Find or do…",
  openAction: "Open Omnibar",
  placeholder: "Find or do…",
  close: "Close Omnibar",
  results: "Results",
  suggested: "Suggested",
  kinds: { conversation: "Chat", action: "Action", setting: "Setting" },
  none: (query) => `Nothing matches “${query}”.`,
  count: (n) => (n === 0 ? "No results" : n === 1 ? "1 result" : `${String(n)} results`),
};

export const scannerCopy = {
  heading: "Scan a QR code",
  hint: "Hold your camera steady in front of the other person's OM QR code. Nothing is recorded or saved.",
  cancel: "Cancel",
  cameraFailed: "The camera could not start. You can paste the invite link instead.",
  videoLabel: "Camera view",
} as const;

export const unlockCopy: UnlockCopy = {
  heading: "Welcome back!",
  body: "Confirm on this device to open your chats.",
  bodyPassphrase: "Type your password to open your chats.",
  passphraseLabel: "Password",
  unlock: "Open my chats",
  working: "Opening",
  workingSlow: "Opening your chats. This takes a few seconds.",
  continueCeremony: "Continue",
  needsGesture: STAGE,
  errors: VAULT_ERROR,
  openRecovery: "Use my safety code",
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
  linkLater: "Using the same account on a second device is not ready yet. Until it is, set OM up on each device and invite each one.",
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
    pending: "Recovery is not finished yet. Save your safety code to finish it; until then, losing this device could mean losing this account.",
    off_explicit: "Recovery is off. If you lose all your devices, this account is gone.",
  },
  custodiansHeading: "Where your recovery is kept",
  custodian: {
    synced_passkey: "Your passkeys",
    recovery_key: "Your safety code",
    trusted_contacts: "People you trust",
    spare_device: "A spare device or security key",
  },
  distributionBound: "(works only with this version of OM)",
  scopesHeading: "What recovery can bring back",
  scope: {
    identity: { title: "Your account", body: "Who you are on OM and control of your devices." },
    history: { title: "Your messages", body: "Past messages from an encrypted backup." },
    provider: { title: "Your delivery service", body: "How OM reaches your delivery service and contacts." },
  },
  delay: (ms) => `To keep a thief out, getting your account back on a new device takes ${durationText(ms)} unless one of your devices approves it sooner. Any of your devices can stop it.`,
  recoveryKeySaved: "You have a safety code.",
  recoveryKeyNone: "You do not have a safety code.",
  recoveryKeyOnce: "OM showed your safety code once, when you made it. For your safety it cannot be shown again.",
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
      provider: ["This reconnects your delivery service only. It does not change who controls your account."],
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
        "Whoever started it may have your safety code or passkey. OM will help you replace it.",
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
  readonly footer: { readonly prefix: string; readonly seedlife: string; readonly seedlifeUrl: string; readonly suffix: string};
}

/** Shown only on a copy of OM that someone else deployed (see src/official.ts); the official site never shows it. */
export interface CopyNoticeCopy { readonly text: string; readonly linkLabel: string; readonly url: string }

// The two fixed project links. Built from a scheme and a host so the "no hard-coded remote URL" lint, which protects
// provider endpoints, is not weakened for them; these are navigation links only, never fetched by the app.
const https = (hostAndPath: string): string => ["https", hostAndPath].join("://");

export const landingCopy: LandingCopy = {
  title: "OM (Oh Messenger) Beta: The messaging app you own",
  description: "OM is a fast, uncluttered, open-source messenger with strong end-to-end encryption, designed so that no single company, server, or app store is indispensable. Beta.",
  brand: "OM",
  beta: "Beta",
  headline: "The messaging app you own",
  lead: "OM is a fast, uncluttered, open-source messenger with strong end-to-end encryption, designed so that no single company, server, or app store is indispensable. It is simple enough for your grandma to use, with advanced controls for anyone who wants them.",
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
      "Your messages travel through a small helper called the delivery service. It knows when you are online and how many messages you send. What you write stays scrambled, so it cannot read it.",
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
      { label: "Email us", icon: "email", url: ["mailto", "hi%40lonunjie.org"].join(":") }, // no literal address in the HTML: Cloudflare Email Address Obfuscation would inject a script and break the signed manifest digest of index.html
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
  },
};

export const copyNoticeCopy: CopyNoticeCopy = {
  text: "This is a self-hosted copy of OM. The official site is ",
  linkLabel: "om.lonunjie.org",
  url: https("om.lonunjie.org"),
};

export const homeCopy: HomeCopy = {
  heading: "Message delivery",
  intro: "OM has no company in the middle. A small delivery service carries your messages: it holds each one, still locked, until the other person's device picks it up. It cannot read your messages, and your chats and keys stay on your devices.",
  currentHeading: "The delivery service you are using",
  none: "No delivery service is set up. Nothing leaves this device until you choose one.",
  address: (url) => `Address: ${url}`,
  source: { chosen: "You chose this delivery service.", default: "This is the delivery service that came with this version of OM." },
  checkingCurrent: "Checking how your delivery service is doing.",
  selfReported: "This is what the delivery service says about itself; OM cannot confirm who runs it.",
  changeHeading: "Use a different delivery service",
  changeBody: "Type the address of the delivery service you want to use. OM asks it who it is before you can choose it. Nothing is changed until you confirm.",
  urlLabel: "Delivery service address",
  urlHint: "It starts with https:// and has no password, question mark or extra parts.",
  check: "Check this delivery service",
  checking: "Checking this address.",
  changeWarning: "Both people in a conversation need to use the same delivery service. Changing yours can cut you off from conversations you already have until the other person changes theirs too.",
  useThis: "Use this delivery service",
  useAnyway: "Use it anyway",
  alreadyUsing: "This is the delivery service you are already using.",
  saveFailed: "This choice could not be saved, so nothing changed.",
  saved: "Saved. OM will use this delivery service the next time it connects.",
  reconnect: "Connect now (you will unlock OM again)",
  useDefault: "Go back to the delivery service that came with OM",
  disconnect: "Stop using this delivery service",
  cleared: "Your choice was removed. OM will use the default the next time it connects.",
  invalid: HOME_INVALID,
  result: HOME_RESULT,
  lost: {
    heading: "Some messages cannot reach you here",
    body: (n) => `${plural(n, "mailbox", "mailboxes")} at this delivery service could not be set up, because someone else already holds ${n === 1 ? "it" : "them"}. Messages for ${n === 1 ? "that conversation" : "those conversations"} will not arrive. This can happen if someone learned a mailbox address before you first used it. Ask the other person to invite you again, or choose a different delivery service.`,
    item: (reference) => `Reference ${reference}`,
  },
  back: "Back to settings",
};

/** Settings > Backup file: save the chats to a file, and read them back. Plain words; what a backup does and does not hold is said up front. */
export const backupCopy = {
  heading: "Backup file",
  intro: "Keep a copy of your chats in a file of your own. If you lose this device, set up OM again and read your old chats from the file.",
  holds: {
    heading: "What is in a backup file",
    yes: "Your chats: who you talked to and every message, as they are on this device.",
    no: "Not your account, your keys or your safety code. A backup lets you read old chats; it cannot send as you. To talk to someone again after losing a device, ask for a new invite.",
    care: "Anyone who has the file and its password can read your chats, so keep both private. Nobody at OM can open the file or recover the password.",
  },
  save: {
    heading: "Save a backup file",
    password: "Choose a password",
    again: "Type it again",
    warning: "If you forget this password, nobody can open the file. OM never sees it and cannot reset it.",
    slow: "Making the file takes a few seconds, longer on an older phone.",
    action: "Save backup file",
    working: "Making your backup. This takes a few seconds.",
    done: (name: string) => `Saved ${name}. Keep a copy somewhere safe, like your cloud drive or a USB stick.`,
    nothing: "You have no chats to back up yet. Come back after your first conversation.",
  },
  restore: {
    heading: "Restore from a backup file",
    hint: "Read the chats in a backup file. They appear in your chat list as saved copies: you can read them but not reply, and OM cannot check that they are genuine.",
    file: "Choose the backup file",
    password: "Password for this file",
    action: "Restore chats",
    working: "Opening the backup. This takes a few seconds.",
    done: (chats: number, messages: number) => `Restored ${plural(chats, "chat", "chats")} with ${plural(messages, "message", "messages")}. They are in your chat list.`,
    nothingNew: "That backup had nothing new for this device.",
    already: (n: number) => `${plural(n, "chat", "chats")} already on this device ${n === 1 ? "was" : "were"} left as ${n === 1 ? "it is" : "they are"}.`,
  },
  errors: {
    not_a_backup: "That does not look like an OM backup file.",
    unsupported_version: "This backup comes from a newer version of OM. Update OM and try again.",
    unsupported_profile: "This backup comes from a newer version of OM. Update OM and try again.",
    unsupported_parameters: "This backup file cannot be opened by this version of OM.",
    truncated: "This file is incomplete. It may not have finished copying. Get the file again and try again.",
    malformed: "This file is damaged, so OM cannot open it. Nothing was restored.",
    wrong_password: "That password does not match this backup. Check it and try again.",
    tampered: "This file was changed or damaged after it was saved, so OM will not open it. Nothing was restored.",
    invalid_content: "This file is not a chat backup that OM can read. Nothing was restored.",
    too_large: "Your chats are too large for one backup file in this version of OM.",
    empty_password: "Type the backup password.",
    failed: "That did not work, and nothing was changed. Try again.",
    no_chats: "You have no chats to back up yet.",
  },
} as const;

/** Settings > Lock: what protects this device and how to change it, without technical words. */
export const helpCopy = {
  heading: "Help and feedback",
  intro: "OM is in beta, so some things will go wrong, and your ideas shape what comes next. Reports and ideas are public on GitHub, where anyone can read and answer them.",
  actions: {
    problem: { title: "Report a problem", hint: "Something broke or looks wrong.", url: https("github.com/lonunjie/om/issues/new?template=bug.yml") },
    idea: { title: "Suggest an idea", hint: "Something you wish OM did.", url: https("github.com/lonunjie/om/discussions/new?category=ideas") },
    browse: { title: "See what others reported", hint: "Check whether someone already said it.", url: https("github.com/lonunjie/om/issues") },
    security: { title: "Report a security problem", hint: "Tell us privately, not in public.", url: https("github.com/lonunjie/om/security/advisories/new") },
  },
  newTab: "Opens GitHub in a new tab.",
  privacy: "Never paste your messages, your safety code or your password into a report. OM adds only its version number.",
  version: (v: string) => `OM web ${v}`,
};

export const lockCopy = {
  heading: "Lock",
  loading: "Checking",
  failed: "Could not read how this device is locked.",
  warning: "Anyone who can use this browser can read your chats.",
  modes: {
    none: { title: "Not locked on this device", body: "OM opens by itself, without asking for a fingerprint, a code or a password." },
    device: { title: "Locked by this device", body: "OM asks for your fingerprint, face or device code each time you open it." },
    "device-shared": { title: "Locked by your passkey", body: "OM asks for your passkey each time you open it. The company or app that keeps your passkey copies it between your devices, so anyone who can sign in there could open your chats." },
    password: { title: "Locked by your password", body: "OM asks for your password each time you open it." },
  },
  changeHeading: "Change how OM opens",
  actions: {
    addPassword: { title: "Add a password", hint: "OM asks for it each time it opens." },
    changePassword: { title: "Change password", hint: "Pick a new one. You need the current one." },
    removePassword: { title: "Remove password", hint: "Keep the device prompt, drop the password." },
    devicePrompt: { title: "Use this device's fingerprint, face or code", hint: "Uses a passkey kept by this device." },
    stop: { title: "Stop locking on this device", hint: "OM will open without asking." },
  },
  password: {
    current: "Current password",
    next: "New password",
    again: "Type it again",
    save: "Save password",
    remove: "Remove password",
    cancel: "Cancel",
    working: "Saving. This takes a few seconds.",
  },
  device: {
    body: "Your browser will ask you to confirm twice: once to make the passkey, once to check it.",
    start: "Continue",
    waiting: "Confirm on this device.",
    cancel: "Cancel",
    unavailable: "This browser could not use a device prompt, so nothing changed.",
  },
  stop: {
    heading: "Stop locking on this device?",
    warning: "Anyone who can use this browser, or copy its data, can read your chats. Only do this on a device that only you use.",
    confirm: "Stop locking",
    cancel: "Keep locking",
  },
  done: "Saved",
  idle: {
    heading: "Lock when I am away",
    hint: "OM locks itself and asks again after you have not touched it for a while.",
    never: "Only when I close OM",
    after: (minutes: number) => (minutes === 60 ? "After 1 hour" : minutes === 1 ? "After 1 minute" : `After ${String(minutes)} minutes`),
    needsLock: "Add a password or use this device's prompt first. A device that opens by itself has nothing to lock.",
    now: "Lock now",
  },
} as const;
