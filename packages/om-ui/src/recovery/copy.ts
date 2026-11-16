import type { RecoveryCustodianCode, RecoveryPolicyState, RecoveryRequestState, RecoveryScope } from "@om/platform-api";
import type { OutcomeCopy } from "../devices/copy";

/**
 * What a contact sees while someone they chat with gets their account back on a new device (ADR-0190 sections 4 and 9).
 * Plain words, no digests, epochs or policy terms. `{days}` is Core's whole days left, formatted by `days`.
 */
export interface RecoveryContactCopy {
  /** Accessible name of the banner region. */
  readonly region: (name: string) => string;
  /** "about 3 days", "about a day", "less than a day". Never a precise or promised time. */
  readonly days: (days: number) => string;
  readonly waiting: {
    readonly title: (name: string) => string;
    readonly body: (days: string) => string;
    readonly check: (name: string) => string;
    readonly checkDetail: (name: string) => string;
    readonly decline: string;
    /** What turning it down does, shown with the button. */
    readonly declineNote: (name: string) => string;
  };
  readonly ready: {
    /** The wait ran and this device needs the person's own yes (Hardened and Maximum Privacy). */
    readonly title: (name: string) => string;
    readonly consent: (name: string) => string;
    readonly confirm: string;
    readonly decline: string;
    /** The wait ran but a step is missing and nothing is asked of the person. */
    readonly checking: string;
  };
  readonly accepted: (name: string) => string;
  readonly vetoed: (name: string) => string;
  readonly declined: (name: string) => string;
  readonly outcome: OutcomeCopy;
}

export type RecoveryPendingKind = "StartRecovery" | "VetoRecovery";

export interface RecoveryCopy {
  readonly heading: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly back: string;
  readonly policy: Readonly<Record<RecoveryPolicyState, string>>;
  readonly custodiansHeading: string;
  readonly custodian: Readonly<Record<RecoveryCustodianCode, string>>;
  readonly distributionBound: string;
  readonly scopesHeading: string;
  readonly scope: Readonly<Record<RecoveryScope, { readonly title: string; readonly body: string }>>;
  /** Core's policy delay, formatted; the UI never computes timing rules. */
  readonly delay: (ms: number) => string;
  readonly recoveryKeySaved: string;
  readonly recoveryKeyNone: string;
  readonly recoveryKeyOnce: string;
  /** Shown on the reserved surface while someone else is recovering this account. */
  readonly incomingAlert: string;
  readonly review: string;
  readonly requestsHeading: string;
  readonly direction: Readonly<Record<"incoming" | "outgoing", string>>;
  readonly requestState: Readonly<Record<RecoveryRequestState, string>>;
  readonly started: (epochMs: number) => string;
  readonly activatesAt: (epochMs: number) => string;
  readonly activatesUnknown: string;
  readonly stopIncoming: string;
  readonly cancelOutgoing: string;
  readonly pendingHeading: string;
  readonly pendingKind: Readonly<Record<RecoveryPendingKind, string>>;
  readonly start: {
    readonly action: string;
    readonly heading: string;
    readonly body: string;
    readonly scopeLegend: string;
    readonly custodianLegend: string;
    readonly consequences: Readonly<Record<RecoveryScope, readonly string[]>>;
    readonly confirm: string;
    readonly cancel: string;
    readonly done: string;
  };
  readonly veto: {
    readonly heading: Readonly<Record<"incoming" | "outgoing", string>>;
    readonly consequences: Readonly<Record<"incoming" | "outgoing", readonly string[]>>;
    readonly confirm: Readonly<Record<"incoming" | "outgoing", string>>;
    readonly cancel: string;
    readonly done: string;
  };
  readonly outcome: OutcomeCopy;
  /** The contact side (ADR-0190): the banner in a conversation with a person who is recovering their account. */
  readonly contact: RecoveryContactCopy;
}
