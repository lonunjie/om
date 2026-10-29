import type {
  ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason,
} from "@om/platform-api";
import type { OnboardingCopy, UnlockCopy } from "../copy";

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
  heading: "Unlock OM", body: "Unlock your data on this device.", passphraseLabel: "Passphrase", unlock: "Unlock", working: "Unlocking", workingSlow: "Unlocking, this takes a few seconds",
  continueCeremony: "Continue", needsGesture: rec(STAGES, (s) => `Another confirmation is needed (${s})`), errors: rec(VAULT_ERRORS, (e) => `Error: ${e}`), openRecovery: "Open recovery",
};

export const testCopy: OnboardingCopy = {
  welcome: { heading: "Welcome", body: "Set up this device.", start: "Get started" },
  protect: {
    heading: "Protect this device", body: "Your data is locked on this device.", action: "Protect this device", working: "Protecting",
    passkeyUser: { name: "me", displayName: "Me" }, defaultProfileLabel: "Me", resultHeading: "Protection chosen",
    method: rec(METHODS, (m) => `Method: ${m}`), reason: rec(WRAPPING_REASONS, (r) => `Because: ${r}`),
    reducedAtRest: "REDUCED: a synced passkey protects this vault.", deviceKeyLimit: "DEVICE KEY: a copy of this browser profile opens local data without a passkey prompt.",
    nativeRecommended: "Use a native app for Hardened protection.", limitsHeading: "What this browser cannot do", limits: rec(LIMITS, (l) => `Limit: ${l}`),
    needsGesture: rec(STAGES, (s) => `Another confirmation is needed (${s})`), continueCeremony: "Continue confirmation",
    errors: rec(VAULT_ERRORS, (e) => `Error: ${e}`), retry: "Try again",
    existing: { already_exists: "This browser already has an OM vault.", corrupt: "The OM vault here is damaged." }, openRecovery: "Open recovery",
    profileFailed: "Could not create your identity.", retryProfile: "Retry identity", next: "Continue",
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
    shareCancelled: "Sharing cancelled", shareFailed: "Sharing failed", linkLabel: "Your invite link", expires: (ms) => `Expires at ${String(ms)}`,
    showQr: "Show my QR", qrLabel: "Your invite QR code", scan: "Scan QR", scanDenied: "Camera not allowed", scanNothing: "Nothing scanned",
    acceptHeading: "Accept an invitation", acceptLabel: "Invitation link", acceptHint: "Paste an om: link you received.", accept: "Accept invitation",
    parseError: { empty: "Enter a link", too_long: "That link is too long", malformed: "That is not an OM invitation" },
    rejected: rec(REASONS, (r) => `Invitation rejected: ${r}`), failed: "Something went wrong", queued: "Saved on this device; it goes out when connected", skip: "Skip for now",
  },
  home: {
    toggle: "Where is my Home?", heading: "Home facts", loading: "Loading", loadFailed: "Could not load Home facts", tier: (t) => `Home tier ${t}`, intro: "A Home holds your messages.",
    observers: rec(OBSERVERS, (o) => `Observer: ${o}`), migrationAvailable: "You can move your Home.", migrationUnavailable: "Moving is not available.",
    origin: rec(ORIGINS, (o) => `Origin: ${o}`),
  },
  ready: { heading: "You are ready", body: "Chats will appear here.", done: "Open chats" },
};
