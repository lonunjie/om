import type {
  OutcomeClass,
  RejectReason,
  UserActionCode,
} from "@om/platform-api";
import type { ReactNode } from "react";
import type {
  BridgeCarry,
  BridgeDirection,
  BridgeEntryView,
  BridgeEventClass,
  BridgeFidelityFlag,
  BridgeIntent,
  BridgeMappingState,
} from "./bridges";

/** The host provides localized labels, including one for every Core outcome this surface accepts. */
export interface BridgeCopy {
  readonly heading: string;
  readonly empty: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly changing: string;
  readonly changeUnknown: string;
  readonly refresh: string;
  readonly review: string;
  readonly reviewName: (bridge: string) => string;
  readonly reviewTitle: string;
  readonly close: string;
  readonly action: Readonly<Record<BridgeIntent["kind"], string>>;
  readonly consequence: Readonly<Record<BridgeIntent["kind"], string>>;
  readonly membership: Readonly<Record<BridgeEntryView["membership"], string>>;
  readonly mapping: Readonly<Record<BridgeMappingState, string>>;
  readonly principal: string;
  readonly network: string;
  readonly scope: string;
  readonly directionLabel: string;
  readonly direction: Readonly<Record<BridgeDirection, string>>;
  readonly exports: string;
  readonly noExports: string;
  readonly eventClass: Readonly<Record<BridgeEventClass, string>>;
  readonly admissionBoundary: string;
  readonly encryptionTo: (bridge: ReactNode) => ReactNode;
  readonly externalSeparate: string;
  readonly externalClaim: string;
  readonly noExternalClaim: string;
  readonly outcome: Readonly<
    Record<
      Exclude<OutcomeClass["kind"], "Rejected" | "NeedsUserAction">,
      string
    >
  >;
  readonly rejected: Readonly<Record<RejectReason, string>>;
  readonly needsAction: Readonly<Record<UserActionCode, string>>;
  readonly message: {
    readonly author: string;
    readonly foreignActor: string;
    readonly unknownActor: string;
    readonly linkedIdentity: string;
    readonly noIdentityLink: string;
    readonly fidelity: string;
    readonly flag: Readonly<Record<BridgeFidelityFlag, string>>;
    readonly carry: Readonly<Record<BridgeCarry, string>>;
  };
}
