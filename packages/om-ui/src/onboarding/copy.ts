import type { HomeCopy } from "../home/copy";
import type {
  ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason,
} from "@om/platform-api";

export type InviteParseErrorCode = "empty" | "too_long" | "malformed";
/** Machine codes for controls that are hidden because a slot or capability is missing. */
export type InviteUnavailableCode = "qr_renderer_missing" | "scan_slot_missing" | "camera_unavailable";

/** All user-visible strings for first run. Components contain no copy; callers supply (localized) text. */
export interface OnboardingCopy {
  /** Page chrome shared by every step. */
  readonly frame: { readonly brand: string; readonly beta: string; readonly step: (n: number, total: number) => string };
  readonly welcome: { readonly heading: string; readonly body: string; readonly start: string };
  readonly protect: {
    readonly heading: string;
    /** Optional: the first page asks one question and says nothing more. */
    readonly body?: string;
    readonly action: string;
    readonly working: string;
    readonly passkeyUser: { readonly name: string; readonly displayName: string };
    readonly defaultProfileLabel: string;
    /**
     * Asks for the name the other person sees (an invitation carries it). Without this slot nothing is asked and the profile
     * gets `defaultProfileLabel`, so every chat partner would read the same word.
     */
    readonly name?: { readonly label: string; readonly missing: string; readonly tooLong: (max: number) => string };
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
    /** Shown while the device's own lock prompt is open: say what is happening before it feels stuck. */
    readonly waiting: { readonly body: string; readonly slow: string; readonly skip: string };
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
    readonly body?: string;
    readonly share: string;
    readonly shareTitle: string;
    readonly shareText: string;
    readonly shareFailed: string;
    /** What to do with the link once it is on screen, in one plain sentence. */
    readonly copyLink: string;
    readonly copied: string;
    readonly copyFailed: string;
    /** Sharing is not available here, so the link was copied instead. */
    readonly sharedByCopy: string;
    readonly linkLabel: string;
    readonly expires: (epochMs: number) => string;
    readonly showQr: string;
    readonly qrLabel: string;
    readonly scan: string;
    readonly scanDenied: string;
    readonly scanNothing: string;
    readonly acceptHeading: string;
    readonly acceptLabel: string;
    readonly haveInvite: string;
    readonly accept: string;
    readonly parseError: Readonly<Record<InviteParseErrorCode, string>>;
    readonly rejected: Readonly<Record<RejectReason, string>>;
    readonly failed: string;
    /** The request is saved on this device and goes out once a provider is connected (nothing was sent yet). */
    readonly queued: string;
    readonly skip: string;
  };
  /** An invitation that arrived as a link (a camera scan or a chat app opened it): confirm, then say what happens next. */
  readonly incoming: {
    readonly heading: string;
    readonly body: string;
    readonly accept: string;
    readonly decline: string;
    readonly accepting: string;
    readonly parseError: string;
    readonly rejected: Readonly<Record<RejectReason, string>>;
    readonly failed: string;
    readonly queued: string;
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
  /** The first-run page where a person sets up their own delivery service (ADR-0186). */
  readonly relay: RelaySetupCopy;
}

/**
 * Wording for `RelaySetup`. The person-facing word is "delivery service". The vendor is named only in `cloudflare` (ADR-0186
 * decision 2: "they better know it's Cloudflare"); the copy test allows provider names in that group and nowhere else.
 */
export interface RelaySetupCopy {
  readonly heading: string;
  /** Why OM is different, as short sentences. */
  readonly why: readonly string[];
  readonly pick: string;
  readonly ownFree: string;
  /** Choices that are listed and not built yet: plain text, never a control. */
  readonly later: { readonly heading: string; readonly items: readonly string[] };
  /** The only strings that may name a vendor. */
  readonly cloudflare: { readonly uses: string; readonly steps: readonly string[]; /** What the address looks like, so a person knows it when the setup shows it. */ readonly addressExample: string; /** R-164 (HOME_ONBOARDING 5): steer to a passkey or security key on the account, with the reason in one sentence. */ readonly protect: string; readonly open: string; readonly opensNewTab: string; readonly notReady: string };
  readonly setup: {
    readonly heading: string;
    readonly valueLabel: string;
    readonly valueNote: string;
    readonly preparing: string;
    readonly prepareFailed: string;
    readonly copy: string;
    readonly copied: string;
    readonly copyFailed: string;
    readonly addressLabel: string;
    readonly addressHint: string;
    readonly check: string;
    readonly checking: string;
    readonly saveFailed: string;
    /** The address answered as a delivery service that does not ask for the setup value, so anyone could use it: refused (ADR-0186, owner admission). */
    readonly notLocked: string;
    readonly back: string;
  };
  /** What a probe can say about the typed address (the same lines as Settings > Message delivery). */
  readonly invalid: HomeCopy["invalid"];
  readonly result: HomeCopy["result"];
}

/** Strings for unlocking an existing vault. Separate from first-run copy: it is used on every later start. */
export interface UnlockCopy {
  readonly heading: string;
  /** What to do when the device itself asks (passkey, fingerprint, face). */
  readonly body: string;
  /** What to do when a password of the person's own is set: the field is already in front of them. */
  readonly bodyPassphrase: string;
  readonly passphraseLabel: string;
  readonly unlock: string;
  readonly working: string;
  readonly workingSlow: string;
  readonly continueCeremony: string;
  readonly needsGesture: Readonly<Record<VaultStage, string>>;
  readonly errors: Readonly<Record<VaultErrorReason, string>>;
  readonly openRecovery: string;
}
