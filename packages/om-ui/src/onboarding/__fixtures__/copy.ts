import type {
  ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason,
} from "@om/platform-api";
import type { OnboardingCopy, UnlockCopy } from "../copy";
import { testJoinCopy } from "../../devices/__fixtures__/copy";

const rec = <K extends string>(keys: readonly K[], f: (k: K) => string): Readonly<Record<K, string>> => Object.fromEntries(keys.map((k) => [k, f(k)])) as Record<K, string>;
const REASONS: readonly RejectReason[] = ["invalid", "unsupported", "unauthorized", "revoked", "expired", "stale_state", "conflict", "policy_denied", "quota", "rate_limited", "integrity_failure", "unavailable", "not_found"];
const VAULT_ERRORS: readonly VaultErrorReason[] = [
  "invalid_profile_id", "invalid_purpose", "invalid_key", "not_found", "corrupt", "locked", "already_exists", "conflict", "needs_user_gesture", "ceremony_failed",
  "prf_unavailable", "unwrap_failed", "unsupported_factor", "passphrase_required", "wrong_passphrase", "unsupported_platform", "malformed", "unsupported_version", "wrong_purpose", "integrity_failure",
  "crypto_failed", "storage_failed", "delete_unverified",
];
const WRAPPING_REASONS: readonly WrappingReason[] = ["prf_verified", "device_key_requested", "webauthn_absent", "prf_not_enabled", "prf_no_output", "webauthn_create_failed", "webauthn_get_failed"];
const METHODS: readonly WrappingMethod[] = ["webauthn-prf", "device-key"];
const STAGES: readonly VaultStage[] = ["enroll_create", "enroll_verify", "unlock", "rotate_unwrap"];
const LIMITS: readonly LimitCode[] = [
  "secure_surface_unavailable", "backup_exclusion_unavailable", "clipboard_sensitive_unsupported", "clipboard_unavailable", "notification_removal_unavailable",
  "notification_removal_partial", "input_privacy_partial", "input_privacy_unavailable", "background_push_none", "local_discovery_not_automatic",
  "local_discovery_unavailable", "call_system_integration_unavailable",
];
const OBSERVERS: readonly HomeObserverCode[] = ["sponsor_sees_metadata", "host_sees_metadata", "host_sees_ciphertext", "no_third_party"];
const ORIGINS: readonly ClientOriginChoiceCode[] = ["default_origin", "independent_origin_available", "independent_origin_active"];

export const testUnlockCopy: UnlockCopy = {
  heading: "Unlock OM", body: "Unlock your data on this device.", bodyPassphrase: "Type your password.", passphraseLabel: "Passphrase", unlock: "Unlock", working: "Unlocking", workingSlow: "Unlocking, this takes a few seconds",
  continueCeremony: "Continue", needsGesture: rec(STAGES, (s) => `Another confirmation is needed (${s})`), errors: rec(VAULT_ERRORS, (e) => `Error: ${e}`), openRecovery: "Open recovery",
};

export const testCopy: OnboardingCopy = {
  frame: { brand: "OM", beta: "Beta", step: (n, total) => `Step ${String(n)} of ${String(total)}` },
  welcome: { heading: "Welcome", body: "Set up this device.", start: "Get started" },
  join: {
    choiceHeading: "How do you want to begin", startNew: "Start new", startNewHint: "First time using OM", useExisting: "I already use OM", useExistingHint: "Add this device to your account",
    protectHeading: "Lock this device first", protectBody: "OM locks itself before it joins.", protectAction: "Lock this device", back: "Go back", link: testJoinCopy,
  },
  protect: {
    heading: "Protect this device", body: "Your data is locked on this device.", action: "Protect this device", working: "Protecting",
    passkeyUser: { name: "me", displayName: "Me" }, defaultProfileLabel: "Me", resultHeading: "Protection chosen", moreOptions: "More options",
    method: rec(METHODS, (m) => `Method: ${m}`), reason: rec(WRAPPING_REASONS, (r) => `Because: ${r}`),
    reducedAtRest: "REDUCED: a synced passkey protects this vault.", deviceKeyLimit: "DEVICE KEY: a copy of this browser profile opens local data without a passkey prompt.",
    nativeRecommended: "Use a native app for Hardened protection.", limitsHeading: "What this browser cannot do", limits: rec(LIMITS, (l) => `Limit: ${l}`),
    needsGesture: rec(STAGES, (s) => `Another confirmation is needed (${s})`), continueCeremony: "Continue confirmation",
    waiting: { body: "Waiting for this device.", slow: "Still waiting.", skip: "Skip this prompt" },
    errors: rec(VAULT_ERRORS, (e) => `Error: ${e}`), retry: "Try again",
    existing: { already_exists: "This browser already has an OM vault.", corrupt: "The OM vault here is damaged." }, openRecovery: "Open recovery",
    profileFailed: "Could not create your identity.", retryProfile: "Retry identity",
    passphrase: {
      toggle: "Also use a passphrase", hint: "Adds a second lock.", warning: "If you forget it, this data cannot be recovered.", label: "Passphrase",
      confirmLabel: "Repeat passphrase", tooShort: (n) => `Use at least ${String(n)} characters.`, mismatch: "The passphrases do not match.", slowNote: "Unlocking takes a few seconds.",
    },
  },
  recovery: {
    heading: "Your recovery key", body: "Write this key down. It is shown once.", shownAnnouncement: "Recovery key shown on screen.", keyLabel: "Recovery key",
    group: (n, t) => `Group ${String(n)} of ${String(t)}:`, loading: "Loading key", unavailable: "The key is no longer available.",
    copy: "Copy key", copied: "Copied", copiedOpen: "Copied. Other apps can read your clipboard.", copyUnavailable: "Copy is not available here; write the key down.", copyFailed: "Copy failed",
    toVerify: "Check my copy", verifyHeading: "Check your recovery key", verifyBody: "Type the requested groups.", verifyField: (n) => `Group ${String(n)}`,
    showAgain: "Show key again", saved: "I saved it", confirming: "Confirming", confirmFailed: "Could not confirm.",
  },
  invite: {
    heading: "Add your first contact", body: "Invite someone you know.", share: "Share invite link", shareTitle: "Join me", shareText: "Open this link in OM.",
    shareFailed: "Sharing failed", copyLink: "Copy link", copied: "Copied", copyFailed: "Copy failed", sharedByCopy: "Link copied", linkLabel: "Your invite link", expires: (ms) => `Expires at ${String(ms)}`,
    showQr: "Show my QR", qrLabel: "Your invite QR code", scan: "Scan QR", scanDenied: "Camera not allowed", scanNothing: "Nothing scanned",
    acceptHeading: "Accept an invitation", acceptLabel: "Invitation link", haveInvite: "Have an invite?", accept: "Accept invitation",
    parseError: { empty: "Enter a link", too_long: "That link is too long", malformed: "That is not an OM invitation" },
    rejected: rec(REASONS, (r) => `Invitation rejected: ${r}`), failed: "Something went wrong", queued: "Saved on this device; it goes out when connected", skip: "Skip for now",
  },
  incoming: {
    heading: "Accept this invite?", body: "Someone sent you an invite.", accept: "Accept invite", decline: "Not now", accepting: "Sending", parseError: "That invite link does not work.", rejected: rec(REASONS, (r) => `Invite not accepted: ${r}`),
    failed: "Something went wrong", queued: "Saved on this device"
  },
  home: {
    toggle: "Where is my Home?", heading: "Home facts", loading: "Loading", loadFailed: "Could not load Home facts", tier: (t) => `Home tier ${t}`, intro: "A Home holds your messages.",
    observers: rec(OBSERVERS, (o) => `Observer: ${o}`), migrationAvailable: "You can move your Home.", migrationUnavailable: "Moving is not available.",
    origin: rec(ORIGINS, (o) => `Origin: ${o}`),
  },
  relay: {
    heading: "OM is different", why: ["You own your data, and you own your delivery service.", "So even when we are not around, your messages are yours."], pick: "Choose how to set up your delivery service. Come back here once it is set up.",
    easyHeading: "Easy", ownFree: "Set up my own, free", advancedHeading: "Advanced", advancedIntro: "Set up your own server. These are coming later.", later: { items: ["On my own server, as a container", "With an install script"] },
    cloudflare: { uses: "Two free accounts: Cloudflare, and GitHub or GitLab. You create or sign in to them on the next steps.", steps: ["Copy your setup value.", "Open Cloudflare setup. Connect a GitHub or GitLab account.", "Paste your setup value in the box named OM setup value and deploy.", "If the page stops, open the setup link again.", "Come back and type the address."], addressExample: "Looks like https://om-relay.you.workers.dev", protect: "Protect that Cloudflare account with a passkey.", open: "Open Cloudflare setup", opensNewTab: "(opens in a new tab)", openAgain: "You can use this button again.", notReady: "Not ready yet.", accounts: { heading: "Before you start", intro: "You need two accounts.", cloudflare: { heading: "A Cloudflare account", body: "Sign in to Cloudflare." }, git: { heading: "A GitHub or GitLab account", body: "Sign in to GitHub.", shareAdvice: "Use a new account if you prefer." }, unverified: "Screens may differ.", confirm: "I have it", confirmed: "Confirmed", next: "Continue", nextWaiting: "Confirm both to continue.", back: "Back" } },
    setup: {
      heading: "Set up your delivery service", valueLabel: "Your setup value", valueNote: "It is not a secret.", preparing: "Making your setup value", prepareFailed: "Setup value failed", copy: "Copy setup value", copied: "Setup value copied",
      copyFailed: "Copy failed", addressLabel: "Address of your delivery service", addressHint: "Starts with https://", check: "Check and continue", checking: "Checking the address", saveFailed: "Not saved", notLocked: "Not locked", back: "Back",
    },
    invalid: { malformed: "Not a web address", not_https: "Must be https", credentials: "No credentials", query_or_fragment: "No query" },
    result: {
      ok: (id) => `Answered as ${id}`, health: { healthy: "Working", degraded: "Reduced service", unhealthy: "Not working", unknown: "Unknown" }, unreachable: "Could not reach it", failing: (n) => `Server error ${String(n)}`,
      notAHome: "Not a delivery service", wrongVersion: (o) => `Wrong version ${o}`, noMailbox: "Does not deliver messages",
    },
  },
};
