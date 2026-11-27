import type { BridgeCopy } from "../bridges/copy";
// SPDX-License-Identifier: MPL-2.0
import type {
  OutcomeClass,
  RejectReason,
  UserActionCode,
} from "@om/platform-api";
import type { ReactNode } from "react";
import type {
  ServiceAccessMode,
  ServiceActionKind,
  ServiceDisclosureKind,
  ServiceEgressState,
  ServiceEntryView,
  ServiceScope,
} from "./services";

/** Localized presentation for every service view and shared Core outcome. */
export interface ServicesCopy {
  readonly heading: string;
  readonly empty: string;
  readonly loading: string;
  readonly changing: string;
  readonly loadFailed: string;
  readonly changeUnknown: string;
  readonly refresh: string;
  readonly close: string;
  readonly review: string;
  readonly reviewName: (name: string) => string;
  readonly reviewTitle: string;
  readonly invocationTitle: string;
  readonly invocationReview: (name: string) => string;
  readonly status: Readonly<Record<ServiceEntryView["status"], string>>;
  readonly scopeLabel: string;
  readonly requestedScopeLabel: string;
  readonly scope: Readonly<Record<ServiceScope["kind"], string>>;
  readonly spaceBoundary: string;
  readonly modeLabel: string;
  readonly mode: Readonly<Record<ServiceAccessMode, string>>;
  readonly modeConsequence: Readonly<Record<ServiceAccessMode, string>>;
  readonly requested: string;
  readonly granted: string;
  readonly noCapabilities: string;
  readonly access: string;
  readonly noRead: string;
  readonly localRead: string;
  readonly noLocalRead: string;
  readonly triggerFilter: string;
  readonly triggerClasses: string;
  readonly noTriggerFilter: string;
  readonly eventClass: BridgeCopy["eventClass"];
  readonly previouslyShared: string;
  readonly addedBy: string;
  readonly addedFrom: string;
  readonly addedAt: string;
  readonly formatTime: (ms: number) => string;
  readonly author: {
    readonly automation: string;
    readonly via: (person: ReactNode, service: ReactNode) => ReactNode;
  };
  readonly action: Readonly<Record<ServiceActionKind, string>>;
  readonly consequence: Readonly<Record<"revoke" | "remove", string>>;
  readonly destination: string;
  readonly disclosure: string;
  readonly disclosureExpires: string;
  readonly shareAndRun: string;
  readonly localDisclosure: string;
  readonly discloseOnce: string;
  readonly item: Readonly<Record<ServiceDisclosureKind, string>>;
  readonly tainted: string;
  readonly untainted: string;
  readonly egress: Readonly<Record<ServiceEgressState, string>>;
  readonly outcome: Readonly<
    Record<
      Exclude<OutcomeClass["kind"], "Rejected" | "NeedsUserAction">,
      string
    >
  >;
  readonly rejected: Readonly<Record<RejectReason, string>>;
  readonly needsAction: Readonly<Record<UserActionCode, string>>;
}
