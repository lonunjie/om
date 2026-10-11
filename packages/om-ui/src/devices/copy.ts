import type {
  AccountControlStateCode, CoreIntentKind, DeviceCautionCode, DeviceRole, DeviceStateCode, LastSeenClass, LinkOfferState, LinkRequestState,
  RejectReason, UserActionCode,
} from "@om/platform-api";

/** Wording for Core outcome classes. A timeout is never terminal, so there is no "failed" for pending outcomes. */
export interface OutcomeCopy {
  readonly working: string;
  readonly pendingAuthorization: string;
  readonly pendingNetwork: string;
  readonly pendingReconciliation: string;
  readonly needsAction: Readonly<Record<UserActionCode, string>>;
  readonly rejected: Readonly<Record<RejectReason, string>>;
  /** The request could not be handed to Core at all (nothing was sent). */
  readonly failed: string;
}

export type FrozenControlState = Exclude<AccountControlStateCode, "normal">;
export type DevicePendingKind = Extract<CoreIntentKind, "RequestDeviceLink" | "ApproveDeviceLink" | "RejectDeviceLink" | "SuspendDevice" | "RevokeDevice">;
export type LinkParseError = "empty" | "too_long" | "malformed";

/** Existing-device side of linking: open the new device's offer, compare the code, approve or reject. */
export interface LinkCopy {
  readonly start: string;
  readonly heading: string;
  readonly body: string;
  readonly scan: string;
  readonly scanNothing: string;
  readonly pasteLabel: string;
  readonly pasteHint: string;
  readonly open: string;
  readonly parseError: Readonly<Record<LinkParseError, string>>;
  readonly cancel: string;
  readonly reviewHeading: string;
  readonly who: (deviceLabel: string) => string;
  readonly codeLabel: string;
  readonly codeHint: string;
  /** Always shown before approval: only link a device you hold; every other device is told (R-166). */
  readonly warning: string;
  readonly announce: string;
  readonly approve: string;
  readonly reject: string;
  readonly expires: (epochMs: number) => string;
  readonly requestState: Readonly<Record<LinkRequestState, string>>;
  readonly linked: string;
  readonly rejectedDone: string;
  readonly done: string;
}

/** New-device side: show the offer, then the same code, then wait for the other device's decision. */
export interface LinkThisDeviceCopy {
  readonly heading: string;
  readonly body: string;
  readonly start: string;
  readonly linkLabel: string;
  readonly qrLabel: string;
  readonly expires: (epochMs: number) => string;
  readonly codeLabel: string;
  readonly codeHint: string;
  readonly state: Readonly<Record<LinkOfferState, string>>;
  readonly outcome: OutcomeCopy;
}

export interface DeviceActionCopy {
  readonly action: string;
  readonly heading: (deviceLabel: string) => string;
  /** Explicit consequences, shown before confirming. */
  readonly consequences: readonly string[];
  readonly confirm: string;
  readonly cancel: string;
  readonly done: string;
}

export interface DevicesCopy {
  readonly heading: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly controlState: Readonly<Record<FrozenControlState, string>>;
  readonly deviceLinkedNotice: (deviceLabel: string, epochMs: number) => string;
  readonly linkRequestsHeading: string;
  readonly linkRequestRow: (deviceLabel: string) => string;
  readonly currentHeading: string;
  readonly otherHeading: string;
  readonly noOtherDevices: string;
  readonly removedHeading: string;
  readonly role: Readonly<Record<DeviceRole, string>>;
  readonly state: Readonly<Record<DeviceStateCode, string>>;
  readonly lastSeen: Readonly<Record<LastSeenClass, string>>;
  readonly cautions: Readonly<Record<DeviceCautionCode, string>>;
  readonly enrolled: (epochMs: number) => string;
  readonly thisDevice: string;
  readonly back: string;
  readonly detailsHeading: (deviceLabel: string) => string;
  readonly noActions: string;
  readonly frozenNoActions: string;
  readonly suspend: DeviceActionCopy;
  readonly revoke: DeviceActionCopy;
  readonly pendingHeading: string;
  readonly pendingKind: Readonly<Record<DevicePendingKind, string>>;
  readonly link: LinkCopy;
  readonly outcome: OutcomeCopy;
}
