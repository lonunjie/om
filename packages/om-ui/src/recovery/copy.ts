import type { RecoveryCustodianCode, RecoveryPolicyState, RecoveryRequestState, RecoveryScope } from "@om/platform-api";
import type { OutcomeCopy } from "../devices/copy";

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
}
