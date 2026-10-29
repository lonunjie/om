import type {
  ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason,
} from "@om/platform-api";

export type InviteParseErrorCode = "empty" | "too_long" | "malformed";
/** Machine codes for controls that are hidden because a slot or capability is missing. */
export type InviteUnavailableCode = "qr_renderer_missing" | "scan_slot_missing" | "camera_unavailable";

/** All user-visible strings for first run. Components contain no copy; callers supply (localized) text. */
export interface OnboardingCopy {
  readonly welcome: { readonly heading: string; readonly body: string; readonly start: string };
  readonly protect: {
    readonly heading: string;
    readonly body: string;
    readonly action: string;
    readonly working: string;
    readonly passkeyUser: { readonly name: string; readonly displayName: string };
    readonly defaultProfileLabel: string;
    readonly resultHeading: string;
    /** Summary label for the collapsed explanation of how chats are protected. */
    readonly moreOptions: string;
    readonly method: Readonly<Record<WrappingMethod, string>>;
    readonly reason: Readonly<Record<WrappingReason, string>>;
    /** ADR-0129: a synced passkey's PRF wraps the vault. */
    readonly reducedAtRest: string;
    /** A copy of this browser profile can open local data without any passkey prompt. */
    readonly deviceKeyLimit: string;
    readonly nativeRecommended: string;
    readonly limitsHeading: string;
    readonly limits: Readonly<Record<LimitCode, string>>;
    readonly needsGesture: Readonly<Record<VaultStage, string>>;
    readonly continueCeremony: string;
    readonly errors: Readonly<Record<VaultErrorReason, string>>;
    readonly retry: string;
    readonly existing: Readonly<Record<"already_exists" | "corrupt", string>>;
    readonly openRecovery: string;
    readonly profileFailed: string;
    readonly retryProfile: string;
    /** Optional passphrase layer (Argon2id). Off by default; a forgotten passphrase cannot be recovered. */
    readonly passphrase: {
      readonly toggle: string;
      readonly hint: string;
      readonly warning: string;
      readonly label: string;
      readonly confirmLabel: string;
      readonly tooShort: (min: number) => string;
      readonly mismatch: string;
      readonly slowNote: string;
    };
  };
  readonly recovery: {
    readonly heading: string;
    readonly body: string;
    readonly shownAnnouncement: string;
    readonly keyLabel: string;
    readonly group: (n: number, total: number) => string;
    readonly loading: string;
    readonly unavailable: string;
    readonly copy: string;
    readonly copied: string;
    readonly copyUnavailable: string;
    /** Shown after a copy that could not be marked sensitive: other apps may be able to read the clipboard. */
    readonly copiedOpen: string;
    readonly copyFailed: string;
    readonly toVerify: string;
    readonly verifyHeading: string;
    readonly verifyBody: string;
    readonly verifyField: (n: number) => string;
    readonly showAgain: string;
    readonly saved: string;
    readonly confirming: string;
    readonly confirmFailed: string;
  };
  readonly invite: {
    readonly heading: string;
    readonly body: string;
    readonly share: string;
    readonly shareTitle: string;
    readonly shareText: string;
    readonly shareCancelled: string;
    readonly shareFailed: string;
    readonly linkLabel: string;
    readonly expires: (epochMs: number) => string;
    readonly showQr: string;
    readonly qrLabel: string;
    readonly scan: string;
    readonly scanDenied: string;
    readonly scanNothing: string;
    readonly acceptHeading: string;
    readonly acceptLabel: string;
    readonly acceptHint: string;
    readonly accept: string;
    readonly parseError: Readonly<Record<InviteParseErrorCode, string>>;
    readonly rejected: Readonly<Record<RejectReason, string>>;
    readonly failed: string;
    /** The request is saved on this device and goes out once a provider is connected (nothing was sent yet). */
    readonly queued: string;
    readonly skip: string;
  };
  readonly home: {
    readonly toggle: string;
    readonly heading: string;
    readonly loading: string;
    readonly loadFailed: string;
    readonly tier: (tier: string) => string;
    /** One plain sentence on what a Home is, shown first. */
    readonly intro: string;
    readonly observers: Readonly<Record<HomeObserverCode, string>>;
    readonly migrationAvailable: string;
    readonly migrationUnavailable: string;
    readonly origin: Readonly<Record<ClientOriginChoiceCode, string>>;
  };
}

/** Strings for unlocking an existing vault. Separate from first-run copy: it is used on every later start. */
export interface UnlockCopy {
  readonly heading: string;
  readonly body: string;
  readonly passphraseLabel: string;
  readonly unlock: string;
  readonly working: string;
  readonly workingSlow: string;
  readonly continueCeremony: string;
  readonly needsGesture: Readonly<Record<VaultStage, string>>;
  readonly errors: Readonly<Record<VaultErrorReason, string>>;
  readonly openRecovery: string;
}
